//! `document.create` verification: the whole expected archive is derived from
//! the request (no source package), so every mutation below is a byte-level
//! contract violation that must be refused before publication.
use hae_diag::{Code, Limits};
use hae_package::create::{create_hwpx, DocumentSpec};
use hae_package::{Origin, PackageState};
use hae_verify::{verify_created, CreateSpec};
use hae_zip::{Archive, METHOD_DEFLATED};
use std::sync::Arc;

const SECTION: &str = "Contents/section0.xml";

fn spec(title: Option<&str>, paragraphs: &[&str]) -> (DocumentSpec, CreateSpec) {
    (
        DocumentSpec {
            title: title.map(str::to_string),
            paragraphs: paragraphs.iter().map(|s| s.to_string()).collect(),
        },
        CreateSpec {
            title: title.map(str::to_string),
            paragraphs: paragraphs.iter().map(|s| s.to_string()).collect(),
        },
    )
}

fn build(title: Option<&str>, paragraphs: &[&str]) -> (Vec<u8>, CreateSpec) {
    let (builder, verifier) = spec(title, paragraphs);
    let bytes = create_hwpx(&builder, &Limits::default()).unwrap();
    (bytes, verifier)
}

fn rewrite(bytes: &[u8], f: impl FnOnce(&mut PackageState)) -> Vec<u8> {
    let limits = Limits::default();
    let archive = Archive::open(bytes.to_vec(), &limits).unwrap();
    let mut state = PackageState::from_archive(&archive);
    f(&mut state);
    let plan = hae_package::zip_plan(&archive, &state).unwrap();
    hae_zip::write(&archive, &plan).unwrap()
}

fn section(state: &mut PackageState, old: &str, new: &str) {
    let text = String::from_utf8(state.get(SECTION).unwrap().to_vec()).unwrap();
    assert_eq!(text.matches(old).count(), 1, "{old}");
    state.set(SECTION, Arc::from(text.replace(old, new).into_bytes()));
}

fn reject(bytes: &[u8], spec: &CreateSpec) {
    let e = verify_created(bytes, spec, &Limits::default()).unwrap_err();
    assert_eq!(e.code(), Code::VerifyFailed, "{e}");
}

#[test]
fn a_built_document_passes_the_creation_gate() {
    let (bytes, spec) = build(Some("새 문서 제목"), &["첫 문단", "", "셋째 문단"]);
    let report = verify_created(&bytes, &spec, &Limits::default()).unwrap();
    assert_eq!(report.changed_entries.len(), 6);
    assert!(!report.prv_text_stale && !report.thumbnail_stale);
    // The archive is a normal HWPX: mimetype first, STORED, and analyzable.
    let archive = Archive::open(bytes.clone(), &Limits::default()).unwrap();
    assert_eq!(archive.entries()[0].name, "mimetype");
    assert!(archive.entries()[0].is_stored());
    let state = PackageState::from_archive(&archive);
    let (info, _) = hae_package::analyze(&state, true, &Limits::default()).unwrap();
    assert_eq!(info.sections, vec![SECTION]);
    assert_eq!(info.header.as_deref(), Some("Contents/header.xml"));
}

#[test]
fn an_empty_spec_still_makes_one_empty_paragraph_with_section_properties() {
    let (bytes, spec) = build(None, &[]);
    verify_created(&bytes, &spec, &Limits::default()).unwrap();
    let archive = Archive::open(bytes.clone(), &Limits::default()).unwrap();
    let x = String::from_utf8(archive.payload(SECTION).unwrap().to_vec()).unwrap();
    assert_eq!(x.matches("<hp:p ").count(), 1, "{x}");
    assert!(x.contains("<hp:secPr "), "{x}");
    assert!(x.contains("<hp:t/>"), "{x}");
}

#[test]
fn text_is_xml_escaped_and_round_trips_through_the_gate() {
    let (bytes, spec) = build(Some("A & B <C>"), &["x < y & z", "\"quoted\""]);
    verify_created(&bytes, &spec, &Limits::default()).unwrap();
    let archive = Archive::open(bytes.clone(), &Limits::default()).unwrap();
    let section = String::from_utf8(archive.payload(SECTION).unwrap().to_vec()).unwrap();
    assert!(
        section.contains("<hp:t>x &lt; y &amp; z</hp:t>"),
        "{section}"
    );
    let hpf = String::from_utf8(archive.payload("Contents/content.hpf").unwrap().to_vec()).unwrap();
    assert!(
        hpf.contains("<opf:title>A &amp; B &lt;C&gt;</opf:title>"),
        "{hpf}"
    );
}

#[test]
fn a_compressed_mimetype_is_rejected() {
    let (bytes, spec) = build(None, &["본문"]);
    let mutated = rewrite(&bytes, |state| {
        state.entries[0].origin = Origin::Added {
            method: METHOD_DEFLATED,
        };
    });
    reject(&mutated, &spec);
}

#[test]
fn a_missing_entry_is_rejected() {
    let (bytes, spec) = build(None, &["본문"]);
    for name in [
        "version.xml",
        "Contents/header.xml",
        "META-INF/container.xml",
    ] {
        let mutated = rewrite(&bytes, |state| {
            state.entries.retain(|e| e.name != name);
        });
        reject(&mutated, &spec);
    }
}

#[test]
fn an_extra_entry_is_rejected() {
    let (bytes, spec) = build(None, &["본문"]);
    let mutated = rewrite(&bytes, |state| {
        state.add(
            "Contents/extra.xml".to_string(),
            Arc::from(b"<x/>".to_vec()),
            METHOD_DEFLATED,
        );
    });
    reject(&mutated, &spec);
}

#[test]
fn the_wrong_title_or_text_is_rejected() {
    let (bytes, spec) = build(Some("제목"), &["첫 문단", "둘째 문단"]);
    let mutated = rewrite(&bytes, |state| {
        let hpf = String::from_utf8(state.get("Contents/content.hpf").unwrap().to_vec()).unwrap();
        state.set(
            "Contents/content.hpf",
            Arc::from(hpf.replace("제목", "다른 제목").into_bytes()),
        );
    });
    reject(&mutated, &spec);
    let mutated = rewrite(&bytes, |state| section(state, "둘째 문단", "다른 문단"));
    reject(&mutated, &spec);
}

#[test]
fn a_wrong_version_producer_is_rejected() {
    let (bytes, spec) = build(None, &["본문"]);
    let mutated = rewrite(&bytes, |state| {
        let v = String::from_utf8(state.get("version.xml").unwrap().to_vec()).unwrap();
        state.set(
            "version.xml",
            Arc::from(v.replace(r#"major="5""#, r#"major="6""#).into_bytes()),
        );
    });
    reject(&mutated, &spec);
    let mutated = rewrite(&bytes, |state| {
        let v = String::from_utf8(state.get("version.xml").unwrap().to_vec()).unwrap();
        state.set(
            "version.xml",
            Arc::from(
                v.replace(
                    r#"tagetApplication="WORDPROCESSOR""#,
                    r#"tagetApplication="HWP""#,
                )
                .into_bytes(),
            ),
        );
    });
    reject(&mutated, &spec);
}

#[test]
fn a_wrong_container_media_type_is_rejected() {
    let (bytes, spec) = build(None, &["본문"]);
    let mutated = rewrite(&bytes, |state| {
        let c = String::from_utf8(state.get("META-INF/container.xml").unwrap().to_vec()).unwrap();
        state.set(
            "META-INF/container.xml",
            Arc::from(
                c.replace(
                    r#"media-type="application/hwpml-package+xml""#,
                    r#"media-type="application/zip""#,
                )
                .into_bytes(),
            ),
        );
    });
    reject(&mutated, &spec);
}

#[test]
fn a_missing_section_property_or_resource_is_rejected() {
    let (bytes, spec) = build(None, &["본문"]);
    let mutated = rewrite(&bytes, |state| {
        let x = String::from_utf8(state.get(SECTION).unwrap().to_vec()).unwrap();
        let start = x.find("<hp:secPr ").unwrap();
        let end = x.find("</hp:secPr>").unwrap() + "</hp:secPr>".len();
        let mut stripped = x.clone();
        stripped.replace_range(start..end, "");
        state.set(SECTION, Arc::from(stripped.into_bytes()));
    });
    reject(&mutated, &spec);
    // Unresolved paraPr reference: the header no longer defines id 0.
    let mutated = rewrite(&bytes, |state| {
        let h = String::from_utf8(state.get("Contents/header.xml").unwrap().to_vec()).unwrap();
        state.set(
            "Contents/header.xml",
            Arc::from(
                h.replace(r#"<hh:paraPr id="0""#, r#"<hh:paraPr id="9""#)
                    .into_bytes(),
            ),
        );
    });
    reject(&mutated, &spec);
}

#[test]
fn a_changed_entry_order_is_rejected() {
    let (bytes, spec) = build(None, &["본문"]);
    let limits = Limits::default();
    let archive = Archive::open(bytes.clone(), &limits).unwrap();
    let state = PackageState::from_archive(&archive);
    // Write section0 before header (order is part of the expected archive).
    let mut plan: Vec<hae_zip::OutEntry> = Vec::new();
    for name in [
        "mimetype",
        "version.xml",
        "META-INF/container.xml",
        "Contents/content.hpf",
        SECTION,
        "Contents/header.xml",
    ] {
        plan.push(hae_zip::OutEntry::Keep(name.to_string()));
    }
    let mutated = hae_zip::write(&archive, &plan).unwrap();
    assert_eq!(state.entries.len(), 6);
    reject(&mutated, &spec);
}

#[test]
fn an_illegal_spec_is_refused_before_any_file_exists() {
    let (builder, _) = spec(None, &["a\tb"]);
    let e = create_hwpx(&builder, &Limits::default()).unwrap_err();
    assert_eq!(e.code(), Code::InvalidRequest);
    let (builder, _) = spec(Some("x\ny"), &[]);
    let e = create_hwpx(&builder, &Limits::default()).unwrap_err();
    assert_eq!(e.code(), Code::InvalidRequest);
    let (builder, _) = spec(None, &["a\u{0}b"]);
    let e = create_hwpx(&builder, &Limits::default()).unwrap_err();
    assert_eq!(e.code(), Code::InvalidRequest);
}
