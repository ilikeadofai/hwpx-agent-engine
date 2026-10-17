//! Hand-written candidate edits. No editor/model code constructs expectations.
use hae_diag::{Code, Limits};
use hae_package::{Entry, Origin, PackageState, NS_HC, NS_HH, NS_HP, NS_HS};
use hae_verify::{verify_archive, verify_candidate, Alignment, VerifyOperation};
use hae_zip::{Archive, OutEntry};
use std::sync::Arc;

const SECTION: &str = "Contents/section0.xml";
const HEADER: &str = "Contents/header.xml";
const HPF: &str = "Contents/content.hpf";
const CACHE: &str = "<hp:linesegarray><hp:lineseg textpos='0'/></hp:linesegarray>";

fn fixture(body: &str) -> PackageState {
    let section = format!("\u{feff}<?xml version='1.0' encoding='UTF-8'?>\r\n<hs:sec xmlns:hs='{NS_HS}' xmlns:hp='{NS_HP}' xmlns:hc='{NS_HC}' xmlns:x='urn:opaque'>{body}</hs:sec>");
    let header = format!("<hh:head xmlns:hh='{NS_HH}' xmlns:x='urn:opaque'><hh:paraProperties itemCnt='1'><hh:paraPr id='0' x:secret='&#65;'><hh:align horizontal='LEFT' vertical='BASELINE'/><hh:lineSpacing type='PERCENT' value='160'/><x:secret a='2'/></hh:paraPr></hh:paraProperties><hh:charProperties><hh:charPr id='0'/><hh:charPr id='1'/></hh:charProperties></hh:head>");
    let hpf = "<opf:package xmlns:opf='http://www.idpf.org/2007/opf/'><opf:manifest><opf:item id='header' href='header.xml' media-type='application/xml'/><opf:item id='sec' href='section0.xml' media-type='application/xml'/><opf:item id='old' href='BinData/old.png' media-type='image/png'/></opf:manifest><opf:spine><opf:itemref idref='header'/><opf:itemref idref='sec'/></opf:spine></opf:package>";
    let mut state = PackageState {
        entries: Vec::new(),
    };
    for (name, bytes) in [
        ("mimetype", hae_package::MIMETYPE.as_bytes().to_vec()),
        ("META-INF/container.xml", b"<container xmlns='urn:oasis:names:tc:opendocument:xmlns:container'><rootfiles><rootfile full-path='Contents/content.hpf'/></rootfiles></container>".to_vec()),
        (HPF, hpf.as_bytes().to_vec()), (HEADER, header.into_bytes()), (SECTION, section.into_bytes()),
        ("BinData/old.png", b"original shared media".to_vec()),
        ("unused.bin", b"must survive".to_vec()),
        ("Preview/PrvText.txt", b"A&BC\r\ntail".to_vec()),
        ("Preview/PrvImage.png", b"thumbnail".to_vec()),
    ] { state.entries.push(Entry { name: name.into(), payload: Arc::from(bytes), origin: Origin::Original }); }
    state
}
fn paragraph(s: &str) -> String {
    format!("<hp:p id='p' paraPrIDRef='0'><hp:run charPrIDRef='0'><hp:t>{s}</hp:t></hp:run>{CACHE}</hp:p>")
}
fn split_fixture() -> PackageState {
    fixture(&format!("{}<hp:p id='p1' paraPrIDRef='0' x:keep='&#65;'><hp:run charPrIDRef='0' x:keep='yes'><hp:t>A&amp;B</hp:t></hp:run><hp:run charPrIDRef='1'><hp:t>C</hp:t></hp:run>{CACHE}<x:keep v='&#x41;'/></hp:p>{}", paragraph("before"), paragraph("tail")))
}
fn replace(state: &mut PackageState, name: &str, old: &str, new: &str) {
    let s = std::str::from_utf8(state.get(name).unwrap()).unwrap();
    assert!(s.contains(old), "fixture substitution missing: {old}");
    let out = s.replacen(old, new, 1);
    state.set(name, Arc::from(out.into_bytes()));
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
fn text_op(path: Vec<usize>, range: std::ops::Range<usize>, replacement: &str) -> VerifyOperation {
    VerifyOperation::TextReplace {
        entry: SECTION.into(),
        path,
        range,
        replacement: replacement.into(),
        preserve: false,
    }
}
fn gate(
    original: &PackageState,
    candidate: &PackageState,
    ops: &[VerifyOperation],
) -> hae_diag::Result<hae_verify::VerifyReport> {
    verify_candidate(original, &zip(candidate), ops, &Limits::default())
}
fn good_split() -> (PackageState, PackageState, Vec<VerifyOperation>) {
    let old = split_fixture();
    let mut new = old.clone();
    replace(&mut new, SECTION, "A&amp;B", "AX");
    replace(&mut new, SECTION, "<hp:t>C</hp:t>", "<hp:t></hp:t>");
    // Earlier top-level cache is preserved; tail caches are removed.
    let s = std::str::from_utf8(new.get(SECTION).unwrap()).unwrap();
    let at = s.find("<hp:p id='p1'").unwrap();
    let out = format!("{}{}", &s[..at], s[at..].replace(CACHE, ""));
    new.set(SECTION, Arc::from(out.into_bytes()));
    replace(&mut new, "Preview/PrvText.txt", "A&BC", "AX");
    (old, new, vec![text_op(vec![1], 1..4, "X")])
}

#[test]
fn split_runs_refs_and_opaque_data_are_preserved() {
    let (old, new, ops) = good_split();
    let r = gate(&old, &new, &ops).unwrap();
    assert_eq!(r.cache_removals.get(SECTION), Some(&2));
    assert!(r.thumbnail_stale);
    assert!(!r.prv_text_stale);
}
#[test]
fn adversarial_out_of_target_and_opaque_mutations_fail() {
    let (old, good, ops) = good_split();
    for (from, to) in [
        ("before", "damage"),
        ("charPrIDRef='1'", "charPrIDRef='0'"),
        ("x:keep='yes'", "x:keep='no'"),
        ("v='&#x41;'", "v='A'"),
        ("<hp:t></hp:t>", "<hp:t>hidden</hp:t>"),
    ] {
        let mut bad = good.clone();
        replace(&mut bad, SECTION, from, to);
        assert_eq!(
            gate(&old, &bad, &ops).unwrap_err().code(),
            Code::VerifyFailed,
            "{from}"
        );
    }
}
#[test]
fn deletion_extra_entries_media_and_thumbnail_mutations_fail() {
    let (old, good, ops) = good_split();
    let mut bad = good.clone();
    bad.entries.retain(|e| e.name != "unused.bin");
    assert!(gate(&old, &bad, &ops).is_err());
    for name in [
        "unused.bin",
        "BinData/old.png",
        "Preview/PrvImage.png",
        "Preview/PrvText.txt",
    ] {
        let mut bad = good.clone();
        bad.set(name, Arc::from(b"tampered".to_vec()));
        assert!(gate(&old, &bad, &ops).is_err(), "{name}");
    }
    let mut bad = good;
    bad.add("extra.bin".into(), Arc::from(b"extra".to_vec()), 0);
    assert!(gate(&old, &bad, &ops).is_err());
}
#[test]
fn malformed_candidate_and_spoofed_expected_text_fail() {
    let (old, good, ops) = good_split();
    assert_eq!(
        verify_candidate(&old, b"bad zip", &ops, &Limits::default())
            .unwrap_err()
            .code(),
        Code::VerifyFailed
    );
    let mut bad = good;
    replace(&mut bad, SECTION, "</hs:sec>", "</wrong>");
    assert!(gate(&old, &bad, &ops).is_err());
    assert!(serde_json::from_str::<VerifyOperation>(r#"{"op":"text.replace","entry":"x","path":[],"range":{"start":0,"end":1},"replacement":"x","expectedText":"anything"}"#).is_err());
}
#[test]
fn cache_tail_is_required_but_earlier_and_unrelated_nested_caches_survive() {
    let (old, good, ops) = good_split();
    let mut bad = good.clone();
    replace(&mut bad, SECTION, "<x:keep", &format!("{CACHE}<x:keep"));
    assert!(gate(&old, &bad, &ops).is_err());
    let mut bad = good;
    replace(&mut bad, SECTION, CACHE, "");
    assert!(gate(&old, &bad, &ops).is_err());
}
#[test]
fn controls_opaque_text_attrs_invalid_ranges_and_cdata_fail_closed() {
    for text in ["A<hp:tab/>B", "A<![CDATA[B]]>C", "A<x:field/>B"] {
        let old = fixture(&paragraph(text));
        assert!(
            gate(&old, &old, &[text_op(vec![0], 0..3, "X")]).is_err(),
            "{text}"
        );
    }
    let mut old = fixture(&paragraph("ABC"));
    replace(&mut old, SECTION, "<hp:t>", "<hp:t x:charPrIDRef='0'>");
    assert!(gate(&old, &old, &[text_op(vec![0], 0..1, "X")]).is_err());
    let old = fixture(&paragraph("ABC"));
    #[allow(clippy::reversed_empty_ranges)]
    for range in [0..0, 2..1, 0..4] {
        assert!(gate(&old, &old, &[text_op(vec![0], range, "X")]).is_err());
    }
    assert!(gate(&old, &old, &[text_op(vec![0], 0..1, "\n")]).is_err());
}
#[test]
fn grapheme_spanning_runs_and_refs_is_one_range() {
    let old=fixture(&format!("<hp:p paraPrIDRef='0'><hp:run charPrIDRef='0'><hp:t>e</hp:t></hp:run><hp:run charPrIDRef='1'><hp:t>&#x301;Z</hp:t></hp:run>{CACHE}</hp:p>"));
    let mut new = old.clone();
    replace(&mut new, SECTION, ">e</hp:t>", ">Q</hp:t>");
    replace(&mut new, SECTION, "&#x301;Z", "Z");
    replace(&mut new, SECTION, CACHE, "");
    assert!(gate(&old, &new, &[text_op(vec![0], 0..1, "Q")]).is_ok());
}
#[test]
fn empty_cell_inserts_into_first_run_and_nested_cache_only() {
    let cell=format!("<hp:tbl><hp:tr><hp:tc><hp:cellSpan colSpan='1' rowSpan='1'/><hp:subList>{}</hp:subList></hp:tc></hp:tr></hp:tbl>",paragraph(""));
    let old = fixture(&format!(
        "<hp:p paraPrIDRef='0'><hp:run charPrIDRef='0'>{cell}</hp:run>{CACHE}</hp:p>{}",
        paragraph("tail")
    ));
    let mut new = old.clone();
    replace(&mut new, SECTION, "<hp:t></hp:t>", "<hp:t>cell</hp:t>");
    let s = String::from_utf8(new.get(SECTION).unwrap().to_vec())
        .unwrap()
        .replace(CACHE, "");
    new.set(SECTION, Arc::from(s.into_bytes()));
    let ops = [VerifyOperation::CellSetText {
        entry: SECTION.into(),
        path: vec![0, 0, 0, 0, 0],
        text: "cell".into(),
        preserve: false,
    }];
    let report = gate(&old, &new, &ops).unwrap();
    assert!(report.prv_text_stale);
    let mut bad = new;
    replace(&mut bad, SECTION, "<hp:t>cell</hp:t>", "<hp:t>wrong</hp:t>");
    assert!(gate(&old, &bad, &ops).is_err());
}
#[test]
fn format_clones_opaque_definition_without_modifying_shared_source() {
    let old = fixture(&format!(
        "{}{}",
        paragraph("A&AMP;B").replace("&AMP;", "&amp;"),
        paragraph("tail")
    ));
    let mut new = old.clone();
    replace(&mut new, SECTION, "paraPrIDRef='0'", "paraPrIDRef='1'");
    let source="<hh:paraPr id='0' x:secret='&#65;'><hh:align horizontal='LEFT' vertical='BASELINE'/><hh:lineSpacing type='PERCENT' value='160'/><x:secret a='2'/></hh:paraPr>";
    let clone = source
        .replace("id='0'", "id='1'")
        .replace("horizontal='LEFT'", "horizontal='CENTER'");
    replace(
        &mut new,
        HEADER,
        "</hh:paraProperties>",
        &format!("{clone}</hh:paraProperties>"),
    );
    replace(&mut new, HEADER, "itemCnt='1'", "itemCnt='2'");
    let s = String::from_utf8(new.get(SECTION).unwrap().to_vec())
        .unwrap()
        .replace(CACHE, "");
    new.set(SECTION, Arc::from(s.into_bytes()));
    let ops = [VerifyOperation::ParagraphFormat {
        entry: SECTION.into(),
        path: vec![0],
        alignment: Some(Alignment::Center),
        line_spacing: None,
    }];
    gate(&old, &new, &ops).unwrap();
    for (from, to) in [
        ("a='2'", "a='3'"),
        ("x:secret='&#65;'", "x:secret='A'"),
        ("vertical='BASELINE'", "vertical='TOP'"),
    ] {
        let mut bad = new.clone();
        replace(&mut bad, HEADER, from, to);
        assert!(gate(&old, &bad, &ops).is_err());
    }
}
#[test]
fn no_requests_require_identical_payloads() {
    let old = split_fixture();
    gate(&old, &old, &[]).unwrap();
    let mut new = old.clone();
    replace(&mut new, SECTION, "before", "after");
    assert!(gate(&old, &new, &[]).is_err());
}

fn png() -> Vec<u8> {
    // Complete 1x1 static PNG (not generated by the verifier).
    let hex="89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000b49444154789c636000020000050001a5f645400000000049454e44ae426082";
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
fn good_image() -> (PackageState, PackageState, Vec<VerifyOperation>) {
    let picture="<hp:pic id='pic' x:opaque='keep'><hp:sz width='100' height='200'/><hc:img binaryItemIDRef='old' bright='0' contrast='0' effect='REAL_PIC'/></hp:pic>";
    let old = fixture(&format!(
        "<hp:p><hp:run>{picture}{picture}</hp:run>{CACHE}</hp:p>"
    ));
    let mut new = old.clone();
    replace(
        &mut new,
        SECTION,
        "binaryItemIDRef='old'",
        "binaryItemIDRef='fresh'",
    );
    replace(&mut new,HPF,"</opf:manifest>","<opf:item id='fresh' href='BinData/new.png' media-type='image/png' isEmbeded='1'/></opf:manifest>");
    new.add("BinData/new.png".into(), Arc::from(png()), 0);
    let ops = vec![VerifyOperation::ImageReplace {
        entry: SECTION.into(),
        path: vec![0, 0, 0],
        bytes: png(),
    }];
    (old, new, ops)
}
#[test]
fn image_replacement_has_exact_new_bytes_and_preserves_shared_original() {
    let (old, good, ops) = good_image();
    let r = gate(&old, &good, &ops).unwrap();
    assert!(r.cache_removals.is_empty());
    for name in ["BinData/new.png", "BinData/old.png"] {
        let mut bad = good.clone();
        bad.set(name, Arc::from(b"corrupt".to_vec()));
        assert!(gate(&old, &bad, &ops).is_err());
    }
    let mut bad = good.clone();
    replace(
        &mut bad,
        SECTION,
        "binaryItemIDRef='old'",
        "binaryItemIDRef='fresh'",
    );
    assert!(gate(&old, &bad, &ops).is_err());
    let mut bad = good.clone();
    replace(&mut bad, SECTION, "height='200'", "height='900'");
    assert!(gate(&old, &bad, &ops).is_err());
    let mut bad = good.clone();
    replace(&mut bad, HPF, "id='fresh'", "id='dangling'");
    assert!(gate(&old, &bad, &ops).is_err());
    let mut bad = good.clone();
    replace(&mut bad, HPF, " isEmbeded='1'", "");
    assert!(
        gate(&old, &bad, &ops).is_err(),
        "media item without isEmbeded=1 must be rejected"
    );
    let mut bad = good;
    replace(
        &mut bad,
        HPF,
        "isEmbeded='1'",
        "isEmbeded='1' x:smuggled='yes'",
    );
    assert!(gate(&old, &bad, &ops).is_err());
}
#[test]
fn existing_style_reuse_and_noop_do_not_allow_extra_clone() {
    let mut old = fixture(&paragraph("ABC"));
    let other="<hh:paraPr id='7' x:secret='&#65;'><hh:align horizontal='CENTER' vertical='BASELINE'/><hh:lineSpacing type='PERCENT' value='160'/><x:secret a='2'/></hh:paraPr>";
    replace(
        &mut old,
        HEADER,
        "</hh:paraProperties>",
        &format!("{other}</hh:paraProperties>"),
    );
    replace(&mut old, HEADER, "itemCnt='1'", "itemCnt='2'");
    let mut new = old.clone();
    replace(&mut new, SECTION, "paraPrIDRef='0'", "paraPrIDRef='7'");
    replace(&mut new, SECTION, CACHE, "");
    let ops = [VerifyOperation::ParagraphFormat {
        entry: SECTION.into(),
        path: vec![0],
        alignment: Some(Alignment::Center),
        line_spacing: None,
    }];
    gate(&old, &new, &ops).unwrap();
    let noop = [VerifyOperation::ParagraphFormat {
        entry: SECTION.into(),
        path: vec![0],
        alignment: Some(Alignment::Left),
        line_spacing: None,
    }];
    gate(&old, &old, &noop).unwrap();
    let mut bad = new;
    replace(&mut bad, HEADER, "itemCnt='2'", "itemCnt='3'");
    assert!(gate(&old, &bad, &ops).is_err());
}
#[test]
fn verify_archive_enforces_method_and_raw_bytes_for_unchanged_entries() {
    let (old, good, ops) = good_image();
    let original = Archive::open(zip(&old), &Limits::default()).unwrap();
    let report = verify_archive(&original, &zip(&good), &ops, &Limits::default()).unwrap();
    assert!(report
        .changed_entries
        .contains(&"BinData/new.png".to_string()));
    let plan: Vec<_> = good
        .entries
        .iter()
        .map(|e| {
            if e.name == "unused.bin" {
                OutEntry::Add {
                    name: e.name.clone(),
                    payload: e.payload.clone(),
                    method: 8,
                }
            } else {
                OutEntry::Add {
                    name: e.name.clone(),
                    payload: e.payload.clone(),
                    method: 0,
                }
            }
        })
        .collect();
    let recoded = hae_zip::write(&original, &plan).unwrap();
    assert!(verify_archive(&original, &recoded, &ops, &Limits::default()).is_err());
}
#[test]
fn noop_identical_image_replacement_changes_nothing() {
    let (mut old, _, _) = good_image();
    old.set("BinData/old.png", Arc::from(png()));
    let ops = vec![VerifyOperation::ImageReplace {
        entry: SECTION.into(),
        path: vec![0, 0, 0],
        bytes: png(),
    }];
    let report = gate(&old, &old, &ops).unwrap();
    assert!(report.changed_entries.is_empty());
}
#[test]
fn cropped_and_effect_images_are_rejected() {
    let plain = "<hp:sz width='100' height='200'/><hc:img binaryItemIDRef='old' bright='0' contrast='0' effect='REAL_PIC'/>";
    let mut inners = vec![
        format!("<hp:sz width='100' height='200'/><hp:imgClip left='0' top='0' right='50' bottom='50'/><hc:img binaryItemIDRef='old' bright='0' contrast='0' effect='REAL_PIC'/>"),
        format!("{plain}<hp:shadow/>"),
        format!("{plain}<hp:effects><hp:shadow/></hp:effects>"),
        format!("<hp:sz width='100' height='200'/><hc:img binaryItemIDRef='old' bright='1' contrast='0' effect='REAL_PIC'/>"),
        format!("<hp:sz width='100' height='200'/><hc:img binaryItemIDRef='old' bright='0' contrast='0' effect='GRAY'/>"),
    ];
    for inner in inners.drain(..) {
        let old = fixture(&format!(
            "<hp:p><hp:run><hp:pic id='pic'>{inner}</hp:pic></hp:run>{CACHE}</hp:p>"
        ));
        let ops = vec![VerifyOperation::ImageReplace {
            entry: SECTION.into(),
            path: vec![0, 0, 0],
            bytes: png(),
        }];
        assert!(gate(&old, &old, &ops).is_err(), "{inner}");
    }
    let old = fixture(&format!(
        "<hp:p><hp:run><hp:pic id='pic' reverse='1'>{plain}</hp:pic></hp:run>{CACHE}</hp:p>"
    ));
    let ops = vec![VerifyOperation::ImageReplace {
        entry: SECTION.into(),
        path: vec![0, 0, 0],
        bytes: png(),
    }];
    assert!(gate(&old, &old, &ops).is_err());
}
