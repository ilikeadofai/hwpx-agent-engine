use super::*;

const HP: &str = "http://www.hancom.co.kr/hwpml/2011/paragraph";

fn sec(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\" ?><hs:sec xmlns:hs=\"http://www.hancom.co.kr/hwpml/2011/section\" xmlns:hp=\"{HP}\">{body}</hs:sec>"
    )
}

fn reject(x: &str) -> String {
    let e = XmlDoc::parse(x.as_bytes().to_vec()).expect_err("expected error");
    format!("{}/{}", e.code(), e.reason().unwrap_or("-"))
}

#[test]
fn parses_and_resolves_namespaces() {
    let x = sec(
        r#"<hp:p id="1"><hp:run charPrIDRef="0"><hp:t>가나&amp;다&#xAC00;<![CDATA[<c>]]></hp:t></hp:run></hp:p>"#,
    );
    let d = XmlDoc::parse(x.as_bytes().to_vec()).unwrap();
    let t = d.iter().find(|(_, e)| e.local == "t").unwrap().0;
    assert!(d.el(t).is(HP, "t"));
    assert_eq!(d.direct_text(t), "가나&다가<c>");
    let p = d.iter().find(|(_, e)| e.local == "p").unwrap().1;
    assert_eq!(p.attr_value("id"), Some("1"));
    assert_eq!(&d.bytes()[p.attr("id").unwrap().value_span.clone()], b"1");
    assert_eq!(d.path(t), vec![0, 0, 0]);
}

#[test]
fn prefix_rebound_to_other_namespace_is_foreign() {
    let x = sec(r#"<hp:p xmlns:hp="urn:other"><hp:t>x</hp:t></hp:p>"#);
    let d = XmlDoc::parse(x.as_bytes().to_vec()).unwrap();
    let p = d.iter().find(|(_, e)| e.local == "p").unwrap().1;
    assert!(!p.is(HP, "p"));
    assert_eq!(p.ns.as_deref(), Some("urn:other"));
    // Default namespace rebinding to the HWPX URI is fine.
    let x = sec(&format!(r#"<p xmlns="{HP}"><t>y</t></p>"#));
    let d = XmlDoc::parse(x.as_bytes().to_vec()).unwrap();
    assert!(d.iter().any(|(_, e)| e.is(HP, "t")));
}

#[test]
fn rejections() {
    assert_eq!(
        reject(r#"<!DOCTYPE x [<!ENTITY a "b">]><x>&a;</x>"#),
        "PACKAGE_REJECTED/dtd_or_entity"
    );
    assert_eq!(reject("<x>&foo;</x>"), "PACKAGE_REJECTED/dtd_or_entity");
    assert_eq!(
        reject("<?xml version=\"1.0\" encoding=\"EUC-KR\"?><x/>"),
        "PACKAGE_REJECTED/non_utf8"
    );
    assert_eq!(reject("<x>\u{1}</x>"), "PACKAGE_REJECTED/malformed_xml");
    assert_eq!(reject("<x>&#1;</x>"), "PACKAGE_REJECTED/malformed_xml");
    assert_eq!(
        reject("<x a=\"1\" a=\"2\"/>"),
        "PACKAGE_REJECTED/malformed_xml"
    );
    assert_eq!(reject("<x><y></x>"), "PACKAGE_REJECTED/malformed_xml");
    assert_eq!(reject("<p:x/>"), "PACKAGE_REJECTED/malformed_xml");
    assert_eq!(reject("<x/><y/>"), "PACKAGE_REJECTED/malformed_xml");
    let e = XmlDoc::parse(vec![0xff, 0xfe, b'<']).unwrap_err();
    assert_eq!(e.reason(), Some("non_utf8"));
    let deep = "<a>".repeat(129) + &"</a>".repeat(129);
    assert_eq!(reject(&deep), "RESOURCE_LIMIT/-");
    let ok = "<a>".repeat(128) + &"</a>".repeat(128);
    XmlDoc::parse(ok.into_bytes()).unwrap();
}

#[test]
fn bom_and_declaration_preserved_by_splice() {
    let x = format!(
        "\u{FEFF}{}",
        sec("<hp:p><hp:run><hp:t>abc</hp:t></hp:run></hp:p>")
    );
    let d = XmlDoc::parse(x.as_bytes().to_vec()).unwrap();
    let t = d.iter().find(|(_, e)| e.local == "t").unwrap().0;
    let tm = TextMap::of(&d, t).unwrap();
    let sp = tm.replace(&d, t, 1, 2, "<&>").unwrap();
    let d2 = d.splice(vec![sp], ParseOptions::default()).unwrap();
    let t2 = d2.iter().find(|(_, e)| e.local == "t").unwrap().0;
    assert_eq!(d2.direct_text(t2), "a<&>c");
    let s = std::str::from_utf8(d2.bytes()).unwrap();
    assert!(s.starts_with("\u{FEFF}<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\" ?>"));
    assert!(s.contains("<hp:t>a&lt;&amp;&gt;c</hp:t>"));
}

#[test]
fn textmap_with_refs_and_cdata() {
    let x = sec("<hp:t>A&#xAC00;B<![CDATA[CD]]>E</hp:t>");
    let d = XmlDoc::parse(x.as_bytes().to_vec()).unwrap();
    let t = d.iter().find(|(_, e)| e.local == "t").unwrap().0;
    let tm = TextMap::of(&d, t).unwrap();
    assert_eq!(tm.text, "A가BCDE");
    assert_eq!(tm.len_cp(), 6);
    // Replace the reference character exactly (boundaries at piece edges).
    let sp = tm.replace(&d, t, 1, 2, "나").unwrap();
    let d2 = d.splice(vec![sp], ParseOptions::default()).unwrap();
    let t2 = d2.iter().find(|(_, e)| e.local == "t").unwrap().0;
    assert_eq!(d2.direct_text(t2), "A나BCDE");
    // Touching CDATA is refused.
    assert_eq!(
        tm.replace(&d, t, 3, 4, "x").unwrap_err().code(),
        Code::Unsupported
    );
    // Out of range.
    assert_eq!(
        tm.replace(&d, t, 5, 9, "x").unwrap_err().code(),
        Code::InvalidRange
    );
}

#[test]
fn empty_element_text_insert_point() {
    let x = sec("<hp:t/><hp:t></hp:t>");
    let d = XmlDoc::parse(x.as_bytes().to_vec()).unwrap();
    let ts: Vec<usize> = d
        .iter()
        .filter(|(_, e)| e.local == "t")
        .map(|(i, _)| i)
        .collect();
    let tm = TextMap::of(&d, ts[1]).unwrap();
    let sp = tm.replace(&d, ts[1], 0, 0, "새").unwrap();
    let d2 = d.splice(vec![sp], ParseOptions::default()).unwrap();
    assert!(std::str::from_utf8(d2.bytes())
        .unwrap()
        .contains("<hp:t>새</hp:t>"));
}

#[test]
fn splice_rejects_overlap_and_broken_result() {
    let x = sec("<hp:t>abc</hp:t>");
    let d = XmlDoc::parse(x.as_bytes().to_vec()).unwrap();
    let r = d.splice(
        vec![(0..10, vec![]), (5..12, vec![])],
        ParseOptions::default(),
    );
    assert_eq!(r.unwrap_err().code(), Code::StateInvariantError);
    let t = d.iter().find(|(_, e)| e.local == "t").unwrap().1;
    let r = d.splice(
        vec![(t.start.clone(), b"<hp:x>".to_vec())],
        ParseOptions::default(),
    );
    assert_eq!(r.unwrap_err().code(), Code::StateInvariantError);
}

#[test]
fn escapes() {
    assert_eq!(escape_text("a]]>b&<"), "a]]&gt;b&amp;&lt;");
    assert_eq!(escape_attr("\"'<\t"), "&quot;&apos;&lt;&#x9;");
    assert_eq!(unescape("a&amp;&#65;&#x42;").unwrap(), "a&AB");
}

// F1 — duplicate expanded attribute names (same URI, different prefixes).
#[test]
fn f1_duplicate_expanded_attributes_rejected() {
    assert_eq!(
        reject(r#"<x xmlns:a="urn:U" xmlns:b="urn:U" a:k="1" b:k="2"/>"#),
        "PACKAGE_REJECTED/malformed_xml"
    );
    // Same local, different URIs: fine.
    XmlDoc::parse(br#"<x xmlns:a="urn:U1" xmlns:b="urn:U2" a:k="1" b:k="2"/>"#.to_vec()).unwrap();
    // Declarations alone are not expanded-attribute collisions.
    XmlDoc::parse(br#"<x xmlns="urn:X" xmlns:a="urn:Y"/>"#.to_vec()).unwrap();
    // Default-ns attributes stay in no namespace: same local, one prefixed: fine.
    XmlDoc::parse(br#"<x xmlns="urn:X" xmlns:a="urn:X" k="1" a:k="2"/>"#.to_vec()).unwrap();
}

// F2 — colons: second colon and empty prefix/local rejected.
#[test]
fn f2_qname_colon_rules() {
    assert_eq!(
        reject(r#"<x xmlns:a="urn:U" a:b:c="1"/>"#),
        "PACKAGE_REJECTED/malformed_xml"
    );
    assert_eq!(
        reject(r#"<a:b:c xmlns:a="urn:U"/>"#),
        "PACKAGE_REJECTED/malformed_xml"
    );
    for bad in ["<:x/>", "<x:/>", "<: />", "<a: />", "<:b/>", "<a::b/>"] {
        assert!(
            XmlDoc::parse(bad.as_bytes().to_vec()).is_err(),
            "expected reject: {bad}"
        );
    }
    XmlDoc::parse(br#"<a:b xmlns:a="urn:U"/>"#.to_vec()).unwrap();
}

// F3 — XML 1.0 NCName start/char rules with real Unicode ranges.
#[test]
fn f3_ncname_validation() {
    assert_eq!(reject("<1bad/>"), "PACKAGE_REJECTED/malformed_xml");
    assert_eq!(reject(r#"<x 1a="2"/>"#), "PACKAGE_REJECTED/malformed_xml");
    assert_eq!(reject("<-bad/>"), "PACKAGE_REJECTED/malformed_xml");
    assert_eq!(reject("<.bad/>"), "PACKAGE_REJECTED/malformed_xml");
    assert_eq!(
        reject(r#"<x a:b:c="1" xmlns:a="urn:U"/>"#),
        "PACKAGE_REJECTED/malformed_xml"
    );
    // Valid: underscore, dot/hyphen inside, Hangul (U+AC00 in 3001-D7FF),
    // CJK (U+4E2D), NameChar extras (· U+00B7, combining U+0300).
    XmlDoc::parse("<_x/>".as_bytes().to_vec()).unwrap();
    XmlDoc::parse("<x-y.z/>".as_bytes().to_vec()).unwrap();
    XmlDoc::parse("<한글/>".as_bytes().to_vec()).unwrap();
    XmlDoc::parse("<中/>".as_bytes().to_vec()).unwrap();
    XmlDoc::parse(
        "<?xml version=\"1.0\"?><x a\u{b7}b=\"1\"/>"
            .as_bytes()
            .to_vec(),
    )
    .unwrap();
    // U+00B7 is NameChar but not NameStartChar.
    assert!(XmlDoc::parse("<x \u{b7}b=\"1\"/>".as_bytes().to_vec()).is_err());
}

// F4 — reserved xml/xmlns bindings.
#[test]
fn f4_reserved_prefix_bindings() {
    assert_eq!(
        reject(r#"<x xmlns:xml="urn:other"/>"#),
        "PACKAGE_REJECTED/malformed_xml"
    );
    assert_eq!(
        reject(r#"<x xmlns:xml="urn:other" xml:lang="en"/>"#),
        "PACKAGE_REJECTED/malformed_xml"
    );
    // Rebinding xml to its correct URI stays accepted.
    let d = XmlDoc::parse(
        br#"<x xmlns:xml="http://www.w3.org/XML/1998/namespace" xml:lang="en"/>"#.to_vec(),
    )
    .unwrap();
    let el = d.el(d.root());
    let lang = el.attrs.iter().find(|a| a.qname == "xml:lang").unwrap();
    assert_eq!(lang.ns.as_deref(), Some(NS_XML));
    // Other prefixes must not take the xml or xmlns namespaces.
    assert!(
        XmlDoc::parse(br#"<x xmlns:a="http://www.w3.org/XML/1998/namespace"/>"#.to_vec()).is_err()
    );
    assert!(XmlDoc::parse(br#"<x xmlns:xmlns="urn:X"/>"#.to_vec()).is_err());
    assert!(XmlDoc::parse(br#"<x xmlns:a="http://www.w3.org/2000/xmlns/"/>"#.to_vec()).is_err());
    // xml prefix works without declaration via the seeded binding.
    let d = XmlDoc::parse(br#"<x xml:lang="en"/>"#.to_vec()).unwrap();
    let el = d.el(d.root());
    assert_eq!(
        el.attrs
            .iter()
            .find(|a| a.qname == "xml:lang")
            .unwrap()
            .ns
            .as_deref(),
        Some(NS_XML)
    );
}
