//! paragraph.format: alignment and percent line spacing (spec §2, §6).
//!
//! The paragraph's current `hh:paraPr` is copied byte-for-byte; only `id`,
//! `hh:align@horizontal` and every `hh:lineSpacing` (including the copies in
//! `hp:switch` case/default blocks) are rewritten. An existing paraPr that is
//! byte-identical apart from its id is reused instead of adding a new one.

use crate::{Draft, Intent, ResourceChange};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::Kind;
use hae_package::NS_HH;
use hae_xml::{escape_attr, XmlDoc};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Alignment {
    Left,
    Right,
    Center,
    Justify,
    Distribute,
    DistributeSpace,
}

impl Alignment {
    pub fn as_str(self) -> &'static str {
        match self {
            Alignment::Left => "LEFT",
            Alignment::Right => "RIGHT",
            Alignment::Center => "CENTER",
            Alignment::Justify => "JUSTIFY",
            Alignment::Distribute => "DISTRIBUTE",
            Alignment::DistributeSpace => "DISTRIBUTE_SPACE",
        }
    }
}

/// Percent line spacing (the only kind supported in 0.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineSpacing {
    pub kind: LineSpacingKind,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineSpacingKind {
    Percent,
}

fn hh(doc: &XmlDoc, i: usize, local: &str) -> bool {
    let e = doc.el(i);
    e.local == local && e.ns.as_deref() == Some(NS_HH)
}

/// Splices (relative to `pp_start`) that turn paraPr `pp` into the target.
fn rewrite(doc: &XmlDoc, pp: usize, new_id: &str, align: Option<Alignment>, ls: Option<LineSpacing>) -> Result<Vec<u8>> {
    let base = doc.el(pp).outer();
    let mut edits: Vec<(Range<usize>, Vec<u8>)> = Vec::new();
    let id = doc.el(pp).attr("id").ok_or_else(|| {
        EngineError::new(Code::Unsupported, Stage::Validate, "paraPr without id").with_reason("paraPr_without_id")
    })?;
    edits.push((id.value_span.clone(), escape_attr(new_id).into_bytes()));
    let mut saw_align = false;
    let mut saw_ls = false;
    for d in doc.descendants(pp) {
        if let Some(a) = align {
            if hh(doc, d, "align") {
                saw_align = true;
                let h = doc.el(d).attr("horizontal").ok_or_else(|| {
                    EngineError::new(Code::Unsupported, Stage::Validate, "align without horizontal").with_reason("paraPr_shape")
                })?;
                edits.push((h.value_span.clone(), a.as_str().as_bytes().to_vec()));
            }
        }
        if let Some(l) = ls {
            if hh(doc, d, "lineSpacing") {
                saw_ls = true;
                let e = doc.el(d);
                let ty = e.attr("type").ok_or_else(|| {
                    EngineError::new(Code::Unsupported, Stage::Validate, "lineSpacing without type").with_reason("paraPr_shape")
                })?;
                let v = e.attr("value").ok_or_else(|| {
                    EngineError::new(Code::Unsupported, Stage::Validate, "lineSpacing without value").with_reason("paraPr_shape")
                })?;
                edits.push((ty.value_span.clone(), b"PERCENT".to_vec()));
                edits.push((v.value_span.clone(), l.value.to_string().into_bytes()));
            }
        }
    }
    if (align.is_some() && !saw_align) || (ls.is_some() && !saw_ls) {
        return Err(EngineError::new(Code::Unsupported, Stage::Validate, "paraPr lacks the element to change")
            .with_reason("paraPr_shape"));
    }
    edits.sort_by_key(|(r, _)| r.start);
    let src = &doc.bytes()[base.clone()];
    let mut out = Vec::with_capacity(src.len());
    let mut last = 0;
    for (r, rep) in edits {
        out.extend_from_slice(&src[last..r.start - base.start]);
        out.extend_from_slice(&rep);
        last = r.end - base.start;
    }
    out.extend_from_slice(&src[last..]);
    Ok(out)
}

/// paraPr bytes with the id value blanked, for equality comparison.
fn canonical(doc: &XmlDoc, pp: usize) -> Option<Vec<u8>> {
    let base = doc.el(pp).outer();
    let id = doc.el(pp).attr("id")?;
    let src = &doc.bytes()[base.clone()];
    let mut v = src[..id.value_span.start - base.start].to_vec();
    v.extend_from_slice(&src[id.value_span.end - base.start..]);
    Some(v)
}

pub(crate) fn paragraph_format(
    d: &mut Draft,
    target: &str,
    align: Option<Alignment>,
    ls: Option<LineSpacing>,
) -> Result<()> {
    let para = d.node(target, Some(Kind::Paragraph))?;
    if align.is_none() && ls.is_none() {
        return Err(EngineError::new(Code::InvalidRequest, Stage::Validate, "nothing to change").with_target(target));
    }
    if let Some(l) = ls {
        if !(1..=1000).contains(&l.value) {
            return Err(EngineError::new(Code::InvalidRequest, Stage::Validate, "line spacing percent must be 1..1000"));
        }
    }
    let header_name = d.doc.info.header.clone().ok_or_else(|| {
        EngineError::new(Code::Unsupported, Stage::Validate, "document has no header.xml").with_reason("no_header")
    })?;
    let hdoc = d.doc.xml[&header_name].clone();
    let sdoc = d.doc.doc_of(&para).clone();
    let pref = sdoc.el(para.el).attr("paraPrIDRef").ok_or_else(|| {
        EngineError::new(Code::Unsupported, Stage::Validate, "paragraph has no paraPrIDRef").with_reason("no_paraPr")
    })?;
    let props = hdoc.iter().map(|(i, _)| i).find(|&i| hh(&hdoc, i, "paraProperties")).ok_or_else(|| {
        EngineError::new(Code::Unsupported, Stage::Validate, "header has no paraProperties").with_reason("no_paraPr")
    })?;
    let pps: Vec<usize> = hdoc.el(props).children.iter().copied().filter(|&c| hh(&hdoc, c, "paraPr")).collect();
    let cur = *pps
        .iter()
        .find(|&&p| hdoc.el(p).attr_value("id") == Some(pref.value.as_str()))
        .ok_or_else(|| EngineError::new(Code::Unsupported, Stage::Validate, format!("paraPr {} not found", pref.value)))?;
    let max_id = pps
        .iter()
        .filter_map(|&p| hdoc.el(p).attr_value("id").and_then(|v| v.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    let new_id = (max_id + 1).to_string();
    let candidate = rewrite(&hdoc, cur, &new_id, align, ls)?;
    // Rewriting with an empty id yields the id-blanked canonical form.
    let cand_canon = rewrite(&hdoc, cur, "", align, ls)?;
    let reuse = pps.iter().copied().find(|&p| canonical(&hdoc, p).as_deref() == Some(cand_canon.as_slice()));
    let (use_id, added) = match reuse {
        Some(p) => (hdoc.el(p).attr_value("id").unwrap_or_default().to_string(), false),
        None => {
            // Append before </hh:paraProperties> and bump itemCnt.
            let end = hdoc.el(props).end.clone().ok_or_else(|| {
                EngineError::new(Code::Unsupported, Stage::Validate, "empty paraProperties").with_reason("paraPr_shape")
            })?;
            let mut edits = vec![(end.start..end.start, candidate)];
            if let Some(c) = hdoc.el(props).attr("itemCnt") {
                let n: u64 = c.value.parse().unwrap_or(pps.len() as u64);
                edits.push((c.value_span.clone(), (n + 1).to_string().into_bytes()));
            }
            d.splice_entry(&header_name, edits)?;
            (new_id.clone(), true)
        }
    };
    if use_id == pref.value {
        return Ok(()); // already formatted that way
    }
    let pref_span = pref.value_span.clone();
    d.splice_entry(&para.entry, vec![(pref_span, escape_attr(&use_id).into_bytes())])?;
    let p2 = d.doc.require(target, Some(Kind::Paragraph))?.clone();
    if d.doc.doc_of(&p2).el(p2.el).attr_value("paraPrIDRef") != Some(use_id.as_str()) {
        return Err(EngineError::new(Code::StateInvariantError, Stage::Apply, "paraPrIDRef not updated"));
    }
    d.resources.push(ResourceChange {
        kind: "paraPr",
        resource_id: use_id.clone(),
        action: if added { "added" } else { "reused" },
    });
    d.anchor(&p2);
    let path = d.doc.doc_of(&p2).path(p2.el);
    d.intents.push(Intent::ParagraphFormat {
        entry: p2.entry.clone(),
        path,
        alignment: align.map(|a| a.as_str().to_string()),
        line_spacing_percent: ls.map(|l| l.value),
    });
    d.changed.push(target.to_string());
    Ok(())
}
