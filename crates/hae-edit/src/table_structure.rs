//! Table row/column insertion and deletion (0.3).
//!
//! The one place where the engine creates and removes nodes. Rules and the
//! reasons behind them: docs/decisions/table-structure-node-creation-design.md.
//! `hae-verify` re-derives every byte of the result from the request alone;
//! this module only plans splices and declares the node-set change
//! ([`TableRemap`]) that the model's commit gate checks against the result.
use crate::table::{axis, child, hp, number, put, shape, unsupported, Cell};
use crate::{CreatedNode, Draft, Intent, StructureReport};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{Kind, Node, TableRemap};
use hae_xml::{escape_attr, XmlDoc};
use serde_json::json;
use std::{collections::BTreeMap, ops::Range};

type Cut = (Range<usize>, Vec<u8>);

/// Same bound as the geometry commands.
pub(crate) const MAX_LINES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Insert,
    Delete,
}

fn invalid(reason: &str, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::InvalidArgument, Stage::Validate, message).with_reason(reason)
}

fn merged_error(
    reason: &str,
    message: &str,
    cells: Vec<&Cell>,
    ids: &BTreeMap<usize, String>,
) -> EngineError {
    unsupported(reason, message).with_detail(json!({
        "cells": cells.iter().take(16).map(|c| json!({
            "nodeId": ids.get(&c.el),
            "row": c.row, "col": c.col, "rowSpan": c.rs, "colSpan": c.cs
        })).collect::<Vec<_>>(),
        "cellCount": cells.len(),
    }))
}

struct Layout {
    table: usize,
    sz: usize,
    rows: usize,
    cols: usize,
    /// `hp:tr` elements and their positions among the table's children.
    tr_el: Vec<usize>,
    tr_pos: Vec<usize>,
    cells: Vec<Cell>,
    /// Cell element -> node id, filled by `structure` for diagnostics.
    ids: BTreeMap<usize, String>,
    /// Indexes into `cells`, per row, in element order (= tc position in the tr).
    by_row: Vec<Vec<usize>>,
}

fn layout(d: &XmlDoc, table: usize, rows_axis: bool) -> Result<Layout> {
    let rows = number(d, table, "rowCnt")? as usize;
    let cols = number(d, table, "colCnt")? as usize;
    if !(1..=MAX_LINES).contains(&rows) || !(1..=MAX_LINES).contains(&cols) {
        return Err(shape("table grid must have 1..1024 rows and columns"));
    }
    if number(d, table, "cellSpacing")? != 0 {
        return Err(shape("nonzero cellSpacing is unsupported"));
    }
    let sz = child(d, table, "sz")?;
    if d.el(sz).attr_value(if rows_axis {
        "heightRelTo"
    } else {
        "widthRelTo"
    }) != Some("ABSOLUTE")
    {
        return Err(shape("relative table dimensions are unsupported"));
    }
    if d.descendants(table).any(|i| hp(d, i, "tbl")) {
        return Err(unsupported(
            "nested_table",
            "nested table structure is unsupported",
        ));
    }
    let mut tr_el = Vec::new();
    let mut tr_pos = Vec::new();
    for (pos, &c) in d.el(table).children.iter().enumerate() {
        if hp(d, c, "run") || hp(d, c, "t") {
            return Err(shape("table has a text carrier child"));
        }
        if hp(d, c, "cellzoneList") {
            return Err(unsupported(
                "table_cellzone_unsupported",
                "tables with cell zones (hp:cellzoneList) are unsupported",
            ));
        }
        if hp(d, c, "tr") {
            if d.el(c).end.is_none() {
                return Err(shape("self-closing table row"));
            }
            tr_el.push(c);
            tr_pos.push(pos);
        }
    }
    if tr_el.len() != rows {
        return Err(shape("row count disagrees with rowCnt"));
    }
    let mut occupied = std::collections::BTreeSet::new();
    let mut cells = Vec::new();
    let mut by_row = vec![Vec::new(); rows];
    for (row, &tr) in tr_el.iter().enumerate() {
        let mut last_col: Option<usize> = None;
        for &tc in &d.el(tr).children {
            if !hp(d, tc, "tc") {
                return Err(shape("row contains non-cell geometry"));
            }
            let addr = child(d, tc, "cellAddr")?;
            let span = child(d, tc, "cellSpan")?;
            let size = child(d, tc, "cellSz")?;
            let (r, c) = (
                number(d, addr, "rowAddr")? as usize,
                number(d, addr, "colAddr")? as usize,
            );
            let (cs, rs) = (
                number(d, span, "colSpan")? as usize,
                number(d, span, "rowSpan")? as usize,
            );
            if r != row || rs == 0 || cs == 0 || r + rs > rows || c + cs > cols {
                return Err(shape("cell address/span outside grid"));
            }
            if last_col.is_some_and(|l| c <= l) {
                return Err(shape("cells of a row are not in column order"));
            }
            last_col = Some(c);
            for y in r..r + rs {
                for x in c..c + cs {
                    if !occupied.insert((y, x)) {
                        return Err(shape("overlapping merged cells"));
                    }
                }
            }
            by_row[row].push(cells.len());
            cells.push(Cell {
                el: tc,
                sz: size,
                row: r,
                col: c,
                rs,
                cs,
                w: number(d, size, "width")?,
                h: number(d, size, "height")?,
            });
        }
    }
    if occupied.len() != rows * cols {
        return Err(shape("incomplete table grid"));
    }
    Ok(Layout {
        table,
        sz,
        rows,
        cols,
        tr_el,
        tr_pos,
        cells,
        ids: BTreeMap::new(),
        by_row,
    })
}

fn patch(raw: &[u8], base: usize, mut cuts: Vec<Cut>) -> Result<Vec<u8>> {
    cuts.sort_by_key(|(r, _)| (r.start, r.end));
    let mut out = Vec::with_capacity(raw.len());
    let mut pos = base;
    for (r, v) in cuts {
        if r.start < pos || r.end > base + raw.len() {
            return Err(shape("overlapping template edits"));
        }
        out.extend_from_slice(&raw[pos - base..r.start - base]);
        out.extend(v);
        pos = r.end;
    }
    out.extend_from_slice(&raw[pos - base..]);
    Ok(out)
}

/// A new, empty cell cloned from `tc`: same attributes, borders and margins;
/// `name` cleared; `header` cleared when asked; new address; the paragraph
/// list replaced by one empty paragraph that keeps the first paragraph's tag
/// and the first run's character shape.
fn blank_cell(d: &XmlDoc, tc: usize, row: usize, col: usize, zero_header: bool) -> Result<Vec<u8>> {
    let tpl = |m: &str| unsupported("cell_template_unsupported", m);
    let outer = d.el(tc).outer();
    let mut cuts: Vec<Cut> = Vec::new();
    if let Some(a) = d.el(tc).attr("name") {
        cuts.push((a.value_span.clone(), Vec::new()));
    }
    if zero_header {
        if let Some(a) = d.el(tc).attr("header") {
            cuts.push((a.value_span.clone(), b"0".to_vec()));
        }
    }
    let addr = child(d, tc, "cellAddr")?;
    for (name, value) in [("colAddr", col), ("rowAddr", row)] {
        let a = d
            .el(addr)
            .attr(name)
            .ok_or_else(|| shape("cellAddr lacks an address attribute"))?;
        cuts.push((a.value_span.clone(), value.to_string().into_bytes()));
    }
    let subs: Vec<usize> = d
        .el(tc)
        .children
        .iter()
        .copied()
        .filter(|&c| hp(d, c, "subList"))
        .collect();
    if subs.len() != 1 || d.el(subs[0]).end.is_none() {
        return Err(tpl("template cell needs exactly one open subList"));
    }
    let sub = subs[0];
    let p = d
        .el(sub)
        .children
        .iter()
        .copied()
        .find(|&c| hp(d, c, "p"))
        .ok_or_else(|| tpl("template cell has no paragraph"))?;
    if d.el(p).end.is_none() {
        return Err(tpl("template paragraph is self-closing"));
    }
    let run = d
        .el(p)
        .children
        .iter()
        .copied()
        .find(|&c| hp(d, c, "run"))
        .ok_or_else(|| tpl("template paragraph has no run"))?;
    let shape_ref = d
        .el(run)
        .attr_value("charPrIDRef")
        .ok_or_else(|| tpl("template run has no charPrIDRef"))?;
    let mut para = d.raw(d.el(p).start.clone()).to_vec();
    para.extend(
        format!(
            "<{} charPrIDRef=\"{}\"/>",
            d.el(run).qname,
            escape_attr(shape_ref)
        )
        .into_bytes(),
    );
    para.extend(format!("</{}>", d.el(p).qname).into_bytes());
    cuts.push((d.el(sub).inner(), para));
    patch(d.raw(outer.clone()), outer.start, cuts)
}

/// Elements a deleted cell may contain. Anything else (pictures, equations,
/// fields, shapes, notes) is refused: their cross references are unverified.
const DELETABLE: &[&str] = &[
    "tc",
    "subList",
    "p",
    "run",
    "t",
    "linesegarray",
    "lineseg",
    "cellAddr",
    "cellSpan",
    "cellSz",
    "cellMargin",
    "tab",
    "lineBreak",
    "nbSpace",
    "fwSpace",
    "hyphen",
];

/// `Ok(true)` when the cell holds text.
fn deletable_text(d: &XmlDoc, cell: usize) -> Result<bool> {
    let mut text = false;
    for i in std::iter::once(cell).chain(d.descendants(cell)) {
        let e = d.el(i);
        if !hae_model::is_hp(e.ns.as_deref()) || !DELETABLE.contains(&e.local.as_str()) {
            return Err(unsupported(
                "delete_content_unsupported",
                "cells holding objects, fields or notes cannot be deleted",
            )
            .with_detail(json!({"element": e.qname})));
        }
        if e.local == "t" && !e.inner().is_empty() {
            text = true;
        }
    }
    Ok(text)
}

#[derive(Default)]
struct Plan {
    cuts: Vec<Cut>,
    children: Vec<Option<usize>>,
    cells: BTreeMap<usize, Vec<Option<usize>>>,
    /// Elements of surviving cells whose address changed.
    shifted: Vec<usize>,
    text_deleted: bool,
    rows_after: usize,
    cols_after: usize,
}

fn set_cell_addr(d: &XmlDoc, c: &Cell, name: &str, value: usize, plan: &mut Plan) -> Result<()> {
    let addr = child(d, c.el, "cellAddr")?;
    put(d, addr, name, value, &mut plan.cuts)?;
    plan.shifted.push(c.el);
    Ok(())
}

fn size_value(base: u32, add: u64, sub: u64) -> Result<u32> {
    let v = u64::from(base) + add;
    if v < sub || v - sub == 0 || v - sub > i32::MAX as u64 {
        return Err(shape("table dimension would leave the valid range"));
    }
    Ok((v - sub) as u32)
}

fn is_header(d: &XmlDoc, g: &Layout, row: usize) -> bool {
    g.by_row[row].iter().any(|&ci| {
        d.el(g.cells[ci].el)
            .attr_value("header")
            .is_some_and(|v| v != "0")
    })
}

fn row_height(g: &Layout, row: usize) -> u32 {
    g.by_row[row]
        .iter()
        .map(|&ci| g.cells[ci].h)
        .max()
        .unwrap_or(0)
}

fn insert_rows(d: &XmlDoc, g: &Layout, index: usize, count: usize) -> Result<Plan> {
    if index > g.rows {
        return Err(invalid(
            "index_out_of_range",
            format!("index {index} is outside 0..={}", g.rows),
        ));
    }
    if g.rows + count > MAX_LINES {
        return Err(invalid(
            "table_size_limit",
            "a table may have at most 1024 rows",
        ));
    }
    let crossing: Vec<&Cell> = g
        .cells
        .iter()
        .filter(|c| c.row < index && index < c.row + c.rs)
        .collect();
    if !crossing.is_empty() {
        return Err(merged_error(
            "insert_inside_merged_cell",
            "the insertion boundary splits a merged cell",
            crossing,
            &g.ids,
        ));
    }
    let simple: Vec<usize> = [index.checked_sub(1), (index < g.rows).then_some(index)]
        .into_iter()
        .flatten()
        .filter(|&t| {
            g.cells
                .iter()
                .all(|c| !(c.row <= t && t < c.row + c.rs) || c.rs == 1)
        })
        .collect();
    let template = simple
        .iter()
        .copied()
        .find(|&t| !is_header(d, g, t))
        .or_else(|| simple.first().copied())
        .ok_or_else(|| {
            unsupported(
                "no_simple_template_line",
                "neither neighbouring row is free of vertically merged cells",
            )
        })?;
    let tr = g.tr_el[template];
    let mut blob = Vec::new();
    for j in 0..count {
        blob.extend_from_slice(d.raw(d.el(tr).start.clone()));
        for &ci in &g.by_row[template] {
            let c = &g.cells[ci];
            blob.extend(blank_cell(d, c.el, index + j, c.col, true)?);
        }
        blob.extend_from_slice(d.raw(d.el(tr).end.clone().expect("checked open")));
    }
    let at = if index > 0 {
        d.el(g.tr_el[index - 1]).outer().end
    } else {
        d.el(g.tr_el[0]).outer().start
    };
    let mut plan = Plan {
        rows_after: g.rows + count,
        cols_after: g.cols,
        ..Plan::default()
    };
    plan.cuts.push((at..at, blob));
    for c in g.cells.iter().filter(|c| c.row >= index) {
        set_cell_addr(d, c, "rowAddr", c.row + count, &mut plan)?;
    }
    put(d, g.table, "rowCnt", g.rows + count, &mut plan.cuts)?;
    let height = size_value(
        number(d, g.sz, "height")?,
        count as u64 * u64::from(row_height(g, template)),
        0,
    )?;
    put(d, g.sz, "height", height, &mut plan.cuts)?;
    let n_children = d.el(g.table).children.len();
    let point = if index > 0 {
        g.tr_pos[index - 1] + 1
    } else {
        g.tr_pos[0]
    };
    plan.children = (0..n_children)
        .map(|p| Some(if p >= point { p + count } else { p }))
        .collect();
    Ok(plan)
}

fn column_widths(d: &XmlDoc, g: &Layout) -> Result<Vec<u32>> {
    axis(&g.cells, g.cols, Some(number(d, g.sz, "width")?), true)
}

fn insert_columns(d: &XmlDoc, g: &Layout, index: usize, count: usize) -> Result<Plan> {
    if index > g.cols {
        return Err(invalid(
            "index_out_of_range",
            format!("index {index} is outside 0..={}", g.cols),
        ));
    }
    if g.cols + count > MAX_LINES {
        return Err(invalid(
            "table_size_limit",
            "a table may have at most 1024 columns",
        ));
    }
    let crossing: Vec<&Cell> = g
        .cells
        .iter()
        .filter(|c| c.col < index && index < c.col + c.cs)
        .collect();
    if !crossing.is_empty() {
        return Err(merged_error(
            "insert_inside_merged_cell",
            "the insertion boundary splits a merged cell",
            crossing,
            &g.ids,
        ));
    }
    let widths = column_widths(d, g)?;
    let template = [index.checked_sub(1), (index < g.cols).then_some(index)]
        .into_iter()
        .flatten()
        .find(|&t| {
            g.cells
                .iter()
                .all(|c| !(c.col <= t && t < c.col + c.cs) || c.cs == 1)
        })
        .ok_or_else(|| {
            unsupported(
                "no_simple_template_line",
                "neither neighbouring column is free of horizontally merged cells",
            )
        })?;
    let mut plan = Plan {
        rows_after: g.rows,
        cols_after: g.cols + count,
        ..Plan::default()
    };
    plan.children = (0..d.el(g.table).children.len()).map(Some).collect();
    for row in 0..g.rows {
        let Some(&ci) = g.by_row[row]
            .iter()
            .find(|&&ci| g.cells[ci].col == template)
        else {
            continue;
        };
        let tc = g.cells[ci].el;
        let mut blob = Vec::new();
        for j in 0..count {
            blob.extend(blank_cell(d, tc, row, index + j, false)?);
        }
        let in_row = &g.by_row[row];
        let before = in_row
            .iter()
            .take_while(|&&c| g.cells[c].col < index)
            .count();
        let at = match in_row.get(before) {
            Some(&c) => d.el(g.cells[c].el).outer().start,
            None => match in_row.last() {
                Some(&c) => d.el(g.cells[c].el).outer().end,
                None => d.el(g.tr_el[row]).inner().start,
            },
        };
        plan.cuts.push((at..at, blob));
        plan.cells.insert(
            g.tr_pos[row],
            (0..in_row.len())
                .map(|p| Some(if p >= before { p + count } else { p }))
                .collect(),
        );
    }
    for c in g.cells.iter().filter(|c| c.col >= index) {
        set_cell_addr(d, c, "colAddr", c.col + count, &mut plan)?;
    }
    put(d, g.table, "colCnt", g.cols + count, &mut plan.cuts)?;
    let width = size_value(
        number(d, g.sz, "width")?,
        count as u64 * u64::from(widths[template]),
        0,
    )?;
    put(d, g.sz, "width", width, &mut plan.cuts)?;
    Ok(plan)
}

fn delete_rows(d: &XmlDoc, g: &Layout, index: usize, count: usize) -> Result<Plan> {
    if index >= g.rows || index + count > g.rows {
        return Err(invalid(
            "index_out_of_range",
            format!("rows {index}..{} are outside 0..{}", index + count, g.rows),
        ));
    }
    if count >= g.rows {
        return Err(invalid(
            "would_empty_table",
            "a table must keep at least one row",
        ));
    }
    let range = index..index + count;
    let merged: Vec<&Cell> = g
        .cells
        .iter()
        .filter(|c| c.row < range.end && c.row + c.rs > range.start && (c.rs != 1 || c.cs != 1))
        .collect();
    if !merged.is_empty() {
        return Err(merged_error(
            "merged_cell_blocks_delete",
            "the rows to delete overlap merged cells",
            merged,
            &g.ids,
        ));
    }
    let mut plan = Plan {
        rows_after: g.rows - count,
        cols_after: g.cols,
        ..Plan::default()
    };
    for row in range.clone() {
        for &ci in &g.by_row[row] {
            plan.text_deleted |= deletable_text(d, g.cells[ci].el)?;
        }
        plan.cuts.push((d.el(g.tr_el[row]).outer(), Vec::new()));
    }
    for c in g.cells.iter().filter(|c| c.row >= range.end) {
        set_cell_addr(d, c, "rowAddr", c.row - count, &mut plan)?;
    }
    put(d, g.table, "rowCnt", g.rows - count, &mut plan.cuts)?;
    let removed: u64 = range.clone().map(|r| u64::from(row_height(g, r))).sum();
    let height = size_value(number(d, g.sz, "height")?, 0, removed)?;
    put(d, g.sz, "height", height, &mut plan.cuts)?;
    let mut shift = 0;
    plan.children = (0..d.el(g.table).children.len())
        .map(|p| {
            if g.tr_pos[range.clone()].contains(&p) {
                shift += 1;
                None
            } else {
                Some(p - shift)
            }
        })
        .collect();
    Ok(plan)
}

fn delete_columns(d: &XmlDoc, g: &Layout, index: usize, count: usize) -> Result<Plan> {
    if index >= g.cols || index + count > g.cols {
        return Err(invalid(
            "index_out_of_range",
            format!(
                "columns {index}..{} are outside 0..{}",
                index + count,
                g.cols
            ),
        ));
    }
    if count >= g.cols {
        return Err(invalid(
            "would_empty_table",
            "a table must keep at least one column",
        ));
    }
    let range = index..index + count;
    let hit: std::collections::BTreeSet<usize> = (0..g.cells.len())
        .filter(|&ci| {
            let c = &g.cells[ci];
            c.col < range.end && c.col + c.cs > range.start
        })
        .collect();
    let merged: Vec<&Cell> = hit
        .iter()
        .map(|&ci| &g.cells[ci])
        .filter(|c| c.rs != 1 || c.cs != 1)
        .collect();
    if !merged.is_empty() {
        return Err(merged_error(
            "merged_cell_blocks_delete",
            "the columns to delete overlap merged cells",
            merged,
            &g.ids,
        ));
    }
    let widths = column_widths(d, g)?;
    let mut plan = Plan {
        rows_after: g.rows,
        cols_after: g.cols - count,
        ..Plan::default()
    };
    plan.children = (0..d.el(g.table).children.len()).map(Some).collect();
    for row in 0..g.rows {
        let in_row = &g.by_row[row];
        let gone = in_row.iter().filter(|ci| hit.contains(ci)).count();
        if gone == 0 {
            continue;
        }
        if gone == in_row.len() {
            return Err(unsupported(
                "row_would_become_empty",
                "deleting these columns would leave a row without cells",
            ));
        }
        let mut map = Vec::with_capacity(in_row.len());
        let mut shift = 0;
        for (p, ci) in in_row.iter().enumerate() {
            if hit.contains(ci) {
                map.push(None);
                shift += 1;
            } else {
                map.push(Some(p - shift));
            }
        }
        plan.cells.insert(g.tr_pos[row], map);
    }
    for &ci in &hit {
        plan.text_deleted |= deletable_text(d, g.cells[ci].el)?;
        plan.cuts.push((d.el(g.cells[ci].el).outer(), Vec::new()));
    }
    for c in g.cells.iter().filter(|c| c.col >= range.end) {
        set_cell_addr(d, c, "colAddr", c.col - count, &mut plan)?;
    }
    put(d, g.table, "colCnt", g.cols - count, &mut plan.cuts)?;
    let removed: u64 = range.map(|c| u64::from(widths[c])).sum();
    let width = size_value(number(d, g.sz, "width")?, 0, removed)?;
    put(d, g.sz, "width", width, &mut plan.cuts)?;
    Ok(plan)
}

pub(crate) fn structure(
    d: &mut Draft,
    command: &str,
    target: &str,
    axis_kind: Axis,
    mode: Mode,
    index: usize,
    count: Option<usize>,
) -> Result<()> {
    let count = count.unwrap_or(1);
    if !(1..=MAX_LINES).contains(&count) {
        return Err(invalid("count_out_of_range", "count must be 1..=1024"));
    }
    let table = d.node(target, Some(Kind::Table))?;
    let members: Vec<Node> = d
        .doc
        .nodes
        .iter()
        .filter(|n| {
            matches!(n.kind, Kind::Paragraph | Kind::Cell) && d.doc.is_descendant(n, &table.id)
        })
        .cloned()
        .collect();
    for n in &members {
        d.doc.check_editable(n)?;
    }
    let xml = d.doc.doc_of(&table).clone();
    let mut g = layout(&xml, table.el, axis_kind == Axis::Row)?;
    for n in d.doc.nodes.iter().filter(|n| n.entry == table.entry) {
        if n.kind == Kind::Cell {
            g.ids.insert(n.el, n.id.clone());
        }
    }
    let rows_before = g.rows;
    let cols_before = g.cols;
    let plan = match (axis_kind, mode) {
        (Axis::Row, Mode::Insert) => insert_rows(&xml, &g, index, count)?,
        (Axis::Row, Mode::Delete) => delete_rows(&xml, &g, index, count)?,
        (Axis::Column, Mode::Insert) => insert_columns(&xml, &g, index, count)?,
        (Axis::Column, Mode::Delete) => delete_columns(&xml, &g, index, count)?,
    };
    let shifted: Vec<String> = plan
        .shifted
        .iter()
        .filter_map(|&el| {
            d.doc
                .nodes
                .iter()
                .find(|n| n.entry == table.entry && n.el == el)
                .map(|n| n.id.clone())
        })
        .collect();
    let remap = TableRemap {
        entry: table.entry.clone(),
        table: d.doc.stable_path_of(&table),
        children: plan.children,
        cells: plan.cells,
    };
    let path = xml.path(table.el);
    let delta = d.splice_structural(&table.entry, plan.cuts, &remap)?;

    // The table's cached layout changed: its paragraphs and everything after
    // it in the section lose their cache. Anchors recorded earlier for
    // removed paragraphs are void.
    if let Some(a) = d.anchors.get_mut(&table.entry) {
        a.1.retain(|id| !delta.removed.contains(id));
    }
    let table_now = d
        .doc
        .node(&table.id)
        .cloned()
        .ok_or_else(|| shape("table vanished"))?;
    let xml_now = d.doc.doc_of(&table_now).clone();
    let carrier = xml_now
        .ancestors(table_now.el)
        .find(|&i| hp(&xml_now, i, "p"))
        .and_then(|i| {
            d.doc
                .nodes
                .iter()
                .find(|n| n.entry == table_now.entry && n.el == i)
        })
        .cloned()
        .ok_or_else(|| shape("table has no paragraph carrier"))?;
    d.anchor(&carrier);
    let paragraphs: Vec<Node> = d
        .doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph && d.doc.is_descendant(n, &table.id))
        .cloned()
        .collect();
    for p in &paragraphs {
        d.anchor(p);
    }

    let created: Vec<CreatedNode> = delta
        .created
        .iter()
        .filter_map(|id| d.doc.node(id))
        .map(|n| {
            let addr = d.doc.cell_addr(n);
            CreatedNode {
                node_id: n.id.clone(),
                kind: n.kind.as_str(),
                row: addr.as_ref().map(|a| a.row as usize),
                col: addr.as_ref().map(|a| a.col as usize),
            }
        })
        .collect();
    d.changed.push(table.id.clone());
    d.changed.extend(delta.created.iter().cloned());
    d.changed.extend(shifted.iter().cloned());
    d.prv_extra |= plan.text_deleted;
    d.structure.push(StructureReport {
        command: command.into(),
        table: table.id.clone(),
        index,
        count,
        rows_before,
        rows_after: plan.rows_after,
        columns_before: cols_before,
        columns_after: plan.cols_after,
        created,
        removed: delta.removed,
        shifted,
    });
    d.intents.push(Intent::TableStructure {
        command: command.into(),
        entry: table.entry.clone(),
        path,
        index,
        count,
    });
    Ok(())
}
