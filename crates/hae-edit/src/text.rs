//! text.replace and cell.setText (spec §6).

use crate::{Draft, Intent};
use hae_diag::{Code, Diagnostic, EngineError, Result, Stage};
use hae_model::{is_hp, Kind, Node, ParaText, PieceKind};
use hae_xml::{escape_text, is_xml_char};
use std::collections::BTreeSet;
use std::ops::Range;

fn check_new_text(text: &str) -> Result<()> {
    if text.contains(['\t', '\n', '\r']) {
        return Err(EngineError::new(
            Code::InvalidRequest,
            Stage::Validate,
            "replacement text must not contain tabs or line breaks",
        ));
    }
    if let Some(c) = text.chars().find(|&c| !is_xml_char(c)) {
        return Err(EngineError::new(
            Code::InvalidRequest,
            Stage::Validate,
            format!(
                "replacement contains forbidden character U+{:04X}",
                c as u32
            ),
        ));
    }
    Ok(())
}

/// Known attributes of hp:t; anything else is uninterpreted (spec §6).
use hae_model::para::TEXT_ATTRS as T_ATTRS;

type Splice = (Range<usize>, Vec<u8>);

/// Splices that replace codepoints `cps` of the paragraph with `new`.
fn plan_replace(
    d: &Draft,
    para: &Node,
    pt: &ParaText,
    cps: Range<usize>,
    new: &str,
    preserve: bool,
) -> Result<(Vec<Splice>, Option<Diagnostic>)> {
    let doc = d.doc.doc_of(para);
    let covered: Vec<&hae_model::Piece> = pt
        .pieces
        .iter()
        .filter(|p| p.cp_start < cps.end && cps.start < p.cp_start + p.cp_len)
        .collect();
    for p in &covered {
        match &p.kind {
            PieceKind::Control { .. } if preserve => {}
            PieceKind::Control { name, .. } => {
                return Err(EngineError::new(
                    Code::RangeCrossesControl,
                    Stage::Validate,
                    format!("range crosses a {name} control"),
                )
                .with_target(&para.id)
                .with_detail(
                    serde_json::to_value(d.doc.paragraph_text_spans(para))
                        .expect("text spans serialize"),
                ));
            }
            PieceKind::Other { name, .. } => {
                return Err(EngineError::new(
                    Code::UnsupportedContentInRange,
                    Stage::Validate,
                    format!("range covers a {name} element"),
                )
                .with_target(&para.id));
            }
            _ => {}
        }
    }
    if preserve
        && (pt
            .verification_barriers
            .iter()
            .any(|&b| cps.start < b && b < cps.end)
            || covered.iter().any(|p| {
                matches!(p.kind, PieceKind::CData { .. })
                    || (matches!(p.kind, PieceKind::Text { .. } | PieceKind::Ref { .. })
                        && pt
                            .text
                            .chars()
                            .skip(p.cp_start.max(cps.start))
                            .take((p.cp_start + p.cp_len).min(cps.end) - p.cp_start.max(cps.start))
                            .any(|c| ['\t', '\n', '\r'].contains(&c)))
            }))
    {
        return Err(EngineError::new(
            Code::UnsupportedContentInRange,
            Stage::Validate,
            "preserve range contains unsupported markup",
        )
        .with_target(&para.id));
    }
    // Uninterpreted attributes on the touched hp:t elements.
    let ts: BTreeSet<usize> = covered.iter().filter_map(|p| p.t).collect();
    for &t in &ts {
        if let Some(a) = doc.el(t).attrs.iter().find(|a| {
            !(a.qname == "xmlns" || a.qname.starts_with("xmlns:"))
                && (a.ns.is_some() || !T_ATTRS.contains(&a.local.as_str()))
        }) {
            return Err(EngineError::new(
                Code::UnsupportedContentInRange,
                Stage::Validate,
                format!("hp:t has uninterpreted attribute {}", a.qname),
            )
            .with_target(&para.id));
        }
    }
    // Byte offset of codepoint `cp` within a text/ref piece.
    let byte_at = |p: &hae_model::Piece, cp: usize| -> usize {
        match &p.kind {
            PieceKind::Text { raw } => {
                let s = std::str::from_utf8(&doc.bytes()[raw.clone()]).unwrap_or("");
                raw.start
                    + s.char_indices()
                        .nth(cp - p.cp_start)
                        .map_or(s.len(), |(i, _)| i)
            }
            PieceKind::Ref { raw } => {
                if cp == p.cp_start {
                    raw.start
                } else {
                    raw.end
                }
            }
            _ => unreachable!(),
        }
    };
    let mut out = Vec::new();
    let mut inserted = false;
    for p in &covered {
        if matches!(p.kind, PieceKind::Control { .. }) {
            continue;
        }
        let s = cps.start.max(p.cp_start);
        let e = cps.end.min(p.cp_start + p.cp_len);
        let (raw, replacement) = if let PieceKind::CData { raw } = &p.kind {
            // Keep the untouched CDATA text in CDATA sections. Only the
            // replacement is escaped normal character data, so `]]>` in
            // user input cannot terminate or inject a CDATA section.
            let content = &doc.bytes()[raw.start + 9..raw.end - 3];
            let value = std::str::from_utf8(content).expect("validated UTF-8 CDATA");
            let before: String = value.chars().take(s - p.cp_start).collect();
            let after: String = value.chars().skip(e - p.cp_start).collect();
            let mut value = Vec::new();
            if !before.is_empty() {
                value.extend_from_slice(format!("<![CDATA[{before}]]>").as_bytes());
            }
            if !inserted {
                value.extend_from_slice(escape_text(new).as_bytes());
            }
            if !after.is_empty() {
                value.extend_from_slice(format!("<![CDATA[{after}]]>").as_bytes());
            }
            (raw.clone(), value)
        } else {
            let r = byte_at(p, s)..byte_at(p, e);
            (
                r,
                if inserted {
                    Vec::new()
                } else {
                    escape_text(new).into_bytes()
                },
            )
        };
        out.push((raw, replacement));
        inserted = true;
    }
    if !inserted && preserve {
        let mut cp = 0;
        let mut empty = None;
        for &r in &doc.el(para.el).children {
            if !(is_hp(doc.el(r).ns.as_deref()) && doc.el(r).local == "run") {
                continue;
            }
            for &t in &doc.el(r).children {
                if is_hp(doc.el(t).ns.as_deref())
                    && doc.el(t).local == "t"
                    && doc.el(t).items.is_empty()
                    && cps.start <= cp
                    && cp <= cps.end
                    && hae_model::para::text_element_supported(doc, t)
                {
                    empty.get_or_insert(t);
                }
                cp += pt
                    .pieces
                    .iter()
                    .filter(|p| {
                        p.t == Some(t)
                            || (p.t.is_none()
                                && matches!(p.kind,PieceKind::Control{el,..} if el==t))
                    })
                    .map(|p| p.cp_len)
                    .sum::<usize>();
            }
        }
        if let Some(t) = empty {
            let el = doc.el(t);
            let cut = if el.end.is_some() {
                (el.inner(), escape_text(new).into_bytes())
            } else {
                let tag = std::str::from_utf8(&doc.bytes()[el.start.clone()]).unwrap();
                (
                    el.start.clone(),
                    format!(
                        "{}>{}</{}>",
                        tag.trim_end_matches('>').trim_end_matches('/').trim_end(),
                        escape_text(new),
                        el.qname
                    )
                    .into_bytes(),
                )
            };
            out.push(cut);
        } else if !new.is_empty() {
            return Err(EngineError::new(
                Code::UnsupportedContentInRange,
                Stage::Validate,
                "preserve range has no existing text fragment",
            )
            .with_reason("no_text_fragment")
            .with_target(&para.id));
        }
    }
    // FORMAT_COLLAPSED: covered runs with different character shapes.
    let shapes: BTreeSet<Option<&str>> = covered
        .iter()
        .filter(|p| !matches!(p.kind, PieceKind::Control { .. }))
        .map(|p| doc.el(p.run).attr_value("charPrIDRef"))
        .collect();
    let diag = (shapes.len() > 1).then(|| {
        Diagnostic::error(
            Code::FormatCollapsed,
            Stage::Apply,
            "replacement uses the first character's shape; other shapes in the range were collapsed",
        )
        .with_target(&para.id)
    });
    Ok((out, diag))
}

fn empty_insert(d: &Draft, para: &Node, new: &str) -> Result<Vec<Splice>> {
    let pt = d.doc.para_text(para)?;
    if let Some(reason) = pt.empty_insert_reason(d.doc.doc_of(para)) {
        return Err(EngineError::new(
            if reason == "no_run" {
                Code::Unsupported
            } else {
                Code::UnsupportedContentInRange
            },
            Stage::Validate,
            "empty paragraph cannot accept text",
        )
        .with_reason(reason)
        .with_target(&para.id));
    }
    let target = &para.id;
    let doc = d.doc.doc_of(para);
    Ok({
        let run = doc
            .el(para.el)
            .children
            .iter()
            .copied()
            .find(|&c| is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "run")
            .ok_or_else(|| {
                EngineError::new(
                    Code::Unsupported,
                    Stage::Validate,
                    "empty cell paragraph has no run",
                )
                .with_target(target)
                .with_reason("no_run")
            })?;
        let r = doc.el(run);
        let t = r
            .children
            .iter()
            .copied()
            .find(|&c| is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "t");
        let prefix = r
            .qname
            .split_once(':')
            .map(|(p, _)| format!("{p}:"))
            .unwrap_or_default();
        match t {
            Some(t) => {
                let te = doc.el(t);
                if te.attrs.iter().any(|a| {
                    !(a.qname == "xmlns" || a.qname.starts_with("xmlns:"))
                        && (a.ns.is_some() || !T_ATTRS.contains(&a.local.as_str()))
                }) {
                    return Err(EngineError::new(
                        Code::UnsupportedContentInRange,
                        Stage::Validate,
                        "empty hp:t has uninterpreted attributes",
                    )
                    .with_target(target));
                }
                if te.end.is_none() {
                    // <hp:t/> → <hp:t ...>text</hp:t>
                    let tag = std::str::from_utf8(&doc.bytes()[te.start.clone()]).unwrap_or("");
                    let open = tag.trim_end_matches('>').trim_end_matches('/').trim_end();
                    vec![(
                        te.start.clone(),
                        format!("{open}>{}</{}>", escape_text(new), te.qname).into_bytes(),
                    )]
                } else {
                    vec![(te.inner(), escape_text(new).into_bytes())]
                }
            }
            None => {
                if r.end.is_none() {
                    let tag = std::str::from_utf8(&doc.bytes()[r.start.clone()]).unwrap();
                    let open = tag.trim_end_matches('>').trim_end_matches('/').trim_end();
                    return Ok(vec![(
                        r.start.clone(),
                        format!(
                            "{open}><{prefix}t>{}</{prefix}t></{}>",
                            escape_text(new),
                            r.qname
                        )
                        .into_bytes(),
                    )]);
                }
                let at = r.inner().end;
                vec![(
                    at..at,
                    format!("<{prefix}t>{}</{prefix}t>", escape_text(new)).into_bytes(),
                )]
            }
        }
    })
}

fn plan_insert(
    d: &Draft,
    para: &Node,
    pt: &ParaText,
    pos: usize,
    new: &str,
) -> Result<Vec<Splice>> {
    if pt.text.is_empty() {
        return empty_insert(d, para, new);
    }
    // Use the complete span set, not the public 64-span response page.
    let cp = pt.grapheme_cps[pos];
    let doc = d.doc.doc_of(para);
    let safe = |p: &&hae_model::Piece| {
        matches!(p.kind, PieceKind::Text { .. } | PieceKind::Ref { .. })
            && p.t
                .is_some_and(|t| hae_model::para::text_element_supported(doc, t))
    };
    let grapheme_safe = |i: usize| {
        let probe = pt.cp_range(i..i + 1).unwrap();
        !pt.verification_barriers
            .iter()
            .any(|&b| probe.start < b && b < probe.end)
            && pt
                .pieces
                .iter()
                .filter(|p| p.cp_start < probe.end && probe.start < p.cp_start + p.cp_len)
                .all(|p| safe(&p))
            && !pt
                .text
                .chars()
                .skip(probe.start)
                .take(probe.end - probe.start)
                .any(|c| ['\t', '\n', '\r'].contains(&c))
    };
    let right = pos < pt.grapheme_len() && grapheme_safe(pos);
    let left = pos > 0 && grapheme_safe(pos - 1);
    if !right && !left {
        return Err(EngineError::new(
            Code::InvalidRange,
            Stage::Validate,
            "position is outside textSpans",
        )
        .with_target(&para.id));
    }
    let source = if right { cp } else { cp - 1 };
    let piece = pt
        .pieces
        .iter()
        .find(|p| p.cp_start <= source && source < p.cp_start + p.cp_len)
        .unwrap();
    let at = match &piece.kind {
        PieceKind::Text { raw } => {
            raw.start
                + std::str::from_utf8(&doc.bytes()[raw.clone()])
                    .unwrap()
                    .char_indices()
                    .nth(cp - piece.cp_start)
                    .map_or(raw.len(), |(i, _)| i)
        }
        PieceKind::Ref { raw } => {
            if cp == piece.cp_start {
                raw.start
            } else {
                raw.end
            }
        }
        _ => unreachable!(),
    };
    Ok(vec![(at..at, escape_text(new).into_bytes())])
}

pub(crate) fn text_replace(
    d: &mut Draft,
    target: &str,
    range: Range<usize>,
    new: &str,
    preserve: bool,
) -> Result<()> {
    let para = d.node(target, Some(Kind::Paragraph))?;
    check_new_text(new)?;
    let pt = d.doc.para_text(&para)?;
    if range.start > range.end || (range.start == range.end && new.is_empty()) {
        return Err(EngineError::new(
            Code::InvalidRange,
            Stage::Validate,
            "reversed range or empty insertion",
        )
        .with_target(target));
    }
    let cps = pt.cp_range(range.clone()).ok_or_else(|| {
        EngineError::new(
            Code::InvalidRange,
            Stage::Validate,
            format!(
                "range {}..{} outside paragraph of {} graphemes",
                range.start,
                range.end,
                pt.grapheme_len()
            ),
        )
        .with_target(target)
    })?;
    let (splices, diag) = if range.start == range.end {
        (plan_insert(d, &para, &pt, range.start, new)?, None)
    } else {
        plan_replace(d, &para, &pt, cps.clone(), new, preserve)?
    };
    let old_text = pt.text.clone();
    let mut expected: String = pt.text.chars().take(cps.start).collect();
    if preserve && cps.start < cps.end {
        let first = pt.pieces.iter().find(|p| {
            p.cp_start < cps.end
                && cps.start < p.cp_start + p.cp_len
                && !matches!(p.kind, PieceKind::Control { .. })
        });
        let mut put = false;
        for p in pt
            .pieces
            .iter()
            .filter(|p| p.cp_start < cps.end && cps.start < p.cp_start + p.cp_len)
        {
            if matches!(p.kind, PieceKind::Control { .. }) {
                expected.extend(pt.text.chars().skip(p.cp_start).take(p.cp_len));
            } else if !put {
                expected.push_str(new);
                put = true;
            }
        }
        // An existing empty t may occur after leading objects. Derive its
        // placement from the selected byte splice, not from a new structure.
        if first.is_none() && !new.is_empty() {
            let at = splices[0].0.start;
            expected = pt.text.chars().take(cps.start).collect();
            let mut put = false;
            for p in pt
                .pieces
                .iter()
                .filter(|p| p.cp_start < cps.end && cps.start < p.cp_start + p.cp_len)
            {
                let el = match p.kind {
                    PieceKind::Control { el, .. } => el,
                    _ => unreachable!(),
                };
                if !put && at < d.doc.doc_of(&para).el(el).outer().start {
                    expected.push_str(new);
                    put = true;
                }
                expected.extend(pt.text.chars().skip(p.cp_start).take(p.cp_len));
            }
            if !put {
                expected.push_str(new);
            }
        }
    } else {
        expected.push_str(new);
    }
    expected.extend(pt.text.chars().skip(cps.end));
    if expected == old_text {
        return Ok(()); // no-op step
    }
    d.splice_entry(&para.entry, splices)?;
    let para2 = d.doc.require(target, Some(Kind::Paragraph))?.clone();
    let got = d.doc.para_text(&para2)?.text;
    if got != expected {
        return Err(EngineError::new(
            Code::StateInvariantError,
            Stage::Apply,
            format!("text.replace produced {got:?}, expected {expected:?}"),
        ));
    }
    if let Some(x) = diag {
        d.diags.push(x);
    }
    if preserve {
        let controls: Vec<_> = pt
            .pieces
            .iter()
            .filter(|p| {
                p.cp_start < cps.end
                    && cps.start < p.cp_start + p.cp_len
                    && matches!(p.kind, PieceKind::Control { .. })
            })
            .collect();
        if !controls.is_empty() {
            let after = d.doc.para_text(&para2)?;
            let selected: BTreeSet<_> = pt
                .pieces
                .iter()
                .filter(|p| matches!(p.kind, PieceKind::Control { .. }))
                .enumerate()
                .filter(|(_, p)| p.cp_start < cps.end && cps.start < p.cp_start + p.cp_len)
                .map(|(i, _)| i)
                .collect();
            let offsets: Vec<_> = after
                .pieces
                .iter()
                .filter(|p| matches!(p.kind, PieceKind::Control { .. }))
                .enumerate()
                .filter(|(i, _)| selected.contains(i))
                .map(|(_, p)| {
                    after
                        .grapheme_cps
                        .partition_point(|&cp| cp <= p.cp_start)
                        .saturating_sub(1)
                })
                .collect();
            d.diags.push(
                Diagnostic::error(
                    Code::ControlsPreserved,
                    Stage::Apply,
                    "controls retained in original XML order",
                )
                .with_target(target)
                .with_detail(serde_json::json!({"count":controls.len(),"offsets":offsets})),
            );
        }
    }
    d.anchor(&para2);
    let path = d.doc.doc_of(&para2).path(para2.el);
    d.intents.push(Intent::TextReplace {
        entry: para2.entry.clone(),
        path,
        expected_text: expected.clone(),
    });
    d.prv.push((old_text, expected));
    d.changed.push(target.to_string());
    Ok(())
}

pub(crate) fn cell_set_text(
    d: &mut Draft,
    target: &str,
    text: Option<&str>,
    paragraphs: Option<&[String]>,
    preserve: bool,
) -> Result<()> {
    let cell = d.node(target, Some(Kind::Cell))?;
    if let Some(reason) = d.doc.cell_block_reason(&cell) {
        return Err(EngineError::new(
            Code::Unsupported,
            Stage::Validate,
            "unsupported cell structure",
        )
        .with_target(target)
        .with_reason(reason));
    }
    let paras: Vec<String> = d
        .doc
        .children(target)
        .filter(|p| p.kind == Kind::Paragraph)
        .map(|p| p.id.clone())
        .collect();
    if text.is_some() && paras.len() != 1 {
        return Err(EngineError::new(Code::Unsupported,Stage::Validate,"use paragraphs for a multi-paragraph cell").with_target(target).with_detail(serde_json::json!({"paragraphCount":paras.len(),"hint":"use paragraphs with exactly paragraphCount strings"})));
    }
    let values: Vec<&str> = match (text, paragraphs) {
        (Some(s), None) => vec![s],
        (None, Some(ss)) if ss.len() == paras.len() => ss.iter().map(String::as_str).collect(),
        _ => {
            return Err(EngineError::new(
                Code::InvalidArgument,
                Stage::Validate,
                "supply exactly one of text or paragraphs, matching the cell paragraph count",
            )
            .with_target(target)
            .with_detail(serde_json::json!({"paragraphCount":paras.len()})))
        }
    };
    for value in &values {
        check_new_text(value)?;
    }
    for (id, value) in paras.iter().zip(values) {
        let para = d.node(id, Some(Kind::Paragraph))?;
        let pt = d.doc.para_text(&para)?;
        if pt.text.is_empty() && value.is_empty() {
            continue;
        }
        text_replace(d, id, 0..pt.grapheme_len(), value, preserve)?;
    }
    d.changed.push(target.to_string());
    Ok(())
}
