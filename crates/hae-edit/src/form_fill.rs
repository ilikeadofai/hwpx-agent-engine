//! form.fill: fill empty value cells from exact label cells (0.3 backlog TB-07).
//!
//! A form table usually has a label cell ("성명") next to an empty value cell.
//! For every requested `label → value` pair this planner finds the one cell
//! whose text equals the label, picks the same-row right cell (or, when there
//! is no right cell, the cell directly below it), requires that cell to be
//! empty and structurally editable, and then reuses the `cell.setText` path
//! (merged anchor cells and multi-paragraph cells included).
//!
//! Nothing is guessed: a missing label, more than one matching label cell, no
//! right/below cell, an ambiguous or non-empty target, two labels resolving to
//! the same cell, or any `cell.setText` block reason is reported per label.
//! `strict` (the default) turns any failure into a rejection before the first
//! byte changes; `strict: false` applies the resolvable labels and reports the
//! rest. The whole request is one `EditOp`, so it is one revision and one undo
//! unit and a failing `cell.setText` still drops the entire candidate.

use crate::text::check_new_text;
use crate::{Draft, Intent};
use hae_diag::{Code, Diagnostic, EngineError, Result, Stage};
use hae_model::query::CellAddr;
use hae_model::{Document, Kind, Node};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub const SKIP_LABEL_NOT_FOUND: &str = "label_not_found";
pub const SKIP_AMBIGUOUS_LABEL: &str = "ambiguous_label";
pub const SKIP_NO_CELL_GEOMETRY: &str = "no_cell_geometry";
pub const SKIP_NO_TARGET_CELL: &str = "no_target_cell";
pub const SKIP_AMBIGUOUS_TARGET: &str = "ambiguous_target";
pub const SKIP_TARGET_NOT_EMPTY: &str = "target_not_empty";
pub const SKIP_CONFLICTING_TARGET: &str = "conflicting_target";

/// One applied label → value pair, addressed by the filled cell's nodeId.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FilledCell {
    pub label: String,
    pub value: String,
    pub node_id: String,
    pub entry: String,
    pub path: Vec<usize>,
}

/// One label the request could not fill, with the machine-readable reason.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFill {
    pub label: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// Per-request report, attached to the result (and to `formFill` in a batch).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FormFillReport {
    pub strict: bool,
    pub scope: Option<String>,
    pub filled: Vec<FilledCell>,
    pub skipped: Vec<SkippedFill>,
}

/// Structural description of one filled cell for the response `intents`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FormFillIntent {
    pub label: String,
    pub value: String,
    pub entry: String,
    pub path: Vec<usize>,
}

struct Resolved {
    label: String,
    value: String,
    cell: String,
    entry: String,
    path: Vec<usize>,
}

fn failure(label: &str, reason: &str) -> SkippedFill {
    SkippedFill {
        label: label.to_string(),
        reason: reason.to_string(),
        detail: None,
    }
}

/// Tables the request may search: all of them, one table, or one section.
fn scoped_tables(doc: &Document, scope: Option<&str>) -> Result<Vec<Node>> {
    match scope {
        None => Ok(doc
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Table)
            .cloned()
            .collect()),
        Some(id) => {
            let n = doc.require(id, None)?.clone();
            match n.kind {
                Kind::Table => Ok(vec![n]),
                Kind::Section => Ok(doc
                    .nodes
                    .iter()
                    .filter(|t| t.kind == Kind::Table && doc.is_descendant(t, id))
                    .cloned()
                    .collect()),
                _ => Err(EngineError::new(
                    Code::Unsupported,
                    Stage::Validate,
                    "form.fill scope must be a table or section nodeId",
                )
                .with_reason("form_scope")
                .with_target(id)),
            }
        }
    }
}

fn nearest_table(doc: &Document, n: &Node) -> Option<String> {
    let mut cur = n.parent.clone();
    while let Some(p) = cur {
        let node = doc.node(&p)?;
        if node.kind == Kind::Table {
            return Some(node.id.clone());
        }
        cur = node.parent.clone();
    }
    None
}

/// Direct paragraph children of a cell, in document order (the exact set
/// `cell.setText` addresses).
fn cell_paragraphs(doc: &Document, cell: &Node) -> Vec<String> {
    doc.children(&cell.id)
        .filter(|p| p.kind == Kind::Paragraph)
        .map(|p| p.id.clone())
        .collect()
}

fn label_text(doc: &Document, cell: &Node) -> String {
    cell_paragraphs(doc, cell)
        .iter()
        .map(|id| {
            doc.node(id)
                .and_then(|p| doc.para_text(p).ok())
                .map(|p| p.text)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn cell_full(doc: &Document, cell: &Node) -> bool {
    cell_paragraphs(doc, cell).iter().any(|id| {
        doc.node(id)
            .is_some_and(|p| doc.para_text(p).is_ok_and(|t| !t.text.is_empty()))
    })
}

fn resolve_target(
    doc: &Document,
    table_cells: &[Node],
    label_cell: &Node,
) -> std::result::Result<Node, SkippedFill> {
    let label = label_cell.id.as_str();
    let skip = |reason: &str| SkippedFill {
        label: String::new(),
        reason: reason.to_string(),
        detail: Some(json!({"labelCell": label})),
    };
    let Some(addr): Option<CellAddr> = doc.cell_addr(label_cell) else {
        return Err(skip(SKIP_NO_CELL_GEOMETRY));
    };
    let addrs: Vec<(Node, CellAddr)> = table_cells
        .iter()
        .filter_map(|c| doc.cell_addr(c).map(|a| (c.clone(), a)))
        .collect();
    let overlaps_rows = |a: &CellAddr, r: u32| a.row <= r && r < a.row + a.row_span;
    let overlaps_cols = |a: &CellAddr, c: u32| a.col <= c && c < a.col + a.col_span;
    let right: Vec<&(Node, CellAddr)> = addrs
        .iter()
        .filter(|(_, a)| a.col == addr.col + addr.col_span && overlaps_rows(a, addr.row))
        .collect();
    let below: Vec<&(Node, CellAddr)> = addrs
        .iter()
        .filter(|(_, a)| a.row == addr.row + addr.row_span && overlaps_cols(a, addr.col))
        .collect();
    let ambiguous = |candidates: &[&(Node, CellAddr)]| SkippedFill {
        label: String::new(),
        reason: SKIP_AMBIGUOUS_TARGET.to_string(),
        detail: Some(json!({
            "labelCell": label,
            "targets": candidates.iter().map(|(c, _)| c.id.clone()).collect::<Vec<_>>(),
        })),
    };
    if right.len() > 1 {
        return Err(ambiguous(&right));
    }
    if let Some((cell, _)) = right.first() {
        return Ok((*cell).clone());
    }
    if below.len() > 1 {
        return Err(ambiguous(&below));
    }
    match below.first() {
        Some((cell, _)) => Ok((*cell).clone()),
        None => Err(skip(SKIP_NO_TARGET_CELL)),
    }
}

/// Resolve every label against the pre-request document, then apply the
/// successful ones. Resolution never consults the state produced by an
/// earlier fill.
pub(crate) fn form_fill(
    d: &mut Draft,
    values: &BTreeMap<String, String>,
    scope: Option<&str>,
    strict: bool,
) -> Result<()> {
    if values.is_empty() {
        return Err(EngineError::new(
            Code::InvalidArgument,
            Stage::Validate,
            "form.fill values must contain at least one label",
        ));
    }
    for (label, value) in values {
        if label.is_empty() {
            return Err(EngineError::new(
                Code::InvalidArgument,
                Stage::Validate,
                "form.fill labels must not be empty",
            ));
        }
        if label.contains(['\t', '\n', '\r']) {
            return Err(EngineError::new(
                Code::InvalidArgument,
                Stage::Validate,
                "form.fill labels must be single-line text without tabs or line breaks",
            ));
        }
        check_new_text(value)?;
        if value.is_empty() {
            return Err(EngineError::new(
                Code::InvalidArgument,
                Stage::Validate,
                "form.fill values must not be empty",
            ));
        }
    }
    let tables = scoped_tables(&d.doc, scope)?;
    // Cells of the scoped tables, in document order; nested-table cells belong
    // to their own nearest table and are never matched by an outer table.
    let mut cells: Vec<(Node, String)> = Vec::new();
    let mut table_cells: BTreeMap<String, Vec<Node>> = BTreeMap::new();
    for table in &tables {
        let own: Vec<Node> = d
            .doc
            .nodes
            .iter()
            .filter(|c| {
                c.kind == Kind::Cell && nearest_table(&d.doc, c).as_deref() == Some(&table.id)
            })
            .cloned()
            .collect();
        cells.extend(own.iter().map(|c| (c.clone(), table.id.clone())));
        table_cells.insert(table.id.clone(), own);
    }

    let mut resolved: Vec<Resolved> = Vec::new();
    let mut skipped: Vec<SkippedFill> = Vec::new();
    for (label, value) in values {
        let matches: Vec<&Node> = cells
            .iter()
            .filter(|(c, _)| label_text(&d.doc, c) == *label)
            .map(|(c, _)| c)
            .collect();
        if matches.is_empty() {
            let mut f = failure(label, SKIP_LABEL_NOT_FOUND);
            f.detail = Some(json!({"matchCount": 0}));
            skipped.push(f);
            continue;
        }
        if matches.len() > 1 {
            let mut f = failure(label, SKIP_AMBIGUOUS_LABEL);
            f.detail = Some(json!({
                "matchCount": matches.len(),
                "labelCells": matches.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            }));
            skipped.push(f);
            continue;
        }
        let label_cell = matches[0];
        let table = table_cells
            .iter()
            .find(|(_, own)| own.iter().any(|c| c.id == label_cell.id))
            .map(|(id, _)| id.clone())
            .expect("matched cell belongs to a scoped table");
        let label_resolved = |f: SkippedFill| SkippedFill {
            label: label.clone(),
            ..f
        };
        let target = match resolve_target(&d.doc, &table_cells[&table], label_cell) {
            Ok(t) => t,
            Err(f) => {
                skipped.push(label_resolved(f));
                continue;
            }
        };
        if cell_full(&d.doc, &target) {
            let mut f = failure(label, SKIP_TARGET_NOT_EMPTY);
            f.detail =
                Some(json!({"target": target.id, "targetText": label_text(&d.doc, &target)}));
            skipped.push(f);
            continue;
        }
        if let Some(reason) = d.doc.cell_set_text_block_reason(&target) {
            let mut f = failure(label, &reason);
            f.detail = Some(json!({"target": target.id}));
            skipped.push(f);
            continue;
        }
        let path = d.doc.doc_of(&target).path(target.el);
        resolved.push(Resolved {
            label: label.clone(),
            value: value.clone(),
            cell: target.id.clone(),
            entry: target.entry.clone(),
            path,
        });
    }

    // Two labels may not resolve to the same value cell.
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut conflicts: BTreeSet<String> = BTreeSet::new();
    for r in &resolved {
        if seen.insert(r.cell.clone(), 0).is_some() {
            conflicts.insert(r.cell.clone());
        }
    }
    if !conflicts.is_empty() {
        for cell in &conflicts {
            for r in resolved.iter().filter(|r| &r.cell == cell) {
                let mut f = failure(&r.label, SKIP_CONFLICTING_TARGET);
                f.detail = Some(json!({"target": cell}));
                skipped.push(f);
            }
        }
        resolved.retain(|r| !conflicts.contains(&r.cell));
    }
    skipped.sort_by(|a, b| a.label.cmp(&b.label));

    if strict && !skipped.is_empty() {
        let reasons: BTreeSet<&str> = skipped.iter().map(|s| s.reason.as_str()).collect();
        let code = if reasons == BTreeSet::from([SKIP_LABEL_NOT_FOUND]) {
            Code::TargetNotFound
        } else if reasons == BTreeSet::from([SKIP_AMBIGUOUS_LABEL]) {
            Code::AmbiguousTarget
        } else {
            Code::Unsupported
        };
        return Err(EngineError::new(
            code,
            Stage::Validate,
            format!(
                "form.fill could not resolve {}/{} labels",
                skipped.len(),
                values.len()
            ),
        )
        .with_reason("form_fill_rejected")
        .with_detail(json!({"strict": true, "failures": skipped})));
    }

    let intent_start = d.intents.len();
    let mut filled: Vec<FilledCell> = Vec::new();
    for r in &resolved {
        let cell = d.doc.require(&r.cell, Some(Kind::Cell))?.clone();
        let paragraphs = cell_paragraphs(&d.doc, &cell);
        let mut texts = vec![r.value.clone()];
        texts.resize(paragraphs.len(), String::new());
        crate::text::cell_set_text(d, &r.cell, None, Some(&texts), false)?;
        filled.push(FilledCell {
            label: r.label.clone(),
            value: r.value.clone(),
            node_id: r.cell.clone(),
            entry: r.entry.clone(),
            path: r.path.clone(),
        });
    }
    if !filled.is_empty() {
        d.intents.truncate(intent_start);
        d.intents.push(Intent::FormFill {
            fills: filled
                .iter()
                .map(|f| FormFillIntent {
                    label: f.label.clone(),
                    value: f.value.clone(),
                    entry: f.entry.clone(),
                    path: f.path.clone(),
                })
                .collect(),
        });
    }
    let report = FormFillReport {
        strict,
        scope: scope.map(str::to_string),
        filled,
        skipped,
    };
    if !report.skipped.is_empty() {
        d.diags.push(
            Diagnostic::error(
                Code::FormFillSkipped,
                Stage::Apply,
                format!(
                    "{} label(s) were skipped; only resolvable labels were filled",
                    report.skipped.len()
                ),
            )
            .with_detail(json!({
                "count": report.skipped.len(),
                "skipped": report.skipped,
            })),
        );
    }
    d.form_fill.push(report);
    Ok(())
}
