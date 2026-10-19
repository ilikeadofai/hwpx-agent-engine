//! New native equation objects (`equation.insert`).
//!
//! This module owns exactly one thing: the bytes of a new `hp:equation`
//! element. It does not place the element, pick a carrier paragraph/run,
//! remap preview caches or touch identity tables — that is the shared
//! carrier/remap layer. The returned bytes are the raw element only.
//!
//! The element layout is prior art copied from the local HWPX workbench
//! skill's native-equation assembler (`scripts/hwpx_workbench/vendor/
//! hwpx_utils.py::equation_xml` line 225, adapted by
//! `scripts/hwpx_workbench/build.py::Builder.eq_xml` line 90; MIT, see that
//! skill's THIRD_PARTY_NOTICES.md and `docs/decisions/new-node-equation-notes.md`).
//! That is a structural/property-source argument only: the current engine
//! generator's output has **not** been opened in Hancom Office here, and the
//! native survival gate is still pending. The workbench builder forces
//! `affectLSpacing="1"`; this module keeps explicit caller-supplied
//! `width`/`height` (the "estimated" policy) and never applies the
//! "recalculate" policy that zeroes `baseLine`/sizes.
//!
//! Script notation limits deliberately mirror the existing validators in
//! `crate::equation` (and `hae-verify::equation`); that private validator is
//! not made public and this module re-derives the same bounded subset.

use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::is_hp;
use hae_package::{PackageState, NS_HC};
use hae_xml::{escape_text, is_xml_char, XmlDoc};

/// Largest accepted requested HWPUNIT dimension.
const MAX_HWPUNIT: u32 = 1_000_000;

/// Fresh drawing-object id base used by the verified workbench assembler.
const OBJECT_ID_BASE: u32 = 1_300_000_000;

/// Fresh z-order base used by the verified workbench assembler.
const Z_ORDER_BASE: u32 = 1;

/// Canonical union of `hp`/`hc` drawing-object and control locals that carry a
/// numeric `id` in a section. Shared verbatim by the editor and the verifier so
/// both derive the same maximum. Superset of the workbench scan set
/// (`equation|pic|tbl|rect|line|container`) and of the locals the local
/// corpora use.
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

/// Default `hp:shapeComment` value written by the verified assembler.
const SHAPE_COMMENT: &str = "수식입니다.";

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::InvalidRequest, Stage::Validate, message).with_reason("equation_insert")
}

fn invalid_script(message: &str) -> EngineError {
    EngineError::new(Code::Unsupported, Stage::Validate, message)
        .with_reason("equation_script_notation")
}

/// Same bounded subset as `crate::equation::validate_script`: no LaTeX,
/// no backslash/quoted escapes, no markup, no controls, balanced `{}`.
fn validate_script(s: &str) -> Result<()> {
    if s.trim().is_empty() || s.chars().any(|c| !is_xml_char(c)) {
        return Err(invalid("script must be nonempty XML character data"));
    }
    if s.contains('\\')
        || s.contains('"')
        || s.contains("<!")
        || s.contains("<?")
        || s.chars().any(char::is_control)
    {
        return Err(invalid_script(
            "equation backslash/quoted escapes, markup and control notation are not verified",
        ));
    }
    let mut depth = 0usize;
    for c in s.chars() {
        if c == '{' {
            depth += 1;
        }
        if c == '}' {
            depth = depth
                .checked_sub(1)
                .ok_or_else(|| invalid_script("unbalanced script groups"))?;
        }
    }
    if depth != 0 {
        return Err(invalid_script("unbalanced script groups"));
    }
    Ok(())
}

fn object_ns(ns: Option<&str>) -> bool {
    is_hp(ns) || ns == Some(NS_HC)
}

/// Largest numeric object id present in one document, or `None`.
fn max_object_id(doc: &XmlDoc) -> Option<u64> {
    let mut max = None;
    for el in doc.elements() {
        if !object_ns(el.ns.as_deref()) || !OBJECT_LOCALS.contains(&el.local.as_str()) {
            continue;
        }
        if let Some(v) = el.attr("id").and_then(|a| a.value.parse::<u64>().ok()) {
            max = Some(max.map_or(v, |m: u64| m.max(v)));
        }
    }
    max
}

/// Largest non-negative `zOrder` present in one document, or `None`.
fn max_z_order(doc: &XmlDoc) -> Option<u64> {
    let mut max = None;
    for el in doc.elements() {
        if let Some(v) = el.attr("zOrder").and_then(|a| a.value.parse::<u64>().ok()) {
            max = Some(max.map_or(v, |m: u64| m.max(v)));
        }
    }
    max
}

/// Highest value seen in the scanned document plus every parsed XML package
/// entry, never panicking on a hostile `u64::MAX` sentinel.
fn package_max(xml: &XmlDoc, state: &PackageState, scan: fn(&XmlDoc) -> Option<u64>) -> u64 {
    let mut max = 0u64;
    if let Some(v) = scan(xml) {
        max = max.max(v);
    }
    for entry in &state.entries {
        if !entry.name.ends_with(".xml") {
            continue;
        }
        if let Ok(doc) = XmlDoc::parse(entry.payload.clone()) {
            if let Some(v) = scan(&doc) {
                max = max.max(v);
            }
        }
    }
    max
}

/// One past the highest object id anywhere in the package, at least
/// [`OBJECT_ID_BASE`].
fn next_object_id(state: &PackageState, xml: &XmlDoc) -> Result<u32> {
    let max = package_max(xml, state, max_object_id).max(u64::from(OBJECT_ID_BASE - 1));
    let next = max
        .checked_add(1)
        .ok_or_else(|| invalid("equation object id exhausted"))?;
    u32::try_from(next).map_err(|_| invalid("equation object id exhausted"))
}

/// One past the highest z-order in the package, at least [`Z_ORDER_BASE`].
fn next_z_order(state: &PackageState, xml: &XmlDoc) -> Result<u32> {
    let max = package_max(xml, state, max_z_order).max(u64::from(Z_ORDER_BASE - 1));
    let next = max
        .checked_add(1)
        .ok_or_else(|| invalid("equation z-order exhausted"))?;
    u32::try_from(next).map_err(|_| invalid("equation z-order exhausted"))
}

/// `(element-prefix, local-xmlns-declaration)` for the new element.
///
/// If the carrier paragraph already carries the `hp` prefix, that binding is
/// in scope for the equation (a descendant of the paragraph) and is reused
/// with no declaration. Otherwise a local `xmlns:hp` is declared on the
/// equation element, so the result never depends on an inherited default
/// namespace that an intermediate element could rebind.
fn prefix_for(xml: &XmlDoc, paragraph: usize) -> Result<(String, String)> {
    let el = xml
        .elements()
        .get(paragraph)
        .ok_or_else(|| invalid("equation insert paragraph index is out of range"))?;
    if el.local != "p" || !is_hp(el.ns.as_deref()) {
        return Err(invalid("equation insert target is not a paragraph element"));
    }
    match el.qname.split_once(':') {
        Some((p, _)) if !p.is_empty() => Ok((format!("{p}:"), String::new())),
        _ => {
            let ns = el.ns.as_deref().unwrap_or_default();
            Ok((
                "hp:".to_string(),
                format!(" xmlns:hp=\"{}\"", escape_text(ns)),
            ))
        }
    }
}

/// Build only the `hp:equation` element bytes for a new equation.
///
/// `paragraph` is the element index of the carrier `hp:p` inside `xml`.
/// `width`/`height` are HWPUNIT, `1..=1_000_000`. The script must use the
/// existing bounded HWP equation notation. No existing equation is required.
pub(crate) fn build(
    state: &PackageState,
    xml: &XmlDoc,
    paragraph: usize,
    script: &str,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    validate_script(script)?;
    if width == 0 || width > MAX_HWPUNIT || height == 0 || height > MAX_HWPUNIT {
        return Err(invalid(
            "equation width/height must be HWPUNIT in 1..=1000000",
        ));
    }
    let id = next_object_id(state, xml)?;
    let z = next_z_order(state, xml)?;
    let (p, xmlns) = prefix_for(xml, paragraph)?;
    let script = escape_text(script);
    let mut out = String::new();
    out.push_str(&format!(
        "<{p}equation id=\"{id}\" zOrder=\"{z}\" numberingType=\"EQUATION\" \
         textWrap=\"TOP_AND_BOTTOM\" textFlow=\"BOTH_SIDES\" lock=\"0\" \
         dropcapstyle=\"None\" version=\"Equation Version 60\" baseLine=\"86\" \
         textColor=\"#000000\" baseUnit=\"1000\" lineMode=\"CHAR\" \
         font=\"HancomEQN\"{xmlns}>"
    ));
    out.push_str(&format!(
        "<{p}sz width=\"{width}\" widthRelTo=\"ABSOLUTE\" height=\"{height}\" \
         heightRelTo=\"ABSOLUTE\" protect=\"0\"/>"
    ));
    out.push_str(&format!(
        "<{p}pos treatAsChar=\"1\" affectLSpacing=\"1\" flowWithText=\"1\" \
         allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" \
         horzRelTo=\"PARA\" vertAlign=\"TOP\" horzAlign=\"LEFT\" vertOffset=\"0\" \
         horzOffset=\"0\"/>"
    ));
    out.push_str(&format!(
        "<{p}outMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>"
    ));
    out.push_str(&format!(
        "<{p}shapeComment>{SHAPE_COMMENT}</{p}shapeComment>"
    ));
    out.push_str(&format!("<{p}script>{script}</{p}script>"));
    out.push_str(&format!("</{p}equation>"));
    Ok(out.into_bytes())
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

    fn para_index(doc: &XmlDoc) -> usize {
        doc.elements()
            .iter()
            .position(|e| e.local == "p")
            .expect("paragraph")
    }

    fn docs(xml: &str) -> (PackageState, XmlDoc) {
        let s = state(&[("Contents/section0.xml", xml)]);
        let d = XmlDoc::parse(xml.as_bytes().to_vec()).unwrap();
        (s, d)
    }

    const SECTION: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>"#,
        r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
        r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" "#,
        r#"xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core">"#,
        r#"<hp:p id="0" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>x</hp:t></hp:run></hp:p></hs:sec>"#
    );

    #[test]
    fn new_equation_needs_no_existing_object() {
        let (s, d) = docs(SECTION);
        let out = build(&s, &d, para_index(&d), "a over b", 5675, 2250).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("<hp:equation ").count(), 1);
        assert!(text.contains(r#"id="1300000000""#), "{text}");
        assert!(text.contains(r#"zOrder="1""#), "{text}");
        assert!(
            text.contains(r#"<hp:script>a over b</hp:script>"#),
            "{text}"
        );
        assert!(text.starts_with("<hp:equation "), "{text}");
        assert!(text.ends_with("</hp:equation>"), "{text}");
        assert!(
            !text.contains("<hp:p ") && !text.contains("<hp:p>"),
            "{text}"
        );
        assert!(text.contains(r#"affectLSpacing="1""#), "{text}");
        assert!(text.contains(r#"baseUnit="1000""#), "{text}");
        assert!(text.contains(r##"textColor="#000000""##), "{text}");
        assert!(text.contains(r#"lineMode="CHAR""#), "{text}");
        assert!(text.contains(r#"font="HancomEQN""#), "{text}");
    }

    #[test]
    fn script_is_xml_escaped() {
        let (s, d) = docs(SECTION);
        let out = build(&s, &d, para_index(&d), "a & b < c > d", 1200, 1200).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("<hp:script>a &amp; b &lt; c &gt; d</hp:script>"),
            "{text}"
        );
        assert!(!text.contains("<hp:script>a & b"), "{text}");
    }

    #[test]
    fn invalid_scripts_are_rejected() {
        let (s, d) = docs(SECTION);
        let p = para_index(&d);
        for bad in [
            "",
            "   ",
            "a\\b",
            "a\"b",
            "<![CDATA[x]]>",
            "<?pi?>",
            "a{b",
            "a}b",
            "x\u{1}",
        ] {
            assert!(build(&s, &d, p, bad, 1200, 1200).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn dimensions_are_bounded_positive_hwpunit() {
        let (s, d) = docs(SECTION);
        let p = para_index(&d);
        assert!(build(&s, &d, p, "x", 0, 1200).is_err());
        assert!(build(&s, &d, p, "x", 1200, 0).is_err());
        assert!(build(&s, &d, p, "x", 1_000_001, 1200).is_err());
        assert!(build(&s, &d, p, "x", 1200, 1_000_001).is_err());
        assert!(build(&s, &d, p, "x", 1, 1).is_ok());
        assert!(build(&s, &d, p, "x", 1_000_000, 1_000_000).is_ok());
    }

    #[test]
    fn ids_pass_existing_objects_and_other_sections() {
        let xml = concat!(
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<hp:p><hp:run><hp:pic id="1300000007"/><hp:rect id="900"/>"#,
            r#"<hp:tbl id="15"/></hp:run></hp:p></hs:sec>"#
        );
        let s = state(&[
            ("Contents/section0.xml", xml),
            (
                "Contents/section1.xml",
                r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:polygon id="1400000000"/></hs:sec>"#,
            ),
        ]);
        let d = XmlDoc::parse(xml.as_bytes().to_vec()).unwrap();
        let out = build(&s, &d, para_index(&d), "x", 100, 100).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(r#"id="1400000001""#), "{text}");
        assert!(text.contains(r#"zOrder="1""#), "{text}");
    }

    #[test]
    fn hostile_id_sentinel_is_rejected_without_panic() {
        let xml = concat!(
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<hp:p><hp:run><hp:pic id="18446744073709551615"/></hp:run></hp:p></hs:sec>"#
        );
        let (s, d) = docs(xml);
        let err = build(&s, &d, para_index(&d), "x", 10, 10).unwrap_err();
        assert_eq!(err.reason(), Some("equation_insert"));
    }

    #[test]
    fn z_order_follows_existing_shapes() {
        let xml = concat!(
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<hp:p><hp:run><hp:pic id="1" zOrder="4"/></hp:run></hp:p></hs:sec>"#
        );
        let (s, d) = docs(xml);
        let text = String::from_utf8(build(&s, &d, para_index(&d), "x", 10, 10).unwrap()).unwrap();
        assert!(text.contains(r#"zOrder="5""#), "{text}");
    }

    #[test]
    fn namespace_prefix_is_reused_from_the_paragraph() {
        let xml = concat!(
            r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
            r#"xmlns:zz="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<zz:p><zz:run><zz:t>x</zz:t></zz:run></zz:p></hs:sec>"#
        );
        let (s, d) = docs(xml);
        let text = String::from_utf8(build(&s, &d, para_index(&d), "x", 10, 10).unwrap()).unwrap();
        assert!(text.starts_with("<zz:equation "), "{text}");
        assert!(text.contains("<zz:script>x</zz:script>"), "{text}");
        assert!(!text.contains("xmlns:hp"), "{text}");
    }

    #[test]
    fn unprefixed_paragraph_declares_local_hp_prefix() {
        let xml = concat!(
            r#"<sec xmlns="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
            r#"<p><run><t>x</t></run></p></sec>"#
        );
        let (s, d) = docs(xml);
        let text = String::from_utf8(build(&s, &d, para_index(&d), "x", 10, 10).unwrap()).unwrap();
        assert!(
            text.starts_with(r#"<hp:equation id="1300000000" zOrder="1""#),
            "{text}"
        );
        assert!(
            text.contains(r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">"#),
            "{text}"
        );
        assert!(text.contains("<hp:sz "), "{text}");
    }

    #[test]
    fn bad_target_is_rejected() {
        let (s, d) = docs(SECTION);
        assert!(build(&s, &d, 99, "x", 10, 10).is_err());
        // Root is `hs:sec`, not a paragraph.
        assert!(build(&s, &d, d.root(), "x", 10, 10).is_err());
        // A non-paragraph hp element is rejected too.
        let nested = XmlDoc::parse(
            concat!(
                r#"<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" "#,
                r#"xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">"#,
                r#"<hp:p><hp:run><hp:pic id="1"/></hp:run></hp:p></hs:sec>"#
            )
            .as_bytes()
            .to_vec(),
        )
        .unwrap();
        let pic = nested
            .elements()
            .iter()
            .position(|e| e.local == "pic")
            .unwrap();
        assert!(build(&s, &nested, pic, "x", 10, 10).is_err());
    }
}
