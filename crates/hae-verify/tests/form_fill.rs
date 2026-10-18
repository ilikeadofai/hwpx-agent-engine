//! Hand-assembled candidates for the form.fill gate. No hae-edit or hae-model
//! dependency: the pre-edit and candidate bytes are written with plain string
//! splices and the request is the only other input.
use hae_diag::{Code, Limits};
use hae_package::PackageState;
use hae_verify::{verify_archive, VerifyOperation, VerifyScope};
use hae_zip::Archive;
use std::collections::BTreeMap;
use std::sync::Arc;

const SECTION: &str = "Contents/section0.xml";
fn limits() -> Limits {
    Limits::default()
}

fn stock() -> Archive {
    Archive::open(
        include_bytes!("../../../tests/fixtures/editing/simple_cell.hwpx").to_vec(),
        &limits(),
    )
    .unwrap()
}
fn p(inner: &str) -> String {
    format!(r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0">{inner}</hp:run></hp:p>"#)
}
fn tc(row: u32, col: u32, inner: &str) -> String {
    format!(
        r#"<hp:tc><hp:cellAddr rowAddr="{row}" colAddr="{col}"/><hp:cellSpan rowSpan="1" colSpan="1"/><hp:subList>{}</hp:subList></hp:tc>"#,
        p(inner)
    )
}
/// 성명 → `<hp:t></hp:t>`, 소속 → `<hp:t />`, plus one unrelated empty
/// paragraph with `<hp:t/>`. Each empty spelling occurs exactly once.
fn form_body() -> String {
    let table = format!(
        r#"<hp:tbl rowCnt="2" colCnt="2"><hp:tr>{}{}</hp:tr><hp:tr>{}{}</hp:tr></hp:tbl>"#,
        tc(0, 0, "<hp:t>성명</hp:t>"),
        tc(0, 1, "<hp:t></hp:t>"),
        tc(1, 0, "<hp:t>소속</hp:t>"),
        tc(1, 1, "<hp:t />"),
    );
    format!("{}{}", p(&table), p("<hp:t/>"))
}
fn section(body: &str) -> String {
    format!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">{body}</hs:sec>"#
    )
}
fn base_state(body: &str) -> PackageState {
    let mut st = PackageState::from_archive(&stock());
    st.set(SECTION, Arc::from(section(body).into_bytes()));
    st
}
fn replace(st: &mut PackageState, a: &str, b: &str) {
    let old = std::str::from_utf8(st.get(SECTION).unwrap())
        .unwrap()
        .to_string();
    assert!(old.contains(a), "missing anchor {a}");
    st.set(SECTION, Arc::from(old.replacen(a, b, 1).into_bytes()));
}
fn write(state: &PackageState) -> Vec<u8> {
    let a = stock();
    hae_zip::write(&a, &hae_package::zip_plan(&a, state).unwrap()).unwrap()
}
fn op(scope: Option<VerifyScope>, strict: bool, values: &[(&str, &str)]) -> VerifyOperation {
    VerifyOperation::FormFill {
        values: values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
        scope,
        strict,
    }
}
fn gate(
    pre: &PackageState,
    candidate: &PackageState,
    request: VerifyOperation,
) -> hae_diag::Result<hae_verify::VerifyReport> {
    let original = Archive::open(write(pre), &limits()).unwrap();
    verify_archive(&original, &write(candidate), &[request], &limits())
}
fn filled_state() -> PackageState {
    let mut st = base_state(&form_body());
    replace(&mut st, "<hp:t></hp:t>", "<hp:t>홍길동</hp:t>");
    replace(&mut st, "<hp:t />", "<hp:t>○○고등학교</hp:t>");
    st
}
fn values() -> Vec<(&'static str, &'static str)> {
    vec![("성명", "홍길동"), ("소속", "○○고등학교")]
}

#[test]
fn form_fill_gate_accepts_the_independently_derived_candidate() {
    let report = gate(
        &base_state(&form_body()),
        &filled_state(),
        op(None, true, &values()),
    )
    .unwrap();
    assert_eq!(report.changed_entries, [SECTION]);
    // Section-root and table scopes locate the same table.
    let scope = |path: Vec<usize>| {
        Some(VerifyScope {
            entry: SECTION.into(),
            path,
        })
    };
    gate(
        &base_state(&form_body()),
        &filled_state(),
        op(scope(vec![]), true, &values()),
    )
    .unwrap();
    gate(
        &base_state(&form_body()),
        &filled_state(),
        op(scope(vec![0, 0, 0]), true, &values()),
    )
    .unwrap();
}

#[test]
fn form_fill_gate_rejects_wrong_values_and_skipped_or_extra_fills() {
    let pre = base_state(&form_body());
    // 1. A different value than requested.
    let mut st = base_state(&form_body());
    replace(&mut st, "<hp:t></hp:t>", "<hp:t>홍길순</hp:t>");
    replace(&mut st, "<hp:t />", "<hp:t>○○고등학교</hp:t>");
    assert_eq!(
        gate(&pre, &st, op(None, true, &values()))
            .unwrap_err()
            .code(),
        Code::VerifyFailed
    );
    // 2. A resolvable label silently left empty.
    let mut st = base_state(&form_body());
    replace(&mut st, "<hp:t></hp:t>", "<hp:t>홍길동</hp:t>");
    assert!(gate(&pre, &st, op(None, true, &values())).is_err());
    // 3. An unrequested empty paragraph filled as well.
    let mut st = filled_state();
    replace(&mut st, "<hp:t/>", "<hp:t>몰래</hp:t>");
    assert!(gate(&pre, &st, op(None, true, &values())).is_err());
    // 4. A label cell rewritten instead of (or beside) the value cell.
    let mut st = filled_state();
    replace(&mut st, "<hp:t>성명</hp:t>", "<hp:t>이름</hp:t>");
    assert!(gate(&pre, &st, op(None, true, &values())).is_err());
    // 5. The value written into the below label cell instead of the right cell.
    let mut st = base_state(&form_body());
    replace(&mut st, "<hp:t></hp:t>", "<hp:t>홍길동</hp:t>");
    replace(&mut st, "<hp:t>소속</hp:t>", "<hp:t>홍길동</hp:t>");
    assert!(gate(&pre, &st, op(None, true, &values())).is_err());
    // 6. Label matching is exact: a spaced label does not resolve.
    assert!(gate(&pre, &st, op(None, true, &[("성 명", "홍길동")])).is_err());
}

#[test]
fn form_fill_gate_derives_the_non_strict_skip_set_itself() {
    // 성명's right cell already holds a value, so a non-strict request may only
    // fill 소속; the unknown label stays unresolved.
    let table = format!(
        r#"<hp:tbl rowCnt="2" colCnt="2"><hp:tr>{}{}</hp:tr><hp:tr>{}{}</hp:tr></hp:tbl>"#,
        tc(0, 0, "<hp:t>성명</hp:t>"),
        tc(0, 1, "<hp:t>이미값</hp:t>"),
        tc(1, 0, "<hp:t>소속</hp:t>"),
        tc(1, 1, "<hp:t></hp:t>"),
    );
    let body = p(&table);
    let pre = base_state(&body);
    let request = vec![
        ("성명", "홍길동"),
        ("소속", "○○고등학교"),
        ("없는라벨", "X"),
    ];
    let mut good = base_state(&body);
    replace(&mut good, "<hp:t></hp:t>", "<hp:t>○○고등학교</hp:t>");
    gate(&pre, &good, op(None, false, &request)).unwrap();
    // Filling the non-empty target in non-strict mode is still rejected.
    let mut bad = base_state(&body);
    replace(&mut bad, "<hp:t>이미값</hp:t>", "<hp:t>홍길동</hp:t>");
    replace(&mut bad, "<hp:t></hp:t>", "<hp:t>○○고등학교</hp:t>");
    assert!(gate(&pre, &bad, op(None, false, &request)).is_err());
    // Skipping a resolvable label is rejected (the gate derives the same set).
    let missing = base_state(&body);
    assert!(gate(&pre, &missing, op(None, false, &request)).is_err());
    // Strict mode rejects the whole request instead of skipping.
    let err = gate(&pre, &good, op(None, true, &request)).unwrap_err();
    assert_eq!(err.code(), Code::VerifyFailed);
}

#[test]
fn form_fill_gate_rejects_conflicting_targets() {
    // (1,1) is both the right cell of "하나" and the below cell of "둘".
    let table = format!(
        r#"<hp:tbl rowCnt="2" colCnt="2"><hp:tr>{}{}</hp:tr><hp:tr>{}{}</hp:tr></hp:tbl>"#,
        tc(0, 0, "<hp:t>머리</hp:t>"),
        tc(0, 1, "<hp:t>둘</hp:t>"),
        tc(1, 0, "<hp:t>하나</hp:t>"),
        tc(1, 1, "<hp:t></hp:t>"),
    );
    let pre = base_state(&p(&table));
    let request = vec![("하나", "V1"), ("둘", "V2")];
    // Non-strict skips both labels, so the unchanged candidate is correct.
    gate(&pre, &pre, op(None, false, &request)).unwrap();
    // Filling the shared cell (either label's view of it) is rejected.
    let mut bad = base_state(&p(&table));
    replace(&mut bad, "<hp:t></hp:t>", "<hp:t>V1</hp:t>");
    assert!(gate(&pre, &bad, op(None, false, &request)).is_err());
    // Strict mode rejects the request instead of skipping.
    let err = gate(&pre, &pre, op(None, true, &request)).unwrap_err();
    assert_eq!(err.code(), Code::VerifyFailed);
}

#[test]
fn form_fill_gate_rejects_wrong_scopes_and_unrequested_payloads() {
    let pre = base_state(&form_body());
    let candidate = filled_state();
    // A scope that is neither the section root nor a table.
    let request = op(
        Some(VerifyScope {
            entry: SECTION.into(),
            path: vec![0],
        }),
        true,
        &values(),
    );
    assert!(gate(&pre, &candidate, request).is_err());
    // A section outside the spine is refused.
    let request = op(
        Some(VerifyScope {
            entry: "Contents/section9.xml".into(),
            path: vec![],
        }),
        true,
        &values(),
    );
    assert!(gate(&pre, &candidate, request).is_err());
    // Any other payload change (here: a run shape reference) is rejected even
    // though both requested fills are present.
    let mut st = filled_state();
    let old = std::str::from_utf8(st.get(SECTION).unwrap())
        .unwrap()
        .to_string();
    let old = old.replace(
        "charPrIDRef=\"0\"><hp:t>홍길동",
        "charPrIDRef=\"3\"><hp:t>홍길동",
    );
    st.set(SECTION, Arc::from(old.into_bytes()));
    assert!(gate(&pre, &st, op(None, true, &values())).is_err());
}
