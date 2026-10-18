//! hae-package: HWPX package relations: mimetype, content.hpf manifest and spine, META-INF.
//!
//! [`PackageState`] is the ordered list of entries with their current payloads.
//! [`analyze`] validates the package graph and parses every XML entry once
//! (spec §4, §11). [`zip_plan`] turns a state back into a raw-copy write plan.

use hae_diag::{EngineError, Limits, PackageRejectReason as R, Result};
use hae_xml::{ParseOptions, XmlDoc};
use hae_zip::{Archive, OutEntry};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

pub const MIMETYPE: &str = "application/hwp+zip";
pub const NS_HP: &str = "http://www.hancom.co.kr/hwpml/2011/paragraph";
pub const NS_HP10: &str = "http://www.hancom.co.kr/hwpml/2016/paragraph";
pub const NS_HS: &str = "http://www.hancom.co.kr/hwpml/2011/section";
pub const NS_HH: &str = "http://www.hancom.co.kr/hwpml/2011/head";
pub const NS_HC: &str = "http://www.hancom.co.kr/hwpml/2011/core";
pub const NS_OCF: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
pub const NS_ODF_MANIFEST: &str = "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0";

fn is_opf(ns: Option<&str>) -> bool {
    matches!(
        ns,
        Some("http://www.idpf.org/2007/opf/") | Some("http://www.idpf.org/2007/opf")
    )
}

fn broken(msg: impl Into<String>) -> EngineError {
    EngineError::rejected(R::BrokenManifest, msg)
}

/// Where an entry's current payload came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Present in the original archive (payload may have been replaced).
    Original,
    /// Added by an edit; written with the given ZIP method.
    Added { method: u16 },
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub payload: Arc<[u8]>,
    pub origin: Origin,
}

/// Ordered entries of a package with their current payloads.
#[derive(Debug, Clone)]
pub struct PackageState {
    pub entries: Vec<Entry>,
}

impl PackageState {
    pub fn from_archive(a: &Archive) -> PackageState {
        PackageState {
            entries: a
                .entries()
                .iter()
                .map(|e| Entry {
                    name: e.name.clone(),
                    payload: e.payload().clone(),
                    origin: Origin::Original,
                })
                .collect(),
        }
    }
    pub fn get(&self, name: &str) -> Option<&Arc<[u8]>> {
        self.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| &e.payload)
    }
    pub fn set(&mut self, name: &str, payload: Arc<[u8]>) -> bool {
        match self.entries.iter_mut().find(|e| e.name == name) {
            Some(e) => {
                e.payload = payload;
                true
            }
            None => false,
        }
    }
    pub fn add(&mut self, name: String, payload: Arc<[u8]>, method: u16) {
        self.entries.push(Entry {
            name,
            payload,
            origin: Origin::Added { method },
        });
    }
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.payload.len() as u64).sum()
    }
    /// spec §11: SHA-256 over canonical JSON of [(name, sha256(payload))] sorted by name.
    pub fn payload_set_sha256(&self) -> String {
        let mut v: Vec<(&str, String)> = self
            .entries
            .iter()
            .map(|e| (e.name.as_str(), sha256_hex(&e.payload)))
            .collect();
        v.sort();
        let mut json = String::from("[");
        for (i, (n, h)) in v.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&serde_json_string(n));
            json.push(',');
            json.push_str(&serde_json_string(h));
        }
        json.push(']');
        sha256_hex(json.as_bytes())
    }
}

/// Minimal JSON string encoder (keeps hae-package free of serde).
fn serde_json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn sha256_hex(b: &[u8]) -> String {
    let d = Sha256::digest(b);
    d.iter().map(|x| format!("{x:02x}")).collect()
}

#[derive(Debug, Clone)]
pub struct ManifestItem {
    pub id: String,
    pub href: String,
    /// Resolved ZIP entry name; empty for an explicitly external image.
    /// External hrefs are opaque metadata, never paths to read or fetch.
    pub entry: String,
    pub is_embedded: bool,
    pub media_type: String,
}

/// Validated package relations.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub rootfile: String,
    pub manifest: Vec<ManifestItem>,
    /// Section entry names in spine order (spine order is the section number).
    pub sections: Vec<String>,
    pub header: Option<String>,
    pub prv_text: Option<String>,
    pub prv_image: Option<String>,
    /// Signature entries found (documents with signatures are read-only).
    pub signature_entries: Vec<String>,
}

impl PackageInfo {
    pub fn item_by_id(&self, id: &str) -> Option<&ManifestItem> {
        self.manifest.iter().find(|m| m.id == id)
    }
}

/// Parsed XML entries by name.
pub type XmlMap = BTreeMap<String, Arc<XmlDoc>>;

fn is_xml_name(n: &str) -> bool {
    let l = n.to_ascii_lowercase();
    l.ends_with(".xml") || l.ends_with(".hpf") || l.ends_with(".rdf")
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..=i])
}

/// Validate the package graph and parse all XML entries.
pub fn analyze(
    state: &PackageState,
    mimetype_stored: bool,
    limits: &Limits,
) -> Result<(PackageInfo, XmlMap)> {
    analyze_cached(state, mimetype_stored, limits, None)
}

/// Like [`analyze`], reusing parsed XML from `prev` for entries whose payload
/// is the same allocation (unchanged since the previous analysis).
pub fn analyze_cached(
    state: &PackageState,
    mimetype_stored: bool,
    limits: &Limits,
    prev: Option<&XmlMap>,
) -> Result<(PackageInfo, XmlMap)> {
    let first = state
        .entries
        .first()
        .ok_or_else(|| EngineError::rejected(R::BadMimetype, "empty package"))?;
    if first.name != "mimetype" {
        return Err(EngineError::rejected(
            R::BadMimetype,
            "mimetype is not the first entry",
        ));
    }
    if !mimetype_stored {
        return Err(EngineError::rejected(
            R::BadMimetype,
            "mimetype entry is compressed",
        ));
    }
    if &first.payload[..] != MIMETYPE.as_bytes() {
        return Err(EngineError::rejected(
            R::BadMimetype,
            "mimetype value is not application/hwp+zip",
        ));
    }
    let names: HashSet<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
    for n in &names {
        let l = n.to_ascii_lowercase();
        if l.starts_with("meta-inf/") && l.contains("encryption") {
            return Err(EngineError::rejected(
                R::Encrypted,
                format!("encryption entry {n}"),
            ));
        }
    }

    let opts = ParseOptions {
        max_depth: limits.max_xml_depth as usize,
    };
    let mut xml: XmlMap = BTreeMap::new();
    for e in &state.entries {
        if is_xml_name(&e.name) {
            if let Some(d) = prev
                .and_then(|m| m.get(&e.name))
                .filter(|d| Arc::ptr_eq(d.bytes(), &e.payload))
            {
                xml.insert(e.name.clone(), d.clone());
                continue;
            }
            let d = XmlDoc::parse_with(e.payload.clone(), opts).map_err(|err| {
                let mut d = err.0;
                d.message = format!("{}: {}", e.name, d.message);
                EngineError(d)
            })?;
            xml.insert(e.name.clone(), Arc::new(d));
        }
    }

    // ODF manifest may declare encryption.
    if let Some(m) = xml.get("META-INF/manifest.xml") {
        if m.iter().any(|(_, el)| {
            el.local == "encryption-data" && el.ns.as_deref() == Some(NS_ODF_MANIFEST)
        }) {
            return Err(EngineError::rejected(
                R::Encrypted,
                "encrypted package (manifest encryption-data)",
            ));
        }
    }

    let container = xml
        .get("META-INF/container.xml")
        .ok_or_else(|| broken("META-INF/container.xml missing"))?;
    let rootfile = container
        .iter()
        .filter(|(_, el)| el.local == "rootfile" && el.ns.as_deref() == Some(NS_OCF))
        .filter_map(|(_, el)| el.attr_value("full-path"))
        .find(|p| p.to_ascii_lowercase().ends_with(".hpf"))
        .ok_or_else(|| broken("no .hpf rootfile in container.xml"))?
        .to_string();
    if rootfile.starts_with('/')
        || rootfile.contains('\\')
        || rootfile.split('/').any(|seg| seg == ".." || seg == ".")
    {
        return Err(EngineError::rejected(
            R::PathEscape,
            format!("rootfile path {rootfile}"),
        ));
    }
    let hpf = xml
        .get(&rootfile)
        .ok_or_else(|| broken(format!("rootfile {rootfile} missing")))?;
    let base = dir_of(&rootfile);

    let mut manifest = Vec::new();
    let mut ids = HashSet::new();
    for (_, el) in hpf.iter() {
        if el.local == "item" && is_opf(el.ns.as_deref()) {
            let id = el
                .attr_value("id")
                .ok_or_else(|| broken("manifest item without id"))?
                .to_string();
            let href = el
                .attr_value("href")
                .ok_or_else(|| broken(format!("manifest item {id} without href")))?
                .to_string();
            if !ids.insert(id.clone()) {
                return Err(broken(format!("duplicate manifest id {id}")));
            }
            let media_type = el.attr_value("media-type").unwrap_or("").to_string();
            // Only an image may be an explicit external reference. Other items
            // (for example OLE objects stored with `isEmbeded="0"` inside
            // BinData/) keep the original rule: a validated path to a real
            // package entry.
            let is_embedded = !(matches!(el.attr_value("isEmbeded"), Some("0" | "false"))
                && media_type.starts_with("image/"));
            if !is_embedded {
                manifest.push(ManifestItem {
                    id,
                    href,
                    entry: String::new(),
                    is_embedded,
                    media_type,
                });
                continue;
            }
            if href.starts_with('/')
                || href.contains('\\')
                || href.split('/').any(|seg| seg == ".." || seg == ".")
            {
                return Err(EngineError::rejected(
                    R::PathEscape,
                    format!("manifest href {href}"),
                ));
            }
            let rel = format!("{base}{href}");
            let entry = if names.contains(href.as_str()) {
                href.clone()
            } else if names.contains(rel.as_str()) {
                rel
            } else {
                return Err(broken(format!(
                    "manifest item {id} points to missing entry {href}"
                )));
            };
            manifest.push(ManifestItem {
                id,
                href,
                entry,
                is_embedded,
                media_type,
            });
        }
    }
    let by_id: HashMap<&str, &ManifestItem> = manifest.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut sections = Vec::new();
    let mut header = None;
    let mut spine_seen = false;
    for (_, el) in hpf.iter() {
        if el.local == "itemref" && is_opf(el.ns.as_deref()) {
            spine_seen = true;
            let idref = el
                .attr_value("idref")
                .ok_or_else(|| broken("itemref without idref"))?;
            let item = by_id
                .get(idref)
                .ok_or_else(|| broken(format!("spine refers to unknown id {idref}")))?;
            let doc = xml
                .get(&item.entry)
                .ok_or_else(|| broken(format!("spine item {} is not XML", item.entry)))?;
            let root = doc.el(doc.root());
            if root.local == "sec" && root.ns.as_deref() == Some(NS_HS) {
                if sections.contains(&item.entry) {
                    return Err(broken(format!(
                        "section {} listed twice in spine",
                        item.entry
                    )));
                }
                sections.push(item.entry.clone());
            } else if root.local == "head" && root.ns.as_deref() == Some(NS_HH) {
                header = Some(item.entry.clone());
            }
        }
    }
    if !spine_seen || sections.is_empty() {
        return Err(broken("spine lists no sections"));
    }
    if header.is_none() {
        header = manifest
            .iter()
            .find(|m| {
                xml.get(&m.entry).is_some_and(|d| {
                    let r = d.el(d.root());
                    r.local == "head" && r.ns.as_deref() == Some(NS_HH)
                })
            })
            .map(|m| m.entry.clone());
    }
    // A section-looking entry outside the spine makes the section order ambiguous.
    for (n, d) in &xml {
        let r = d.el(d.root());
        if r.local == "sec" && r.ns.as_deref() == Some(NS_HS) && !sections.contains(n) {
            return Err(broken(format!("section {n} is not in the spine")));
        }
    }
    // Binary references in sections must resolve.
    for s in &sections {
        let d = &xml[s];
        for (_, el) in d.iter() {
            for a in &el.attrs {
                if a.local == "binaryItemIDRef" && !by_id.contains_key(a.value.as_str()) {
                    return Err(broken(format!(
                        "{s}: binaryItemIDRef {} not in manifest",
                        a.value
                    )));
                }
            }
        }
    }
    let signature_entries = state
        .entries
        .iter()
        .map(|e| e.name.clone())
        .filter(|n| {
            n.to_ascii_lowercase().starts_with("meta-inf/")
                && n.to_ascii_lowercase().contains("signature")
        })
        .collect();
    let prv_text = names
        .contains("Preview/PrvText.txt")
        .then(|| "Preview/PrvText.txt".to_string());
    let prv_image = state
        .entries
        .iter()
        .find(|e| e.name.starts_with("Preview/PrvImage"))
        .map(|e| e.name.clone());
    Ok((
        PackageInfo {
            rootfile,
            manifest,
            sections,
            header,
            prv_text,
            prv_image,
            signature_entries,
        },
        xml,
    ))
}

/// Raw-copy write plan: entries whose payload equals the original are kept
/// byte-for-byte, changed ones replaced, new ones added in state order.
pub fn zip_plan(original: &Archive, state: &PackageState) -> Result<Vec<OutEntry>> {
    let mut plan = Vec::with_capacity(state.entries.len());
    let mut seen = HashSet::new();
    for e in &state.entries {
        if !seen.insert(e.name.as_str()) {
            return Err(EngineError::new(
                hae_diag::Code::StateInvariantError,
                hae_diag::Stage::Save,
                format!("entry {} twice in state", e.name),
            ));
        }
        match e.origin {
            Origin::Original => {
                let o = original.get(&e.name).ok_or_else(|| {
                    EngineError::new(
                        hae_diag::Code::StateInvariantError,
                        hae_diag::Stage::Save,
                        format!("original entry {} missing", e.name),
                    )
                })?;
                if Arc::ptr_eq(o.payload(), &e.payload) || o.payload()[..] == e.payload[..] {
                    plan.push(OutEntry::Keep(e.name.clone()));
                } else {
                    plan.push(OutEntry::Replace(e.name.clone(), e.payload.clone()));
                }
            }
            Origin::Added { method } => plan.push(OutEntry::Add {
                name: e.name.clone(),
                payload: e.payload.clone(),
                method,
            }),
        }
    }
    Ok(plan)
}

/// Open bytes as a package: ZIP validation, relation analysis, XML parsing.
pub fn open(
    bytes: Vec<u8>,
    limits: &Limits,
) -> Result<(Arc<Archive>, PackageState, PackageInfo, XmlMap)> {
    let archive = Archive::open(bytes, limits)?;
    let state = PackageState::from_archive(&archive);
    let stored = archive.entries().first().is_some_and(|e| e.is_stored());
    let (info, xml) = analyze(&state, stored, limits)?;
    Ok((Arc::new(archive), state, info, xml))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../tests/fixtures/corpus/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn hancom_and_relative_hrefs() {
        let (_, _, info, _) = open(corpus("bodojaryo-reference.hwpx"), &Limits::default()).unwrap();
        assert_eq!(info.sections, vec!["Contents/section0.xml"]);
        assert_eq!(info.header.as_deref(), Some("Contents/header.xml"));
        assert_eq!(info.prv_text.as_deref(), Some("Preview/PrvText.txt"));
        let (_, _, info, _) = open(corpus("irb-sample.hwpx"), &Limits::default()).unwrap();
        assert_eq!(info.sections, vec!["Contents/section0.xml"]);
        assert_eq!(info.header, None);
    }

    #[test]
    fn spine_order_for_many_sections() {
        let (_, _, info, _) = open(
            corpus("research-school-plan-reference.hwpx"),
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(info.sections.len(), 28);
    }

    fn entry(name: &str, payload: &[u8]) -> Entry {
        Entry {
            name: name.to_string(),
            payload: Arc::from(payload.to_vec()),
            origin: Origin::Original,
        }
    }

    fn harness_state(hpf: &str, extra: Vec<Entry>) -> PackageState {
        let container = r#"<?xml version="1.0" encoding="UTF-8"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.hpf"/></rootfiles></container>"#;
        let section = r#"<?xml version="1.0" encoding="UTF-8"?><hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section"><hs:p>hi</hs:p></hs:sec>"#;
        let mut entries = vec![
            entry("mimetype", MIMETYPE.as_bytes()),
            entry("META-INF/container.xml", container.as_bytes()),
            entry("content.hpf", hpf.as_bytes()),
            entry("Contents/section0.xml", section.as_bytes()),
        ];
        entries.extend(extra);
        PackageState { entries }
    }

    const HPF_BASE: &str = r#"<?xml version="1.0" encoding="UTF-8"?><opf:package xmlns:opf="http://www.idpf.org/2007/opf"><opf:manifest>ITEMS</opf:manifest><opf:spine><opf:itemref idref="sec0"/></opf:spine></opf:package>"#;

    #[test]
    fn explicit_external_images_keep_opaque_hrefs_without_payload_access() {
        for href in [
            "https://example.invalid/p.png",
            "/not/a/local/file.png",
            "../outside.png",
        ] {
            let hpf = HPF_BASE.replace("ITEMS", &format!(r#"<opf:item id="sec0" href="Contents/section0.xml" media-type="text/xml"/><opf:item id="external" href="{href}" media-type="image/png" isEmbeded="0"/>"#));
            let (info, _) =
                analyze(&harness_state(&hpf, vec![]), true, &Limits::default()).unwrap();
            let item = info.item_by_id("external").unwrap();
            assert!(!item.is_embedded);
            assert!(item.entry.is_empty());
            assert_eq!(item.href, href);
        }
    }

    #[test]
    fn non_image_item_with_embedded_flag_zero_is_still_a_package_entry() {
        let hpf = HPF_BASE.replace("ITEMS", r#"<opf:item id="sec0" href="Contents/section0.xml" media-type="text/xml"/><opf:item id="ole1" href="BinData/ole1.ole" media-type="application/ole" isEmbeded="0"/>"#);
        let extra = vec![entry("BinData/ole1.ole", b"ole")];
        let (info, _) = analyze(&harness_state(&hpf, extra), true, &Limits::default()).unwrap();
        let item = info.item_by_id("ole1").unwrap();
        assert!(item.is_embedded);
        assert_eq!(item.entry, "BinData/ole1.ole");
        // The same declaration with no matching entry is still rejected.
        assert!(analyze(&harness_state(&hpf, vec![]), true, &Limits::default()).is_err());
    }

    #[test]
    fn external_admission_does_not_allow_missing_embedded_payloads_or_external_spine() {
        for external in [
            r#"<opf:item id="bad" href="https://example.invalid/x" media-type="image/png" isEmbeded="1"/>"#,
            r#"<opf:item id="bad" href="missing.png" media-type="image/png"/>"#,
            r#"<opf:item id="bad" href="https://example.invalid/x" media-type="text/xml" isEmbeded="0"/>"#,
        ] {
            let hpf = HPF_BASE.replace("ITEMS", &format!(r#"<opf:item id="sec0" href="Contents/section0.xml" media-type="text/xml"/>{external}"#));
            assert!(analyze(&harness_state(&hpf, vec![]), true, &Limits::default()).is_err());
        }
        let hpf = HPF_BASE.replace("ITEMS",r#"<opf:item id="sec0" href="https://example.invalid/x" media-type="image/png" isEmbeded="false"/>"#);
        assert!(analyze(&harness_state(&hpf, vec![]), true, &Limits::default()).is_err());
    }

    #[test]
    fn c2_manifest_href_allows_double_dot_inside_segment() {
        let hpf = HPF_BASE.replace(
            "ITEMS",
            r#"<opf:item id="sec0" href="Contents/section0.xml" media-type="text/xml"/><opf:item id="blob" href="BinData/a..b.bin" media-type="application/octet-stream"/>"#,
        );
        let st = harness_state(&hpf, vec![entry("BinData/a..b.bin", b"payload-bytes")]);
        let (info, _) = analyze(&st, true, &Limits::default()).unwrap();
        assert!(info.item_by_id("blob").is_some());
        assert_eq!(info.item_by_id("blob").unwrap().entry, "BinData/a..b.bin");
    }

    #[test]
    fn c2_manifest_href_rejects_dotdot_segments() {
        for href in [
            "../outside.xml",
            "a/../b.xml",
            "a/../../b.xml",
            "./x.xml",
            "a/./b.xml",
        ] {
            let hpf = HPF_BASE.replace(
                "ITEMS",
                &format!(
                    r#"<opf:item id="sec0" href="Contents/section0.xml" media-type="text/xml"/><opf:item id="evil" href="{href}" media-type="text/xml"/>"#
                ),
            );
            let st = harness_state(&hpf, vec![]);
            let err = analyze(&st, true, &Limits::default()).unwrap_err();
            assert_eq!(
                err.reason(),
                Some("path_escape"),
                "href {href:?} should be path_escape, got {err}"
            );
        }
    }

    #[test]
    fn plan_keeps_unchanged() {
        let b = corpus("gyehoek-reference.hwpx");
        let (a, mut st, _, _) = open(b.clone(), &Limits::default()).unwrap();
        let plan = zip_plan(&a, &st).unwrap();
        assert!(plan.iter().all(|p| matches!(p, OutEntry::Keep(_))));
        assert_eq!(hae_zip::write(&a, &plan).unwrap(), b);
        let h0 = st.payload_set_sha256();
        st.set(
            "Contents/section0.xml",
            Arc::from(st.get("Contents/section0.xml").unwrap().to_vec()),
        );
        assert_eq!(st.payload_set_sha256(), h0, "same bytes, new allocation");
        assert!(zip_plan(&a, &st)
            .unwrap()
            .iter()
            .all(|p| matches!(p, OutEntry::Keep(_))));
    }
}
