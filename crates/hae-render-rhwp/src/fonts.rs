//! Explicit font directory: file list, content hashes and a font database for
//! rasterisation. Fonts are never taken from the system; only the configured
//! directory counts (spec §10 records the font list and its hash).

use hae_package::sha256_hex;
use resvg::usvg::fontdb;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

const MAX_FONT_FILES: usize = 4096;
const MAX_FONT_DEPTH: usize = 8;
/// (relative path, absolute path, bytes, mtime) of one font file.
type Listed = (String, PathBuf, u64, Option<SystemTime>);

const FONT_EXTS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];

/// One font file in the configured directory.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// Scanned font directory.
#[derive(Debug)]
pub struct FontSet {
    dir: PathBuf,
    pub files: Vec<FontFile>,
    /// SHA-256 over the sorted `path \t bytes \t sha256` lines.
    pub hash: String,
    fingerprint: Vec<(String, u64, Option<SystemTime>)>,
    db: OnceLock<Arc<fontdb::Database>>,
}

fn collect(root: &Path, dir: &Path, depth: usize, out: &mut Vec<Listed>) -> std::io::Result<()> {
    if depth > MAX_FONT_DEPTH {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(dir)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let md = fs::symlink_metadata(&p)?;
        if md.is_dir() {
            collect(root, &p, depth + 1, out)?;
        } else if md.is_file() {
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if FONT_EXTS.contains(&ext.as_str()) {
                if out.len() >= MAX_FONT_FILES {
                    return Err(std::io::Error::other("too many font files"));
                }
                let rel = p.strip_prefix(root).unwrap_or(&p);
                out.push((
                    rel.to_string_lossy().into_owned(),
                    p,
                    md.len(),
                    md.modified().ok(),
                ));
            }
        }
        // Symlinks are skipped: they would point outside the audited directory.
    }
    Ok(())
}

fn scan_fingerprint(dir: &Path) -> std::io::Result<Vec<Listed>> {
    let mut v = Vec::new();
    collect(dir, dir, 0, &mut v)?;
    v.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(v)
}

impl FontSet {
    /// Scan and hash `dir`.
    pub fn scan(dir: &Path) -> std::io::Result<FontSet> {
        let listed = scan_fingerprint(dir)?;
        let mut files = Vec::with_capacity(listed.len());
        let mut lines = String::new();
        for (rel, path, bytes, _) in &listed {
            let data = fs::read(path)?;
            let sha = sha256_hex(&data);
            lines.push_str(&format!("{rel}\t{bytes}\t{sha}\n"));
            files.push(FontFile {
                path: rel.clone(),
                bytes: *bytes,
                sha256: sha,
            });
        }
        Ok(FontSet {
            dir: dir.to_path_buf(),
            files,
            hash: sha256_hex(lines.as_bytes()),
            fingerprint: listed.into_iter().map(|(r, _, b, m)| (r, b, m)).collect(),
            db: OnceLock::new(),
        })
    }

    /// True if the directory listing (names, sizes, mtimes) is unchanged.
    pub fn is_current(&self) -> bool {
        match scan_fingerprint(&self.dir) {
            Ok(now) => {
                now.len() == self.fingerprint.len()
                    && now
                        .iter()
                        .zip(&self.fingerprint)
                        .all(|(a, b)| a.0 == b.0 && a.2 == b.1 && a.3 == b.2)
            }
            Err(_) => false,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Font database containing exactly the scanned files (loaded lazily).
    pub fn database(&self) -> Arc<fontdb::Database> {
        self.db
            .get_or_init(|| {
                let mut db = fontdb::Database::new();
                for f in &self.files {
                    let _ = db.load_font_file(self.dir.join(&f.path));
                }
                // Generic families resolve to a family of this directory (the
                // alphabetically first), never to a system font. Without this
                // a document that names only absent fonts rasterises blank.
                let first = db
                    .faces()
                    .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
                    .min();
                if let Some(fam) = first {
                    db.set_serif_family(fam.clone());
                    db.set_sans_serif_family(fam.clone());
                    db.set_monospace_family(fam.clone());
                    db.set_cursive_family(fam.clone());
                    db.set_fantasy_family(fam);
                }
                Arc::new(db)
            })
            .clone()
    }

    /// Lower-cased family names available in the set.
    pub fn families(&self) -> BTreeSet<String> {
        self.database()
            .faces()
            .flat_map(|f| f.families.iter().map(|(n, _)| n.to_lowercase()))
            .collect()
    }
}

/// Font family names referenced by `font-family` attributes/declarations in an
/// SVG, minus generic families. Order of first appearance is not kept.
pub fn referenced_families(svg: &str) -> BTreeSet<String> {
    const GENERIC: [&str; 6] = [
        "serif",
        "sans-serif",
        "monospace",
        "cursive",
        "fantasy",
        "inherit",
    ];
    let mut out = BTreeSet::new();
    let mut rest = svg;
    while let Some(i) = rest.find("font-family") {
        rest = &rest[i + "font-family".len()..];
        let r = rest.trim_start();
        let Some(r) = r.strip_prefix('=').or_else(|| r.strip_prefix(':')) else {
            continue;
        };
        let r = r.trim_start();
        let (val, adv) = match r.chars().next() {
            Some(q @ ('"' | '\'')) => {
                // Attribute value; quotes inside use the other quote kind.
                match r[1..].find(q) {
                    Some(end) => (&r[1..1 + end], 1 + end + 1),
                    None => continue,
                }
            }
            _ => match r.find([';', '"', '}']) {
                Some(end) => (&r[..end], end),
                None => continue,
            },
        };
        let val = val.replace("&apos;", "'").replace("&quot;", "\"");
        for fam in val.split(',') {
            let f = fam.trim().trim_matches(|c| c == '\'' || c == '"').trim();
            if f.is_empty() || GENERIC.contains(&f.to_lowercase().as_str()) {
                continue;
            }
            out.insert(f.to_string());
        }
        rest = &r[adv.min(r.len())..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referenced_families_unescapes_entities() {
        let s = r#"<text font-family="&apos;HY헤드라인M&apos;,&apos;Noto Sans KR&apos;,sans-serif">a</text>"#;
        let f = referenced_families(s);
        assert!(f.contains("HY헤드라인M"), "{f:?}");
        assert!(f.contains("Noto Sans KR"));
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn referenced_families_from_attribute_and_style() {
        let s = r#"<text font-family="한컴바탕, 'Batang', serif">a</text>
            <g style="font-family:Malgun Gothic;fill:red"><text font-family='Arial'>b</text></g>"#;
        let f = referenced_families(s);
        assert!(f.contains("한컴바탕"), "{f:?}");
        assert!(f.contains("Batang"));
        assert!(f.contains("Malgun Gothic"));
        assert!(f.contains("Arial"));
        assert!(!f.contains("serif"));
    }

    #[test]
    fn scan_hashes_ttf_files_and_detects_changes() {
        let tmp = crate::sandbox::TempDir::new_in(&std::env::temp_dir(), "hae-font-test-").unwrap();
        fs::write(tmp.path().join("a.ttf"), b"not a real font").unwrap();
        fs::write(tmp.path().join("notes.txt"), b"ignored").unwrap();
        let fs1 = FontSet::scan(tmp.path()).unwrap();
        assert_eq!(fs1.files.len(), 1);
        assert_eq!(fs1.files[0].path, "a.ttf");
        assert_eq!(fs1.hash.len(), 64);
        assert!(fs1.is_current());
        fs::write(tmp.path().join("b.otf"), b"other").unwrap();
        assert!(!fs1.is_current());
        let fs2 = FontSet::scan(tmp.path()).unwrap();
        assert_ne!(fs1.hash, fs2.hash);
    }
}
