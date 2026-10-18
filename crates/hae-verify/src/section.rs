//! Independently derive section attribute changes from the original and request.
//! No hae-edit/hae-model dependency and no candidate-derived expected values.
use crate::{change, fail, hp, unsupported, PageSettings, Splice, VerifyOperation};
use hae_diag::Result;
use hae_package::PackageState;
use hae_xml::{escape_attr, XmlDoc};
use serde::Serialize;

fn one(doc: &XmlDoc, parent: usize, local: &str) -> Result<usize> {
    let matches: Vec<_> = doc
        .el(parent)
        .children
        .iter()
        .copied()
        .filter(|&n| hp(doc.el(n), local))
        .collect();
    match matches.as_slice() {
        [n] => Ok(*n),
        _ => Err(unsupported(format!(
            "requires exactly one existing {local}"
        ))),
    }
}
fn choice(value: Option<&str>, choices: &[&str]) -> Result<()> {
    match value {
        Some(s) if !choices.contains(&s) => Err(fail("invalid section enum")),
        _ => Ok(()),
    }
}
fn object<T: Serialize>(settings: &T) -> serde_json::Map<String, serde_json::Value> {
    serde_json::to_value(settings)
        .unwrap()
        .as_object()
        .unwrap()
        .clone()
}
fn attribute_changes(
    doc: &XmlDoc,
    node: usize,
    values: serde_json::Map<String, serde_json::Value>,
) -> Vec<Splice> {
    let el = doc.el(node);
    let mut result = vec![];
    let mut missing = vec![];
    for (key, value) in values {
        let text = match value {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => u8::from(b).to_string(),
            _ => continue,
        };
        match el.attr(&key) {
            Some(a) if a.value != text => {
                result.push((a.value_span.clone(), escape_attr(&text).into_bytes()))
            }
            Some(_) => (),
            None => {
                missing.extend_from_slice(format!(" {key}=\"{}\"", escape_attr(&text)).as_bytes())
            }
        }
    }
    if !missing.is_empty() {
        let end = el.start.end;
        let insertion = if doc.bytes()[end - 2] == b'/' {
            end - 2
        } else {
            end - 1
        };
        result.push((insertion..insertion, missing));
    }
    result
}
fn body_area(doc: &XmlDoc, page: usize, margin: usize, request: &PageSettings) -> Result<()> {
    let margins = request.margins.clone().unwrap_or_default();
    let explicit = |node, key| {
        doc.el(node)
            .attr_value(key)
            .ok_or_else(|| unsupported(format!("unspecified {key}: cannot derive body dimensions")))
    };
    let number = |node, key, new: Option<u32>| -> Result<u64> {
        if let Some(n) = new {
            return Ok(u64::from(n));
        }
        explicit(node, key)?
            .parse::<u32>()
            .map(u64::from)
            .map_err(|_| unsupported("invalid original page measurement"))
    };
    let a = number(page, "width", request.width)?;
    let b = number(page, "height", request.height)?;
    let orientation = match &request.landscape {
        Some(v) => v.as_str(),
        None => explicit(page, "landscape")?,
    };
    choice(Some(orientation), &["WIDELY", "NARROWLY"])?;
    let (horizontal, vertical) = match orientation {
        "NARROWLY" => (b, a),
        _ => (a, b),
    };
    let binding = match &request.gutter_type {
        Some(v) => v.as_str(),
        None => explicit(page, "gutterType")?,
    };
    choice(Some(binding), &["LEFT_ONLY", "LEFT_RIGHT", "TOP_BOTTOM"])?;
    let gutter = number(margin, "gutter", margins.gutter)?;
    let x = number(margin, "left", margins.left)? + number(margin, "right", margins.right)?;
    let y = number(margin, "top", margins.top)?
        + number(margin, "bottom", margins.bottom)?
        + number(margin, "header", margins.header)?
        + number(margin, "footer", margins.footer)?;
    let (x, y) = if binding == "TOP_BOTTOM" {
        (x, y + gutter)
    } else {
        (x + gutter, y)
    };
    if a == 0 || b == 0 || x >= horizontal || y >= vertical {
        return Err(fail("non-positive body area"));
    }
    Ok(())
}
pub(crate) fn check(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    owner: usize,
    op: &VerifyOperation,
) -> Result<bool> {
    if !hp(doc.el(owner), "p") || doc.el(owner).parent != Some(doc.root()) {
        return Err(fail("section target is not a top-level paragraph"));
    }
    let secs: Vec<_> = doc
        .iter()
        .filter(|(_, e)| hp(e, "secPr"))
        .map(|(i, _)| i)
        .collect();
    let sec = match secs.as_slice() {
        [n] => *n,
        _ => return Err(unsupported("section requires exactly one existing secPr")),
    };
    if doc.ancestors(sec).find(|&n| hp(doc.el(n), "p")) != Some(owner) {
        return Err(fail("section request targets a different paragraph"));
    }
    let cuts = match op {
        VerifyOperation::SectionSetPage { settings, .. } => {
            let has = object(settings)
                .values()
                .any(|v| !v.is_null() && !v.is_object())
                || settings
                    .margins
                    .as_ref()
                    .is_some_and(|m| object(m).values().any(|v| !v.is_null()));
            if !has {
                return Err(fail("empty page request"));
            }
            choice(settings.landscape.as_deref(), &["NARROWLY", "WIDELY"])?;
            choice(
                settings.gutter_type.as_deref(),
                &["LEFT_ONLY", "LEFT_RIGHT", "TOP_BOTTOM"],
            )?;
            let page = one(doc, sec, "pagePr")?;
            let margin = one(doc, page, "margin")?;
            body_area(doc, page, margin, settings)?;
            let mut edits = attribute_changes(doc, page, object(settings));
            if let Some(m) = &settings.margins {
                edits.extend(attribute_changes(doc, margin, object(m)));
            }
            edits
        }
        VerifyOperation::SectionSetPageNumbering { settings, .. } => {
            if settings.page.is_none() && settings.page_starts_on.is_none() {
                return Err(fail("empty numbering request"));
            }
            choice(settings.page_starts_on.as_deref(), &["BOTH", "EVEN", "ODD"])?;
            attribute_changes(doc, one(doc, sec, "startNum")?, object(settings))
        }
        VerifyOperation::SectionSetVisibility { settings, .. } => {
            if object(settings).values().all(|v| v.is_null()) {
                return Err(fail("empty visibility request"));
            }
            for v in [settings.border.as_deref(), settings.fill.as_deref()] {
                choice(v, &["SHOW_ALL", "HIDE_ALL", "SHOW_FIRST", "HIDE_FIRST"])?;
            }
            attribute_changes(doc, one(doc, sec, "visibility")?, object(settings))
        }
        _ => return Err(fail("incorrect section operation")),
    };
    if cuts.is_empty() {
        return Ok(false);
    }
    change(state, entry, doc, cuts)?;
    Ok(true)
}
