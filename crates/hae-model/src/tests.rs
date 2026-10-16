use super::*;
use crate::query::Query;

fn corpus(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../tests/fixtures/corpus/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn open(name: &str) -> Document {
    Document::open(corpus(name), &Limits::default()).unwrap()
}

fn count(d: &Document, k: Kind) -> usize {
    d.nodes.iter().filter(|n| n.kind == k).count()
}

#[test]
fn counts_match_stage0_mapping() {
    // Stage-0 S2 mapping counted 308 cells and 8 tables in the 28-section doc.
    let d = open("research-school-plan-reference.hwpx");
    assert_eq!(count(&d, Kind::Section), 28);
    assert_eq!(count(&d, Kind::Table), 8);
    assert_eq!(count(&d, Kind::Cell), 308);
    assert!(d.permissions.editable());
}

#[test]
fn ids_unique_and_parents_consistent() {
    for f in [
        "bodojaryo-reference.hwpx",
        "gyehoek-reference.hwpx",
        "irb-sample.hwpx",
        "ilban-gian.hwpx",
    ] {
        let d = open(f);
        let mut ids = std::collections::HashSet::new();
        for n in &d.nodes {
            assert!(ids.insert(n.id.clone()));
            if n.kind != Kind::Section {
                let p = d.node(n.parent.as_ref().unwrap()).unwrap();
                assert!(d.xml[&n.entry].ancestors(n.el).any(|a| a == p.el), "{f}");
            }
        }
    }
}

#[test]
fn rebuild_unchanged_keeps_ids() {
    let d = open("bodojaryo-reference.hwpx");
    let d2 = d.rebuild(d.state.clone(), Vec::new()).unwrap();
    let a: Vec<_> = d.nodes.iter().map(|n| (&n.id, n.kind)).collect();
    let b: Vec<_> = d2.nodes.iter().map(|n| (&n.id, n.kind)).collect();
    assert_eq!(a, b);
    assert!(d2.is_unmodified());
    assert_eq!(d2.to_hwpx().unwrap(), corpus("bodojaryo-reference.hwpx"));
}

#[test]
fn rebuild_rejects_identity_change() {
    let d = open("bodojaryo-reference.hwpx");
    let mut st = d.state.clone();
    let s = &d.info.sections[0];
    let x = std::str::from_utf8(st.get(s).unwrap()).unwrap().to_string();
    // Remove the first table: identity sequence changes.
    let i = x.find("<hp:tbl").unwrap();
    let j = x[i..].find("</hp:tbl>").unwrap() + i + "</hp:tbl>".len();
    let y = format!("{}{}", &x[..i], &x[j..]);
    st.set(s, Arc::from(y.into_bytes()));
    assert_eq!(
        d.rebuild(st, Vec::new()).unwrap_err().code(),
        Code::StateInvariantError
    );
}

#[test]
fn map_rows_roundtrip_and_tamper() {
    let d = open("gyehoek-reference.hwpx");
    let rows = d.map_rows();
    let d2 = Document::open_with_map(
        d.document_id.clone(),
        d.original.clone(),
        d.state.clone(),
        &rows,
        &d.limits,
    )
    .unwrap();
    assert_eq!(
        d.nodes.iter().map(|n| &n.id).collect::<Vec<_>>(),
        d2.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    let mut bad = rows.clone();
    bad[3].id = bad[4].id.clone();
    assert_eq!(
        Document::open_with_map(
            d.document_id.clone(),
            d.original.clone(),
            d.state.clone(),
            &bad,
            &d.limits
        )
        .unwrap_err()
        .code(),
        Code::ProjectIntegrityError
    );
    let mut bad = rows.clone();
    bad[5].key.kind = Kind::Equation;
    assert!(Document::open_with_map(
        d.document_id.clone(),
        d.original.clone(),
        d.state.clone(),
        &bad,
        &d.limits
    )
    .is_err());
    let mut bad = rows;
    bad.pop();
    assert!(Document::open_with_map(
        d.document_id.clone(),
        d.original.clone(),
        d.state.clone(),
        &bad,
        &d.limits
    )
    .is_err());
}

#[test]
fn paragraph_text_and_query() {
    let d = open("bodojaryo-reference.hwpx");
    let paras: Vec<&Node> = d
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph)
        .collect();
    assert!(!paras.is_empty());
    let all: String = paras
        .iter()
        .map(|p| d.node_text(p))
        .collect::<Vec<_>>()
        .join("|");
    assert!(all.contains("보도"), "{}", &all[..200.min(all.len())]);
    let r = d
        .query(&Query {
            kind: Some(Kind::Paragraph),
            text_contains: Some("보도".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(!r.matches.is_empty());
    // Table cell lookup.
    let t = d.nodes.iter().find(|n| n.kind == Kind::Table).unwrap();
    let r = d
        .query(&Query {
            table: Some(t.id.clone()),
            row: Some(0),
            col: Some(0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(r.matches.len(), 1);
    assert_eq!(r.matches[0].cell.as_ref().unwrap().row, 0);
}

#[test]
fn grapheme_ranges_and_unaligned_search() {
    let x = "<hs:sec xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\"><hp:p><hp:run><hp:t>e\u{301}x👨\u{200d}👩</hp:t><hp:t>a<hp:tab/>b</hp:t></hp:run></hp:p></hs:sec>";
    let doc = hae_xml::XmlDoc::parse(x.as_bytes().to_vec()).unwrap();
    let p = doc.iter().find(|(_, e)| e.local == "p").unwrap().0;
    let pt = ParaText::build(&doc, p);
    assert_eq!(pt.text, "e\u{301}x👨\u{200d}👩a\tb");
    assert_eq!(pt.grapheme_len(), 6);
    assert_eq!(pt.cp_range(0..1), Some(0..2));
    let (m, un) = pt.find_all("e");
    assert!(m.is_empty());
    assert_eq!(un, 1);
    let (m, _) = pt.find_all("x");
    assert_eq!(m, vec![1..2]);
    assert!(pt
        .pieces
        .iter()
        .any(|p| matches!(&p.kind, PieceKind::Control { name, .. } if name == "tab")));
}

#[test]
fn inspect_pages() {
    let d = open("gyehoek-reference.hwpx");
    let (a, next) = d.inspect(0, 10);
    assert_eq!(a.len(), 10);
    let (b, _) = d.inspect(next.unwrap(), 10);
    assert_ne!(a[0].node_id, b[0].node_id);
    let j = serde_json::to_value(&a[0]).unwrap();
    assert_eq!(j["kind"], "section");
}

#[test]
fn hancom_uri_prefix_is_not_a_supported_namespace() {
    assert!(!is_known_ns(Some("http://www.hancom.co.kr/future/unknown")));
    assert!(is_known_ns(Some(NS_HP)));
    let doc = XmlDoc::parse(br#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:x="http://www.hancom.co.kr/future/unknown"><x:reference idref="1"/></hs:sec>"#.to_vec()).unwrap();
    assert_eq!(
        identity_elements(&doc)
            .iter()
            .filter(|(_, k)| *k == Kind::Opaque)
            .count(),
        1
    );
}

#[test]
fn duplicate_hwpx_ids_are_all_locked_by_opaque_reference() {
    let limits = Limits::default();
    let original = hae_zip::Archive::open(corpus("ref_empty.hwpx"), &limits).unwrap();
    let mut state = PackageState::from_archive(&original);
    let section=br#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:x="urn:opaque"><hp:p id="1"><hp:run><hp:t>first</hp:t></hp:run></hp:p><hp:p id="1"><hp:run><hp:t>second</hp:t></hp:run></hp:p><x:reference target="1"/></hs:sec>"#;
    state.set("Contents/section0.xml", Arc::from(section.to_vec()));
    let bytes = hae_zip::write(
        &original,
        &hae_package::zip_plan(&original, &state).unwrap(),
    )
    .unwrap();
    let d = Document::open(bytes, &limits).unwrap();
    let paragraphs: Vec<_> = d
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph)
        .collect();
    assert_eq!(paragraphs.len(), 2);
    assert!(paragraphs.iter().all(|n| d.check_editable(n).is_err()));
}
