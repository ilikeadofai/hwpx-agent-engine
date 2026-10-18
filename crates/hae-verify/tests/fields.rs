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
// ---------------------------------------------------------------------------
// Real `type="CLICK_HERE"` markup: the public 간이기안문/일반기안문 templates in
// tests/fixtures/corpus (MIT, chrisryugj/kordoc). Requests and candidates are
// written by hand from the original XML; no editor or model crate is imported.
// ---------------------------------------------------------------------------

const ILBAN: &[u8] = include_bytes!("../../../tests/fixtures/corpus/ilban-gian.hwpx");
const GANI: &[u8] = include_bytes!("../../../tests/fixtures/corpus/gani-gian.hwpx");
const NS_HP: &str = "http://www.hancom.co.kr/hwpml/2011/paragraph";
fn real(input: &[u8]) -> Archive {
    Archive::open(input.to_vec(), &Limits::default()).unwrap()
}
fn is_hp(e: &hae_xml::Element, name: &str) -> bool {
    e.ns.as_deref() == Some(NS_HP) && e.local == name
}
fn section_of(a: &Archive) -> String {
    String::from_utf8(a.get(SECTION).unwrap().payload().to_vec()).unwrap()
}
/// Element path of the paragraph that owns the named begin marker.
fn paragraph_path(a: &Archive, name: &str) -> Vec<usize> {
    let doc = hae_xml::XmlDoc::parse(section_of(a).into_bytes()).unwrap();
    let begin = doc
        .iter()
        .find(|(_, e)| is_hp(e, "fieldBegin") && e.attr_value("name") == Some(name))
        .unwrap_or_else(|| panic!("no field {name}"))
        .0;
    let para = doc
        .ancestors(begin)
        .find(|&i| is_hp(doc.el(i), "p"))
        .unwrap();
    doc.path(para)
}
fn real_request(a: &Archive, name: &str, text: &str) -> VerifyOperation {
    serde_json::from_value(serde_json::json!({
        "op": "field.setText", "entry": SECTION, "path": paragraph_path(a, name),
        "text": text, "name": name, "fieldid": null, "occurrence": null
    }))
    .unwrap()
}
/// Hand-written candidate: the new value sits between the begin marker's
/// wrapper and the end marker, exactly as the request describes.
fn fill(xml: &str, text: &str, id: &str) -> String {
    let needle = format!("</hp:ctrl><hp:ctrl><hp:fieldEnd beginIDRef=\"{id}\"");
    assert!(xml.contains(&needle), "{id}");
    xml.replacen(
        &needle,
        &format!(
            "</hp:ctrl><hp:t>{}</hp:t><hp:ctrl><hp:fieldEnd beginIDRef=\"{id}\"",
            hae_xml::escape_text(text)
        ),
        1,
    )
}
fn state_of(a: &Archive, xml: String) -> PackageState {
    let mut st = PackageState::from_archive(a);
    st.set(SECTION, Arc::from(xml.into_bytes()));
    st
}
fn real_gate(a: &Archive, st: &PackageState, requests: &[VerifyOperation]) -> hae_diag::Result<()> {
    let plan = hae_package::zip_plan(a, st).unwrap();
    verify_archive(
        a,
        &hae_zip::write(a, &plan).unwrap(),
        requests,
        &Limits::default(),
    )
    .map(|_| ())
}

#[test]
fn field_gate_accepts_real_template_candidate() {
    let a = real(ILBAN);
    let st = state_of(&a, fill(&section_of(&a), "행정안전부", "1700000001"));
    real_gate(&a, &st, &[real_request(&a, "행정기관명", "행정안전부")]).unwrap();
    // A value that is only whitespace is still a value, and empty is derived too.
    let st = state_of(&a, fill(&section_of(&a), "  ", "1700000001"));
    real_gate(&a, &st, &[real_request(&a, "행정기관명", "  ")]).unwrap();
}

#[test]
fn field_gate_accepts_real_template_cell_fill() {
    // gani-gian fills happen inside table cells (approval line and title cells).
    let a = real(GANI);
    let st = state_of(&a, fill(&section_of(&a), "과장 홍길동", "1800000005"));
    real_gate(&a, &st, &[real_request(&a, "결재직위1", "과장 홍길동")]).unwrap();
}

#[test]
fn field_gate_rejects_real_template_marker_mutation() {
    let a = real(ILBAN);
    let mut st = state_of(&a, fill(&section_of(&a), "행정안전부", "1700000001"));
    let x = section_of(&a).replacen("beginIDRef=\"1700000001\"", "beginIDRef=\"1700000002\"", 1);
    st.set(SECTION, Arc::from(x.into_bytes()));
    assert!(real_gate(&a, &st, &[real_request(&a, "행정기관명", "행정안전부")]).is_err());
}

#[test]
fn field_gate_rejects_real_template_parameter_mutation() {
    // The value region matches, but one Command byte of that field changed.
    let a = real(ILBAN);
    let mut st = state_of(&a, fill(&section_of(&a), "행정안전부", "1700000001"));
    let x = section_of(&a).replacen(
        "wstring:5:행정기관명 HelpState",
        "wstring:5:행정기관명X HelpState",
        1,
    );
    st.set(SECTION, Arc::from(x.into_bytes()));
    assert!(real_gate(&a, &st, &[real_request(&a, "행정기관명", "행정안전부")]).is_err());
}

#[test]
fn field_gate_rejects_real_template_unrelated_cell_text() {
    let a = real(GANI);
    let other = "<hp:t>협조자</hp:t>";
    assert!(section_of(&a).contains(other));
    // The requested fill plus an unrelated cell label edit is refused.
    let mut st = state_of(&a, fill(&section_of(&a), "과장 홍길동", "1800000005"));
    let x = fill(&section_of(&a), "과장 홍길동", "1800000005").replacen(
        other,
        "<hp:t>협조자!</hp:t>",
        1,
    );
    st.set(SECTION, Arc::from(x.into_bytes()));
    assert!(real_gate(&a, &st, &[real_request(&a, "결재직위1", "과장 홍길동")]).is_err());
    // The unrelated edit alone no longer matches the request either.
    let mut st = PackageState::from_archive(&a);
    let x = section_of(&a).replacen(other, "<hp:t>협조자!</hp:t>", 1);
    st.set(SECTION, Arc::from(x.into_bytes()));
    assert!(real_gate(&a, &st, &[]).is_err());
    assert!(real_gate(&a, &st, &[real_request(&a, "결재직위1", "과장 홍길동")]).is_err());
}
