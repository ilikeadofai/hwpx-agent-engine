//! Handwritten candidates. No editor or model dependency in this gate oracle.
use hae_diag::{Code, Limits};
use hae_package::PackageState;
use hae_verify::{verify_archive, EquationProps, VerifyOperation};
use hae_zip::{Archive, OutEntry};
use std::sync::Arc;
const SECTION: &str = "Contents/section0.xml";
const OLD: &str = "{a} over {b}  TIMES  {c} over {d} = {ac} over {bd}";
fn original() -> Archive {
    Archive::open(
        include_bytes!("../../../tests/fixtures/editing/equations.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap()
}
fn change(s: &mut PackageState, from: &str, to: &str) {
    let xml = std::str::from_utf8(s.get(SECTION).unwrap()).unwrap();
    assert!(xml.contains(from), "{from}");
    s.set(SECTION, Arc::from(xml.replacen(from, to, 1).into_bytes()));
}
fn op() -> VerifyOperation {
    VerifyOperation::EquationSetScript {
        entry: SECTION.into(),
        path: vec![0, 0, 2],
        script: "x < y & z".into(),
    }
}
fn script_state() -> PackageState {
    let mut s = PackageState::from_archive(&original());
    change(&mut s, OLD, "x &lt; y &amp; z");
    let xml = std::str::from_utf8(s.get(SECTION).unwrap())
        .unwrap()
        .replace("<hp:linesegarray/>", "");
    s.set(SECTION, Arc::from(xml.into_bytes()));
    s
}
fn props() -> VerifyOperation {
    VerifyOperation::EquationSetProperties {
        entry: SECTION.into(),
        path: vec![0, 0, 2],
        props: EquationProps {
            base_line: Some(77),
            font: Some("Eq & custom".into()),
            ..Default::default()
        },
    }
}
fn property_state() -> PackageState {
    let mut s = PackageState::from_archive(&original());
    change(&mut s, "baseLine=\"66\"", "baseLine=\"77\"");
    change(&mut s, "font=\"HancomEQN\"", "font=\"Eq &amp; custom\"");
    let xml = std::str::from_utf8(s.get(SECTION).unwrap())
        .unwrap()
        .replace("<hp:linesegarray/>", "");
    s.set(SECTION, Arc::from(xml.into_bytes()));
    s
}
fn bytes(s: &PackageState) -> Vec<u8> {
    let a = original();
    hae_zip::write(&a, &hae_package::zip_plan(&a, s).unwrap()).unwrap()
}
fn gate(s: &PackageState, ops: &[VerifyOperation]) -> hae_diag::Result<hae_verify::VerifyReport> {
    verify_archive(&original(), &bytes(s), ops, &Limits::default())
}
#[test]
fn equation_gate_accepts_independently_handwritten_script_and_cache_policy() {
    let r = gate(&script_state(), &[op()]).unwrap();
    assert_eq!(r.changed_entries, [SECTION]);
    assert_eq!(r.cache_removals[SECTION], 4);
}
#[test]
fn equation_gate_accepts_independently_handwritten_properties_and_opaque_bytes() {
    gate(&property_state(), &[props()]).unwrap();
}
#[test]
fn equation_gate_rejects_script_tampering() {
    let mut s = script_state();
    change(&mut s, "x &lt; y &amp; z", "x &lt; y &amp; WRONG");
    assert_eq!(gate(&s, &[op()]).unwrap_err().code(), Code::VerifyFailed);
}
#[test]
fn equation_gate_rejects_unrequested_attribute_change() {
    let mut s = script_state();
    change(&mut s, "baseLine=\"66\"", "baseLine=\"78\"");
    assert!(gate(&s, &[op()]).is_err());
}
#[test]
fn equation_gate_rejects_size_change() {
    let mut s = script_state();
    change(&mut s, "width=\"5675\"", "width=\"9999\"");
    assert!(gate(&s, &[op()]).is_err());
}
#[test]
fn equation_gate_rejects_another_equation_change() {
    let mut s = script_state();
    change(
        &mut s,
        "<hp:script></hp:script>",
        "<hp:script>damaged</hp:script>",
    );
    assert!(gate(&s, &[op()]).is_err());
}
#[test]
fn equation_gate_rejects_untouched_entity_reencoding() {
    let mut s = script_state();
    change(&mut s, "x:keep=\"&#65;\"", "x:keep=\"A\"");
    assert!(gate(&s, &[op()]).is_err());
}
#[test]
fn equation_gate_rejects_changes_without_requests() {
    assert!(gate(&script_state(), &[]).is_err());
    assert!(gate(&property_state(), &[]).is_err());
}
#[test]
fn equation_gate_rejects_untouched_zip_recompression() {
    let a = original();
    let s = script_state();
    let mut plan = hae_package::zip_plan(&a, &s).unwrap();
    let at = s
        .entries
        .iter()
        .position(|e| e.name == "Contents/header.xml")
        .unwrap();
    let e = a.get("Contents/header.xml").unwrap();
    plan[at] = OutEntry::Replace(e.name.clone(), e.payload().clone());
    let b = hae_zip::write(&a, &plan).unwrap();
    let c = Archive::open(b.clone(), &Limits::default()).unwrap();
    assert_ne!(a.raw_local_header(&e.name), c.raw_local_header(&e.name));
    assert!(verify_archive(&a, &b, &[op()], &Limits::default()).is_err());
}
#[test]
fn equation_properties_gate_rejects_script_geometry_position_comment_and_other_props() {
    for (a, b) in [
        (OLD, "damaged"),
        ("height=\"2250\"", "height=\"2251\""),
        ("treatAsChar=\"1\"", "treatAsChar=\"0\""),
        ("수식입니다.", "변조"),
        ("textColor=\"#000000\"", "textColor=\"#FF0000\""),
    ] {
        let mut s = property_state();
        change(&mut s, a, b);
        assert!(gate(&s, &[props()]).is_err(), "{a}");
    }
}
#[test]
fn equation_gate_rejects_missing_requested_edit_and_retained_cache() {
    assert!(gate(&PackageState::from_archive(&original()), &[op()]).is_err());
    let mut s = PackageState::from_archive(&original());
    change(&mut s, OLD, "x &lt; y &amp; z");
    assert!(gate(&s, &[op()]).is_err());
}
#[test]
fn equation_gate_rejects_invalid_requests_independently() {
    for script in ["", "x\0y", "\\unknown", "{x", "x}", "\"x\"", "<!--x-->"] {
        let request = VerifyOperation::EquationSetScript {
            entry: SECTION.into(),
            path: vec![0, 0, 2],
            script: script.into(),
        };
        assert!(gate(&script_state(), &[request]).is_err());
    }
    let invalid = VerifyOperation::EquationSetProperties {
        entry: SECTION.into(),
        path: vec![0, 0, 2],
        props: EquationProps::default(),
    };
    assert!(gate(&property_state(), &[invalid]).is_err());
}
