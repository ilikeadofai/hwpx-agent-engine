//! Character shape edits. Raw text/entities and non-text children never change.
use crate::{Draft, Intent, ResourceChange};
use hae_diag::{Code, Diagnostic, EngineError, Result, Stage};
use hae_model::{Kind, PieceKind};
use hae_package::NS_HH;
use hae_xml::{escape_attr, Item, XmlDoc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharProps {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikeout: Option<bool>,
    pub height_hwpunit: Option<u32>,
    pub text_color: Option<String>,
}
impl CharProps {
    pub fn validate(&self) -> Result<()> {
        if self == &Self::default()
            || self
                .height_hwpunit
                .is_some_and(|h| !(100..=100000).contains(&h))
            || self.text_color.as_ref().is_some_and(|c| {
                c.len() != 7
                    || !c.starts_with('#')
                    || !c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
            })
        {
            return Err(EngineError::new(Code::InvalidRequest, Stage::Validate, "props requires at least one property, heightHwpunit 100..100000, textColor #RRGGBB"));
        }
        Ok(())
    }
}
fn unsupported(msg: &str) -> EngineError {
    EngineError::new(Code::Unsupported, Stage::Validate, msg).with_reason("charPr_shape")
}
fn hh(d: &XmlDoc, i: usize, n: &str) -> bool {
    d.el(i).is(NS_HH, n)
}
fn patch(d: &XmlDoc, root: usize, cuts: Vec<(Range<usize>, Vec<u8>)>) -> Result<Vec<u8>> {
    let base = d.el(root).outer();
    let mut out = Vec::new();
    let mut at = base.start;
    let mut cuts = cuts;
    cuts.sort_by_key(|(r, _)| (r.start, r.end));
    for (r, b) in cuts {
        if r.start < at || r.end > base.end {
            return Err(unsupported("overlapping character shape changes"));
        }
        out.extend_from_slice(&d.bytes()[at..r.start]);
        out.extend(b);
        at = r.end;
    }
    out.extend_from_slice(&d.bytes()[at..base.end]);
    Ok(out)
}
fn definition(d: &XmlDoc, cp: usize, id: &str, p: &CharProps) -> Result<Vec<u8>> {
    let e = d.el(cp);
    let end = e
        .end
        .as_ref()
        .ok_or_else(|| unsupported("self-closing charPr"))?;
    let id_attr = e.attr("id").ok_or_else(|| unsupported("charPr lacks id"))?;
    let mut cuts = vec![(id_attr.value_span.clone(), escape_attr(id).into_bytes())];
    let mut added_attrs = String::new();
    for (name, value) in [
        ("height", p.height_hwpunit.map(|v| v.to_string())),
        (
            "textColor",
            p.text_color.as_ref().map(|v| v.to_ascii_uppercase()),
        ),
    ] {
        if let Some(value) = value {
            if let Some(a) = e.attr(name) {
                cuts.push((a.value_span.clone(), value.into_bytes()));
            } else {
                added_attrs.push_str(&format!(" {name}=\"{value}\""));
            }
        }
    }
    if !added_attrs.is_empty() {
        cuts.push((e.start.end - 1..e.start.end - 1, added_attrs.into_bytes()));
    }
    let prefix = e
        .qname
        .rsplit_once(':')
        .map_or(String::new(), |(p, _)| format!("{p}:"));
    let mut insertions: BTreeMap<usize, String> = BTreeMap::new();
    // OWPML CharShapeType child order: font metrics, italic, bold, underline, strikeout, effects.
    let order = [
        "fontRef",
        "ratio",
        "spacing",
        "relSz",
        "offset",
        "italic",
        "bold",
        "underline",
        "strikeout",
        "outline",
        "shadow",
        "emboss",
        "engrave",
        "supscript",
        "subscript",
    ];
    for (name, value) in [
        ("italic", p.italic),
        ("bold", p.bold),
        ("underline", p.underline),
        ("strikeout", p.strikeout),
    ] {
        let Some(value) = value else { continue };
        let found: Vec<_> = e
            .children
            .iter()
            .copied()
            .filter(|&i| hh(d, i, name))
            .collect();
        if found.len() > 1 {
            return Err(unsupported("duplicate character property"));
        }
        if let Some(&i) = found.first() {
            let c = d.el(i);
            if name == "bold" || name == "italic" {
                if !c.attrs.is_empty() || !c.items.is_empty() {
                    return Err(unsupported("non-empty boolean character property"));
                }
                if !value {
                    cuts.push((c.outer(), Vec::new()));
                }
            } else {
                let (attr, v) = if name == "underline" {
                    ("type", if value { "BOTTOM" } else { "NONE" })
                } else {
                    ("shape", if value { "SOLID" } else { "NONE" })
                };
                let a = c
                    .attr(attr)
                    .ok_or_else(|| unsupported("line property lacks type/shape"))?;
                cuts.push((a.value_span.clone(), v.as_bytes().to_vec()));
                if name == "underline" && value && c.attr_value("shape") == Some("NONE") {
                    cuts.push((
                        c.attr("shape").unwrap().value_span.clone(),
                        b"SOLID".to_vec(),
                    ));
                }
            }
        } else if value {
            let rank = order.iter().position(|n| *n == name).unwrap();
            let at = e
                .children
                .iter()
                .copied()
                .find(|&i| {
                    d.el(i).ns.as_deref() == Some(NS_HH)
                        && order
                            .iter()
                            .position(|n| *n == d.el(i).local)
                            .is_some_and(|r| r > rank)
                })
                .map_or(end.start, |i| d.el(i).outer().start);
            let tag = match name {
                "underline" => format!(
                    "<{prefix}underline type=\"BOTTOM\" shape=\"SOLID\" color=\"#000000\"/>"
                ),
                "strikeout" => format!("<{prefix}strikeout shape=\"SOLID\" color=\"#000000\"/>"),
                _ => format!("<{prefix}{name}/>"),
            };
            insertions.entry(at).or_default().push_str(&tag);
        }
    }
    cuts.extend(
        insertions
            .into_iter()
            .map(|(at, s)| (at..at, s.into_bytes())),
    );
    patch(d, cp, cuts)
}
fn without_id(d: &XmlDoc, i: usize) -> Result<Vec<u8>> {
    patch(
        d,
        i,
        vec![(
            d.el(i)
                .attr("id")
                .ok_or_else(|| unsupported("charPr lacks id"))?
                .value_span
                .clone(),
            Vec::new(),
        )],
    )
}

/// One fragment inside a run. `selected` applies only to ordinary text.
fn run_bytes(
    doc: &XmlDoc,
    run: usize,
    selected: &[(usize, Range<usize>)],
    id: &str,
) -> Result<Vec<u8>> {
    let r = doc.el(run);
    let end = r
        .end
        .as_ref()
        .ok_or_else(|| unsupported("text run has no end"))?;
    let pref = r
        .attr("charPrIDRef")
        .ok_or_else(|| unsupported("run lacks charPrIDRef"))?;
    let mut fragments: Vec<(bool, Vec<u8>)> = Vec::new();
    for item in &r.items {
        let Item::Element(t) = item else {
            fragments.push((false, doc.bytes()[item.raw().unwrap().clone()].to_vec()));
            continue;
        };
        let e = doc.el(*t);
        let ranges: Vec<_> = selected
            .iter()
            .filter(|(i, _)| i == t)
            .map(|(_, r)| r.clone())
            .collect();
        if ranges.is_empty() {
            fragments.push((false, doc.bytes()[e.outer()].to_vec()));
            continue;
        }
        let te = e
            .end
            .as_ref()
            .ok_or_else(|| unsupported("selected text has no end tag"))?;
        let mut boundaries = vec![e.inner().start, e.inner().end];
        for b in &ranges {
            boundaries.extend([b.start, b.end]);
        }
        boundaries.sort();
        boundaries.dedup();
        let mut parts: Vec<(bool, Range<usize>)> = Vec::new();
        for w in boundaries.windows(2) {
            let flag = ranges.iter().any(|b| b.start <= w[0] && w[1] <= b.end);
            if let Some((f, last)) = parts.last_mut() {
                if *f == flag {
                    last.end = w[1];
                    continue;
                }
            }
            parts.push((flag, w[0]..w[1]));
        }
        for (flag, span) in parts {
            let mut b = doc.bytes()[e.start.clone()].to_vec();
            b.extend_from_slice(&doc.bytes()[span]);
            b.extend_from_slice(&doc.bytes()[te.clone()]);
            fragments.push((flag, b));
        }
    }
    let mut start = doc.bytes()[r.start.start..pref.value_span.start].to_vec();
    start.extend_from_slice(escape_attr(id).as_bytes());
    start.extend_from_slice(&doc.bytes()[pref.value_span.end..r.start.end]);
    let mut out = Vec::new();
    let mut active = None;
    for (flag, bytes) in fragments {
        if active != Some(flag) {
            if active.is_some() {
                out.extend_from_slice(&doc.bytes()[end.clone()]);
            }
            out.extend_from_slice(if flag {
                &start
            } else {
                &doc.bytes()[r.start.clone()]
            });
            active = Some(flag);
        }
        out.extend(bytes);
    }
    if active.is_some() {
        out.extend_from_slice(&doc.bytes()[end.clone()]);
    }
    Ok(out)
}

pub(crate) fn text_format(
    d: &mut Draft,
    target: &str,
    range: Range<usize>,
    props: &CharProps,
    preserve: bool,
) -> Result<()> {
    props.validate()?;
    let para = d.node(target, Some(Kind::Paragraph))?;
    let pt = d.doc.para_text(&para)?;
    if range.start >= range.end {
        return Err(EngineError::new(
            Code::InvalidRange,
            Stage::Validate,
            "text.format needs a non-empty range",
        ));
    }
    let cps = pt.cp_range(range.clone()).ok_or_else(|| {
        EngineError::new(
            Code::InvalidRange,
            Stage::Validate,
            "range outside paragraph",
        )
    })?;
    if pt
        .verification_barriers
        .iter()
        .any(|&b| cps.start < b && b < cps.end)
        || pt.pieces.iter().any(|p| {
            p.cp_start < cps.end
                && cps.start < p.cp_start + p.cp_len
                && matches!(p.kind, PieceKind::CData { .. })
        })
    {
        return Err(EngineError::new(
            Code::UnsupportedContentInRange,
            Stage::Validate,
            "format range contains unsupported lexical markup",
        )
        .with_target(target));
    }
    // Reuse the text.replace admission checks (but never its replacement plan).
    crate::text::plan_replace(d, &para, &pt, cps.clone(), "", preserve)?;
    let section = d.doc.doc_of(&para).clone();
    let mut runs: BTreeMap<usize, Vec<(usize, Range<usize>)>> = BTreeMap::new();
    for p in &pt.pieces {
        if p.cp_start >= cps.end || p.cp_start + p.cp_len <= cps.start {
            continue;
        }
        let raw = match &p.kind {
            PieceKind::Text { raw } | PieceKind::Ref { raw } => raw,
            _ => continue,
        };
        let t = p.t.unwrap();
        if section.el(t).attr("charPrIDRef").is_some() {
            return Err(unsupported("text-local character reference"));
        }
        let a = cps.start.max(p.cp_start) - p.cp_start;
        let b = cps.end.min(p.cp_start + p.cp_len) - p.cp_start;
        let slice = if matches!(p.kind, PieceKind::Ref { .. }) {
            raw.clone()
        } else {
            let s = std::str::from_utf8(&section.bytes()[raw.clone()]).unwrap();
            let byte = |cp| s.char_indices().nth(cp).map_or(s.len(), |(i, _)| i);
            raw.start + byte(a)..raw.start + byte(b)
        };
        runs.entry(p.run).or_default().push((t, slice));
    }
    if runs.is_empty() {
        return Err(EngineError::new(
            Code::UnsupportedContentInRange,
            Stage::Validate,
            "range has no ordinary text",
        )
        .with_reason("no_text_fragment"));
    }
    let header_name = d
        .doc
        .info
        .header
        .clone()
        .ok_or_else(|| unsupported("missing header"))?;
    let header = d.doc.xml[&header_name].clone();
    let containers: Vec<_> = header
        .iter()
        .filter(|(_, e)| e.is(NS_HH, "charProperties"))
        .map(|(i, _)| i)
        .collect();
    if containers.len() != 1 {
        return Err(unsupported("ambiguous charProperties"));
    }
    let container = containers[0];
    let definitions: Vec<_> = header
        .el(container)
        .children
        .iter()
        .copied()
        .filter(|&i| hh(&header, i, "charPr"))
        .collect();
    let count = header
        .el(container)
        .attr("itemCnt")
        .ok_or_else(|| unsupported("missing charProperties itemCnt"))?;
    if count.value.parse::<usize>().ok() != Some(definitions.len()) {
        return Err(unsupported("inconsistent charProperties itemCnt"));
    }
    let mut ids = BTreeSet::new();
    let mut max = 0u64;
    for &i in &definitions {
        let id = header
            .el(i)
            .attr_value("id")
            .ok_or_else(|| unsupported("charPr without id"))?;
        if !ids.insert(id) {
            return Err(unsupported("duplicate charPr id"));
        }
        max = max.max(
            id.parse()
                .map_err(|_| unsupported("non-numeric charPr id"))?,
        );
    }
    let mut added: Option<(String, Vec<u8>, Vec<u8>)> = None;
    let mut cuts = Vec::new();
    let mut resources = BTreeMap::new();
    for (run, spans) in runs {
        let old = section
            .el(run)
            .attr_value("charPrIDRef")
            .ok_or_else(|| unsupported("run lacks charPrIDRef"))?;
        let cp = *definitions
            .iter()
            .find(|&&i| header.el(i).attr_value("id") == Some(old))
            .ok_or_else(|| unsupported("unresolved charPrIDRef"))?;
        let canonical = definition(&header, cp, "", props)?;
        let reuse = definitions
            .iter()
            .copied()
            .find(|&i| without_id(&header, i).ok().as_ref() == Some(&canonical));
        let (id, is_new) = if let Some(i) = reuse {
            (header.el(i).attr_value("id").unwrap().to_string(), false)
        } else if let Some((id, canon, _)) = &added {
            if canon != &canonical {
                return Err(unsupported("request needs more than one new charPr")
                    .with_reason("multiple_charPr_required"));
            }
            (id.clone(), true)
        } else {
            let id = max
                .checked_add(1)
                .ok_or_else(|| unsupported("charPr ID exhausted"))?
                .to_string();
            let bytes = definition(&header, cp, &id, props)?;
            added = Some((id.clone(), canonical, bytes));
            (id, true)
        };
        if id == old {
            continue;
        }
        cuts.push((
            section.el(run).outer(),
            run_bytes(&section, run, &spans, &id)?,
        ));
        resources.insert(id, if is_new { "added" } else { "reused" });
    }
    if cuts.is_empty() {
        return Ok(());
    }
    if let Some((_, _, bytes)) = added {
        let at = header
            .el(container)
            .end
            .as_ref()
            .ok_or_else(|| unsupported("self-closing charProperties"))?
            .start;
        d.splice_entry(
            &header_name,
            vec![
                (
                    count.value_span.clone(),
                    (definitions.len() + 1).to_string().into_bytes(),
                ),
                (at..at, bytes),
            ],
        )?;
    }
    d.splice_entry(&para.entry, cuts)?;
    let para2 = d.doc.require(target, Some(Kind::Paragraph))?.clone();
    d.anchor(&para2);
    d.changed.push(target.into());
    for (id, action) in resources {
        d.resources.push(ResourceChange {
            kind: "charPr",
            resource_id: id,
            action,
        });
    }
    d.intents.push(Intent::TextFormat {
        entry: para.entry.clone(),
        path: section.path(para.el),
        range,
        props: props.clone(),
    });
    if preserve {
        let offsets: Vec<_> = pt
            .pieces
            .iter()
            .filter(|p| {
                matches!(p.kind, PieceKind::Control { .. })
                    && cps.start <= p.cp_start
                    && p.cp_start < cps.end
            })
            .map(|p| {
                pt.grapheme_cps
                    .partition_point(|&cp| cp <= p.cp_start)
                    .saturating_sub(1)
            })
            .collect();
        if !offsets.is_empty() {
            d.diags.push(
                Diagnostic::error(
                    Code::ControlsPreserved,
                    Stage::Apply,
                    "controls retained at original positions",
                )
                .with_target(target)
                .with_detail(serde_json::json!({"count":offsets.len(),"offsets":offsets})),
            );
        }
    }
    Ok(())
}
