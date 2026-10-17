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
            for &c in &r.children {
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
                            Item::CData { raw, text: s } => push(
                                &mut text,
                                &mut pieces,
                                &mut cp,
                                s,
                                PieceKind::CData { raw: raw.clone() },
                                run,
                                Some(c),
                            ),
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
                            Item::Comment(_) | Item::Pi(_) => {}
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
        }
    }

    /// Maximal editable intervals, rounded inward to whole graphemes. A grapheme
    /// touching a control or uninterpreted hp:t is excluded in its entirety.
    pub fn replaceable_spans(&self, doc: &XmlDoc) -> TextSpans {
        let mut result = TextSpans::default();
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
        for piece in &self.pieces {
            if piece.cp_len == 0 {
                continue;
            }
            let safe = matches!(
                piece.kind,
                PieceKind::Text { .. } | PieceKind::Ref { .. } | PieceKind::CData { .. }
            ) && piece.t.is_some_and(|t| text_element_supported(doc, t));
            if safe {
                start.get_or_insert(piece.cp_start);
                end = piece.cp_start + piece.cp_len;
            } else if let Some(a) = start.take() {
                emit(a, end, &mut result);
                if result.text_spans_truncated {
                    return result;
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
