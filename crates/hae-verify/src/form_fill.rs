//! Independent expected-byte derivation for form.fill. No model/edit dependency.
//!
//! The gate re-derives the label cells, the right/below value cell, emptiness,
//! the per-label skip set and the exact expected payload bytes from the request
//! and the pre-edit XML alone. A candidate that changes a different cell,
//! writes a different value, fills a skipped label or misses a resolvable one
//! fails the final payload comparison.

use crate::text::paragraph_text;
use crate::{anchor, fail, hp, parse, unsupported, Anchors};
use hae_diag::{Limits, Result};
use hae_package::{PackageInfo, PackageState, XmlMap};
use hae_xml::{Item, XmlDoc};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
struct Addr {
    row: u32,
    col: u32,
    row_span: u32,
    col_span: u32,
}

struct Cell {
    entry: String,
    tc: usize,
    table: usize,
    addr: Option<Addr>,
    text: String,
    paragraphs: Vec<usize>,
}

fn num(doc: &XmlDoc, parent: usize, local: &str, attr: &str) -> Option<u32> {
    doc.el(parent)
        .children
        .iter()
        .copied()
        .find(|&c| hp(doc.el(c), local))
        .and_then(|c| doc.el(c).attr_value(attr))
        .and_then(|v| v.parse().ok())
}

fn nearest_table(doc: &XmlDoc, i: usize) -> Option<usize> {
    doc.ancestors(i).find(|&a| hp(doc.el(a), "tbl"))
}

fn tables(doc: &XmlDoc) -> Vec<usize> {
    doc.iter()
        .filter(|(_, e)| hp(e, "tbl"))
        .map(|(i, _)| i)
        .collect()
}

/// Paragraphs `cell.setText` addresses: `hp:p` under the cell's own `subList`.
fn cell_paragraphs(doc: &XmlDoc, tc: usize) -> Vec<usize> {
    doc.descendants(tc)
        .filter(|&i| {
            hp(doc.el(i), "p")
                && doc
                    .el(i)
                    .parent
                    .is_some_and(|par| hp(doc.el(par), "subList") && doc.el(par).parent == Some(tc))
        })
        .collect()
}

fn empty_paragraph_block(doc: &XmlDoc, p: usize) -> Option<&'static str> {
    // Comments, processing instructions and CDATA are lexical barriers that
    // make an empty paragraph unsafe to fill.
    for i in std::iter::once(p).chain(doc.descendants(p)) {
        for item in &doc.el(i).items {
            if matches!(item, Item::Comment(_) | Item::Pi(_) | Item::CData { .. }) {
                return Some("verify_unsupported_markup");
            }
        }
    }
    let Some(run) = doc
        .el(p)
        .children
        .iter()
        .copied()
        .find(|&c| hp(doc.el(c), "run"))
    else {
        return Some("no_run");
    };
    if let Some(t) = doc
        .el(run)
        .children
        .iter()
        .copied()
        .find(|&c| hp(doc.el(c), "t"))
    {
        for a in &doc.el(t).attrs {
            if a.qname == "xmlns" || a.qname.starts_with("xmlns:") {
                continue;
            }
            if a.ns.is_some() || a.local != "charPrIDRef" {
                return Some("unsupported_content_in_range");
            }
        }
    }
    None
}

/// The same block reasons the editor's cell.setText discovery reports.
fn target_block(doc: &XmlDoc, tc: usize) -> Option<&'static str> {
    if doc.ancestors(tc).any(|a| hp(doc.el(a), "tc")) {
        return Some("nested_cell");
    }
    let paragraphs = cell_paragraphs(doc, tc);
    if paragraphs.is_empty() {
        return Some("cell_paragraph_count");
    }
    if doc.descendants(tc).any(|d| hp(doc.el(d), "tbl")) {
        return Some("nested_table");
    }
    let top = doc
        .ancestors(tc)
        .find(|&a| doc.el(a).parent == Some(doc.root()))
        .unwrap_or(tc);
    if !hp(doc.el(top), "p") {
        return Some("verify_no_top_level_paragraph");
    }
    paragraphs
        .iter()
        .find_map(|&p| empty_paragraph_block(doc, p))
}

fn scope_tables(doc: &XmlDoc, target: usize) -> Result<Vec<usize>> {
    if target == doc.root() {
        return Ok(tables(doc));
    }
    if hp(doc.el(target), "tbl") {
        return Ok(vec![target]);
    }
    Err(unsupported("form.fill scope is not a table or section"))
}

struct Resolved {
    entry: String,
    tc: usize,
    value: String,
}

/// Derive every filled cell and write the expected bytes into `state`.
/// Returns true when at least one cell changed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    state: &mut PackageState,
    info: &PackageInfo,
    xml: &XmlMap,
    values: &BTreeMap<String, String>,
    scope: &Option<crate::VerifyScope>,
    strict: bool,
    limits: &Limits,
    anchors: &mut Anchors,
    preview: &mut Vec<(String, String)>,
    remaps: &crate::Remaps,
) -> Result<bool> {
    if values.is_empty() {
        return Err(unsupported("form.fill needs at least one label"));
    }
    for (label, value) in values {
        if label.is_empty() || label.contains(['\t', '\n', '\r']) {
            return Err(unsupported("form.fill label is empty or multi-line"));
        }
        crate::text::new_text(value)?;
        if value.is_empty() {
            return Err(unsupported("form.fill value is empty"));
        }
    }
    let entries: Vec<String> = match scope {
        Some(s) => {
            if !info.sections.iter().any(|e| e == &s.entry) {
                return Err(unsupported("targets must belong to a spine section"));
            }
            vec![s.entry.clone()]
        }
        None => info.sections.clone(),
    };
    let mut docs: BTreeMap<String, XmlDoc> = BTreeMap::new();
    let mut cells: Vec<Cell> = Vec::new();
    for entry in &entries {
        let doc = parse(state, entry, limits)?;
        let selected = match scope {
            None => tables(&doc),
            Some(s) => {
                let target = crate::resolve(xml, remaps, &s.entry, &doc, &s.path)?;
                scope_tables(&doc, target)?
            }
        };
        for table in selected {
            for tc in doc
                .descendants(table)
                .filter(|&i| hp(doc.el(i), "tc") && nearest_table(&doc, i) == Some(table))
            {
                let paragraphs = cell_paragraphs(&doc, tc);
                let addr = match (
                    num(&doc, tc, "cellAddr", "rowAddr"),
                    num(&doc, tc, "cellAddr", "colAddr"),
                ) {
                    (Some(row), Some(col)) => Some(Addr {
                        row,
                        col,
                        row_span: num(&doc, tc, "cellSpan", "rowSpan").unwrap_or(1),
                        col_span: num(&doc, tc, "cellSpan", "colSpan").unwrap_or(1),
                    }),
                    _ => None,
                };
                cells.push(Cell {
                    entry: entry.clone(),
                    tc,
                    table,
                    addr,
                    text: paragraphs
                        .iter()
                        .map(|&p| paragraph_text(&doc, p))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    paragraphs,
                });
            }
        }
        docs.insert(entry.clone(), doc);
    }
    // Resolve every label against the pre-request state.
    let mut resolved: Vec<Resolved> = Vec::new();
    for (label, value) in values {
        let matches: Vec<&Cell> = cells.iter().filter(|c| c.text == *label).collect();
        if matches.len() != 1 {
            if strict {
                return Err(fail(format!(
                    "form.fill label {label:?} resolved to {} cells",
                    matches.len()
                )));
            }
            continue;
        }
        let label_cell = matches[0];
        let Some(label_addr) = label_cell.addr else {
            // A label cell without cellAddr cannot resolve a target (editor:
            // no_cell_geometry).
            if strict {
                return Err(fail(format!(
                    "form.fill label {label:?} has no cell geometry"
                )));
            }
            continue;
        };
        let table_cells: Vec<&Cell> = cells
            .iter()
            .filter(|c| {
                c.entry == label_cell.entry && c.table == label_cell.table && c.addr.is_some()
            })
            .collect();
        let right: Vec<&&Cell> = table_cells
            .iter()
            .filter(|c| {
                let a = c.addr.unwrap();
                a.col == label_addr.col + label_addr.col_span
                    && a.row <= label_addr.row
                    && label_addr.row < a.row + a.row_span
            })
            .collect();
        let below: Vec<&&Cell> = table_cells
            .iter()
            .filter(|c| {
                let a = c.addr.unwrap();
                a.row == label_addr.row + label_addr.row_span
                    && a.col <= label_addr.col
                    && label_addr.col < a.col + a.col_span
            })
            .collect();
        let candidate: &Cell = if right.len() == 1 {
            right[0]
        } else if right.is_empty() && below.len() == 1 {
            below[0]
        } else {
            if strict {
                return Err(fail(format!(
                    "form.fill label {label:?} has no unique right/below cell"
                )));
            }
            continue;
        };
        if candidate
            .paragraphs
            .iter()
            .any(|&p| !paragraph_text(&docs[&candidate.entry], p).is_empty())
        {
            if strict {
                return Err(fail(format!("form.fill target for {label:?} is not empty")));
            }
            continue;
        }
        if let Some(reason) = target_block(&docs[&candidate.entry], candidate.tc) {
            if strict {
                return Err(fail(format!(
                    "form.fill target for {label:?} is blocked: {reason}"
                )));
            }
            continue;
        }
        resolved.push(Resolved {
            entry: candidate.entry.clone(),
            tc: candidate.tc,
            value: value.clone(),
        });
    }
    // Two labels may not resolve to the same value cell (mirrors the editor).
    {
        let mut seen: BTreeSet<(String, usize)> = BTreeSet::new();
        let mut conflicting: BTreeSet<(String, usize)> = BTreeSet::new();
        for r in &resolved {
            if !seen.insert((r.entry.clone(), r.tc)) {
                conflicting.insert((r.entry.clone(), r.tc));
            }
        }
        if !conflicting.is_empty() {
            if strict {
                return Err(fail("form.fill has two labels on one target cell"));
            }
            resolved.retain(|r| !conflicting.contains(&(r.entry.clone(), r.tc)));
        }
    }
    let mut changed = false;
    for r in &resolved {
        let doc = &docs[&r.entry];
        let paragraphs = cell_paragraphs(doc, r.tc);
        let mut texts = vec![r.value.clone()];
        texts.resize(paragraphs.len(), String::new());
        let changes = crate::text::cell(state, &r.entry, doc, r.tc, None, Some(&texts), false)?;
        if changes.first().and_then(|(_, c)| c.as_ref()).is_none() {
            return Err(unsupported("form.fill target paragraph did not change"));
        }
        if changes.iter().skip(1).any(|(_, c)| c.is_some()) {
            return Err(unsupported("form.fill changed a non-first paragraph"));
        }
        let first = paragraphs[0];
        anchor(anchors, &r.entry, doc, first)?;
        preview.push((String::new(), r.value.clone()));
        changed = true;
    }
    // `expected` is rebuilt by the caller from the mutated state; nothing else
    // here touches the payload.
    Ok(changed)
}
