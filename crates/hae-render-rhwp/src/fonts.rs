//! Explicit font directory: file list, content hashes and a font database for
//! rasterisation. Fonts are never taken from the system; only the configured
//! directory counts (spec §10 records the font list and its hash).

use hae_package::sha256_hex;
use resvg::usvg::fontdb;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

pub const MAX_FONT_FILES: usize = 4096;
const MAX_FONT_DEPTH: usize = 8;
pub const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_FONT_FILE_BYTES: u64 = 32 * 1024 * 1024;
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

/// One font payload staged for the raster worker. `file` is a generated
/// numeric filename; `path`, `bytes` and `sha256` are the values hashed into
/// [`FontSet::hash`], so the worker can verify the exact set it receives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SnapshotEntry {
    pub file: String,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// SHA-256 over the sorted `path \t bytes \t sha256` lines of a font set.
fn set_hash<'a>(files: impl IntoIterator<Item = (&'a str, u64, &'a str)>) -> String {
    let mut lines = String::new();
    for (rel, bytes, sha) in files {
        lines.push_str(&format!("{rel}\t{bytes}\t{sha}\n"));
    }
    sha256_hex(lines.as_bytes())
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
    // Bounded immutable bytes; database loading never rereads live files.
    payloads: Vec<Vec<u8>>,
}

fn collect(
    root: &Path,
    dir: &Path,
    depth: usize,
    visited: &mut usize,
    out: &mut Vec<Listed>,
) -> std::io::Result<()> {
    if depth > MAX_FONT_DEPTH {
        return Ok(());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        *visited += 1;
        if *visited > MAX_FONT_FILES * 2 {
            return Err(std::io::Error::other("too many font directory entries"));
        }
        entries.push(entry?);
    }
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let md = fs::symlink_metadata(&p)?;
        if md.is_dir() {
            collect(root, &p, depth + 1, visited, out)?;
        } else if md.is_file() {
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if FONT_EXTS.contains(&ext.as_str()) {
                let rel = p.strip_prefix(root).unwrap_or(&p);
                out.push((
                    rel.to_string_lossy().into_owned(),
                    p,
                    md.len(),
                    md.modified().ok(),
                ));
                if out.len() > MAX_FONT_FILES {
                    return Err(std::io::Error::other("too many font files"));
                }
            }
        }
        // Symlinks are skipped: they would point outside the audited directory.
    }
    Ok(())
}

fn scan_fingerprint(dir: &Path) -> std::io::Result<Vec<Listed>> {
    let mut v = Vec::new();
    collect(dir, dir, 0, &mut 0, &mut v)?;
    v.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(v)
}

/// Metadata-only bounded preview preflight. Counts/bytes are lower bounds when
/// `measurements_complete` is false (directory traversal stopped at a limit/error).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontScanReport {
    pub file_count: usize,
    pub total_bytes: u64,
    pub measurements_complete: bool,
    /// Filename heuristic only; does not establish cmap coverage or font validity.
    pub korean_filename_matches: usize,
    pub error: Option<String>,
}

fn validate_listing(listed: &[Listed], max_bytes: u64) -> std::io::Result<()> {
    let max_bytes = max_bytes.min(MAX_FONT_BYTES);
    listed.iter().try_fold(0u64, |n, (_, _, bytes, _)| {
        if *bytes > MAX_FONT_FILE_BYTES {
            return Err(std::io::Error::other("font exceeds per-file memory limit"));
        }
        n.checked_add(*bytes)
            .filter(|n| *n <= max_bytes)
            .ok_or_else(|| std::io::Error::other("font payload exceeds memory limit"))
    })?;
    // Include payload/database copies and metadata before any payload read.
    // The adapter supplies (remaining - 1 MiB) / 4 as max_bytes.
    let reservation: u64 = listed
        .iter()
        .map(|(rel, _, bytes, _)| bytes.saturating_mul(4) + 8192 + rel.len() as u64 * 8)
        .sum();
    if reservation > max_bytes.saturating_mul(4).saturating_add(1024 * 1024) {
        return Err(std::io::Error::other("font workspace exceeds memory limit"));
    }
    Ok(())
}

fn preflight(dir: &Path, max_bytes: u64) -> (Vec<Listed>, FontScanReport) {
    let mut listed = Vec::new();
    let traversal = collect(dir, dir, 0, &mut 0, &mut listed);
    listed.sort_by(|a, b| a.0.cmp(&b.0));
    let measurements_complete = traversal.is_ok();
    let error = traversal
        .and_then(|()| validate_listing(&listed, max_bytes))
        .err();
    let report = FontScanReport {
        file_count: listed.len(),
        total_bytes: listed.iter().fold(0u64, |n, f| n.saturating_add(f.2)),
        measurements_complete,
        korean_filename_matches: listed
            .iter()
            .filter(|(_, path, _, _)| {
                let name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .replace(['-', '_', ' '], "");
                [
                    "notosanskr",
                    "notoserifkr",
                    "notosanscjkkr",
                    "notoserifcjkkr",
                    "nanum",
                    "malgun",
                    "batang",
                    "gulim",
                    "dotum",
                    "나눔",
                    "맑은",
                    "바탕",
                    "굴림",
                    "돋움",
                ]
                .iter()
                .any(|hint| name.contains(hint))
            })
            .count(),
        error: error.map(|e| e.to_string()),
    };
    (listed, report)
}

/// Apply exactly the preview directory/extension/count/byte/workspace rules,
/// without reading font payloads or constructing a font database.
pub fn inspect_font_dir(dir: &Path) -> FontScanReport {
    preflight(dir, MAX_FONT_BYTES).1
}

impl FontSet {
    /// Scan and hash `dir`.
    pub fn scan(dir: &Path) -> std::io::Result<FontSet> {
        Self::scan_with_budget(dir, MAX_FONT_BYTES)
    }

    /// Bound the complete font payload before opening any font file.
    pub fn scan_with_budget(dir: &Path, max_bytes: u64) -> std::io::Result<FontSet> {
        let (listed, report) = preflight(dir, max_bytes);
        if let Some(error) = report.error {
            return Err(std::io::Error::other(error));
        }
        let mut payloads = Vec::with_capacity(listed.len());
        let mut files = Vec::with_capacity(listed.len());
        for (rel, path, bytes, _) in &listed {
            let mut data = Vec::new();
            fs::File::open(path)?
                .take(bytes + 1)
                .read_to_end(&mut data)?;
            if data.len() as u64 != *bytes {
                return Err(std::io::Error::other("font changed during bounded read"));
            }
            let sha = sha256_hex(&data);
            payloads.push(data);
            files.push(FontFile {
                path: rel.clone(),
                bytes: *bytes,
                sha256: sha,
            });
        }
        Ok(FontSet {
            dir: dir.to_path_buf(),
            hash: set_hash(
                files
                    .iter()
                    .map(|f| (f.path.as_str(), f.bytes, f.sha256.as_str())),
            ),
            files,
            fingerprint: listed.into_iter().map(|(r, _, b, m)| (r, b, m)).collect(),
            db: OnceLock::new(),
            payloads,
        })
    }

    /// Write the already hashed payloads to `dir` under generated numeric
    /// filenames, never reading the live font directory again. The set was
    /// bounded at scan time (file count, per-file and total bytes); files are
    /// created exclusively so an existing entry is an error, not overwritten.
    pub(crate) fn write_snapshot(&self, dir: &Path) -> std::io::Result<Vec<SnapshotEntry>> {
        let mut entries = Vec::with_capacity(self.files.len());
        for (i, (file, data)) in self.files.iter().zip(&self.payloads).enumerate() {
            if data.len() as u64 != file.bytes {
                return Err(std::io::Error::other("font payload length changed"));
            }
            let name = format!("{i:04}.font");
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o644)
                .open(dir.join(&name))?
                .write_all(data)?;
            entries.push(SnapshotEntry {
                file: name,
                path: file.path.clone(),
                bytes: file.bytes,
                sha256: file.sha256.clone(),
            });
        }
        Ok(entries)
    }

    /// Load exactly the staged `entries` from `dir` (worker side). Every file
    /// must be a regular file of the declared length and SHA-256, and the
    /// recomputed set hash must equal `expected_hash`; nothing else in `dir`
    /// and no system font is consulted. The declared total is bounded before
    /// any file is opened.
    pub(crate) fn from_snapshot(
        dir: &Path,
        entries: &[SnapshotEntry],
        expected_hash: &str,
        max_bytes: u64,
    ) -> std::io::Result<FontSet> {
        let bad = |m: &str| std::io::Error::other(m.to_string());
        if entries.len() > MAX_FONT_FILES {
            return Err(bad("too many snapshot fonts"));
        }
        let max_bytes = max_bytes.min(MAX_FONT_BYTES);
        entries.iter().try_fold(0u64, |n, e| {
            if e.bytes > MAX_FONT_FILE_BYTES {
                return Err(bad("snapshot font exceeds per-file limit"));
            }
            n.checked_add(e.bytes)
                .filter(|n| *n <= max_bytes)
                .ok_or_else(|| bad("snapshot fonts exceed the total limit"))
        })?;
        let mut files = Vec::with_capacity(entries.len());
        let mut payloads = Vec::with_capacity(entries.len());
        for e in entries {
            let numeric = e.file.strip_suffix(".font").is_some_and(|stem| {
                !stem.is_empty() && stem.len() <= 8 && stem.bytes().all(|b| b.is_ascii_digit())
            });
            if !numeric {
                return Err(bad("snapshot font filename is not generated"));
            }
            let path = dir.join(&e.file);
            if !fs::symlink_metadata(&path)?.is_file() {
                return Err(bad("snapshot font is not a regular file"));
            }
            let mut data = Vec::new();
            fs::File::open(&path)?
                .take(e.bytes + 1)
                .read_to_end(&mut data)?;
            if data.len() as u64 != e.bytes || sha256_hex(&data) != e.sha256 {
                return Err(bad("snapshot font differs from its declared hash"));
            }
            files.push(FontFile {
                path: e.path.clone(),
                bytes: e.bytes,
                sha256: e.sha256.clone(),
            });
            payloads.push(data);
        }
        let hash = set_hash(
            files
                .iter()
                .map(|f| (f.path.as_str(), f.bytes, f.sha256.as_str())),
        );
        if hash != expected_hash {
            return Err(bad("snapshot font set hash differs from the expected hash"));
        }
        Ok(FontSet {
            dir: dir.to_path_buf(),
            files,
            hash,
            // A snapshot is immutable; there is no live directory to track.
            fingerprint: Vec::new(),
            db: OnceLock::new(),
            payloads,
        })
    }

    /// Conservative reservation for retained payloads, a database copy and
    /// font parser/metadata workspace. This is accounting, not an RSS bound.
    pub fn memory_reservation(&self) -> u64 {
        self.files
            .iter()
            .map(|f| f.bytes.saturating_mul(4) + 8192 + f.path.len() as u64 * 8)
            .sum()
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
                for data in &self.payloads {
                    db.load_font_data(data.clone());
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

    /// True once the font database has been built. The product preview must
    /// never build it in the host process (the worker does), so tests observe it.
    #[doc(hidden)]
    pub fn database_loaded(&self) -> bool {
        self.db.get().is_some()
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
    fn font_payload_budget_counts_the_complete_set() {
        let tmp =
            crate::sandbox::TempDir::new_in(&std::env::temp_dir(), "hae-font-total-").unwrap();
        fs::write(tmp.path().join("a.ttf"), [0u8; 64]).unwrap();
        fs::write(tmp.path().join("b.ttf"), [0u8; 64]).unwrap();
        assert!(FontSet::scan_with_budget(tmp.path(), 100).is_err());
        assert_eq!(
            FontSet::scan_with_budget(tmp.path(), 128)
                .unwrap()
                .files
                .len(),
            2
        );
    }

    #[test]
    fn oversized_font_is_rejected_before_reading() {
        let tmp =
            crate::sandbox::TempDir::new_in(&std::env::temp_dir(), "hae-font-limit-").unwrap();
        let file = fs::File::create(tmp.path().join("huge.ttf")).unwrap();
        file.set_len(64 * 1024 * 1024 + 1).unwrap();
        assert!(FontSet::scan(tmp.path()).is_err());
    }

    #[test]
    fn snapshot_carries_the_hashed_payloads_not_the_live_files() {
        use crate::sandbox::TempDir;
        let live = TempDir::new_in(&std::env::temp_dir(), "hae-font-live-").unwrap();
        fs::write(live.path().join("a.ttf"), b"original").unwrap();
        fs::write(live.path().join("b.otf"), b"second").unwrap();
        let set = FontSet::scan(live.path()).unwrap();
        // The live directory changes after hashing; the snapshot must not.
        fs::write(live.path().join("a.ttf"), b"mutated!").unwrap();
        let snap = TempDir::new_in(&std::env::temp_dir(), "hae-font-snap-").unwrap();
        let entries = set.write_snapshot(snap.path()).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.file.as_str()).collect::<Vec<_>>(),
            ["0000.font", "0001.font"]
        );
        assert_eq!(
            fs::read(snap.path().join("0000.font")).unwrap(),
            b"original"
        );
        // Existing files are never overwritten.
        assert!(set.write_snapshot(snap.path()).is_err());

        let loaded = FontSet::from_snapshot(snap.path(), &entries, &set.hash, 1024).unwrap();
        assert_eq!(loaded.hash, set.hash);
        assert_eq!(loaded.files.len(), 2);
        assert!(!loaded.database_loaded());

        // Wrong expected hash, size budget and generated-name rules.
        assert!(FontSet::from_snapshot(snap.path(), &entries, &"0".repeat(64), 1024).is_err());
        assert!(FontSet::from_snapshot(snap.path(), &entries, &set.hash, 10).is_err());
        let mut renamed = entries.clone();
        renamed[0].file = "../a.ttf".into();
        assert!(FontSet::from_snapshot(snap.path(), &renamed, &set.hash, 1024).is_err());
        // Tampering, truncation and symlinks are detected.
        fs::write(snap.path().join("0000.font"), b"tampered").unwrap();
        assert!(FontSet::from_snapshot(snap.path(), &entries, &set.hash, 1024).is_err());
        fs::remove_file(snap.path().join("0000.font")).unwrap();
        std::os::unix::fs::symlink(live.path().join("b.otf"), snap.path().join("0000.font"))
            .unwrap();
        assert!(FontSet::from_snapshot(snap.path(), &entries, &set.hash, 1024).is_err());
    }

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
