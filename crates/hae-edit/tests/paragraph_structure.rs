//! Editor-side tests for paragraph.insert / paragraph.delete (0.4).
//! The independent byte gate lives in `hae-verify/tests/paragraph_structure.rs`.
use hae_diag::{Code, Limits};
use hae_edit::{apply, Applied, EditOp};
use hae_model::{Document, Kind};
use std::collections::BTreeSet;

const PARAGRAPHS: &[u8] = include_bytes!("../../../tests/fixtures/paragraph/paragraphs.hwpx");
const NARROW: &[u8] = include_bytes!("../../../tests/fixtures/editing/narrow_edits.hwpx");

fn open(bytes: &[u8]) -> Document {
    Document::open(bytes.to_vec(), &Limits::default()).unwrap()
}

fn xml(d: &Document) -> String {
    String::from_utf8(d.payload("Contents/section0.xml").unwrap().to_vec()).unwrap()
}

fn by_text(d: &Document, needle: &str) -> String {
    d.nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph)
        .find(|n| d.para_text(n).unwrap().text == needle)
        .unwrap_or_else(|| panic!("no paragraph with text {needle:?}"))
        .id
        .clone()
}

/// Top-level paragraphs (`hp:p` directly under the section) in document order.
fn top_level(d: &Document) -> Vec<String> {
    let section = d.nodes.iter().find(|n| n.kind == Kind::Section).unwrap();
    d.nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph && n.parent.as_deref() == Some(section.id.as_str()))
        .map(|n| n.id.clone())
        .collect()
}

fn top(d: &Document, i: usize) -> String {
    top_level(d)[i].clone()
}

fn insert(target: &str, position: &str, text: Option<&str>, count: Option<usize>) -> EditOp {
    EditOp::ParagraphInsert {
        target: target.into(),
        position: position.into(),
        text: text.map(str::to_string),
        count,
    }
}

fn delete(target: &str, count: Option<usize>) -> EditOp {
    EditOp::ParagraphDelete {
        target: target.into(),
        count,
    }
}

fn reject(make: impl Fn(&Document) -> EditOp, code: Code, reason: &str) {
    let d = open(PARAGRAPHS);
    let op = make(&d);
    let e = apply(&d, &[op]).unwrap_err();
    assert_eq!(e.code(), code, "{e}");
    assert_eq!(e.reason(), Some(reason), "{e}");
}

#[test]
fn insert_after_clones_paragraph_and_run_shape_without_id() {
    let d = open(PARAGRAPHS);
    let target = by_text(&d, "둘째 문단");
    let before = d.nodes.len();
    let a = apply(&d, &[insert(&target, "after", Some("새 문단"), None)]).unwrap();
    assert_eq!(a.paragraph.len(), 1);
    let report = &a.paragraph[0];
    assert_eq!(report.command, "paragraph.insert");
    assert_eq!(report.position, Some("after"));
    assert_eq!(report.count, 1);
    assert_eq!(report.created.len(), 1);
    assert!(report.removed.is_empty());
    let new = &report.created[0];
    assert_eq!(new.kind, "paragraph");
    assert!(a.doc.node(&new.node_id).is_some());
    assert_eq!(a.doc.nodes.len(), before + 1);
    // The source paragraph uses paraPrIDRef=1/charPrIDRef=1: both are cloned,
    // the paragraph id is dropped, and no layout cache is created.
    let x = xml(&a.doc);
    assert!(
        x.contains(
            r#"<hp:p paraPrIDRef="1" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0"><hp:run charPrIDRef="1"><hp:t>새 문단</hp:t></hp:run></hp:p>"#
        ),
        "{x}"
    );
    // The new paragraph sits after the target, before the third paragraph.
    let second = x.find("둘째 문단").unwrap();
    let fresh = x.find("새 문단").unwrap();
    let third = x.find("셋째 문단").unwrap();
    assert!(second < fresh && fresh < third);
    // Inserted text has no preview line to edit: PrvText stays byte-identical
    // and is reported stale.
    assert!(a.stale.prv_text);
    assert_eq!(
        d.payload("Preview/PrvText.txt").unwrap()[..],
        a.doc.payload("Preview/PrvText.txt").unwrap()[..]
    );
}

#[test]
fn insert_before_with_count_two_makes_empty_paragraphs() {
    let d = open(PARAGRAPHS);
    let target = by_text(&d, "셋째 문단");
    let a = apply(&d, &[insert(&target, "before", None, Some(2))]).unwrap();
    assert_eq!(a.paragraph[0].count, 2);
    assert_eq!(a.paragraph[0].created.len(), 2);
    let x = xml(&a.doc);
    // The template run is open (`<hp:run …><hp:t>…`), so the empty clone keeps
    // the open form with no content instead of forcing a self-closing tag.
    let marker = r#"<hp:p paraPrIDRef="0" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0"><hp:run charPrIDRef="0"></hp:run></hp:p>"#;
    assert_eq!(x.matches(marker).count(), 2, "{x}");
    assert!(!x.contains("<hp:t></hp:t>"));
    // Both new paragraphs precede the target.
    let target_at = x.find("셋째 문단").unwrap();
    let last_new = x[..target_at].rfind(marker).unwrap();
    assert!(last_new < target_at);
    // An empty insertion adds no preview line: PrvText is not stale.
    assert!(!a.stale.prv_text);
    // Created ids are distinct and are real paragraph nodes.
    let ids: BTreeSet<&str> = a.paragraph[0]
        .created
        .iter()
        .map(|c| c.node_id.as_str())
        .collect();
    assert_eq!(ids.len(), 2);
    for id in ids {
        assert_eq!(a.doc.node(id).unwrap().kind, Kind::Paragraph);
    }
}

#[test]
fn insert_rejects_before_the_section_property_paragraph_but_allows_after() {
    reject(
        |d| insert(&top(d, 0), "before", Some("앞"), None),
        Code::Unsupported,
        "paragraph_before_section_property",
    );
    let d = open(PARAGRAPHS);
    let target = by_text(&d, "첫 문단");
    let a = apply(&d, &[insert(&target, "after", Some("뒤"), None)]).unwrap();
    assert_eq!(a.paragraph[0].created.len(), 1);
}

#[test]
fn insert_arguments_are_validated() {
    let d = open(PARAGRAPHS);
    let target = by_text(&d, "셋째 문단");
    let e = apply(&d, &[insert(&target, "after", None, Some(0))]).unwrap_err();
    assert_eq!(e.code(), Code::InvalidArgument);
    assert_eq!(e.reason(), Some("count_out_of_range"));
    let e = apply(&d, &[insert(&target, "after", None, Some(1025))]).unwrap_err();
    assert_eq!(e.reason(), Some("count_out_of_range"));
    let e = apply(&d, &[insert(&target, "middle", None, None)]).unwrap_err();
    assert_eq!(e.code(), Code::InvalidRequest);
    let e = apply(&d, &[insert(&target, "after", Some("a\tb"), None)]).unwrap_err();
    assert_eq!(e.code(), Code::InvalidRequest);
    let e = apply(&d, &[insert(&target, "after", Some("a\u{0}b"), None)]).unwrap_err();
    assert_eq!(e.code(), Code::InvalidRequest);
}

#[test]
fn insert_into_a_cell_parents_the_new_paragraph_under_the_cell() {
    let d = open(PARAGRAPHS);
    let target = by_text(&d, "셀 문단 1");
    let cell = d.node(&target).unwrap().parent.clone().unwrap();
    let a = apply(&d, &[insert(&target, "before", Some("셀 머리"), None)]).unwrap();
    let new = &a.paragraph[0].created[0].node_id;
    assert_eq!(
        a.doc.node(new).unwrap().parent.as_deref(),
        Some(cell.as_str())
    );
    let x = xml(&a.doc);
    let cell_start = x.find(r#"<hp:cellAddr rowAddr="0" colAddr="1"/>"#).unwrap();
    let fresh = x.find("셀 머리").unwrap();
    let first_cell_para = x.find("셀 문단 1").unwrap();
    assert!(cell_start < fresh && fresh < first_cell_para, "{x}");
    assert!(x[cell_start..fresh].contains("<hp:subList>"));
}

#[test]
fn insert_refuses_footnote_like_containers() {
    // The narrow-edits fixture has paragraphs inside hp:subList of a header
    // control (not a table cell): the same subList shape but the wrong owner.
    let d = open(NARROW);
    let target = by_text(&d, "Header A");
    let e = apply(&d, &[insert(&target, "after", Some("x"), None)]).unwrap_err();
    assert_eq!(e.code(), Code::Unsupported);
    assert_eq!(e.reason(), Some("paragraph_scope_unsupported"));
}

#[test]
fn delete_removes_count_paragraphs_and_reports_removed_ids() {
    let d = open(PARAGRAPHS);
    let second = by_text(&d, "둘째 문단");
    let third = by_text(&d, "셋째 문단");
    let next = top(&d, 3);
    let a = apply(&d, &[delete(&second, Some(2))]).unwrap();
    let report = &a.paragraph[0];
    assert_eq!(report.command, "paragraph.delete");
    assert_eq!(report.position, None);
    assert_eq!(report.count, 2);
    assert_eq!(
        report.removed.iter().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from([second.clone(), third.clone()])
    );
    assert!(a.doc.node(&second).is_none());
    assert!(a.doc.node(&third).is_none());
    let x = xml(&a.doc);
    assert!(!x.contains("둘째 문단") && !x.contains("셋째 문단"));
    assert!(x.contains("첫 문단"));
    // The paragraph that took the deleted place is reported as changed.
    assert!(a.changed_ids.contains(&next));
    assert!(a.stale.prv_text);
}

#[test]
fn delete_refuses_a_paragraph_that_holds_a_table() {
    reject(
        |d| delete(&top(d, 3), None),
        Code::Unsupported,
        "paragraph_holds_table",
    );
}

#[test]
fn delete_refuses_a_paragraph_that_holds_an_object() {
    let d = open(PARAGRAPHS);
    let target = top(&d, 4);
    let e = apply(&d, &[delete(&target, None)]).unwrap_err();
    assert_eq!(e.code(), Code::Unsupported);
    assert_eq!(e.reason(), Some("paragraph_content_unsupported"));
    let element = e.diagnostic().detail.as_ref().unwrap()["element"]
        .as_str()
        .unwrap();
    assert!(element.starts_with("hp:"), "{element}");
}

#[test]
fn delete_refuses_the_section_property_paragraph() {
    reject(
        |d| delete(&top(d, 0), None),
        Code::Unsupported,
        "paragraph_content_unsupported",
    );
}

#[test]
fn delete_refuses_the_last_paragraph_of_a_container() {
    reject(
        |d| delete(&by_text(d, "한 문단 셀"), None),
        Code::Unsupported,
        "paragraph_would_empty_container",
    );
    reject(
        |d| delete(&by_text(d, "셀 문단 1"), Some(2)),
        Code::Unsupported,
        "paragraph_would_empty_container",
    );
    reject(
        |d| delete(&by_text(d, "셀 문단 2"), Some(2)),
        Code::InvalidArgument,
        "paragraph_count_out_of_range",
    );
}

#[test]
fn delete_count_beyond_the_container_is_refused() {
    reject(
        |d| delete(&by_text(d, "셋째 문단"), Some(9)),
        Code::InvalidArgument,
        "paragraph_count_out_of_range",
    );
    let d = open(PARAGRAPHS);
    let e = apply(&d, &[delete(&by_text(&d, "셋째 문단"), Some(0))]).unwrap_err();
    assert_eq!(e.reason(), Some("count_out_of_range"));
}

#[test]
fn applied_reports_are_serialized_for_clients() {
    let d = open(PARAGRAPHS);
    let target = by_text(&d, "셋째 문단");
    let Applied { paragraph, .. } =
        apply(&d, &[insert(&target, "after", Some("가"), None)]).unwrap();
    let v = serde_json::to_value(&paragraph).unwrap();
    assert_eq!(v[0]["command"], "paragraph.insert");
    assert_eq!(v[0]["position"], "after");
    assert_eq!(v[0]["created"][0]["kind"], "paragraph");
    assert_eq!(v[0]["created"][0]["row"], serde_json::Value::Null);
}
