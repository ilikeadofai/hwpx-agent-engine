//! No editor/model imports: candidates and requests are constructed by hand.
use hae_diag::Limits;
use hae_package::PackageState;
use hae_verify::{verify_archive, VerifyOperation};
use hae_zip::Archive;
use std::sync::Arc;
const SECTION: &str = "Contents/section0.xml";
fn original() -> Archive {
    Archive::open(
        include_bytes!("../../../tests/fixtures/editing/fields.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap()
}
fn request() -> VerifyOperation {
    serde_json::from_value(serde_json::json!({"op":"field.setText","entry":SECTION,"path":[1],"name":"link","fieldid":null,"occurrence":null,"text":"new link"})).unwrap()
}
fn replace(st: &mut PackageState, entry: &str, a: &str, b: &str) {
    let x = std::str::from_utf8(st.get(entry).unwrap()).unwrap();
    assert!(x.contains(a));
    st.set(entry, Arc::from(x.replacen(a, b, 1).into_bytes()));
}
fn candidate() -> PackageState {
    let mut st = PackageState::from_archive(&original());
    replace(
        &mut st,
        SECTION,
        "<hp:t>link</hp:t>",
        "<hp:t>new link</hp:t>",
    );
    st
}
fn bytes(st: &PackageState) -> Vec<u8> {
    let a = original();
    hae_zip::write(&a, &hae_package::zip_plan(&a, st).unwrap()).unwrap()
}
fn gate(st: &PackageState) -> hae_diag::Result<hae_verify::VerifyReport> {
    verify_archive(&original(), &bytes(st), &[request()], &Limits::default())
}
#[test]
fn field_gate_accepts_independently_handwritten_candidate() {
    gate(&candidate()).unwrap();
}
#[test]
fn field_gate_rejects_marker_mutation() {
    let mut st = candidate();
    replace(&mut st, SECTION, "beginIDRef=\"3\"", "beginIDRef=\"33\"");
    assert!(gate(&st).is_err());
}
#[test]
fn field_gate_rejects_name_mutation() {
    let mut st = candidate();
    replace(&mut st, SECTION, "name=\"link\"", "name=\"changed\"");
    assert!(gate(&st).is_err());
}
#[test]
fn field_gate_rejects_other_field_value_mutation() {
    let mut st = candidate();
    replace(&mut st, SECTION, "old&amp;value", "other");
    assert!(gate(&st).is_err());
}
#[test]
fn field_gate_rejects_unrequested_header_change() {
    let mut st = candidate();
    replace(
        &mut st,
        "Contents/header.xml",
        "height=\"1000\"",
        "height=\"1200\"",
    );
    assert!(gate(&st).is_err());
}
#[test]
fn field_gate_rejects_command_parameter_mutation() {
    let mut st = candidate();
    replace(&mut st, SECTION, "guide&amp;raw", "changed");
    assert!(gate(&st).is_err());
}
#[test]
fn field_gate_rejects_untouched_entry_reencoding() {
    let a = original();
    let good = bytes(&candidate());
    let mut plan = hae_package::zip_plan(&a, &candidate()).unwrap();
    // Force rewriting a payload-identical deflated member, losing original local header/data.
    let name = "Contents/header.xml";
    let i = plan
        .iter()
        .position(|e| matches!(e,hae_zip::OutEntry::Keep(n) if n==name))
        .unwrap();
    plan[i] = hae_zip::OutEntry::Replace(name.into(), a.get(name).unwrap().payload().clone());
    let bad = hae_zip::write(&a, &plan).unwrap();
    assert!(verify_archive(&a, &good, &[request()], &Limits::default()).is_ok());
    assert!(verify_archive(&a, &bad, &[request()], &Limits::default()).is_err());
}
