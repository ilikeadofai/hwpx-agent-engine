//! text.replaceAll gate: hand-written expected candidates and one-defect mutants.
//! Nothing here uses hae-edit or hae-model; the gate must derive the match set.
use hae_diag::Limits;
use hae_package::{Entry, Origin, PackageState, NS_HC, NS_HH, NS_HP, NS_HS};
use hae_verify::{verify_candidate, VerifyOperation, VerifyScope};
use hae_zip::{Archive, OutEntry};
use std::sync::Arc;

const SECTION: &str = "Contents/section0.xml";
const HEADER: &str = "Contents/header.xml";
const HPF: &str = "Contents/content.hpf";
const CACHE: &str = "<hp:linesegarray><hp:lineseg textpos='0'/></hp:linesegarray>";

fn fixture(body: &str) -> PackageState {
    let section = format!("<?xml version='1.0' encoding='UTF-8'?>\n<hs:sec xmlns:hs='{NS_HS}' xmlns:hp='{NS_HP}' xmlns:hc='{NS_HC}' xmlns:x='urn:opaque'>{body}</hs:sec>");
    let header = format!("<hh:head xmlns:hh='{NS_HH}'><hh:paraProperties itemCnt='1'><hh:paraPr id='0'><hh:align horizontal='LEFT' vertical='BASELINE'/><hh:lineSpacing type='PERCENT' value='160'/></hh:paraPr></hh:paraProperties><hh:charProperties><hh:charPr id='0'/><hh:charPr id='1'/></hh:charProperties></hh:head>");
    let hpf = "<opf:package xmlns:opf='http://www.idpf.org/2007/opf/'><opf:manifest><opf:item id='header' href='header.xml' media-type='application/xml'/><opf:item id='sec' href='section0.xml' media-type='application/xml'/></opf:manifest><opf:spine><opf:itemref idref='header'/><opf:itemref idref='sec'/></opf:spine></opf:package>";
    let mut state = PackageState {
        entries: Vec::new(),
    };
    for (name, bytes) in [
        ("mimetype", hae_package::MIMETYPE.as_bytes().to_vec()),
        ("META-INF/container.xml", b"<container xmlns='urn:oasis:names:tc:opendocument:xmlns:container'><rootfiles><rootfile full-path='Contents/content.hpf'/></rootfiles></container>".to_vec()),
        (HPF, hpf.as_bytes().to_vec()), (HEADER, header.into_bytes()), (SECTION, section.into_bytes()),
        ("Preview/PrvText.txt", b"cat dog cat\r\ntail cat\r\nzzz".to_vec()),
        ("Preview/PrvImage.png", b"thumbnail".to_vec()),
    ] { state.entries.push(Entry { name: name.into(), payload: Arc::from(bytes), origin: Origin::Original }); }
    state
}
fn zip(state: &PackageState) -> Vec<u8> {
    let empty = Archive::open(
        vec![
            0x50, 0x4b, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        &Limits::default(),
    )
    .unwrap();
    let plan: Vec<_> = state
        .entries
        .iter()
        .map(|e| OutEntry::Add {
            name: e.name.clone(),
            payload: e.payload.clone(),
            method: 0,
        })
        .collect();
    hae_zip::write(&empty, &plan).unwrap()
}
fn edit(state: &mut PackageState, name: &str, old: &str, new: &str) {
    let s = std::str::from_utf8(state.get(name).unwrap()).unwrap();
    assert!(s.contains(old), "fixture substitution missing: {old}");
    let out = s.replacen(old, new, 1);
    state.set(name, Arc::from(out.into_bytes()));
}
fn gate(
    original: &PackageState,
    candidate: &PackageState,
    ops: &[VerifyOperation],
) -> hae_diag::Result<hae_verify::VerifyReport> {
    verify_candidate(original, &zip(candidate), ops, &Limits::default())
}
fn all(find: &str, replacement: &str, max: usize) -> VerifyOperation {
    VerifyOperation::TextReplaceAll {
        find: find.into(),
        replacement: replacement.into(),
        scope: None,
        max_matches: max,
    }
}
fn scoped(find: &str, replacement: &str, path: Vec<usize>) -> VerifyOperation {
    VerifyOperation::TextReplaceAll {
        find: find.into(),
        replacement: replacement.into(),
        scope: Some(VerifyScope {
            entry: SECTION.into(),
            path,
        }),
        max_matches: 1000,
    }
}
fn p(attrs: &str, inner: &str) -> String {
    format!("<hp:p {attrs} paraPrIDRef='0'><hp:run charPrIDRef='0'>{inner}</hp:run>{CACHE}</hp:p>")
}
fn document() -> PackageState {
    let cell = format!(
        "<hp:tbl><hp:tr><hp:tc><hp:subList>{}</hp:subList></hp:tc></hp:tr></hp:tbl>",
        p("id='c'", "<hp:t>cat in cell</hp:t>")
    );
    let body = [
        p("id='p0'", "<hp:t>cat dog cat</hp:t>"),
        format!("<hp:p id='p1' paraPrIDRef='0'><hp:run charPrIDRef='0'><hp:t>xca</hp:t></hp:run><hp:run charPrIDRef='1'><hp:t>t y &amp;cat</hp:t></hp:run>{CACHE}</hp:p>"),
        p("id='p2'", "<hp:t>cat<![CDATA[x]]>cat</hp:t>"),
        p("id='p3'", "<hp:t>cat</hp:t><hp:equation id='E'/><hp:t>cat</hp:t>"),
        format!("<hp:p id='p4' paraPrIDRef='0'><hp:run charPrIDRef='0'>{cell}</hp:run>{CACHE}</hp:p>"),
        p("id='p5'", "<hp:t>cat&#x301; cat</hp:t>"),
        p("id='ref'", "<hp:t>cat</hp:t>") + "<x:ref to='ref'/>",
        p("id='p7'", "<hp:t>tail cat</hp:t>"),
    ]
    .concat();
    fixture(&body)
}
/// Hand-written expectation for find="cat", text="lion" over the whole document.
fn expected_all() -> PackageState {
    let mut new = document();
    edit(&mut new, SECTION, "cat dog cat", "lion dog lion");
    edit(&mut new, SECTION, "<hp:t>xca</hp:t>", "<hp:t>xlion</hp:t>");
    edit(
        &mut new,
        SECTION,
        "<hp:t>t y &amp;cat</hp:t>",
        "<hp:t> y &amp;lion</hp:t>",
    );
    edit(
        &mut new,
        SECTION,
        "<hp:t>cat<![CDATA[x]]>cat</hp:t>",
        "<hp:t>lion<![CDATA[x]]>lion</hp:t>",
    );
    edit(
        &mut new,
        SECTION,
        "<hp:t>cat</hp:t><hp:equation id='E'/><hp:t>cat</hp:t>",
        "<hp:t>lion</hp:t><hp:equation id='E'/><hp:t>lion</hp:t>",
    );
    edit(&mut new, SECTION, "cat in cell", "lion in cell");
    edit(&mut new, SECTION, "cat&#x301; cat", "cat&#x301; lion");
    edit(&mut new, SECTION, "tail cat", "tail lion");
    let s = std::str::from_utf8(new.get(SECTION).unwrap())
        .unwrap()
        .replace(CACHE, "");
    new.set(SECTION, Arc::from(s.into_bytes()));
    new.set(
        "Preview/PrvText.txt",
        Arc::from(b"lion dog lion\r\ntail lion\r\nzzz".to_vec()),
    );
    new
}

#[test]
fn independently_derived_replacement_set_is_accepted() {
    let r = gate(&document(), &expected_all(), &[all("cat", "lion", 11)]).unwrap();
    assert_eq!(r.cache_removals.get(SECTION), Some(&9));
    assert!(r.prv_text_stale && r.thumbnail_stale);
}

#[test]
fn match_budget_is_enforced_by_the_gate() {
    assert!(gate(&document(), &expected_all(), &[all("cat", "lion", 10)]).is_err());
}

#[test]
fn every_single_defect_candidate_is_rejected() {
    let ops = [all("cat", "lion", 1000)];
    let good = expected_all();
    let defects: &[(&str, &str, &str)] = &[
        (SECTION, "lion dog lion", "lion dog cat"),
        (SECTION, "lion dog lion", "cat dog lion"),
        (SECTION, "cat&#x301; lion", "lion&#x301; lion"),
        (
            SECTION,
            "<hp:t>cat</hp:t></hp:run></hp:p><x:ref",
            "<hp:t>lion</hp:t></hp:run></hp:p><x:ref",
        ),
        (SECTION, "tail lion", "tail Lion"),
        (SECTION, "<hp:equation id='E'/>", ""),
        (SECTION, "<![CDATA[x]]>", "<![CDATA[y]]>"),
        (SECTION, "lion in cell", "cat in cell"),
        (SECTION, "<hp:t>xlion</hp:t>", "<hp:t>x</hp:t>"),
        (
            SECTION,
            "<hp:t> y &amp;lion</hp:t>",
            "<hp:t>lion y &amp;lion</hp:t>",
        ),
        (
            SECTION,
            "tail lion</hp:t></hp:run>",
            &format!("tail lion</hp:t></hp:run>{CACHE}"),
        ),
        (SECTION, "&amp;lion", "&lion"),
        ("Preview/PrvText.txt", "tail lion", "tail cat"),
        ("Preview/PrvImage.png", "thumbnail", "changed"),
    ];
    for (entry, from, to) in defects {
        let mut bad = good.clone();
        edit(&mut bad, entry, from, to);
        assert!(
            gate(&document(), &bad, &ops).is_err(),
            "accepted defect {from:?} -> {to:?}"
        );
    }
    // The unedited document, i.e. every match omitted, is not acceptable either.
    assert!(gate(&document(), &document(), &ops).is_err());
}

#[test]
fn scope_limits_expected_matches_and_cache_anchor() {
    let old = document();
    // Only the paragraph at path [0]; caches are still cleared from there on.
    let mut new = old.clone();
    edit(&mut new, SECTION, "cat dog cat", "lion dog lion");
    let s = std::str::from_utf8(new.get(SECTION).unwrap())
        .unwrap()
        .replace(CACHE, "");
    new.set(SECTION, Arc::from(s.into_bytes()));
    // The unedited nested paragraph keeps its own cache: only top-level ones go.
    edit(
        &mut new,
        SECTION,
        "<hp:t>cat in cell</hp:t></hp:run>",
        &format!("<hp:t>cat in cell</hp:t></hp:run>{CACHE}"),
    );
    new.set(
        "Preview/PrvText.txt",
        Arc::from(b"lion dog lion\r\ntail cat\r\nzzz".to_vec()),
    );
    let op = [scoped("cat", "lion", vec![0])];
    gate(&old, &new, &op).unwrap();
    let mut wide = new.clone();
    edit(&mut wide, SECTION, "tail cat", "tail lion");
    assert!(gate(&old, &wide, &op).is_err());
    // A table cell scope reaches only its paragraph and keeps earlier caches.
    let mut cell = old.clone();
    edit(&mut cell, SECTION, "cat in cell", "lion in cell");
    let s = std::str::from_utf8(cell.get(SECTION).unwrap())
        .unwrap()
        .to_string();
    let at = s.find("<hp:p id='p4'").unwrap();
    cell.set(
        SECTION,
        Arc::from(format!("{}{}", &s[..at], s[at..].replace(CACHE, "")).into_bytes()),
    );
    let op = [scoped("cat", "lion", vec![4, 0, 0, 0, 0])];
    gate(&old, &cell, &op).unwrap();
    let mut kept = cell.clone();
    edit(
        &mut kept,
        SECTION,
        "id='p7' paraPrIDRef='0'><hp:run charPrIDRef='0'><hp:t>tail cat</hp:t></hp:run>",
        &format!(
            "id='p7' paraPrIDRef='0'><hp:run charPrIDRef='0'><hp:t>tail cat</hp:t></hp:run>{CACHE}"
        ),
    );
    assert!(gate(&old, &kept, &op).is_err());
}

#[test]
fn no_match_and_identical_replacement_require_the_original_bytes() {
    let old = document();
    for op in [all("zzzz", "x", 5), all("cat", "cat", 5)] {
        gate(&old, &old, std::slice::from_ref(&op)).unwrap();
        let mut moved = old.clone();
        edit(&mut moved, SECTION, "dog", "DOG");
        assert!(gate(&old, &moved, &[op]).is_err());
    }
    assert!(gate(&old, &old, &[all("", "x", 5)]).is_err());
    assert!(gate(&old, &old, &[all("cat", "a\tb", 5)]).is_err());
}

#[test]
fn replace_all_composes_with_other_steps_in_order() {
    let old = document();
    let mut new = expected_all();
    edit(&mut new, SECTION, "lion dog lion", "L dog lion");
    new.set(
        "Preview/PrvText.txt",
        Arc::from(b"L dog lion\r\ntail lion\r\nzzz".to_vec()),
    );
    let ops = [
        all("cat", "lion", 1000),
        VerifyOperation::TextReplace {
            entry: SECTION.into(),
            path: vec![0],
            range: 0..4,
            replacement: "L".into(),
            preserve: false,
        },
    ];
    gate(&old, &new, &ops).unwrap();
}
