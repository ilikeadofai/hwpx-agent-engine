//! Independent request/source derivation. Never consumes editor plans or ids.
use crate::{attr, change, fail, hp};
use hae_diag::{Limits, Result};
use hae_package::{PackageInfo, PackageState, NS_HH};
use hae_xml::XmlDoc;

pub(crate) fn style(
    state: &mut PackageState,
    info: &PackageInfo,
    entry: &str,
    doc: &XmlDoc,
    p: usize,
    id: u32,
    limits: &Limits,
) -> Result<bool> {
    if !hp(doc.el(p), "p") {
        return Err(fail("style target must be a paragraph"));
    }
    let header = crate::parse(
        state,
        info.header.as_deref().ok_or_else(|| fail("no header"))?,
        limits,
    )?;
    let id = id.to_string();
    if header
        .iter()
        .filter(|(_, e)| e.is(NS_HH, "style") && e.attr_value("id") == Some(&id))
        .count()
        != 1
    {
        return Err(fail("requested style ID missing or ambiguous"));
    }
    let a = attr(doc, p, "styleIDRef")?;
    if a.value == id {
        return Ok(false);
    }
    change(
        state,
        entry,
        doc,
        vec![(a.value_span.clone(), id.into_bytes())],
    )?;
    Ok(true)
}

pub(crate) fn header(doc: &XmlDoc, section: usize, kind: &str, index: usize) -> Result<usize> {
    if section != doc.root() || !["header", "footer"].contains(&kind) {
        return Err(fail(
            "header scope must be section root and kind header/footer",
        ));
    }
    let paragraphs: Vec<_> = doc
        .iter()
        .filter(|(_, e)| hp(e, kind))
        .flat_map(|(i, _)| {
            doc.el(i)
                .children
                .iter()
                .copied()
                .filter(|&s| hp(doc.el(s), "subList"))
                .flat_map(|s| {
                    doc.el(s)
                        .children
                        .iter()
                        .copied()
                        .filter(|&p| hp(doc.el(p), "p"))
                })
        })
        .collect();
    let p = *paragraphs
        .get(index)
        .ok_or_else(|| fail("requested header/footer paragraph missing"))?;
    let mut text = false;
    for i in doc.descendants(p) {
        let e = doc.el(i);
        if !(hp(e, "run") || hp(e, "t") || hp(e, "linesegarray") || hp(e, "lineseg")) {
            return Err(fail(
                "header paragraph contains control or unsupported content",
            ));
        }
        text |= hp(e, "t");
    }
    if !text {
        return Err(fail("header lacks existing text carrier"));
    }
    Ok(p)
}
