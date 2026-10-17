//! text.replaceAll (0.2 design §7-2).
//!
//! Exact codepoint matches are chosen leftmost and non-overlapping per
//! paragraph. A match is replaced only when it lies wholly inside a complete
//! (uncapped) textSpans interval of an editable paragraph; every other match is
//! skipped and reported by reason and location. All splices of an entry are
//! planned against the unmodified document and applied in one step, so the
//! command is atomic by construction (the candidate is dropped on any error).

use crate::text::{check_new_text, plan_replace, Splice};
use crate::{Draft, Intent};
use hae_diag::{Code, Diagnostic, EngineError, Result, Stage};
use hae_model::para::TextSpan;
use hae_model::{is_hp, Kind, Node, ParaText, PieceKind};
use hae_package::{NS_HC, NS_HH, NS_HP, NS_HP10, NS_HS};
use hae_xml::XmlDoc;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

pub const DEFAULT_MAX_MATCHES: usize = 1000;
pub const HARD_MAX_MATCHES: usize = 10_000;
/// Previews returned in a response; the counts always cover every match.
const MAX_LISTED_MATCHES: usize = 200;
const MAX_LOCATIONS_PER_REASON: usize = 50;
const CONTEXT_CHARS: usize = 16;

pub const SKIP_CROSSES_CONTROL: &str = "crosses_control";
pub const SKIP_GRAPHEME_BOUNDARY: &str = "grapheme_boundary";
pub const SKIP_UNSUPPORTED_MARKUP: &str = "unsupported_markup";
pub const SKIP_NOT_EDITABLE: &str = "paragraph_not_editable";

/// Half-open grapheme range in the paragraph as it was before the command.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct GraphemeRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MatchPreview {
    pub node_id: String,
    pub range: GraphemeRange,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedLocation {
    pub node_id: String,
    pub range: GraphemeRange,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedGroup {
    pub reason: &'static str,
    pub count: usize,
    pub locations: Vec<SkippedLocation>,
    pub locations_truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceAllReport {
    pub find: String,
    pub replacement: String,
    pub scope: Option<String>,
    pub max_matches: usize,
    pub replacement_count: usize,
    pub paragraph_count: usize,
    pub skipped_count: usize,
    pub skipped: Vec<SkippedGroup>,
    pub matches: Vec<MatchPreview>,
    pub matches_truncated: bool,
}

struct Planned {
    node: Node,
    pt: ParaText,
    chosen: Vec<(Range<usize>, GraphemeRange)>,
}

/// Leftmost, non-overlapping exact matches as codepoint ranges.
fn codepoint_matches(text: &str, find: &str) -> Vec<Range<usize>> {
    let n = find.chars().count();
    let mut out = Vec::new();
    let (mut cp, mut last) = (0, 0);
    for (byte, _) in text.match_indices(find) {
        cp += text[last..byte].chars().count();
        out.push(cp..cp + n);
        cp += n;
        last = byte + find.len();
    }
    out
}

/// Attribute tokens of elements the engine does not interpret. An edited
/// paragraph (or an ancestor/descendant) whose `id` they name is left alone.
fn opaque_tokens(doc: &XmlDoc) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    for (_, e) in doc.iter() {
        let known = matches!(
            e.ns.as_deref(),
            Some(NS_HP) | Some(NS_HP10) | Some(NS_HC) | Some(NS_HH) | Some(NS_HS)
        );
        for a in &e.attrs {
            let opaque = !known
                || a.ns.as_deref().is_some_and(|ns| {
                    ![
                        NS_HP,
                        NS_HP10,
                        NS_HC,
                        NS_HH,
                        NS_HS,
                        "http://www.w3.org/2000/xmlns/",
                    ]
                    .contains(&ns)
                });
            if opaque && !a.qname.starts_with("xmlns") {
                tokens.extend(a.value.split_whitespace().map(str::to_string));
            }
        }
    }
    tokens
}

fn paragraph_blocked(d: &Draft, n: &Node, cache: &mut BTreeMap<String, BTreeSet<String>>) -> bool {
    if n.locked.is_some() {
        return true;
    }
    let doc = d.doc.doc_of(n);
    let top = doc
        .ancestors(n.el)
        .find(|&a| doc.el(a).parent == Some(doc.root()))
        .unwrap_or(n.el);
    if !(is_hp(doc.el(top).ns.as_deref()) && doc.el(top).local == "p") {
        return true;
    }
    let tokens = cache
        .entry(n.entry.clone())
        .or_insert_with(|| opaque_tokens(doc));
    !tokens.is_empty()
        && std::iter::once(n.el)
            .chain(doc.ancestors(n.el))
            .chain(doc.descendants(n.el))
            .filter_map(|i| doc.el(i).attr_value("id"))
            .any(|id| tokens.contains(id))
}

/// `None` means replaceable; otherwise the skip reason. The returned range is
/// the match in grapheme coordinates (rounded outward when misaligned).
fn classify(
    pt: &ParaText,
    spans: &[TextSpan],
    m: &Range<usize>,
    blocked: bool,
) -> (Option<&'static str>, GraphemeRange) {
    let cps = &pt.grapheme_cps;
    let (a, b) = (pt.grapheme_at_cp(m.start), pt.grapheme_at_cp(m.end));
    let range = GraphemeRange {
        start: a.unwrap_or_else(|| cps.partition_point(|&c| c <= m.start) - 1),
        end: b.unwrap_or_else(|| cps.partition_point(|&c| c < m.end).min(cps.len() - 1)),
    };
    if blocked {
        return (Some(SKIP_NOT_EDITABLE), range);
    }
    if a.is_none() || b.is_none() {
        return (Some(SKIP_GRAPHEME_BOUNDARY), range);
    }
    if spans
        .iter()
        .any(|s| s.start <= range.start && range.end <= s.end)
    {
        return (None, range);
    }
    let crosses = pt
        .pieces
        .iter()
        .filter(|p| p.cp_start < m.end && m.start < p.cp_start + p.cp_len)
        .any(|p| matches!(p.kind, PieceKind::Control { .. }));
    (
        Some(if crosses {
            SKIP_CROSSES_CONTROL
        } else {
            SKIP_UNSUPPORTED_MARKUP
        }),
        range,
    )
}

fn preview(chars: &[char], m: &Range<usize>, replacement: &str) -> (String, String) {
    let a = m.start.saturating_sub(CONTEXT_CHARS);
    let b = (m.end + CONTEXT_CHARS).min(chars.len());
    let lead = if a > 0 { "…" } else { "" };
    let tail = if b < chars.len() { "…" } else { "" };
    let left: String = chars[a..m.start].iter().collect();
    let right: String = chars[m.end..b].iter().collect();
    let found: String = chars[m.start..m.end].iter().collect();
    (
        format!("{lead}{left}{found}{right}{tail}"),
        format!("{lead}{left}{replacement}{right}{tail}"),
    )
}

pub(crate) fn text_replace_all(
    d: &mut Draft,
    find: &str,
    new: &str,
    scope: Option<&str>,
    max_matches: usize,
) -> Result<()> {
    check_new_text(new)?;
    if find.is_empty() {
        return Err(EngineError::new(
            Code::InvalidRequest,
            Stage::Validate,
            "find must not be empty",
        ));
    }
    if max_matches == 0 || max_matches > HARD_MAX_MATCHES {
        return Err(EngineError::new(
            Code::InvalidArgument,
            Stage::Validate,
            format!("maxMatches must be between 1 and {HARD_MAX_MATCHES}"),
        ));
    }
    if let Some(s) = scope {
        d.doc.require(s, None)?;
    }
    let mut report = ReplaceAllReport {
        find: find.to_string(),
        replacement: new.to_string(),
        scope: scope.map(str::to_string),
        max_matches,
        replacement_count: 0,
        paragraph_count: 0,
        skipped_count: 0,
        skipped: Vec::new(),
        matches: Vec::new(),
        matches_truncated: false,
    };
    if find == new {
        // Nothing can change; the verifier applies the same rule.
        d.replace_all.push(report);
        return Ok(());
    }

    let candidates: Vec<Node> = d
        .doc
        .nodes
        .iter()
        .filter(|n| {
            n.kind == Kind::Paragraph
                && scope.is_none_or(|s| n.id == s || d.doc.is_descendant(n, s))
        })
        .cloned()
        .collect();
    let mut opaque_cache = BTreeMap::new();
    let mut planned: Vec<Planned> = Vec::new();
    let mut groups: BTreeMap<&'static str, SkippedGroup> = BTreeMap::new();
    let mut replaced = 0usize;
    for node in candidates {
        let pt = d.doc.para_text(&node)?;
        if !pt.text.contains(find) {
            continue;
        }
        let blocked = paragraph_blocked(d, &node, &mut opaque_cache);
        let spans = if blocked {
            Vec::new()
        } else {
            pt.all_replaceable_spans(d.doc.doc_of(&node))
        };
        let chars: Vec<char> = pt.text.chars().collect();
        let mut chosen = Vec::new();
        for m in codepoint_matches(&pt.text, find) {
            match classify(&pt, &spans, &m, blocked) {
                (None, range) => {
                    replaced += 1;
                    if replaced > max_matches {
                        return Err(EngineError::new(
                            Code::ResourceLimit,
                            Stage::Validate,
                            format!("more than maxMatches={max_matches} replaceable matches"),
                        )
                        .with_detail(serde_json::json!({
                            "maxMatches": max_matches,
                            "hint": "narrow the search, set scope, or raise maxMatches"
                        })));
                    }
                    if report.matches.len() < MAX_LISTED_MATCHES {
                        let (before, after) = preview(&chars, &m, new);
                        report.matches.push(MatchPreview {
                            node_id: node.id.clone(),
                            range,
                            before,
                            after,
                        });
                    } else {
                        report.matches_truncated = true;
                    }
                    chosen.push((m, range));
                }
                (Some(reason), range) => {
                    let g = groups.entry(reason).or_insert_with(|| SkippedGroup {
                        reason,
                        count: 0,
                        locations: Vec::new(),
                        locations_truncated: false,
                    });
                    g.count += 1;
                    if g.locations.len() < MAX_LOCATIONS_PER_REASON {
                        g.locations.push(SkippedLocation {
                            node_id: node.id.clone(),
                            range,
                        });
                    } else {
                        g.locations_truncated = true;
                    }
                }
            }
        }
        if !chosen.is_empty() {
            planned.push(Planned { node, pt, chosen });
        }
    }
    report.replacement_count = replaced;
    report.paragraph_count = planned.len();
    report.skipped_count = groups.values().map(|g| g.count).sum();
    report.skipped = groups.into_values().collect();

    // Plan every splice against the unmodified document, then apply per entry.
    let mut by_entry: BTreeMap<String, Vec<Splice>> = BTreeMap::new();
    let mut expected = Vec::with_capacity(planned.len());
    let mut collapsed = 0usize;
    for pl in &planned {
        let chars: Vec<char> = pl.pt.text.chars().collect();
        let (mut text, mut at) = (String::new(), 0);
        for (m, _) in &pl.chosen {
            let (splices, diag) = plan_replace(d, &pl.node, &pl.pt, m.clone(), new, false)?;
            by_entry
                .entry(pl.node.entry.clone())
                .or_default()
                .extend(splices);
            collapsed += usize::from(diag.is_some());
            text.extend(&chars[at..m.start]);
            text.push_str(new);
            at = m.end;
        }
        text.extend(&chars[at..]);
        expected.push(text);
    }
    for (entry, splices) in by_entry {
        d.splice_entry(&entry, splices)?;
    }
    for (pl, expected) in planned.iter().zip(expected) {
        let para = d.doc.require(&pl.node.id, Some(Kind::Paragraph))?.clone();
        let got = d.doc.para_text(&para)?.text;
        if got != expected {
            return Err(EngineError::new(
                Code::StateInvariantError,
                Stage::Apply,
                format!("text.replaceAll produced {got:?}, expected {expected:?}"),
            ));
        }
        d.anchor(&para);
        d.prv.push((pl.pt.text.clone(), expected));
        d.changed.push(pl.node.id.clone());
    }
    if collapsed > 0 {
        d.diags.push(
            Diagnostic::error(
                Code::FormatCollapsed,
                Stage::Apply,
                "replacements use the first character's shape; other shapes inside matches were collapsed",
            )
            .with_detail(serde_json::json!({ "matches": collapsed })),
        );
    }
    if report.skipped_count > 0 {
        d.diags.push(
            Diagnostic::error(
                Code::MatchesSkipped,
                Stage::Apply,
                format!(
                    "{} match(es) were skipped because they cross objects, graphemes or unsupported markup",
                    report.skipped_count
                ),
            )
            .with_detail(serde_json::json!({
                "count": report.skipped_count,
                "reasons": report.skipped,
            })),
        );
    }
    if replaced > 0 {
        d.intents.push(Intent::TextReplaceAll {
            find: find.to_string(),
            replacement: new.to_string(),
            replacement_count: replaced,
            paragraph_count: planned.len(),
        });
    }
    d.replace_all.push(report);
    Ok(())
}
