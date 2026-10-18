use hae_diag::{Code, Limits};
use hae_edit::{apply, CharProps, EditOp};
use hae_model::{Document, Kind};
use std::ops::Range;

fn fixture() -> Document {
    Document::open(
        include_bytes!("../../../tests/fixtures/editing/char_format.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap()
}
fn format(
    d: &Document,
    p: usize,
    range: Range<usize>,
    props: serde_json::Value,
    preserve: bool,
) -> hae_diag::Result<hae_edit::Applied> {
    let target = d
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph)
        .nth(p)
        .unwrap()
        .id
        .clone();
    apply(
        d,
        &[EditOp::TextFormat {
            target,
            range,
            props: serde_json::from_value::<CharProps>(props).unwrap(),
            preserve,
        }],
    )
}
fn payload(d: &Document, name: &str) -> String {
    String::from_utf8(d.payload(name).unwrap().to_vec()).unwrap()
}
fn runs(d: &Document, p: usize) -> Vec<(String, String)> {
    let n = d
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph)
        .nth(p)
        .unwrap();
    let x = d.doc_of(n);
    x.el(n.el)
        .children
        .iter()
        .filter(|&&r| x.el(r).local == "run")
        .map(|&r| {
            let text = x
                .descendants(r)
                .filter(|&i| x.el(i).local == "t")
                .map(|i| {
                    x.el(i)
                        .items
                        .iter()
                        .filter_map(|v| v.text())
                        .collect::<String>()
                })
                .collect();
            (x.el(r).attr_value("charPrIDRef").unwrap().into(), text)
        })
        .collect()
}
#[test]
fn char_format_reuses_existing_shape_and_splits_before_inside_after() {
    let d = fixture();
    let a = format(&d, 0, 1..4, serde_json::json!({"bold":true}), false).unwrap();
    assert_eq!(
        runs(&a.doc, 0),
        [
            ("0".into(), "A".into()),
            ("1".into(), "BCD".into()),
            ("0".into(), "EF".into())
        ]
    );
    assert_eq!(
        d.payload("Contents/header.xml"),
        a.doc.payload("Contents/header.xml")
    );
    assert_eq!(
        payload(&a.doc, "Contents/section0.xml")
            .matches("x:keep=\"&#65;\"")
            .count(),
        3
    );
    assert_eq!(d.map_rows(), a.doc.map_rows());
    assert_eq!(
        d.payload("Preview/PrvText.txt"),
        a.doc.payload("Preview/PrvText.txt")
    );
}
#[test]
fn char_format_creates_one_shape_and_updates_count() {
    let d = fixture();
    let a = format(&d, 0, 0..6, serde_json::json!({"italic":true,"heightHwpunit":2400,"textColor":"#aB12Cd","underline":true,"strikeout":true}), false).unwrap();
    let h = payload(&a.doc, "Contents/header.xml");
    assert!(h.contains("charProperties itemCnt=\"5\""));
    assert!(h.contains("id=\"4\" height=\"2400\" textColor=\"#AB12CD\""));
    assert!(h.contains("<hh:italic/>"));
    assert!(h.contains("underline type=\"BOTTOM\" shape=\"SOLID\""));
    assert!(h.contains("strikeout shape=\"SOLID\""));
    assert_eq!(runs(&a.doc, 0), [("4".into(), "ABCDEF".into())]);
    let b = format(&a.doc,0,0..6,serde_json::json!({"italic":true,"heightHwpunit":2400,"textColor":"#AB12CD","underline":true,"strikeout":true}),false).unwrap();
    assert!(b.changed_ids.is_empty());
    assert_eq!(a.doc.to_hwpx().unwrap(), b.doc.to_hwpx().unwrap());
}
#[test]
fn char_format_false_restores_base_reference_without_new_resource() {
    let d = fixture();
    let a = format(&d, 0, 0..6, serde_json::json!({"bold":true}), false).unwrap();
    let b = format(
        &a.doc,
        0,
        0..6,
        serde_json::json!({"bold":false,"italic":false,"underline":false,"strikeout":false}),
        false,
    )
    .unwrap();
    assert_eq!(runs(&b.doc, 0), [("0".into(), "ABCDEF".into())]);
    assert_eq!(
        d.payload("Contents/header.xml"),
        b.doc.payload("Contents/header.xml")
    );
}
#[test]
fn char_format_preserve_keeps_tab_and_its_old_run_shape() {
    let d = fixture();
    assert_eq!(
        format(&d, 1, 0..3, serde_json::json!({"bold":true}), false)
            .unwrap_err()
            .code(),
        Code::RangeCrossesControl
    );
    let a = format(&d, 1, 0..3, serde_json::json!({"bold":true}), true).unwrap();
    let x = payload(&a.doc, "Contents/section0.xml");
    assert!(x.contains("<hp:run charPrIDRef=\"1\"><hp:t>a</hp:t></hp:run><hp:run charPrIDRef=\"0\"><hp:t><hp:tab width=\"4000\"/></hp:t></hp:run><hp:run charPrIDRef=\"1\"><hp:t>b</hp:t></hp:run>"));
    let p = a
        .doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph)
        .nth(1)
        .unwrap();
    assert_eq!(a.doc.para_text(p).unwrap().text, "a\tb");
}
#[test]
fn char_format_graphemes_across_runs_and_entity_spellings_survive() {
    let d = fixture();
    let a = format(&d, 2, 0..3, serde_json::json!({"bold":true}), false).unwrap();
    assert_eq!(
        runs(&a.doc, 2),
        [
            ("1".into(), "e".into()),
            ("1".into(), "\u{301}&👨‍👩".into()),
            ("0".into(), "Z".into())
        ]
    );
    assert!(payload(&a.doc, "Contents/section0.xml").contains("\u{301}&amp;👨‍👩"));
}
#[test]
fn char_format_rejects_invalid_props_and_empty_reversed_outside_ranges() {
    let d = fixture();
    let before = d.to_hwpx().unwrap();
    for props in [
        serde_json::json!({}),
        serde_json::json!({"heightHwpunit":99}),
        serde_json::json!({"heightHwpunit":100001}),
        serde_json::json!({"textColor":"red"}),
        serde_json::json!({"textColor":"#12345G"}),
    ] {
        assert_eq!(
            format(&d, 0, 0..1, props, false).unwrap_err().code(),
            Code::InvalidRequest
        );
    }
    // `4..2` is deliberately reversed: the command must reject it rather than
    // treat it as an empty range.
    #[allow(clippy::reversed_empty_ranges)]
    for range in [0..0, 4..2, 0..7] {
        assert_eq!(
            format(&d, 0, range, serde_json::json!({"bold":true}), false)
                .unwrap_err()
                .code(),
            Code::InvalidRange
        );
    }
    assert_eq!(before, d.to_hwpx().unwrap());
    for props in [
        serde_json::json!({"fontRef":0}),
        serde_json::json!({"bold":"true"}),
        serde_json::json!({"heightHwpunit":100.5}),
    ] {
        assert!(serde_json::from_value::<CharProps>(props).is_err());
    }
}
#[test]
fn char_format_untouched_zip_entries_keep_raw_compressed_bytes() {
    let d = fixture();
    let a = format(&d, 0, 1..2, serde_json::json!({"bold":true}), false).unwrap();
    let archive = hae_zip::Archive::open(a.doc.to_hwpx().unwrap(), &Limits::default()).unwrap();
    for old in d.original.entries() {
        if old.payload() == archive.get(&old.name).unwrap().payload() {
            assert_eq!(d.original.raw_data(&old.name), archive.raw_data(&old.name));
            assert_eq!(
                d.original.raw_local_header(&old.name),
                archive.raw_local_header(&old.name)
            );
        }
    }
}
fn with_body(body: &str) -> Document {
    let d = fixture();
    let mut state = d.state.clone();
    let section = format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:x="urn:opaque">{body}</hs:sec>"#
    );
    state.set(
        "Contents/section0.xml",
        std::sync::Arc::from(section.into_bytes()),
    );
    let bytes = hae_zip::write(
        &d.original,
        &hae_package::zip_plan(&d.original, &state).unwrap(),
    )
    .unwrap();
    Document::open(bytes, &Limits::default()).unwrap()
}
#[test]
fn char_format_rejects_cdata_and_comment_crossings_in_both_modes() {
    for text in ["a<![CDATA[b]]>c", "a<!--keep-->bc", "a<?keep yes?>bc"] {
        let d = with_body(&format!(
            r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p>"#
        ));
        for preserve in [false, true] {
            assert_eq!(
                format(&d, 0, 0..3, serde_json::json!({"bold":true}), preserve)
                    .unwrap_err()
                    .code(),
                Code::UnsupportedContentInRange
            );
        }
    }
}
#[test]
fn char_format_preserve_retains_equation_node_ids_and_fields_fail() {
    let d = with_body(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>a</hp:t><hp:equation id="99"/><hp:t>b</hp:t></hp:run></hp:p>"#,
    );
    let before: Vec<_> = d
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.kind, n.parent.clone()))
        .collect();
    let a = format(&d, 0, 0..3, serde_json::json!({"bold":true}), true).unwrap();
    assert_eq!(
        before,
        a.doc
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.kind, n.parent.clone()))
            .collect::<Vec<_>>()
    );
    let after = payload(&a.doc, "Contents/section0.xml");
    assert!(after.contains("<hp:equation id=\"99\"/>"));
    let d = with_body(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>a</hp:t><hp:ctrl><hp:fieldBegin id="1"/></hp:ctrl><hp:t>b</hp:t></hp:run></hp:p>"#,
    );
    assert_eq!(
        format(&d, 0, 0..3, serde_json::json!({"bold":true}), true)
            .unwrap_err()
            .reason(),
        Some("field_boundary_in_range")
    );
    // Foreign-namespace content is not a control: `preserve` keeps controls in
    // place but it does not unlock a paragraph whose meaning we cannot
    // interpret, because re-splitting its runs could change that content.
    let d = with_body(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>a</hp:t><x:opaque keep="yes"/><hp:t>b</hp:t></hp:run></hp:p>"#,
    );
    assert_eq!(
        format(&d, 0, 0..3, serde_json::json!({"bold":true}), true)
            .unwrap_err()
            .reason(),
        Some("contains_opaque")
    );
}
