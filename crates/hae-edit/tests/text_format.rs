use hae_diag::{Code, Limits};
use hae_edit::{apply, Alignment, EditOp};
use hae_model::{Document, Kind};
use hae_package::PackageState;
use hae_zip::Archive;
use std::sync::Arc;

fn fixture(body: &str) -> Document {
    let limits = Limits::default();
    let seed = include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec();
    let archive = Archive::open(seed, &limits).unwrap();
    let mut state = PackageState::from_archive(&archive);
    let section = format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:x="urn:test:opaque">{body}</hs:sec>"#
    );
    state.set("Contents/section0.xml", Arc::from(section.into_bytes()));
    state.set("Contents/header.xml", Arc::from(br#"<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" xmlns:x="urn:opaque"><hh:refList><hh:paraProperties itemCnt="1"><hh:paraPr id="0" x:keep="preserve"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160"/><x:untouched>opaque</x:untouched></hh:paraPr></hh:paraProperties></hh:refList></hh:head>"#.to_vec()));
    let plan = hae_package::zip_plan(&archive, &state).unwrap();
    Document::open(hae_zip::write(&archive, &plan).unwrap(), &limits).unwrap()
}
fn p(inner: &str) -> String {
    format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{inner}</hp:t></hp:run><hp:linesegarray/></hp:p>"#
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
fn xml(d: &Document) -> String {
    String::from_utf8(d.payload("Contents/section0.xml").unwrap().to_vec()).unwrap()
}
fn text(d: &Document, id: &str) -> String {
    d.para_text(d.node(id).unwrap()).unwrap().text
}
#[test]
fn split_run_range_keeps_unselected_bytes_and_first_shape() {
    let d = fixture(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>abc</hp:t></hp:run><hp:run charPrIDRef="1"><hp:t>def</hp:t></hp:run></hp:p>"#,
    );
    let id = para(&d, 0);
    let a = apply(
        &d,
        &[EditOp::TextReplace {
            target: id.clone(),
            range: 1..5,
            text: "X".into(),
        }],
    )
    .unwrap();
    assert_eq!(text(&a.doc, &id), "aXf");
    assert!(xml(&a.doc).contains(r#"<hp:run charPrIDRef="0"><hp:t>aX</hp:t></hp:run><hp:run charPrIDRef="1"><hp:t>f</hp:t></hp:run>"#));
    assert!(a
        .diagnostics
        .iter()
        .any(|x| x.code == Code::FormatCollapsed));
    assert_eq!(d.map_rows(), a.doc.map_rows());
}
#[test]
fn cdata_text_can_be_partially_replaced_without_losing_neighbors() {
    let d = fixture(&p("pre<![CDATA[a<b&c]]>post"));
    let id = para(&d, 0);
    let a = apply(
        &d,
        &[EditOp::TextReplace {
            target: id.clone(),
            range: 4..7,
            text: "]]> &".into(),
        }],
    )
    .unwrap();
    assert_eq!(text(&a.doc, &id), "prea]]> &cpost");
    assert!(xml(&a.doc).contains("pre"));
    Document::open(a.doc.to_hwpx().unwrap(), &Limits::default()).unwrap();
}
#[test]
fn cache_policy_clears_only_edited_tail_and_preserves_node_ids() {
    let d = fixture(&(p("first") + &p("second") + &p("third")));
    let id = para(&d, 1);
    let a = apply(
        &d,
        &[EditOp::TextReplace {
            target: id,
            range: 0..6,
            text: "changed".into(),
        }],
    )
    .unwrap();
    assert_eq!(xml(&a.doc).matches("<hp:linesegarray/>").count(), 1);
    assert!(xml(&a.doc).contains("first</hp:t></hp:run><hp:linesegarray/>"));
    assert_eq!(d.map_rows(), a.doc.map_rows());
    assert_eq!(a.cache_removals["Contents/section0.xml"], 2);
}
#[test]
fn invalid_second_batch_step_preserves_original_document() {
    let d = fixture(&p("a<hp:tab/>b"));
    let id = para(&d, 0);
    let before = d.to_hwpx().unwrap();
    let e = apply(
        &d,
        &[
            EditOp::TextReplace {
                target: id.clone(),
                range: 0..1,
                text: "ok".into(),
            },
            EditOp::TextReplace {
                target: id,
                range: 0..4,
                text: "bad".into(),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(e.code(), Code::RangeCrossesControl);
    assert_eq!(d.to_hwpx().unwrap(), before);
}
#[test]
fn empty_cell_with_unknown_text_attribute_is_rejected() {
    let d = fixture(
        r#"<hp:p><hp:run><hp:tbl><hp:tr><hp:tc><hp:cellAddr rowAddr="0" colAddr="0"/><hp:cellSpan rowSpan="1" colSpan="1"/><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t x:unknown="keep"/></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>"#,
    );
    let cell = d
        .nodes
        .iter()
        .find(|n| n.kind == Kind::Cell)
        .unwrap()
        .id
        .clone();
    let e = apply(
        &d,
        &[EditOp::CellSetText {
            target: cell,
            text: "new".into(),
        }],
    )
    .unwrap_err();
    assert_eq!(e.code(), Code::UnsupportedContentInRange);
}
#[test]
fn format_clone_preserves_unknown_header_data_and_other_paragraph() {
    let d = fixture(&(p("first") + &p("second")));
    let id = para(&d, 1);
    let a = apply(
        &d,
        &[EditOp::ParagraphFormat {
            target: id,
            alignment: Some(Alignment::Center),
            line_spacing: None,
        }],
    )
    .unwrap();
    let h = String::from_utf8(a.doc.payload("Contents/header.xml").unwrap().to_vec()).unwrap();
    assert_eq!(h.matches("x:keep=\"preserve\"").count(), 2);
    assert_eq!(h.matches("<x:untouched>opaque</x:untouched>").count(), 2);
    assert!(h.contains("horizontal=\"CENTER\""));
    assert!(xml(&a.doc).contains("<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>first"));
}
