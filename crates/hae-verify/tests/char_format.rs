//! Candidates are hand assembled without hae-edit or hae-model.
use hae_diag::{Code, Limits};
use hae_package::PackageState;
use hae_verify::{verify_archive, CharProps, VerifyOperation};
use hae_zip::Archive;
use std::sync::Arc;
const HEADER: &str = "Contents/header.xml";
const SECTION: &str = "Contents/section0.xml";
fn original() -> Archive {
    Archive::open(
        include_bytes!("../../../tests/fixtures/editing/char_format.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap()
}
fn replace(st: &mut PackageState, name: &str, a: &str, b: &str) {
    let old = std::str::from_utf8(st.get(name).unwrap()).unwrap();
    assert!(old.contains(a), "missing {a}");
    st.set(name, Arc::from(old.replacen(a, b, 1).into_bytes()));
}
fn op() -> VerifyOperation {
    VerifyOperation::TextFormat {
        entry: SECTION.into(),
        path: vec![0],
        range: 1..4,
        props: CharProps {
            bold: Some(true),
            ..Default::default()
        },
        preserve: false,
    }
}
fn candidate() -> PackageState {
    let mut st = PackageState::from_archive(&original());
    replace(&mut st,SECTION,"<hp:t>ABCDEF</hp:t>","<hp:t>A</hp:t></hp:run><hp:run charPrIDRef=\"1\" xmlns:x=\"urn:format\" x:keep=\"&#65;\"><hp:t>BCD</hp:t></hp:run><hp:run charPrIDRef=\"0\" xmlns:x=\"urn:format\" x:keep=\"&#65;\"><hp:t>EF</hp:t>");
    replace(&mut st, SECTION, "<hp:linesegarray/>", "");
    st
}
fn gate(st: &PackageState, request: VerifyOperation) -> hae_diag::Result<hae_verify::VerifyReport> {
    let a = original();
    let bytes = hae_zip::write(&a, &hae_package::zip_plan(&a, st).unwrap()).unwrap();
    verify_archive(&a, &bytes, &[request], &Limits::default())
}
#[test]
fn text_format_independent_gate_accepts_handwritten_split_and_raw_zip_preservation() {
    let st = candidate();
    let report = gate(&st, op()).unwrap();
    assert_eq!(report.changed_entries, [SECTION]);
    assert!(!report.prv_text_stale);
}
#[test]
fn text_format_gate_rejects_outside_range_shape_change() {
    let mut st = candidate();
    replace(
        &mut st,
        SECTION,
        "<hp:run charPrIDRef=\"0\" xmlns:x=\"urn:format\" x:keep=\"&#65;\"><hp:t>EF",
        "<hp:run charPrIDRef=\"1\" xmlns:x=\"urn:format\" x:keep=\"&#65;\"><hp:t>EF",
    );
    assert_eq!(gate(&st, op()).unwrap_err().code(), Code::VerifyFailed);
}
#[test]
fn text_format_gate_rejects_missing_selected_shape() {
    let mut st = candidate();
    replace(
        &mut st,
        SECTION,
        "charPrIDRef=\"1\" xmlns:x",
        "charPrIDRef=\"0\" xmlns:x",
    );
    assert!(gate(&st, op()).is_err());
}
#[test]
fn text_format_gate_rejects_text_or_entity_mutation() {
    let mut st = candidate();
    replace(&mut st, SECTION, "BCD", "BCX");
    assert!(gate(&st, op()).is_err());
}
#[test]
fn text_format_gate_rejects_unrequested_header_changes() {
    let mut st = candidate();
    replace(&mut st, HEADER, "height=\"1000\"", "height=\"1200\"");
    assert!(gate(&st, op()).is_err());
}
#[test]
fn text_format_gate_rejects_bad_item_count() {
    let mut st = candidate();
    replace(
        &mut st,
        HEADER,
        "charProperties itemCnt=\"4\"",
        "charProperties itemCnt=\"5\"",
    );
    assert!(gate(&st, op()).is_err());
}
#[test]
fn text_format_gate_rejects_dangling_reference() {
    let mut st = candidate();
    replace(
        &mut st,
        SECTION,
        "charPrIDRef=\"1\" xmlns:x",
        "charPrIDRef=\"99\" xmlns:x",
    );
    assert!(gate(&st, op()).is_err());
}
#[test]
fn text_format_gate_rejects_opaque_attribute_loss_and_preview_mutation() {
    for (entry, from, to) in [
        (SECTION, "x:keep=\"&#65;\"", "x:keep=\"A\""),
        ("Preview/PrvText.txt", "", "damaged preview"),
    ] {
        let mut st = candidate();
        if from.is_empty() {
            st.set(entry, Arc::from(to.as_bytes()));
        } else {
            replace(&mut st, entry, from, to);
        }
        assert!(gate(&st, op()).is_err());
    }
}
#[test]
fn text_format_gate_accepts_new_shape_but_rejects_id_and_property_tampering() {
    let mut st = candidate();
    let source = std::str::from_utf8(st.get(HEADER).unwrap()).unwrap();
    let start = source.find("<hh:charPr id=\"0\"").unwrap();
    let finish = start + source[start..].find("</hh:charPr>").unwrap() + "</hh:charPr>".len();
    let clone = source[start..finish]
        .replace("id=\"0\"", "id=\"4\"")
        .replace("<hh:underline", "<hh:italic/><hh:underline");
    replace(
        &mut st,
        HEADER,
        "</hh:charProperties>",
        &format!("{clone}</hh:charProperties>"),
    );
    replace(
        &mut st,
        HEADER,
        "charProperties itemCnt=\"4\"",
        "charProperties itemCnt=\"5\"",
    );
    replace(
        &mut st,
        SECTION,
        "charPrIDRef=\"1\" xmlns:x",
        "charPrIDRef=\"4\" xmlns:x",
    );
    let request = || VerifyOperation::TextFormat {
        entry: SECTION.into(),
        path: vec![0],
        range: 1..4,
        props: CharProps {
            italic: Some(true),
            ..Default::default()
        },
        preserve: false,
    };
    gate(&st, request()).unwrap();
    for (a, b) in [("id=\"4\"", "id=\"8\""), ("<hh:italic/>", "<hh:bold/>")] {
        let mut wrong = st.clone();
        replace(&mut wrong, HEADER, a, b);
        assert!(gate(&wrong, request()).is_err());
    }
}
