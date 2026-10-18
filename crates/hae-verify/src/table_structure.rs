//! Request/source-only verifier for table row/column insertion and deletion.
//!
//! Derives the expected bytes of the whole entry from the pre-request XML and
//! the request (`entry`, `path`, `index`, `count`) alone. It consumes no
//! editor plan, node id or node-set declaration, and has no `hae-model` or
//! `hae-edit` dependency. The rules mirror docs/decisions/
//! table-structure-node-creation-design.md §6 but are implemented separately:
//! a rule bug shared by both would still be caught by the Hancom gate only.
use crate::table::{num, one, set, tracks, Region};
use crate::{attr, change, fail, hp, unsupported, Splice, VerifyOperation};
use hae_diag::Result;
use hae_package::PackageState;
use hae_xml::{escape_attr, XmlDoc};
use std::collections::BTreeMap;

const LIMIT: usize = 1024;

/// Verifier-owned path remapping, used to follow requests that address nodes
/// of a table changed by an earlier step of the same batch.
#[derive(Debug, Clone, Default)]
pub(crate) struct Remap {
    table: Vec<usize>,
    children: Vec<Option<usize>>,
    cells: BTreeMap<usize, Vec<Option<usize>>>,
}
impl Remap {
    /// Path after the change; `None` when the element was removed.
    pub(crate) fn map(&self, path: &[usize]) -> Option<Vec<usize>> {
        let t = self.table.len();
        if path.len() <= t || path[..t] != self.table[..] {
            return Some(path.to_vec());
        }
        let mut out = self.table.clone();
        out.push(self.children.get(path[t]).copied().flatten()?);
        match (path.get(t + 1), self.cells.get(&path[t])) {
            (Some(&c), Some(row)) => {
                out.push(row.get(c).copied().flatten()?);
                out.extend_from_slice(&path[t + 2..]);
            }
            _ => out.extend_from_slice(&path[t + 1..]),
        }
        Some(out)
    }
}

pub(crate) struct Outcome {
    pub table: Vec<usize>,
    pub remap: Remap,
    pub text_deleted: bool,
}

struct Tc {
    el: usize,
    row: usize,
    col: usize,
    rs: usize,
    cs: usize,
    w: u32,
    h: u32,
}
struct Grid {
    table: usize,
    size: usize,
    nr: usize,
    nc: usize,
    /// `tr` element and its position among the table's children.
    trs: Vec<(usize, usize)>,
    tcs: Vec<Tc>,
}
impl Grid {
    fn row(&self, r: usize) -> impl Iterator<Item = &Tc> {
        self.tcs.iter().filter(move |c| c.row == r)
    }
    fn covers_row(c: &Tc, r: usize) -> bool {
        c.row <= r && r < c.row + c.rs
    }
    fn covers_col(c: &Tc, x: usize) -> bool {
        c.col <= x && x < c.col + c.cs
    }
    fn row_height(&self, r: usize) -> u32 {
        self.row(r).map(|c| c.h).max().unwrap_or(0)
    }
    fn widths(&self, d: &XmlDoc) -> Result<Vec<u32>> {
        let regions: Vec<Region> = self
            .tcs
            .iter()
            .map(|c| Region {
                element: c.el,
                size: 0,
                y: c.row,
                x: c.col,
                dy: c.rs,
                dx: c.cs,
                width: c.w,
                height: c.h,
            })
            .collect();
        tracks(&regions, self.nc, Some(num(d, self.size, "width")?), true)
    }
}

fn grid(d: &XmlDoc, table: usize, rows: bool) -> Result<Grid> {
    let nr = num(d, table, "rowCnt")? as usize;
    let nc = num(d, table, "colCnt")? as usize;
    if !(1..=LIMIT).contains(&nr)
        || !(1..=LIMIT).contains(&nc)
        || num(d, table, "cellSpacing")? != 0
    {
        return Err(unsupported("table grid limits or spacing"));
    }
    let size = one(d, table, "sz")?;
    for a in [if rows { "heightRelTo" } else { "widthRelTo" }] {
        if attr(d, size, a)?.value != "ABSOLUTE" {
            return Err(unsupported("relative table extent"));
        }
    }
    if d.descendants(table).any(|i| hp(d.el(i), "tbl")) {
        return Err(unsupported("nested table structure"));
    }
    let mut trs = Vec::new();
    for (pos, &c) in d.el(table).children.iter().enumerate() {
        let e = d.el(c);
        if hp(e, "run") || hp(e, "t") {
            return Err(unsupported("table has a text carrier child"));
        }
        if hp(e, "cellzoneList") {
            return Err(unsupported("table cell zones"));
        }
        if hp(e, "tr") {
            if e.end.is_none() {
                return Err(unsupported("self-closing table row"));
            }
            trs.push((c, pos));
        }
    }
    if trs.len() != nr {
        return Err(fail("table row count mismatch"));
    }
    let mut tcs = Vec::new();
    for (r, &(tr, _)) in trs.iter().enumerate() {
        let mut last: Option<usize> = None;
        for &c in &d.el(tr).children {
            if !hp(d.el(c), "tc") {
                return Err(unsupported("non-cell row child"));
            }
            let a = one(d, c, "cellAddr")?;
            let s = one(d, c, "cellSpan")?;
            let z = one(d, c, "cellSz")?;
            let t = Tc {
                el: c,
                row: num(d, a, "rowAddr")? as usize,
                col: num(d, a, "colAddr")? as usize,
                rs: num(d, s, "rowSpan")? as usize,
                cs: num(d, s, "colSpan")? as usize,
                w: num(d, z, "width")?,
                h: num(d, z, "height")?,
            };
            if t.row != r || t.rs == 0 || t.cs == 0 || t.row + t.rs > nr || t.col + t.cs > nc {
                return Err(fail("invalid cell bounds"));
            }
            if last.is_some_and(|l| t.col <= l) {
                return Err(fail("cells are not in column order"));
            }
            last = Some(t.col);
            tcs.push(t);
        }
    }
    // Every grid slot must be covered exactly once.
    let mut slots = vec![0u8; nr * nc];
    for t in &tcs {
        for y in t.row..t.row + t.rs {
            for x in t.col..t.col + t.cs {
                slots[y * nc + x] += 1;
            }
        }
    }
    if slots.iter().any(|&n| n != 1) {
        return Err(fail("table grid overlap/hole"));
    }
    Ok(Grid {
        table,
        size,
        nr,
        nc,
        trs,
        tcs,
    })
}

fn splice(raw: &[u8], base: usize, mut cuts: Vec<Splice>) -> Result<Vec<u8>> {
    cuts.sort_by_key(|(r, _)| (r.start, r.end));
    let mut out = Vec::new();
    let mut at = base;
    for (r, v) in cuts {
        if r.start < at || r.end > base + raw.len() {
            return Err(fail("overlapping template edits"));
        }
        out.extend_from_slice(&raw[at - base..r.start - base]);
        out.extend(v);
        at = r.end;
    }
    out.extend_from_slice(&raw[at - base..]);
    Ok(out)
}

/// Empty clone of `tc` at a new address (design §6.3).
fn clone_empty(
    d: &XmlDoc,
    tc: usize,
    row: usize,
    col: usize,
    reset_header: bool,
) -> Result<Vec<u8>> {
    let whole = d.el(tc).outer();
    let mut cuts: Vec<Splice> = vec![];
    if let Some(a) = d.el(tc).attr("name") {
        cuts.push((a.value_span.clone(), vec![]));
    }
    if reset_header {
        if let Some(a) = d.el(tc).attr("header") {
            cuts.push((a.value_span.clone(), b"0".to_vec()));
        }
    }
    let address = one(d, tc, "cellAddr")?;
    cuts.push((
        attr(d, address, "colAddr")?.value_span.clone(),
        col.to_string().into_bytes(),
    ));
    cuts.push((
        attr(d, address, "rowAddr")?.value_span.clone(),
        row.to_string().into_bytes(),
    ));
    let lists: Vec<usize> = d
        .el(tc)
        .children
        .iter()
        .copied()
        .filter(|&c| hp(d.el(c), "subList"))
        .collect();
    let [list] = lists[..] else {
        return Err(unsupported("template cell needs exactly one subList"));
    };
    if d.el(list).end.is_none() {
        return Err(unsupported("template subList is self-closing"));
    }
    let first = |parent: usize, name: &str| {
        d.el(parent)
            .children
            .iter()
            .copied()
            .find(|&c| hp(d.el(c), name))
    };
    let p = first(list, "p").ok_or_else(|| unsupported("template cell has no paragraph"))?;
    if d.el(p).end.is_none() {
        return Err(unsupported("template paragraph is self-closing"));
    }
    let run = first(p, "run").ok_or_else(|| unsupported("template paragraph has no run"))?;
    let shape = &d
        .el(run)
        .attr("charPrIDRef")
        .ok_or_else(|| unsupported("template run has no charPrIDRef"))?
        .value;
    let mut body = d.raw(d.el(p).start.clone()).to_vec();
    body.extend_from_slice(
        format!(
            "<{} charPrIDRef=\"{}\"/></{}>",
            d.el(run).qname,
            escape_attr(shape),
            d.el(p).qname
        )
        .as_bytes(),
    );
    cuts.push((d.el(list).inner(), body));
    splice(d.raw(whole.clone()), whole.start, cuts)
}

const PLAIN: [&str; 16] = [
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

/// True when the cell carries text. Rejects any non-plain content.
fn plain_cell(d: &XmlDoc, tc: usize) -> Result<bool> {
    let mut text = false;
    for i in std::iter::once(tc).chain(d.descendants(tc)) {
        let e = d.el(i);
        if !crate::hp(e, e.local.as_str()) || !PLAIN.contains(&e.local.as_str()) {
            return Err(unsupported("deleted cell holds objects, fields or notes"));
        }
        text |= e.local == "t" && !e.inner().is_empty();
    }
    Ok(text)
}

fn checked(base: u32, plus: u64, minus: u64) -> Result<u32> {
    let v = (u64::from(base) + plus)
        .checked_sub(minus)
        .ok_or_else(|| fail("table dimension below zero"))?;
    if v == 0 || v > 2147483647 {
        return Err(fail("table dimension outside the valid range"));
    }
    Ok(v as u32)
}

pub(crate) fn check(
    state: &mut PackageState,
    entry: &str,
    d: &XmlDoc,
    table: usize,
    op: &VerifyOperation,
) -> Result<Outcome> {
    let (rows, insert, index, count) = match *op {
        VerifyOperation::TableInsertRow { index, count, .. } => (true, true, index, count),
        VerifyOperation::TableDeleteRow { index, count, .. } => (true, false, index, count),
        VerifyOperation::TableInsertColumn { index, count, .. } => (false, true, index, count),
        VerifyOperation::TableDeleteColumn { index, count, .. } => (false, false, index, count),
        _ => return Err(fail("not a table structure operation")),
    };
    if !hp(d.el(table), "tbl") {
        return Err(fail("structure target is not a table"));
    }
    if !(1..=LIMIT).contains(&count) {
        return Err(fail("count outside 1..=1024"));
    }
    let g = grid(d, table, rows)?;
    let lines = if rows { g.nr } else { g.nc };
    let mut cuts: Vec<Splice> = vec![];
    let mut remap = Remap {
        table: crate::stable_path(d, table),
        children: (0..d.el(table).children.len()).map(Some).collect(),
        cells: BTreeMap::new(),
    };
    let mut text_deleted = false;
    let move_addr = |c: &Tc, delta: i64, cuts: &mut Vec<Splice>| -> Result<()> {
        let a = one(d, c.el, "cellAddr")?;
        let (name, now) = if rows {
            ("rowAddr", c.row)
        } else {
            ("colAddr", c.col)
        };
        set(d, a, name, (now as i64 + delta) as usize, cuts)
    };
    if insert {
        if index > lines || lines + count > LIMIT {
            return Err(fail("insertion index or resulting size out of range"));
        }
        // A boundary inside a merged cell is refused.
        let straddles = |c: &Tc| {
            let (lo, span) = if rows { (c.row, c.rs) } else { (c.col, c.cs) };
            lo < index && index < lo + span
        };
        if g.tcs.iter().any(straddles) {
            return Err(unsupported("insertion boundary splits a merged cell"));
        }
        let plain_line = |t: usize| {
            g.tcs.iter().all(|c| {
                if rows {
                    !Grid::covers_row(c, t) || c.rs == 1
                } else {
                    !Grid::covers_col(c, t) || c.cs == 1
                }
            })
        };
        let neighbours: Vec<usize> = [index.checked_sub(1), (index < lines).then_some(index)]
            .into_iter()
            .flatten()
            .filter(|&t| plain_line(t))
            .collect();
        let header = |t: usize| {
            g.row(t)
                .any(|c| d.el(c.el).attr_value("header").is_some_and(|v| v != "0"))
        };
        let template = if rows {
            neighbours
                .iter()
                .copied()
                .find(|&t| !header(t))
                .or(neighbours.first().copied())
        } else {
            neighbours.first().copied()
        }
        .ok_or_else(|| unsupported("no plain neighbouring line to clone"))?;
        if rows {
            let (tr, _) = g.trs[template];
            let mut blob = vec![];
            for k in 0..count {
                blob.extend_from_slice(d.raw(d.el(tr).start.clone()));
                for c in g.row(template) {
                    blob.extend(clone_empty(d, c.el, index + k, c.col, true)?);
                }
                blob.extend_from_slice(
                    d.raw(d.el(tr).end.clone().ok_or_else(|| fail("open row"))?),
                );
            }
            let at = match index {
                0 => d.el(g.trs[0].0).outer().start,
                n => d.el(g.trs[n - 1].0).outer().end,
            };
            cuts.push((at..at, blob));
            for c in g.tcs.iter().filter(|c| c.row >= index) {
                move_addr(c, count as i64, &mut cuts)?;
            }
            set(d, g.table, "rowCnt", g.nr + count, &mut cuts)?;
            let add = count as u64 * u64::from(g.row_height(template));
            set(
                d,
                g.size,
                "height",
                checked(num(d, g.size, "height")?, add, 0)?,
                &mut cuts,
            )?;
            let point = if index == 0 {
                g.trs[0].1
            } else {
                g.trs[index - 1].1 + 1
            };
            remap.children = remap
                .children
                .iter()
                .enumerate()
                .map(|(p, _)| Some(if p >= point { p + count } else { p }))
                .collect();
        } else {
            let widths = g.widths(d)?;
            for r in 0..g.nr {
                let Some(source) = g.row(r).find(|c| c.col == template) else {
                    continue;
                };
                let mut blob = vec![];
                for k in 0..count {
                    blob.extend(clone_empty(d, source.el, r, index + k, false)?);
                }
                let cells: Vec<&Tc> = g.row(r).collect();
                let before = cells.iter().filter(|c| c.col < index).count();
                let at = if let Some(c) = cells.get(before) {
                    d.el(c.el).outer().start
                } else if let Some(c) = cells.last() {
                    d.el(c.el).outer().end
                } else {
                    d.el(g.trs[r].0).inner().start
                };
                cuts.push((at..at, blob));
                remap.cells.insert(
                    g.trs[r].1,
                    (0..cells.len())
                        .map(|p| Some(if p >= before { p + count } else { p }))
                        .collect(),
                );
            }
            for c in g.tcs.iter().filter(|c| c.col >= index) {
                move_addr(c, count as i64, &mut cuts)?;
            }
            set(d, g.table, "colCnt", g.nc + count, &mut cuts)?;
            let add = count as u64 * u64::from(widths[template]);
            set(
                d,
                g.size,
                "width",
                checked(num(d, g.size, "width")?, add, 0)?,
                &mut cuts,
            )?;
        }
    } else {
        if index >= lines || index + count > lines || count >= lines {
            return Err(fail("deletion range outside the table or empties it"));
        }
        let end = index + count;
        let in_range = |c: &Tc| {
            let (lo, span) = if rows { (c.row, c.rs) } else { (c.col, c.cs) };
            lo < end && lo + span > index
        };
        let doomed: Vec<&Tc> = g.tcs.iter().filter(|c| in_range(c)).collect();
        if doomed.iter().any(|c| c.rs != 1 || c.cs != 1) {
            return Err(unsupported("deletion overlaps merged cells"));
        }
        for c in &doomed {
            text_deleted |= plain_cell(d, c.el)?;
        }
        if rows {
            for r in index..end {
                cuts.push((d.el(g.trs[r].0).outer(), vec![]));
            }
            for c in g.tcs.iter().filter(|c| c.row >= end) {
                move_addr(c, -(count as i64), &mut cuts)?;
            }
            set(d, g.table, "rowCnt", g.nr - count, &mut cuts)?;
            let gone: u64 = (index..end).map(|r| u64::from(g.row_height(r))).sum();
            set(
                d,
                g.size,
                "height",
                checked(num(d, g.size, "height")?, 0, gone)?,
                &mut cuts,
            )?;
            let removed: Vec<usize> = (index..end).map(|r| g.trs[r].1).collect();
            remap.children = (0..d.el(table).children.len())
                .map(|p| {
                    (!removed.contains(&p)).then(|| p - removed.iter().filter(|&&x| x < p).count())
                })
                .collect();
        } else {
            let widths = g.widths(d)?;
            for r in 0..g.nr {
                let cells: Vec<&Tc> = g.row(r).collect();
                let n = cells.iter().filter(|c| in_range(c)).count();
                if n == 0 {
                    continue;
                }
                if n == cells.len() {
                    return Err(unsupported("a row would be left without cells"));
                }
                let mut kept = 0;
                remap.cells.insert(
                    g.trs[r].1,
                    cells
                        .iter()
                        .map(|c| {
                            if in_range(c) {
                                None
                            } else {
                                kept += 1;
                                Some(kept - 1)
                            }
                        })
                        .collect(),
                );
            }
            for c in &doomed {
                cuts.push((d.el(c.el).outer(), vec![]));
            }
            for c in g.tcs.iter().filter(|c| c.col >= end) {
                move_addr(c, -(count as i64), &mut cuts)?;
            }
            set(d, g.table, "colCnt", g.nc - count, &mut cuts)?;
            let gone: u64 = (index..end).map(|x| u64::from(widths[x])).sum();
            set(
                d,
                g.size,
                "width",
                checked(num(d, g.size, "width")?, 0, gone)?,
                &mut cuts,
            )?;
        }
    }
    change(state, entry, d, cuts)?;
    Ok(Outcome {
        table: remap.table.clone(),
        remap,
        text_deleted,
    })
}
