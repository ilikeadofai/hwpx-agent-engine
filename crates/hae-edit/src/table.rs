//! Existing table geometry only. No element or shared resource is created.
use crate::{Draft, Intent};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{Kind, Node};
use hae_package::{NS_HC, NS_HH};
use hae_xml::XmlDoc;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, ops::Range};

type Cut = (Range<usize>, Vec<u8>);
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellMargin {
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub top: Option<u32>,
    pub bottom: Option<u32>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellFormat {
    pub border_fill_id: Option<String>,
    pub background: Option<String>,
    pub cell_margin: Option<CellMargin>,
}
pub(crate) fn bad(message: &str) -> EngineError {
    EngineError::new(Code::InvalidArgument, Stage::Validate, message)
}
pub(crate) fn unsupported(reason: &str, message: &str) -> EngineError {
    EngineError::new(Code::Unsupported, Stage::Validate, message).with_reason(reason)
}
/// Applies the shared capability predicate (hae-model) at execution time, so a command
/// advertised as unavailable is rejected with the same reason.
pub(crate) fn gate(reason: Option<String>) -> Result<()> {
    match reason {
        Some(r) => Err(unsupported(
            &r,
            "target does not satisfy the advertised source preconditions",
        )),
        None => Ok(()),
    }
}
pub(crate) fn shape(message: &str) -> EngineError {
    unsupported("table_geometry_unsupported", message)
}
pub(crate) fn hp(d: &XmlDoc, i: usize, n: &str) -> bool {
    hae_model::is_hp(d.el(i).ns.as_deref()) && d.el(i).local == n
}
pub(crate) fn child(d: &XmlDoc, i: usize, n: &str) -> Result<usize> {
    let v: Vec<_> = d
        .el(i)
        .children
        .iter()
        .copied()
        .filter(|&c| hp(d, c, n))
        .collect();
    if v.len() != 1 {
        return Err(shape("missing or duplicate table geometry element"));
    }
    Ok(v[0])
}
pub(crate) fn number(d: &XmlDoc, i: usize, n: &str) -> Result<u32> {
    d.el(i)
        .attr_value(n)
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|&v| v <= i32::MAX as u32)
        .ok_or_else(|| shape("missing or out-of-range table geometry attribute"))
}
pub(crate) fn put(
    d: &XmlDoc,
    i: usize,
    n: &str,
    v: impl ToString,
    cuts: &mut Vec<Cut>,
) -> Result<()> {
    let a = d
        .el(i)
        .attr(n)
        .ok_or_else(|| shape("missing editable attribute"))?;
    let value = v.to_string();
    if a.value != value {
        cuts.push((a.value_span.clone(), value.into_bytes()));
    }
    Ok(())
}
pub(crate) struct Cell {
    pub(crate) el: usize,
    pub(crate) sz: usize,
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) rs: usize,
    pub(crate) cs: usize,
    pub(crate) w: u32,
    pub(crate) h: u32,
}
struct Grid {
    table: usize,
    sz: usize,
    cells: Vec<Cell>,
    cols: usize,
    rows: usize,
}
pub(crate) fn axis(
    cells: &[Cell],
    count: usize,
    total: Option<u32>,
    horizontal: bool,
) -> Result<Vec<u32>> {
    let mut positions = vec![None; count + 1];
    positions[0] = Some(0i64);
    positions[count] = total.map(i64::from);
    for _ in 0..=count {
        let mut progress = false;
        for c in cells {
            let (a, b, size) = if horizontal {
                (c.col, c.col + c.cs, c.w)
            } else {
                (c.row, c.row + c.rs, c.h)
            };
            match (positions[a], positions[b]) {
                (Some(x), Some(y)) if y - x != i64::from(size) => {
                    return Err(shape("cell sizes disagree with table grid/total"))
                }
                (Some(x), None) => {
                    positions[b] = Some(x + i64::from(size));
                    progress = true;
                }
                (None, Some(y)) => {
                    positions[a] = Some(y - i64::from(size));
                    progress = true;
                }
                _ => {}
            }
        }
        if !progress {
            break;
        }
    }
    positions
        .windows(2)
        .map(|v| match (v[0], v[1]) {
            (Some(a), Some(b)) if b > a && b - a <= i64::from(i32::MAX) => Ok((b - a) as u32),
            _ => Err(shape(
                "merged grid has unresolved or non-positive track dimensions",
            )),
        })
        .collect()
}
fn grid(d: &XmlDoc, table: usize) -> Result<Grid> {
    let rows = number(d, table, "rowCnt")? as usize;
    let cols = number(d, table, "colCnt")? as usize;
    if rows == 0 || cols == 0 || rows > 1024 || cols > 1024 {
        return Err(shape("table grid must have 1..1024 rows and columns"));
    }
    if number(d, table, "cellSpacing")? != 0 {
        return Err(shape("nonzero cellSpacing is unsupported"));
    }
    let sz = child(d, table, "sz")?;
    let mut occupied = BTreeSet::new();
    let mut cells = Vec::new();
    let mut row = 0;
    for &tr in &d.el(table).children {
        if !hp(d, tr, "tr") {
            continue;
        }
        for &tc in &d.el(tr).children {
            if !hp(d, tc, "tc") {
                return Err(shape("row contains non-cell geometry"));
            }
            if d.descendants(tc).any(|i| hp(d, i, "tbl")) {
                return Err(unsupported(
                    "nested_table",
                    "nested table geometry is unsupported",
                ));
            }
            let addr = child(d, tc, "cellAddr")?;
            let span = child(d, tc, "cellSpan")?;
            let cs = number(d, span, "colSpan")? as usize;
            let rs = number(d, span, "rowSpan")? as usize;
            let r = number(d, addr, "rowAddr")? as usize;
            let c = number(d, addr, "colAddr")? as usize;
            if r != row || rs == 0 || cs == 0 || r + rs > rows || c + cs > cols {
                return Err(shape("cell address/span outside grid"));
            }
            for y in r..r + rs {
                for x in c..c + cs {
                    if !occupied.insert((y, x)) {
                        return Err(shape("overlapping merged cells"));
                    }
                }
            }
            let size = child(d, tc, "cellSz")?;
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
        row += 1;
    }
    if row != rows || occupied.len() != rows * cols {
        return Err(shape("incomplete table grid"));
    }
    Ok(Grid {
        table,
        sz,
        cells,
        cols,
        rows,
    })
}
fn grid_axis(d: &XmlDoc, g: &Grid, horizontal: bool) -> Result<Vec<u32>> {
    let rel = if horizontal {
        "widthRelTo"
    } else {
        "heightRelTo"
    };
    if d.el(g.sz).attr_value(rel) != Some("ABSOLUTE") {
        return Err(shape("relative requested table dimension is unsupported"));
    }
    // hp:sz.height is a placement extent, not the sum of cell minima.
    let extent = if horizontal {
        Some(number(d, g.sz, "width")?)
    } else {
        None
    };
    axis(
        &g.cells,
        if horizontal { g.cols } else { g.rows },
        extent,
        horizontal,
    )
}
fn total(values: &[u32]) -> Result<u32> {
    let sum = values
        .iter()
        .try_fold(0u32, |a, &v| a.checked_add(v))
        .ok_or_else(|| bad("table dimension overflow"))?;
    if sum > i32::MAX as u32 || values.contains(&0) {
        return Err(bad(
            "dimensions must be positive signed HWPUNIT integers; sum <= 2147483647",
        ));
    }
    Ok(sum)
}
fn dimensions(values: Option<&[u32]>, uniform: Option<u32>, count: usize) -> Result<Vec<u32>> {
    let result =
        match (values, uniform) {
            (Some(v), None) if v.len() == count => v.to_vec(),
            (None, Some(v)) => vec![v; count],
            _ => return Err(bad(
                "supply exactly one dimension array of exact track count or one uniform dimension",
            )),
        };
    total(&result)?;
    Ok(result)
}
fn margins(d: &XmlDoc, g: &Grid, c: &Cell) -> Result<[u32; 4]> {
    let own = d
        .el(c.el)
        .attr_value("hasMargin")
        .ok_or_else(|| shape("cell lacks hasMargin"))?;
    let el = match own {
        "1" => child(d, c.el, "cellMargin")?,
        "0" => child(d, g.table, "inMargin")?,
        _ => return Err(shape("invalid hasMargin")),
    };
    Ok([
        number(d, el, "left")?,
        number(d, el, "right")?,
        number(d, el, "top")?,
        number(d, el, "bottom")?,
    ])
}
fn minimum(w: u32, h: u32, m: [u32; 4]) -> Result<()> {
    if u64::from(m[0]) + u64::from(m[1]) >= u64::from(w)
        || u64::from(m[2]) + u64::from(m[3]) >= u64::from(h)
    {
        return Err(bad(
            "cell dimensions must exceed effective margins by at least one HWPUNIT",
        ));
    }
    Ok(())
}
fn record(d: &mut Draft, node: &Node, table: &Node, cuts: Vec<Cut>, intent: Intent) -> Result<()> {
    if cuts.is_empty() {
        return Ok(());
    }
    let paras: Vec<_> = d
        .doc
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Paragraph && d.doc.is_descendant(n, &table.id))
        .cloned()
        .collect();
    for p in paras {
        d.doc.check_editable(&p)?;
        d.anchor(&p);
    }
    // Empty tables cannot occur in accepted grids, but the carrier anchor is explicit.
    let doc = d.doc.doc_of(table);
    let parent = doc
        .ancestors(table.el)
        .find(|&i| hp(doc, i, "p"))
        .and_then(|i| {
            d.doc
                .nodes
                .iter()
                .find(|n| n.entry == table.entry && n.el == i)
        })
        .cloned()
        .ok_or_else(|| shape("table has no paragraph carrier"))?;
    d.anchor(&parent);
    d.splice_entry(&node.entry, cuts)?;
    d.changed.push(node.id.clone());
    if node.id != table.id {
        d.changed.push(table.id.clone());
    }
    d.intents.push(intent);
    Ok(())
}
fn resize(
    d: &mut Draft,
    node: &Node,
    table: &Node,
    g: &Grid,
    widths: Vec<u32>,
    heights: Vec<u32>,
    command: &str,
) -> Result<()> {
    let doc = d.doc.doc_of(table);
    let tw = if widths.is_empty() {
        number(doc, g.sz, "width")?
    } else {
        total(&widths)?
    };
    let th = if heights.is_empty() {
        number(doc, g.sz, "height")?
    } else {
        total(&heights)?
    };
    let mut cuts = Vec::new();
    let mut changed_cells = Vec::new();
    for c in &g.cells {
        let w = if widths.is_empty() {
            c.w
        } else {
            total(&widths[c.col..c.col + c.cs])?
        };
        let h = if heights.is_empty() {
            c.h
        } else {
            total(&heights[c.row..c.row + c.rs])?
        };
        let m = margins(doc, g, c)?;
        // An unchanged minimum may equal its margins in a native document.
        if !widths.is_empty() && w != c.w && u64::from(m[0]) + u64::from(m[1]) >= u64::from(w)
            || !heights.is_empty() && h != c.h && u64::from(m[2]) + u64::from(m[3]) >= u64::from(h)
        {
            return Err(bad(
                "requested cell dimension must exceed effective margins",
            ));
        }
        if w != c.w || h != c.h {
            let n = d
                .doc
                .nodes
                .iter()
                .find(|n| n.entry == table.entry && n.el == c.el)
                .ok_or_else(|| shape("cell is not indexed"))?;
            d.doc.check_editable(n)?;
            changed_cells.push(n.id.clone());
        }
        if !widths.is_empty() {
            put(doc, c.sz, "width", w, &mut cuts)?;
        }
        if !heights.is_empty() {
            put(doc, c.sz, "height", h, &mut cuts)?;
        }
    }
    if !widths.is_empty() {
        put(doc, g.sz, "width", tw, &mut cuts)?;
    }
    if !heights.is_empty() {
        put(doc, g.sz, "height", th, &mut cuts)?;
    }
    let intent = Intent::TableGeometry {
        command: command.into(),
        entry: table.entry.clone(),
        path: doc.path(node.el),
        widths,
        heights,
        table_size: (tw, th),
        changed_cells: changed_cells.clone(),
    };
    record(d, node, table, cuts, intent)?;
    d.changed.extend(changed_cells);
    Ok(())
}
pub(crate) fn tracks(
    d: &mut Draft,
    target: &str,
    values: Option<&[u32]>,
    uniform: Option<u32>,
    columns: bool,
) -> Result<()> {
    let table = d.node(target, Some(Kind::Table))?;
    gate(d.doc.table_command_block_reason(
        if columns {
            "table.setColumnWidths"
        } else {
            "table.setRowHeights"
        },
        &table,
    ))?;
    let g = grid(d.doc.doc_of(&table), table.el)?;
    let old = grid_axis(d.doc.doc_of(&table), &g, columns)?;
    let requested = dimensions(values, uniform, old.len())?;
    let (w, h) = if columns {
        (requested, vec![])
    } else {
        (vec![], requested)
    };
    resize(
        d,
        &table,
        &table,
        &g,
        w,
        h,
        if columns {
            "table.setColumnWidths"
        } else {
            "table.setRowHeights"
        },
    )
}
pub(crate) fn parent_table(d: &Draft, cell: &Node) -> Result<Node> {
    let xml = d.doc.doc_of(cell);
    let el = xml
        .ancestors(cell.el)
        .find(|&i| hp(xml, i, "tbl"))
        .ok_or_else(|| shape("cell lacks parent table"))?;
    let t = d
        .doc
        .nodes
        .iter()
        .find(|n| n.entry == cell.entry && n.el == el)
        .cloned()
        .ok_or_else(|| shape("table is not indexed"))?;
    d.doc.check_editable(&t)?;
    Ok(t)
}
pub(crate) fn cell_size(
    d: &mut Draft,
    target: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<()> {
    if width.is_none() && height.is_none() {
        return Err(bad("cell.setSize requires width and/or height"));
    }
    let node = d.node(target, Some(Kind::Cell))?;
    gate(d.doc.cell_command_block_reason("cell.setSize", &node))?;
    let table = parent_table(d, &node)?;
    let g = grid(d.doc.doc_of(&table), table.el)?;
    let cell = g
        .cells
        .iter()
        .find(|c| c.el == node.el)
        .ok_or_else(|| shape("cell outside grid"))?;
    if cell.rs != 1 || cell.cs != 1 {
        return Err(unsupported("merged_cell_size_unsupported","use table track commands to resize a merged region; direct merged cell size is unsupported"));
    }
    let doc = d.doc.doc_of(&table);
    let mut w = if width.is_some() {
        grid_axis(doc, &g, true)?
    } else {
        vec![]
    };
    let mut h = if height.is_some() {
        grid_axis(doc, &g, false)?
    } else {
        vec![]
    };
    if let Some(v) = width {
        w[cell.col] = v;
    }
    if let Some(v) = height {
        h[cell.row] = v;
    }
    resize(d, &node, &table, &g, w, h, "cell.setSize")
}
fn blank_fill(d: &XmlDoc, i: usize) -> Result<Vec<u8>> {
    let outer = d.el(i).outer();
    let mut edits: Vec<Cut> = vec![(
        d.el(i)
            .attr("id")
            .ok_or_else(|| shape("borderFill without id"))?
            .value_span
            .clone(),
        Vec::new(),
    )];
    edits.extend(
        d.el(i)
            .children
            .iter()
            .filter(|&&c| d.el(c).is(NS_HC, "fillBrush"))
            .map(|&c| (d.el(c).outer(), Vec::new())),
    );
    let mut out = Vec::new();
    let mut pos = outer.start;
    edits.sort_by_key(|(r, _)| r.start);
    for (r, v) in edits {
        out.extend_from_slice(d.raw(pos..r.start));
        out.extend(v);
        pos = r.end;
    }
    out.extend_from_slice(d.raw(pos..outer.end));
    Ok(out)
}
fn fill_color(d: &XmlDoc, i: usize) -> Option<String> {
    let brushes: Vec<_> = d
        .el(i)
        .children
        .iter()
        .copied()
        .filter(|&c| d.el(c).is(NS_HC, "fillBrush"))
        .collect();
    if brushes.is_empty() {
        return Some("none".into());
    }
    if brushes.len() != 1 {
        return None;
    }
    let brush = d.el(brushes[0]);
    if brush.children.len() != 1 {
        return None;
    }
    let win = d.el(brush.children[0]);
    if !win.is(NS_HC, "winBrush")
        || win.attr_value("hatchStyle").is_some()
        || win.attr_value("alpha").is_some_and(|v| v != "0")
    {
        return None;
    }
    win.attr_value("faceColor").map(|v| {
        if v.eq_ignore_ascii_case("none") {
            "none".into()
        } else {
            v.to_ascii_uppercase()
        }
    })
}
pub(crate) fn cell_format(d: &mut Draft, target: &str, format: &CellFormat) -> Result<()> {
    if format == &CellFormat::default()
        || format
            .cell_margin
            .as_ref()
            .is_some_and(|m| m == &CellMargin::default())
    {
        return Err(bad(
            "cell.setFormat requires borderFillId, background or nonempty cellMargin",
        ));
    }
    if let Some(c) = &format.background {
        if c != "none"
            && (c.len() != 7
                || !c.starts_with('#')
                || !c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))
        {
            return Err(bad("background must be #RRGGBB or none"));
        }
    }
    let node = d.node(target, Some(Kind::Cell))?;
    gate(d.doc.cell_command_block_reason("cell.setFormat", &node))?;
    let table = parent_table(d, &node)?;
    let doc = d.doc.doc_of(&node);
    let mut cuts = Vec::new();
    if format.border_fill_id.is_some() || format.background.is_some() {
        let header = d
            .doc
            .info
            .header
            .as_ref()
            .and_then(|n| d.doc.xml.get(n))
            .ok_or_else(|| shape("missing header"))?;
        let defs: Vec<_> = header
            .iter()
            .filter(|(_, e)| e.is(NS_HH, "borderFill"))
            .map(|(i, _)| i)
            .collect();
        let mut ids = BTreeSet::new();
        for &i in &defs {
            if !ids.insert(
                header
                    .el(i)
                    .attr_value("id")
                    .ok_or_else(|| shape("borderFill lacks id"))?,
            ) {
                return Err(shape("duplicate borderFill ID"));
            }
        }
        let base = format
            .border_fill_id
            .as_deref()
            .or_else(|| doc.el(node.el).attr_value("borderFillIDRef"))
            .ok_or_else(|| shape("cell lacks borderFillIDRef"))?;
        let source = *defs
            .iter()
            .find(|&&i| header.el(i).attr_value("id") == Some(base))
            .ok_or_else(|| {
                EngineError::new(
                    Code::TargetNotFound,
                    Stage::Validate,
                    "borderFillId is not defined in header",
                )
            })?;
        let selected = if let Some(color) = &format.background {
            let color = if color == "none" {
                color.clone()
            } else {
                color.to_ascii_uppercase()
            };
            let borders = blank_fill(header, source)?;
            defs.iter().copied().find(|&i| fill_color(header,i).as_deref()==Some(&color) && blank_fill(header,i).is_ok_and(|b|b==borders)).ok_or_else(||unsupported("background_resource_unavailable","no existing borderFill has the requested background and identical borders; new resources are forbidden"))?
        } else {
            source
        };
        put(
            doc,
            node.el,
            "borderFillIDRef",
            header.el(selected).attr_value("id").unwrap(),
            &mut cuts,
        )?;
    }
    if let Some(m) = &format.cell_margin {
        let g = grid(doc, table.el)?;
        let cell = g.cells.iter().find(|c| c.el == node.el).unwrap();
        let el = child(doc, node.el, "cellMargin")?;
        for (name, value) in [
            ("left", m.left),
            ("right", m.right),
            ("top", m.top),
            ("bottom", m.bottom),
        ] {
            if let Some(value) = value {
                if value > i32::MAX as u32 {
                    return Err(bad("margin exceeds signed HWPUNIT"));
                }
                put(doc, el, name, value, &mut cuts)?;
            }
        }
        // Enabling hasMargin activates all four stored values, including unspecified sides.
        let effective = [
            m.left.unwrap_or(number(doc, el, "left")?),
            m.right.unwrap_or(number(doc, el, "right")?),
            m.top.unwrap_or(number(doc, el, "top")?),
            m.bottom.unwrap_or(number(doc, el, "bottom")?),
        ];
        minimum(cell.w, cell.h, effective)?;
        put(doc, node.el, "hasMargin", 1, &mut cuts)?;
    }
    let intent = Intent::CellFormat {
        entry: node.entry.clone(),
        path: doc.path(node.el),
        format: format.clone(),
    };
    record(d, &node, &table, cuts, intent)
}
