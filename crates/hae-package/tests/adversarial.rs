//! Task 1-2a fixtures: every adversarial case is accepted or rejected as expected
//! through the full open path (ZIP → XML → package relations).

use hae_diag::Limits;
use std::path::PathBuf;

/// Fixture reason names that map onto engine reason names.
fn aliases(r: &str) -> &'static [&'static str] {
    match r {
        "truncated" => &["malformed_zip"],
        "bad_zip_structure" => &["malformed_zip", "overlapping_entries"],
        "unsupported_method" => &["unsupported_compression"],
        "forbidden_char" => &["malformed_xml"],
        "header_mismatch" => &["header_mismatch", "size_mismatch"],
        "encrypted" => &["encrypted"],
        _ => &[],
    }
}

#[test]
fn adversarial_cases() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let exp: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("adversarial_expected.json")).unwrap())
            .unwrap();
    let cases = exp["cases"].as_object().unwrap();
    assert!(cases.len() >= 80);
    let mut failures = Vec::new();
    for (name, c) in cases {
        let bytes = std::fs::read(dir.join("adversarial").join(name)).unwrap();
        let got = hae_package::open(bytes, &Limits::default());
        let want_accept = c["expect"] == "accept";
        match (&got, want_accept) {
            (Ok(_), true) => {}
            (Err(e), false) => {
                let code = c["code"].as_str().unwrap();
                if e.code().as_str() != code {
                    failures.push(format!("{name}: code {} want {code} ({e})", e.code()));
                    continue;
                }
                if code == "PACKAGE_REJECTED" {
                    let want = c["reason"].as_str().unwrap();
                    let got_r = e.reason().unwrap_or("-");
                    let al = aliases(want);
                    if got_r != want && !al.contains(&got_r) {
                        failures.push(format!("{name}: reason {got_r} want {want} ({e})"));
                    }
                }
            }
            (Ok(_), false) => failures.push(format!("{name}: accepted, want reject")),
            (Err(e), true) => failures.push(format!("{name}: rejected ({e}), want accept")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
