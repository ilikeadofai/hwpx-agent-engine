//! Independent expectation for a new native equation object.
//!
//! The verifier derives the expected `hp:equation` bytes from the pre-edit
//! request and the pre-edit section document alone. It never reads the
//! editor's output, does not call `hae-edit`, and has no `hae-model`
//! dependency. The element contract is the same one documented in
//! `docs/decisions/new-node-equation-notes.md`, re-implemented here so a
//! divergence between the two implementations fails the save gate instead of
//! being masked by shared code.
//!
//! Property provenance is the local HWPX workbench native assembler
//! (`vendor/hwpx_utils.py::equation_xml` line 225 with the `affectLSpacing="1"`
//! override from `build.py::Builder.eq_xml` line 90; MIT). The current engine
//! output's native Hancom survival is **not** demonstrated by this module.

use crate::{fail, hp};
use hae_diag::Result;
use hae_package::{PackageState, NS_HC, NS_HP, NS_HP10};
use hae_xml::{escape_text, is_xml_char, XmlDoc};

/// Largest accepted requested HWPUNIT dimension.
const MAX_HWPUNIT: u32 = 1_000_000;

/// Fresh drawing-object id base used by the verified workbench assembler.
const OBJECT_ID_BASE: u32 = 1_300_000_000;

/// Fresh z-order base used by the verified workbench assembler.
const Z_ORDER_BASE: u32 = 1;

/// Canonical union of `hp`/`hc` drawing-object and control locals that carry a
/// numeric `id`. Must stay byte-for-byte identical to the editor's list.
const OBJECT_LOCALS: [&str; 19] = [
    "tbl",
    "pic",
    "equation",
    "ole",
    "container",
    "chart",
    "textart",
    "shape",
    "group",
    "rect",
    "ellipse",
    "line",
    "arc",
    "polygon",
    "curve",
    "connectLine",
    "button",
    "cellzone",
    "video",
];

const SHAPE_COMMENT: &str = "수식입니다.";

/// The verifier's own copy of the bounded script notation rules.
fn script_ok(s: &str) -> bool {
    !s.trim().is_empty()
        && s.chars().all(is_xml_char)
        && !s.contains('\\')
        && !s.contains('"')
        && !s.contains("<!")
        && !s.contains("<?")
        && !s.chars().any(char::is_control)
        && {
            let mut depth = 0i64;
            for c in s.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                if depth < 0 {
                    return false;
                }
            }
            depth == 0
        }
}

fn drawing_ns(ns: Option<&str>) -> bool {
    hp_ns(ns) || ns == Some(NS_HC)
}

fn hp_ns(ns: Option<&str>) -> bool {
    matches!(ns, Some(NS_HP) | Some(NS_HP10))
}

/// Highest object `id` / `zOrder` across the scanned document and every parsed
/// package entry. Saturates with `checked_add` later, never panics.
fn scan_max(state: &PackageState, xml: &XmlDoc, ids: bool) -> u64 {
    let one = |doc: &XmlDoc| -> u64 {
        let mut max = 0u64;
        for el in doc.elements() {
            let value = if ids {
                if !drawing_ns(el.ns.as_deref()) || !OBJECT_LOCALS.contains(&el.local.as_str()) {
                    continue;
                }
                el.attr("id")
            } else {
                el.attr("zOrder")
            };
            if let Some(v) = value.and_then(|a| a.value.parse::<u64>().ok()) {
                max = max.max(v);
            }
        }
        max
    };
    let mut max = one(xml);
    for entry in &state.entries {
        if entry.name.ends_with(".xml") {
            if let Ok(doc) = XmlDoc::parse(entry.payload.clone()) {
                max = max.max(one(&doc));
            }
        }
    }
    max
}

fn fresh(max: u64, base: u32, what: &str) -> Result<u32> {
    let next = max
        .max(u64::from(base - 1))
        .checked_add(1)
        .ok_or_else(|| fail(format!("equation {what} exhausted")))?;
    u32::try_from(next).map_err(|_| fail(format!("equation {what} exhausted")))
}

/// Expected `hp:equation` element bytes for one new equation request.
///
/// The editor and this verifier must receive the same pre-insertion section
/// document and the same pre-edit package for their id/z-order scans to agree.
pub(crate) fn build(
    state: &PackageState,
    xml: &XmlDoc,
    paragraph: usize,
    script: &str,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    if !script_ok(script) {
        return Err(fail("invalid equation script notation"));
    }
    if width == 0 || width > MAX_HWPUNIT || height == 0 || height > MAX_HWPUNIT {
        return Err(fail("equation dimension outside 1..=1000000 HWPUNIT"));
    }
    let para = xml
        .elements()
        .get(paragraph)
        .ok_or_else(|| fail("equation insert paragraph is out of range"))?;
    if !hp(para, "p") {
        return Err(fail("equation insert target is not a paragraph"));
    }
    let (prefix, xmlns) = match para.qname.split_once(':') {
        Some((p, _)) if !p.is_empty() => (format!("{p}:"), String::new()),
        _ => (
            "hp:".to_string(),
            format!(
                " xmlns:hp=\"{}\"",
                escape_text(para.ns.as_deref().unwrap_or_default())
            ),
        ),
    };
    let id = fresh(scan_max(state, xml, true), OBJECT_ID_BASE, "object id")?;
    let z = fresh(scan_max(state, xml, false), Z_ORDER_BASE, "z-order")?;
    Ok(format!(
        "<{prefix}equation id=\"{id}\" zOrder=\"{z}\" numberingType=\"EQUATION\" \
         textWrap=\"TOP_AND_BOTTOM\" textFlow=\"BOTH_SIDES\" lock=\"0\" \
         dropcapstyle=\"None\" version=\"Equation Version 60\" baseLine=\"86\" \
         textColor=\"#000000\" baseUnit=\"1000\" lineMode=\"CHAR\" \
         font=\"HancomEQN\"{xmlns}>\
         <{prefix}sz width=\"{width}\" widthRelTo=\"ABSOLUTE\" height=\"{height}\" \
         heightRelTo=\"ABSOLUTE\" protect=\"0\"/>\
         <{prefix}pos treatAsChar=\"1\" affectLSpacing=\"1\" flowWithText=\"1\" \
         allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" \
         horzRelTo=\"PARA\" vertAlign=\"TOP\" horzAlign=\"LEFT\" vertOffset=\"0\" \
         horzOffset=\"0\"/>\
         <{prefix}outMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>\
         <{prefix}shapeComment>{SHAPE_COMMENT}</{prefix}shapeComment>\
         <{prefix}script>{}</{prefix}script>\
         </{prefix}equation>",
        escape_text(script)
    )
    .into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hae_package::{Entry, Origin};
    use std::sync::Arc;

    fn state(files: &[(&str, &str)]) -> PackageState {
        PackageState {
            entries: files
                .iter()
                .map(|(name, body)| Entry {
                    name: (*name).into(),
                    payload: Arc::from(body.as_bytes().to_vec()),
                    origin: Origin::Original,
                })
                .collect(),
        }
    }

    const SECTION: &str = concat!(
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
        r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
        r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">"#,
        r#"<hp:p id="0"><hp:run charPrIDRef="0"><hp:t>x</hp:t></hp:run></hp:p></hs:sec>"#
    );

    fn docs(xml: &str) -> (PackageState, XmlDoc) {
        let s = state(&[("Contents/section0.xml", xml)]);
        let d = XmlDoc::parse(xml.as_bytes().to_vec()).unwrap();
        (s, d)
    }

    fn para_index(doc: &XmlDoc) -> usize {
        doc.elements().iter().position(|e| e.local == "p").unwrap()
    }

    #[test]
    fn exact_bytes_match_the_frozen_contract() {
        let (s, d) = docs(SECTION);
        let got = build(&s, &d, para_index(&d), "a over b", 5675, 2250).unwrap();
        let want = concat!(
            r#"<hp:equation id="1300000000" zOrder="1" numberingType="EQUATION" "#,
            r#"textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" lock="0" "#,
            r#"dropcapstyle="None" version="Equation Version 60" baseLine="86" "#,
            r##"textColor="#000000" baseUnit="1000" lineMode="CHAR" font="HancomEQN">"##,
            r#"<hp:sz width="5675" widthRelTo="ABSOLUTE" height="2250" heightRelTo="ABSOLUTE" protect="0"/>"#,
            r#"<hp:pos treatAsChar="1" affectLSpacing="1" flowWithText="1" allowOverlap="0" "#,
            r#"holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT" "#,
            r#"vertOffset="0" horzOffset="0"/>"#,
            r#"<hp:outMargin left="0" right="0" top="0" bottom="0"/>"#,
            r#"<hp:shapeComment>수식입니다.</hp:shapeComment>"#,
            r#"<hp:script>a over b</hp:script></hp:equation>"#
        );
        assert_eq!(String::from_utf8(got).unwrap(), want);
    }

    #[test]
    fn escaped_and_rejected_scripts() {
        let (s, d) = docs(SECTION);
        let p = para_index(&d);
        let ok = String::from_utf8(build(&s, &d, p, "a & b < c", 10, 10).unwrap()).unwrap();
        assert!(
            ok.contains("<hp:script>a &amp; b &lt; c</hp:script>"),
            "{ok}"
        );
        for bad in ["", "  ", "a\\b", "a\"b", "<!x", "<?x", "a{b", "}", "a\u{0}"] {
            assert!(build(&s, &d, p, bad, 10, 10).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn missing_equation_is_fine_and_ids_are_union_scanned() {
        // No equation anywhere; union members in this and another section set
        // the id floor.
        let xml = concat!(
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<hp:p><hp:run><hp:group id="1300000009"/><!-- <hp:rect id="5"/> -->"#,
            r#"<hp:button id="3"/></hp:run></hp:p></hs:sec>"#
        );
        let s = state(&[
            ("Contents/section0.xml", xml),
            (
                "Contents/section1.xml",
                r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:cellzone id="1500000000"/></hs:sec>"#,
            ),
        ]);
        let d = XmlDoc::parse(xml.as_bytes().to_vec()).unwrap();
        let ok = String::from_utf8(build(&s, &d, para_index(&d), "x", 10, 10).unwrap()).unwrap();
        assert!(ok.contains(r#"id="1500000001""#), "{ok}");
    }

    #[test]
    fn dimensions_and_overflow() {
        let (s, d) = docs(SECTION);
        let p = para_index(&d);
        assert!(build(&s, &d, p, "x", 0, 1).is_err());
        assert!(build(&s, &d, p, "x", 1, 0).is_err());
        assert!(build(&s, &d, p, "x", 1_000_001, 1).is_err());
        assert!(build(&s, &d, p, "x", 1, 1_000_001).is_err());
        assert!(build(&s, &d, p, "x", 1_000_000, 1_000_000).is_ok());
        let hostile = concat!(
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<hp:p><hp:run><hp:tbl id="18446744073709551615"/></hp:run></hp:p></hs:sec>"#
        );
        let (hs, hd) = docs(hostile);
        assert!(build(&hs, &hd, para_index(&hd), "x", 10, 10).is_err());
    }

    #[test]
    fn namespace_and_target_checks() {
        let prefixed = concat!(
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:q="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<q:p><q:run/></q:p></hs:sec>"#
        );
        let (s, d) = docs(prefixed);
        let ok = String::from_utf8(build(&s, &d, para_index(&d), "x", 10, 10).unwrap()).unwrap();
        assert!(ok.starts_with("<q:equation "), "{ok}");
        assert!(!ok.contains("xmlns:hp"), "{ok}");

        let plain = concat!(
            r#"<sec xmlns="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<p><run/></p></sec>"#
        );
        let (s2, d2) = docs(plain);
        let ok2 =
            String::from_utf8(build(&s2, &d2, para_index(&d2), "x", 10, 10).unwrap()).unwrap();
        assert!(
            ok2.contains(r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph""#),
            "{ok2}"
        );

        let (s3, d3) = docs(SECTION);
        assert!(build(&s3, &d3, 99, "x", 10, 10).is_err());
        assert!(build(&s3, &d3, d3.root(), "x", 10, 10).is_err());
    }
}
