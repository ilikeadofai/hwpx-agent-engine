//! paragraph.format boundary regressions (perfection-20261019/format-boundary).
//!
//! The reproduction is a syntactically valid document whose only `hh:paraPr`
//! has the maximum numeric id (`u64::MAX`) and a paragraph referencing it. A
//! format that needs a new definition used to compute `max_id + 1`, which
//! panicked in debug and wrapped in release (the save gate then rejected it as
//! "paragraph ID exhausted"). These tests pin the safe lazy-allocation
//! contract: reuse an equivalent definition (or noop) without allocating, and
//! only fail with a typed `UNSUPPORTED` when a genuinely new id is impossible.
//!
//! Fixtures are built from the shared adversarial seed so the headers stay
//! small and synthetic; no user document is copied.

use hae_diag::{Code, Limits};
use hae_edit::{apply, Alignment, EditOp};
use hae_model::{Document, Kind};
use hae_package::PackageState;
use hae_zip::Archive;
use std::sync::Arc;

const MAX_ID: &str = "18446744073709551615";
const MAX_ID_MINUS_1: &str = "18446744073709551614";

/// Build a synthetic document from an explicit `hh:paraProperties` container
/// and section body. Everything else (unknown attrs/elements) is caller
/// controlled so byte-preservation can be asserted.
fn doc_with(para_props: &str, section_body: &str) -> Document {
    let limits = Limits::default();
    let seed = include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec();
    let archive = Archive::open(seed, &limits).unwrap();
    let mut state = PackageState::from_archive(&archive);
    let section = format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:x="urn:test:opaque">{section_body}</hs:sec>"#
    );
    state.set("Contents/section0.xml", Arc::from(section.into_bytes()));
    let header = format!(
        r#"<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:x="urn:opaque"><hh:refList><hh:charProperties itemCnt="1"><hh:charPr id="0"/></hh:charProperties>{para_props}</hh:refList></hh:head>"#
    );
    state.set("Contents/header.xml", Arc::from(header.into_bytes()));
    let plan = hae_package::zip_plan(&archive, &state).unwrap();
    Document::open(hae_zip::write(&archive, &plan).unwrap(), &limits).unwrap()
}

/// One paraPr with an alignment and percent line spacing, plus a stable unknown
/// attribute and child element to detect byte loss on clone.
fn pp(id: &str, horizontal: &str) -> String {
    format!(
        r#"<hh:paraPr id="{id}" x:keep="preserve"><hh:align horizontal="{horizontal}" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/><x:untouched>opaque</x:untouched></hh:paraPr>"#
    )
}

fn p(text: &str, pref: &str) -> String {
    format!(
        r#"<hp:p paraPrIDRef="{pref}"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run><hp:linesegarray/></hp:p>"#
    )
}

fn para(d: &Document, i: usize) -> String {
    d.nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph)
        .nth(i)
        .unwrap()
        .id
        .clone()
}

fn header(d: &Document) -> String {
    String::from_utf8(d.payload("Contents/header.xml").unwrap().to_vec()).unwrap()
}

fn section(d: &Document) -> String {
    String::from_utf8(d.payload("Contents/section0.xml").unwrap().to_vec()).unwrap()
}

fn center(target: String) -> EditOp {
    EditOp::ParagraphFormat {
        target,
        alignment: Some(Alignment::Center),
        line_spacing: None,
    }
}

/// RED before the fix: `max_id + 1` panicked (debug) / wrapped (release).
/// The clone must instead fail gracefully and leave the document untouched.
#[test]
fn clone_needing_a_new_id_at_max_fails_unsupported_without_mutation() {
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="1">{}</hh:paraProperties>"#,
            pp(MAX_ID, "JUSTIFY")
        ),
        &p("target", MAX_ID),
    );
    let before = d.to_hwpx().unwrap();
    let e = apply(&d, &[center(para(&d, 0))]).unwrap_err();
    assert_eq!(e.code(), Code::Unsupported);
    assert_eq!(e.reason(), Some("paraPr_shape"));
    assert_eq!(d.to_hwpx().unwrap(), before);
}

/// A same-format request needs no new definition, so the max-ID boundary must
/// not be treated as exhaustion: it is a byte-identical noop with no resource
/// or history entry.
#[test]
fn same_format_noop_at_max_id_is_byte_identical_and_reports_nothing() {
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="1">{}</hh:paraProperties>"#,
            pp(MAX_ID, "CENTER")
        ),
        &p("target", MAX_ID),
    );
    let a = apply(&d, &[center(para(&d, 0))]).unwrap();
    assert!(a.changed_ids.is_empty());
    assert!(a.resource_changes.is_empty());
    assert!(a.intents.is_empty());
    assert_eq!(a.doc.to_hwpx().unwrap(), d.to_hwpx().unwrap());
    assert_eq!(header(&a.doc), header(&d));
}

/// Reusing an existing equivalent definition must beat allocation even when no
/// new id is available: the paragraph is re-pointed, the header is unchanged.
#[test]
fn reuse_at_max_id_avoids_exhaustion() {
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="2">{}{}</hh:paraProperties>"#,
            pp(MAX_ID_MINUS_1, "CENTER"),
            pp(MAX_ID, "JUSTIFY"),
        ),
        &p("target", MAX_ID),
    );
    let a = apply(&d, &[center(para(&d, 0))]).unwrap();
    assert_eq!(header(&a.doc), header(&d));
    assert!(section(&a.doc).contains(&format!(r#"<hp:p paraPrIDRef="{MAX_ID_MINUS_1}""#)));
    assert_eq!(
        a.resource_changes,
        [hae_edit::ResourceChange {
            kind: "paraPr",
            resource_id: MAX_ID_MINUS_1.into(),
            action: "reused",
        }]
    );
}

/// A requested definition that already exists under a different id must be
/// reused instead of appended; unrelated header bytes survive untouched.
#[test]
fn requests_reuse_equivalent_existing_definition_and_preserve_unrelated_bytes() {
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="2">{}{}</hh:paraProperties>"#,
            pp("3", "JUSTIFY"),
            pp("9", "CENTER"),
        ),
        &p("target", "3"),
    );
    let a = apply(&d, &[center(para(&d, 0))]).unwrap();
    // No definition was appended: itemCnt and bytes are unchanged.
    assert_eq!(header(&a.doc), header(&d));
    assert_eq!(header(&a.doc).matches("<hh:paraPr ").count(), 2);
    assert!(header(&a.doc).contains(r#"itemCnt="2""#));
    assert_eq!(header(&a.doc).matches("x:keep=\"preserve\"").count(), 2);
    assert_eq!(
        header(&a.doc)
            .matches("<x:untouched>opaque</x:untouched>")
            .count(),
        2
    );
    // Only the target paragraph moved to the equivalent definition.
    assert!(section(&a.doc).contains(r#"<hp:p paraPrIDRef="9""#));
    assert_eq!(
        a.resource_changes,
        [hae_edit::ResourceChange {
            kind: "paraPr",
            resource_id: "9".into(),
            action: "reused",
        }]
    );
}

/// Repeated formatting of two paragraphs in the same shape adds exactly one
/// definition: the second step reuses the first's clone and leaves the header
/// byte-identical, preserving unrelated bytes and stable ids.
#[test]
fn repeated_formatting_reuses_clone_and_preserves_unrelated_bytes() {
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="1">{}</hh:paraProperties>"#,
            pp("0", "JUSTIFY")
        ),
        &(p("first", "0") + &p("second", "0")),
    );
    let a = apply(&d, &[center(para(&d, 1))]).unwrap();
    let first_header = header(&a.doc);
    assert!(first_header.contains(r#"itemCnt="2""#));
    let b = apply(&a.doc, &[center(para(&a.doc, 0))]).unwrap();
    assert_eq!(header(&b.doc), first_header);
    assert_eq!(header(&b.doc).matches("<hh:paraPr ").count(), 2);
    assert_eq!(
        b.resource_changes,
        [hae_edit::ResourceChange {
            kind: "paraPr",
            resource_id: "1".into(),
            action: "reused",
        }]
    );
    assert!(
        section(&b.doc).contains(r#"<hp:p paraPrIDRef="1"><hp:run charPrIDRef="0"><hp:t>first"#)
    );
    assert!(
        section(&b.doc).contains(r#"<hp:p paraPrIDRef="1"><hp:run charPrIDRef="0"><hp:t>second"#)
    );
    assert_eq!(header(&b.doc).matches("x:keep=\"preserve\"").count(), 2);
}

/// Malformed or inconsistent `itemCnt` must fail closed before any clone is
/// spliced in; the original counts are never silently repaired.
#[test]
fn malformed_or_inconsistent_itemcnt_fails_closed_on_cloning() {
    for count in ["5", "abc", "18446744073709551616", "-1", ""] {
        let d = doc_with(
            &format!(
                r#"<hh:paraProperties itemCnt="{count}">{}</hh:paraProperties>"#,
                pp("0", "JUSTIFY")
            ),
            &p("target", "0"),
        );
        let before = d.to_hwpx().unwrap();
        let e = apply(&d, &[center(para(&d, 0))]).unwrap_err();
        assert_eq!(e.code(), Code::Unsupported, "itemCnt={count:?}");
        assert_eq!(e.reason(), Some("paraPr_shape"), "itemCnt={count:?}");
        assert_eq!(d.to_hwpx().unwrap(), before, "itemCnt={count:?}");
    }
}

/// A noop must not require a well-formed `itemCnt` to be repaired, but an
/// inconsistent container is only rejected when a clone is actually attempted.
#[test]
fn consistent_itemcnt_is_required_only_when_cloning() {
    // Noop at the current definition: no container count is consulted.
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="7">{}</hh:paraProperties>"#,
            pp("0", "CENTER")
        ),
        &p("target", "0"),
    );
    let a = apply(&d, &[center(para(&d, 0))]).unwrap();
    assert_eq!(a.doc.to_hwpx().unwrap(), d.to_hwpx().unwrap());
}

/// Non-numeric, duplicate and ambiguous definitions are rejected up front,
/// matching the save gate's policy instead of quietly allocating an id.
#[test]
fn invalid_para_pr_ids_and_ambiguous_container_are_rejected() {
    // Non-numeric id.
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="1">{}</hh:paraProperties>"#,
            pp("abc", "JUSTIFY")
        ),
        &p("target", "abc"),
    );
    let before = d.to_hwpx().unwrap();
    assert_eq!(
        apply(&d, &[center(para(&d, 0))]).unwrap_err().code(),
        Code::Unsupported
    );
    assert_eq!(d.to_hwpx().unwrap(), before);

    // Duplicate numeric ids.
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="2">{}{}</hh:paraProperties>"#,
            pp("0", "JUSTIFY"),
            pp("0", "CENTER"),
        ),
        &p("target", "0"),
    );
    let before = d.to_hwpx().unwrap();
    let e = apply(&d, &[center(para(&d, 0))]).unwrap_err();
    assert_eq!(e.code(), Code::Unsupported);
    assert_eq!(e.reason(), Some("paraPr_shape"));
    assert_eq!(d.to_hwpx().unwrap(), before);

    // Two paraProperties containers: which one owns the id is ambiguous.
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="1">{}</hh:paraProperties><hh:paraProperties itemCnt="0"></hh:paraProperties>"#,
            pp("0", "JUSTIFY")
        ),
        &p("target", "0"),
    );
    let before = d.to_hwpx().unwrap();
    assert_eq!(
        apply(&d, &[center(para(&d, 0))]).unwrap_err().code(),
        Code::Unsupported
    );
    assert_eq!(d.to_hwpx().unwrap(), before);
}

/// An unresolved original reference stays a graceful validation error.
#[test]
fn unresolved_paragraph_reference_is_rejected_without_mutation() {
    let d = doc_with(
        &format!(
            r#"<hh:paraProperties itemCnt="1">{}</hh:paraProperties>"#,
            pp("0", "JUSTIFY")
        ),
        &p("target", "7"),
    );
    let before = d.to_hwpx().unwrap();
    assert_eq!(
        apply(&d, &[center(para(&d, 0))]).unwrap_err().code(),
        Code::Unsupported
    );
    assert_eq!(d.to_hwpx().unwrap(), before);
}
