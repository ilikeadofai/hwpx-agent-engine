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
