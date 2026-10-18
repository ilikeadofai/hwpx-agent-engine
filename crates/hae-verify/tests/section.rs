//! Handwritten candidates: never call hae-edit/model to derive expected bytes.
use hae_diag::{Code, Limits};
use hae_package::PackageState;
use hae_verify::{verify_archive, PageNumbering, PageSettings, VerifyOperation, Visibility};
use hae_xml::XmlDoc;
use hae_zip::{Archive, OutEntry};
use std::sync::Arc;
const SECTION: &str = "Contents/section0.xml";
fn original() -> Archive {
    Archive::open(
        include_bytes!("../../../tests/fixtures/editing/section.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap()
}
fn request() -> VerifyOperation {
    VerifyOperation::SectionSetPage {
        entry: SECTION.into(),
        path: vec![0],
        settings: PageSettings {
            width: Some(61000),
            ..Default::default()
        },
    }
}
fn replace(st: &mut PackageState, name: &str, from: &str, to: &str) {
    let old = std::str::from_utf8(st.get(name).unwrap()).unwrap();
    assert!(old.contains(from), "{from}");
    st.set(name, Arc::from(old.replacen(from, to, 1).into_bytes()));
}
fn remove_caches(st: &mut PackageState) {
    let doc = XmlDoc::parse(st.get(SECTION).unwrap().clone()).unwrap();
    let mut bytes = doc.bytes().to_vec();
    let mut spans: Vec<_> = doc
        .iter()
        .filter(|(_, e)| e.local == "linesegarray")
        .map(|(_, e)| e.outer())
        .collect();
    spans.sort_by_key(|r| std::cmp::Reverse(r.start));
    for span in spans {
        bytes.drain(span);
    }
    st.set(SECTION, Arc::from(bytes));
}
fn candidate() -> PackageState {
    let mut st = PackageState::from_archive(&original());
    replace(&mut st, SECTION, "width=\"59528\"", "width=\"61000\"");
    remove_caches(&mut st);
    st
}
fn bytes(st: &PackageState) -> Vec<u8> {
    let a = original();
    hae_zip::write(&a, &hae_package::zip_plan(&a, st).unwrap()).unwrap()
}
fn gate(st: &PackageState, ops: &[VerifyOperation]) -> hae_diag::Result<hae_verify::VerifyReport> {
    verify_archive(&original(), &bytes(st), ops, &Limits::default())
}
#[test]
fn section_independent_gate_accepts_page_candidate_and_caches() {
    let r = gate(&candidate(), &[request()]).unwrap();
    assert_eq!(r.cache_removals[SECTION], 2);
    assert_eq!(r.changed_entries, [SECTION]);
    assert!(!r.prv_text_stale);
}
#[test]
fn section_gate_rejects_unrequested_margin_change() {
    let mut st = candidate();
    replace(&mut st, SECTION, "left=\"8504\"", "left=\"8505\"");
    assert_eq!(
        gate(&st, &[request()]).unwrap_err().code(),
        Code::VerifyFailed
    );
}
#[test]
fn section_gate_rejects_unrequested_paper_change() {
    let mut st = candidate();
    replace(&mut st, SECTION, "height=\"84186\"", "height=\"84187\"");
    assert!(gate(&st, &[request()]).is_err());
}
#[test]
fn section_gate_rejects_other_section_change() {
    let mut st = candidate();
    replace(
        &mut st,
        "Contents/section1.xml",
        "width=\"59528\"",
        "width=\"61000\"",
    );
    assert!(gate(&st, &[request()]).is_err());
}
#[test]
fn section_gate_rejects_child_element_reordering() {
    let mut st = candidate();
    replace(&mut st,SECTION,"<hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/><hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/>","<hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/><hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/>");
    assert!(gate(&st, &[request()]).is_err());
}
#[test]
fn section_gate_rejects_unedited_zip_reencoding() {
    let a = original();
    let mut output = hae_package::zip_plan(&a, &candidate()).unwrap();
    for entry in &mut output {
        if matches!(entry,OutEntry::Keep(n) if n=="Contents/header.xml") {
            *entry = OutEntry::Replace(
                "Contents/header.xml".into(),
                a.get("Contents/header.xml").unwrap().payload().clone(),
            );
        }
    }
    let bytes = hae_zip::write(&a, &output).unwrap();
    assert!(verify_archive(&a, &bytes, &[request()], &Limits::default()).is_err());
}
#[test]
fn section_gate_rejects_unrequested_start_number_change() {
    let mut st = candidate();
    replace(&mut st, SECTION, "page=\"0\"", "page=\"5\"");
    assert!(gate(&st, &[request()]).is_err());
}
#[test]
fn section_gate_rejects_opaque_entity_respelling_and_body_text_change() {
    for (from, to) in [
        ("Section zero synthetic text.", "Damaged body."),
        ("tabStop=\"8000\"", "tabStop=\"&#56;000\""),
    ] {
        let mut st = candidate();
        replace(&mut st, SECTION, from, to);
        assert!(gate(&st, &[request()]).is_err());
    }
}
#[test]
fn section_gate_rejects_missing_or_other_section_cache_removal() {
    let mut st = PackageState::from_archive(&original());
    replace(&mut st, SECTION, "width=\"59528\"", "width=\"61000\"");
    assert!(gate(&st, &[request()]).is_err());
    let mut st = candidate();
    replace(&mut st, "Contents/section1.xml", "<hp:linesegarray/>", "");
    assert!(gate(&st, &[request()]).is_err());
}
#[test]
fn section_numbering_independent_acceptance_preserves_other_counters() {
    let mut st = PackageState::from_archive(&original());
    replace(
        &mut st,
        SECTION,
        "pageStartsOn=\"BOTH\" page=\"0\"",
        "pageStartsOn=\"ODD\" page=\"5\"",
    );
    remove_caches(&mut st);
    let op = VerifyOperation::SectionSetPageNumbering {
        entry: SECTION.into(),
        path: vec![0],
        settings: PageNumbering {
            page: Some(5),
            page_starts_on: Some("ODD".into()),
        },
    };
    gate(&st, std::slice::from_ref(&op)).unwrap();
    for key in ["pic", "tbl", "equation"] {
        let mut bad = st.clone();
        replace(
            &mut bad,
            SECTION,
            &format!("{key}=\"0\""),
            &format!("{key}=\"1\""),
        );
        assert!(gate(&bad, std::slice::from_ref(&op)).is_err());
    }
}
#[test]
fn section_visibility_independent_acceptance_and_unrequested_flag_rejection() {
    let mut st = PackageState::from_archive(&original());
    replace(
        &mut st,
        SECTION,
        "hideFirstHeader=\"0\"",
        "hideFirstHeader=\"1\"",
    );
    remove_caches(&mut st);
    let op = VerifyOperation::SectionSetVisibility {
        entry: SECTION.into(),
        path: vec![0],
        settings: Visibility {
            hide_first_header: Some(true),
            ..Default::default()
        },
    };
    gate(&st, std::slice::from_ref(&op)).unwrap();
    replace(
        &mut st,
        SECTION,
        "hideFirstMasterPage=\"0\"",
        "hideFirstMasterPage=\"1\"",
    );
    assert!(gate(&st, &[op]).is_err());
}
#[test]
fn section_gate_rejects_invalid_requests_even_if_candidate_is_unchanged() {
    let st = PackageState::from_archive(&original());
    for value in [
        serde_json::json!({"width":0}),
        serde_json::json!({"landscape":"INVALID"}),
        serde_json::json!({"margins":{"left":100000}}),
        serde_json::json!({}),
    ] {
        let op = VerifyOperation::SectionSetPage {
            entry: SECTION.into(),
            path: vec![0],
            settings: serde_json::from_value(value).unwrap(),
        };
        assert!(gate(&st, &[op]).is_err());
    }
}
#[test]
fn section_gate_rejects_non_owner_paragraph_and_root_target() {
    for path in [vec![1], vec![]] {
        let op = VerifyOperation::SectionSetPage {
            entry: SECTION.into(),
            path,
            settings: PageSettings {
                width: Some(61000),
                ..Default::default()
            },
        };
        assert!(gate(&candidate(), &[op]).is_err());
    }
}
#[test]
fn section_gate_accepts_ordered_batch_and_rejects_preview_mutation() {
    let mut st = candidate();
    replace(&mut st, SECTION, "page=\"0\"", "page=\"5\"");
    replace(
        &mut st,
        SECTION,
        "hideFirstFooter=\"0\"",
        "hideFirstFooter=\"1\"",
    );
    let ops = vec![
        request(),
        VerifyOperation::SectionSetPageNumbering {
            entry: SECTION.into(),
            path: vec![0],
            settings: PageNumbering {
                page: Some(5),
                ..Default::default()
            },
        },
        VerifyOperation::SectionSetVisibility {
            entry: SECTION.into(),
            path: vec![0],
            settings: Visibility {
                hide_first_footer: Some(true),
                ..Default::default()
            },
        },
    ];
    gate(&st, &ops).unwrap();
    st.set("Preview/PrvText.txt", Arc::from(&b"damaged"[..]));
    assert!(gate(&st, &ops).is_err());
}
