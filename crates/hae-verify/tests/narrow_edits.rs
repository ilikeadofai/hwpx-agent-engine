//! Handwritten candidates, with no model/editor dependency or planner output.
use hae_diag::{Code, Limits};
use hae_package::PackageState;
use hae_verify::{verify_archive, VerifyOperation};
use hae_xml::{ParseOptions, XmlDoc};
use hae_zip::Archive;
use serde_json::json;
use std::sync::Arc;
const S: &str = "Contents/section0.xml";
const H: &str = "Contents/header.xml";
fn original() -> Archive {
    Archive::open(
        include_bytes!("../../../tests/fixtures/editing/narrow_edits.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap()
}
fn xml(st: &PackageState, entry: &str) -> XmlDoc {
    XmlDoc::parse_with(st.get(entry).unwrap().clone(), ParseOptions::default()).unwrap()
}
fn replace(st: &mut PackageState, entry: &str, from: &str, to: &str) {
    let s = std::str::from_utf8(st.get(entry).unwrap()).unwrap();
    assert!(s.contains(from), "missing {from}");
    st.set(entry, Arc::from(s.replacen(from, to, 1).into_bytes()));
}
fn request(cmd: &str) -> VerifyOperation {
    serde_json::from_value(match cmd {
    "style"=>json!({"op":"paragraph.setStyle","targets":[{"entry":S,"path":[0]}],"styleId":1}),
    "fit"=>json!({"op":"image.fit","entry":S,"path":[3,0,0],"mode":"contain"}),
    _=>json!({"op":"header.setText","entry":S,"path":[],"kind":"header","paragraphIndex":0,"text":"New header"}),
}).unwrap()
}
fn candidate(cmd: &str) -> PackageState {
    let mut st = PackageState::from_archive(&original());
    match cmd {
        "style" => {
            replace(&mut st, S, "styleIDRef=\"0\"", "styleIDRef=\"1\"");
            let d = xml(&st, S);
            let cuts = d
                .el(d.root())
                .children
                .iter()
                .take(2)
                .flat_map(|&p| d.el(p).children.iter().copied())
                .filter(|&i| d.el(i).local == "linesegarray")
                .map(|i| (d.el(i).outer(), vec![]))
                .collect();
            st.set(
                S,
                d.splice(cuts, ParseOptions::default())
                    .unwrap()
                    .bytes()
                    .clone(),
            );
        }
        "fit" => {
            replace(
                &mut st,
                S,
                "curSz width=\"1200\" height=\"1200\"",
                "curSz width=\"1200\" height=\"600\"",
            );
            replace(
                &mut st,
                S,
                "sz width=\"1200\" widthRelTo=\"ABSOLUTE\" height=\"1200\"",
                "sz width=\"1200\" widthRelTo=\"ABSOLUTE\" height=\"600\"",
            );
            replace(&mut st,S,"scaMatrix e1=\"1\" e2=\"0\" e3=\"0\" e4=\"0\" e5=\"1\" e6=\"0\"","scaMatrix e1=\"1\" e2=\"0\" e3=\"0\" e4=\"0.000000\" e5=\"0.500000\" e6=\"0.000000\"");
        }
        _ => {
            replace(
                &mut st,
                S,
                "Header A</hp:t></hp:run><hp:linesegarray/>",
                "New header</hp:t></hp:run>",
            );
            replace(&mut st, "Preview/PrvText.txt", "Header A", "New header");
        }
    }
    st
}
fn gate(st: &PackageState, cmd: &str) -> hae_diag::Result<hae_verify::VerifyReport> {
    let a = original();
    let bytes = hae_zip::write(&a, &hae_package::zip_plan(&a, st).unwrap()).unwrap();
    verify_archive(&a, &bytes, &[request(cmd)], &Limits::default())
}
fn tamper(cmd: &str, entry: &str, from: &str, to: &str) {
    let mut st = candidate(cmd);
    replace(&mut st, entry, from, to);
    assert_eq!(gate(&st, cmd).unwrap_err().code(), Code::VerifyFailed);
}
#[test]
fn style_gate_accepts_handwritten_candidate() {
    gate(&candidate("style"), "style").unwrap();
}
#[test]
fn style_gate_rejects_unrequested_paragraph() {
    tamper(
        "style",
        S,
        "<hp:p paraPrIDRef=\"0\" styleIDRef=\"0\">",
        "<hp:p paraPrIDRef=\"0\" styleIDRef=\"1\">",
    );
}
#[test]
fn style_gate_rejects_wrong_requested_id() {
    tamper("style", S, "styleIDRef=\"1\"", "styleIDRef=\"0\"");
}
#[test]
fn style_gate_rejects_other_paragraph_attribute() {
    tamper("style", S, "paraPrIDRef=\"0\"", "paraPrIDRef=\"9\"");
}
#[test]
fn style_gate_rejects_new_style_definition() {
    tamper(
        "style",
        H,
        "</hh:styles>",
        "<hh:style id=\"9\"/></hh:styles>",
    );
}
#[test]
fn style_gate_rejects_text_mutation() {
    tamper("style", S, "Body A", "Body X");
}
#[test]
fn fit_gate_accepts_handwritten_geometry() {
    gate(&candidate("fit"), "fit").unwrap();
}
#[test]
fn fit_gate_rejects_pixel_instead_of_hwpunit_clip() {
    tamper("fit", S, "right=\"150\"", "right=\"2\"");
}
#[test]
fn fit_gate_rejects_inconsistent_current_size() {
    tamper(
        "fit",
        S,
        "curSz width=\"1200\" height=\"600\"",
        "curSz width=\"1200\" height=\"601\"",
    );
}
#[test]
fn fit_gate_rejects_unrequested_original_size() {
    tamper("fit", S, "orgSz width=\"1200\"", "orgSz width=\"1201\"");
}
#[test]
fn fit_gate_rejects_lost_rendering_matrix() {
    tamper(
        "fit",
        S,
        "<hc:rotMatrix e1=\"1\" e2=\"0\" e3=\"0\" e4=\"0\" e5=\"1\" e6=\"0\"/>",
        "",
    );
}
#[test]
fn fit_gate_rejects_lost_embedding() {
    tamper("fit", "Contents/content.hpf", " isEmbeded=\"1\"", "");
}
#[test]
fn fit_gate_rejects_wrong_source_dimension() {
    tamper("fit", S, "dimwidth=\"150\"", "dimwidth=\"151\"");
}
#[test]
fn fit_gate_rejects_unrequested_media_bytes() {
    let mut st = candidate("fit");
    st.set("BinData/narrow.png", Arc::from(b"bad".as_slice()));
    assert!(gate(&st, "fit").is_err());
}
#[test]
fn header_gate_accepts_handwritten_candidate() {
    gate(&candidate("header"), "header").unwrap();
}
#[test]
fn header_gate_rejects_other_header_paragraph() {
    tamper("header", S, "Header B", "Other");
}
#[test]
fn header_gate_rejects_body_change() {
    tamper("header", S, "Body A", "Other");
}
#[test]
fn header_gate_rejects_footer_change() {
    tamper("header", S, "Footer A", "Other");
}
#[test]
fn header_gate_rejects_margin_change() {
    tamper("header", S, "textHeight=\"2000\"", "textHeight=\"2001\"");
}
#[test]
fn header_gate_rejects_number_control_loss() {
    tamper("header", S, "<hp:autoNum num=\"1\"/>", "");
}
#[test]
fn header_gate_rejects_missing_preview_sync() {
    tamper("header", "Preview/PrvText.txt", "New header", "Header A");
}
#[test]
fn fit_gate_accepts_handwritten_cover_crop_and_stretch() {
    let a = original();
    for mode in ["cover", "stretch"] {
        let mut st = PackageState::from_archive(&a);
        if mode == "cover" {
            replace(
                &mut st,
                S,
                "imgClip left=\"0\" right=\"150\"",
                "imgClip left=\"37\" right=\"112\"",
            );
        }
        let bytes = hae_zip::write(&a, &hae_package::zip_plan(&a, &st).unwrap()).unwrap();
        let op =
            serde_json::from_value(json!({"op":"image.fit","entry":S,"path":[3,0,0],"mode":mode}))
                .unwrap();
        verify_archive(&a, &bytes, &[op], &Limits::default()).unwrap();
    }
}
