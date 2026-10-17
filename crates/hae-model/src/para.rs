//! Paragraph text view: the paragraph's own runs flattened into pieces.
//!
//! Text in `hp:t` becomes text pieces; `hp:tab`/`hp:lineBreak` and objects in
//! a run (tables, pictures, controls...) become control pieces with a
//! placeholder character; other children of `hp:t` (fwSpace, markpen, unknown)
//! become "other" pieces that a text range may not cover (spec §6).

use crate::is_hp;
use hae_xml::{Item, XmlDoc};
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::BTreeSet;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub const OBJECT_CHAR: char = '\u{FFFC}';
/// Same hp:t attribute allowlist used by the mutation planner.
pub const TEXT_ATTRS: &[&str] = &["charPrIDRef"];
pub const MAX_TEXT_SPANS: usize = 64;

pub fn text_element_supported(doc: &XmlDoc, t: usize) -> bool {
    doc.el(t).attrs.iter().all(|a| {
        a.qname == "xmlns"
            || a.qname.starts_with("xmlns:")
            || (a.ns.is_none() && TEXT_ATTRS.contains(&a.local.as_str()))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSpans {
    pub text_spans: Vec<TextSpan>,
    pub text_spans_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PieceKind {
    /// Character data inside `hp:t` (`raw` maps 1:1 to text bytes).
    Text { raw: Range<usize> },
    /// A character or predefined entity reference inside `hp:t`.
    Ref { raw: Range<usize> },
    /// CDATA inside `hp:t`.
    CData { raw: Range<usize> },
    /// Tab or line break inside `hp:t`, or an object/control in the run.
    Control { el: usize, name: String },
    /// Any other element inside `hp:t` (not plain text, not a known control).
    Other { el: usize, name: String },
}

#[derive(Debug, Clone)]
pub struct Piece {
    /// Codepoint offset in the paragraph text.
    pub cp_start: usize,
    pub cp_len: usize,
    pub kind: PieceKind,
    /// Element index of the run.
    pub run: usize,
    /// Element index of the `hp:t`, if inside one.
    pub t: Option<usize>,
}

/// Flattened text of one paragraph.
#[derive(Debug, Clone)]
pub struct ParaText {
    pub para: usize,
    pub text: String,
    pub pieces: Vec<Piece>,
    /// Codepoint offset of each grapheme boundary (len = graphemes + 1).
    pub grapheme_cps: Vec<usize>,
    /// Zero-width lexical barriers, independently enforced by the save gate.
    pub verification_barriers: BTreeSet<usize>,
}

fn placeholder(name: &str) -> char {
    match name {
        "tab" => '\t',
        "lineBreak" => '\n',
        _ => OBJECT_CHAR,
    }
}

impl ParaText {
    pub fn build(doc: &XmlDoc, para: usize) -> ParaText {
        let mut text = String::new();
        let mut pieces = Vec::new();
        let mut cp = 0usize;
        let mut verification_barriers = BTreeSet::new();
        let push = |text: &mut String,
                    pieces: &mut Vec<Piece>,
                    cp: &mut usize,
                    s: &str,
                    kind: PieceKind,
                    run: usize,
                    t: Option<usize>| {
            let n = s.chars().count();
            text.push_str(s);
            pieces.push(Piece {
                cp_start: *cp,
                cp_len: n,
                kind,
                run,
                t,
            });
            *cp += n;
        };
        for &run in &doc.el(para).children {
            let r = doc.el(run);
            if !(is_hp(r.ns.as_deref()) && r.local == "run") {
                continue;
            }
            for item in &r.items {
                let Item::Element(c) = item else {
                    if item.text().is_some_and(|s| !s.trim().is_empty())
                        || matches!(item, Item::Comment(_) | Item::Pi(_))
                    {
                        verification_barriers.insert(cp);
                    }
                    continue;
                };
                let c = *c;
                let ce = doc.el(c);
                if is_hp(ce.ns.as_deref()) && ce.local == "t" {
                    for it in &ce.items {
                        match it {
                            Item::Text { raw, text: s } => push(
                                &mut text,
                                &mut pieces,
                                &mut cp,
                                s,
                                PieceKind::Text { raw: raw.clone() },
                                run,
                                Some(c),
                            ),
                            Item::Ref { raw, text: s } => push(
                                &mut text,
                                &mut pieces,
                                &mut cp,
                                s,
                                PieceKind::Ref { raw: raw.clone() },
                                run,
                                Some(c),
                            ),
                            Item::CData { raw, text: s } => {
                                push(
                                    &mut text,
                                    &mut pieces,
                                    &mut cp,
                                    s,
                                    PieceKind::CData { raw: raw.clone() },
                                    run,
                                    Some(c),
                                );
                                verification_barriers.insert(cp);
                            }
                            Item::Element(e) => {
                                let ee = doc.el(*e);
                                let name = ee.local.clone();
                                let kind = if is_hp(ee.ns.as_deref())
                                    && (name == "tab" || name == "lineBreak")
                                {
                                    PieceKind::Control {
                                        el: *e,
                                        name: name.clone(),
                                    }
                                } else {
                                    PieceKind::Other {
                                        el: *e,
                                        name: name.clone(),
                                    }
                                };
                                let ph = match &kind {
                                    PieceKind::Control { .. } => placeholder(&name),
                                    _ => match name.as_str() {
                                        "fwSpace" => '\u{3000}',
                                        "nbSpace" => '\u{A0}',
                                        "hyphen" => '-',
                                        _ => OBJECT_CHAR,
                                    },
                                };
                                push(
                                    &mut text,
                                    &mut pieces,
                                    &mut cp,
                                    &ph.to_string(),
                                    kind,
                                    run,
                                    Some(c),
                                );
                            }
                            Item::Comment(_) | Item::Pi(_) => {
                                verification_barriers.insert(cp);
                            }
                        }
                    }
                } else {
                    // secPr and colPr-only controls are layout settings, not text positions.
                    let skip = is_hp(ce.ns.as_deref()) && ce.local == "secPr";
                    if !skip {
                        push(
                            &mut text,
                            &mut pieces,
                            &mut cp,
                            &OBJECT_CHAR.to_string(),
                            PieceKind::Control {
                                el: c,
                                name: ce.local.clone(),
                            },
                            run,
                            None,
                        );
                    }
                }
            }
        }
        let mut grapheme_cps = vec![0];
        let mut acc = 0;
        for g in text.graphemes(true) {
            acc += g.chars().count();
            grapheme_cps.push(acc);
        }
        ParaText {
            para,
            text,
            pieces,
            grapheme_cps,
            verification_barriers,
        }
    }

    pub fn field_in_range(&self, doc: &XmlDoc, range: Range<usize>) -> bool {
        self.pieces
            .iter()
            .filter(|p| p.cp_start < range.end && range.start < p.cp_start + p.cp_len)
            .any(|p| {
                let el = match p.kind {
                    PieceKind::Control { el, .. } | PieceKind::Other { el, .. } => el,
                    _ => return false,
                };
                std::iter::once(el).chain(doc.descendants(el)).any(|i| {
                    is_hp(doc.el(i).ns.as_deref())
                        && matches!(doc.el(i).local.as_str(), "fieldBegin" | "fieldEnd")
                })
            })
    }

    /// Existing run and lexical insertion byte at each object-only boundary.
    pub fn object_insert_sites(&self, doc: &XmlDoc) -> Vec<(usize, usize, usize)> {
        if self.pieces.is_empty()
            || !self.verification_barriers.is_empty()
            || self
                .pieces
                .iter()
                .any(|p| !matches!(p.kind, PieceKind::Control { .. }))
        {
            return vec![];
        }
        let anchor = |piece: &Piece, cp: usize, before: bool| {
            let el = match piece.kind {
                PieceKind::Control { el, .. } => el,
                _ => return None,
            };
            let unit = piece.t.unwrap_or(el);
            if let Some(t) = piece.t {
                let mut ps = self.pieces.iter().filter(|p| p.t == Some(t));
                let edge = if before {
                    ps.next().map(|p| p.cp_start)
                } else {
                    ps.next_back().map(|p| p.cp_start + p.cp_len)
                };
                if edge != Some(cp) {
                    return None;
                }
            }
            if doc.el(unit).parent != Some(piece.run) {
                return None;
            }
            Some((
                piece.run,
                if before {
                    doc.el(unit).outer().start
                } else {
                    doc.el(unit).outer().end
                },
            ))
        };
        self.grapheme_cps
            .iter()
            .enumerate()
            .filter_map(|(g, &cp)| {
                let right = self
                    .pieces
                    .iter()
                    .find(|p| p.cp_start == cp)
                    .and_then(|p| anchor(p, cp, true));
                let left = self
                    .pieces
                    .iter()
                    .rev()
                    .find(|p| p.cp_start + p.cp_len == cp)
                    .and_then(|p| anchor(p, cp, false));
                right.or(left).map(|(run, at)| (g, run, at))
            })
            .collect()
    }

    /// Whether whole-paragraph preserve replacement has a supported existing
    /// text fragment. Object subtrees stay opaque and are never interpreted.
    pub fn preserve_replaceable(&self, doc: &XmlDoc) -> bool {
        let len = self.text.chars().count();
        if self.field_in_range(doc, 0..len) {
            return false;
        }
        if self.verification_barriers.iter().any(|&p| 0 < p && p < len) {
            return false;
        }
        let mut text = false;
        for p in &self.pieces {
            if p.t.is_some_and(|t| !text_element_supported(doc, t)) {
                return false;
            }
            match p.kind {
                PieceKind::Text { .. } | PieceKind::Ref { .. } => {
                    if self
                        .text
                        .chars()
                        .skip(p.cp_start)
                        .take(p.cp_len)
                        .any(|c| ['\t', '\n', '\r'].contains(&c))
                    {
                        return false;
                    }
                    text |= p.cp_len > 0;
                }
                PieceKind::Control { .. } => {}
                _ => return false,
            }
        }
        if text {
            return true;
        }
        // The first empty fragment is authoritative; do not bypass unknown
        // attributes by silently choosing a later run/shape.
        doc.el(self.para)
            .children
            .iter()
            .copied()
            .filter(|&r| is_hp(doc.el(r).ns.as_deref()) && doc.el(r).local == "run")
            .flat_map(|r| doc.el(r).children.iter().copied())
            .find(|&t| {
                is_hp(doc.el(t).ns.as_deref())
                    && doc.el(t).local == "t"
                    && doc.el(t).items.is_empty()
            })
            .is_some_and(|t| text_element_supported(doc, t))
    }

    pub fn empty_insert_reason(&self, doc: &XmlDoc) -> Option<&'static str> {
        if !self.text.is_empty() || !self.verification_barriers.is_empty() {
            return Some("verify_unsupported_markup");
        }
        let run = doc
            .el(self.para)
            .children
            .iter()
            .copied()
            .find(|&i| is_hp(doc.el(i).ns.as_deref()) && doc.el(i).local == "run");
        let Some(run) = run else {
            return Some("no_run");
        };
        for &t in &doc.el(run).children {
            if is_hp(doc.el(t).ns.as_deref()) && doc.el(t).local == "t" {
                if !text_element_supported(doc, t) {
                    return Some("unsupported_content_in_range");
                }
                if !doc.el(t).items.is_empty() {
                    return Some("verify_unsupported_markup");
                }
                break;
            }
        }
        None
    }

    /// Maximal editable intervals, rounded inward to whole graphemes. A grapheme
    /// touching a control or uninterpreted hp:t is excluded in its entirety.
    pub fn replaceable_spans(&self, doc: &XmlDoc) -> TextSpans {
        let mut result = TextSpans::default();
        if self.text.is_empty() && self.empty_insert_reason(doc).is_none() {
            result.text_spans.push(TextSpan { start: 0, end: 0 });
            return result;
        }
        let sites = self.object_insert_sites(doc);
        if !sites.is_empty() {
            result.text_spans_truncated = sites.len() > MAX_TEXT_SPANS;
            result.text_spans = sites
                .into_iter()
                .take(MAX_TEXT_SPANS)
                .map(|(p, _, _)| TextSpan { start: p, end: p })
                .collect();
            return result;
        }
        let mut start = None;
        let mut end = 0;
        let emit = |start: usize, end: usize, result: &mut TextSpans| {
            let a = self.grapheme_cps.partition_point(|&cp| cp < start);
            let b = self
                .grapheme_cps
                .partition_point(|&cp| cp <= end)
                .saturating_sub(1);
            if a < b {
                if result.text_spans.len() == MAX_TEXT_SPANS {
                    result.text_spans_truncated = true;
                } else {
                    result.text_spans.push(TextSpan { start: a, end: b });
                }
            }
        };
        let chars: Vec<_> = self.text.chars().collect();
        for piece in &self.pieces {
            let safe = matches!(piece.kind, PieceKind::Text { .. } | PieceKind::Ref { .. })
                && piece.t.is_some_and(|t| text_element_supported(doc, t));
            for (cp, ch) in chars
                .iter()
                .enumerate()
                .skip(piece.cp_start)
                .take(piece.cp_len)
            {
                let safe = safe && !['\t', '\n', '\r'].contains(ch);
                if !safe || self.verification_barriers.contains(&cp) {
                    if let Some(a) = start.take() {
                        emit(a, end, &mut result);
                        if result.text_spans_truncated {
                            return result;
                        }
                    }
                }
                if safe {
                    start.get_or_insert(cp);
                    end = cp + 1;
                }
            }
        }
        if let Some(a) = start {
            emit(a, end, &mut result);
        }
        result
    }

    pub fn grapheme_len(&self) -> usize {
        self.grapheme_cps.len() - 1
    }

    /// Codepoint range for a grapheme range, if valid.
    pub fn cp_range(&self, g: Range<usize>) -> Option<Range<usize>> {
        if g.start > g.end || g.end > self.grapheme_len() {
            return None;
        }
        Some(self.grapheme_cps[g.start]..self.grapheme_cps[g.end])
    }

    /// Grapheme index for a codepoint offset if it is a boundary.
    pub fn grapheme_at_cp(&self, cp: usize) -> Option<usize> {
        self.grapheme_cps.binary_search(&cp).ok()
    }

    /// Plain text for display: placeholders included.
    pub fn display(&self) -> &str {
        &self.text
    }

    /// Codepoint-exact search (spec §6/§7). Returns grapheme-aligned matches
    /// and the number of matches whose ends are not grapheme boundaries.
    pub fn find_all(&self, needle: &str) -> (Vec<Range<usize>>, usize) {
        if needle.is_empty() {
            return (Vec::new(), 0);
        }
        let hay: Vec<char> = self.text.chars().collect();
        let nd: Vec<char> = needle.chars().collect();
        let mut out = Vec::new();
        let mut unaligned = 0;
        if nd.len() > hay.len() {
            return (out, 0);
        }
        for s in 0..=hay.len() - nd.len() {
            if hay[s..s + nd.len()] == nd[..] {
                match (self.grapheme_at_cp(s), self.grapheme_at_cp(s + nd.len())) {
                    (Some(a), Some(b)) => out.push(a..b),
                    _ => unaligned += 1,
                }
            }
        }
        (out, unaligned)
    }
}
