//! Request/source-only table verifier. No hae-model or hae-edit dependency.
use crate::{
    attr, change, fail, hp, parse, patched, unsupported, CellFormat, Splice, VerifyOperation,
};
use hae_diag::{Limits, Result};
use hae_package::{PackageInfo, PackageState, NS_HC, NS_HH};
use hae_xml::XmlDoc;
use std::collections::{BTreeSet, VecDeque};

fn one(d: &XmlDoc, p: usize, name: &str) -> Result<usize> {
    let mut found = d
        .el(p)
        .children
        .iter()
        .copied()
        .filter(|&i| hp(d.el(i), name));
    let first = found
        .next()
        .ok_or_else(|| unsupported("table child missing"))?;
    if found.next().is_some() {
        return Err(unsupported("duplicate table child"));
    }
    Ok(first)
}
fn num(d: &XmlDoc, i: usize, a: &str) -> Result<u32> {
    let value = attr(d, i, a)?
        .value
        .parse::<u32>()
        .map_err(|_| fail("invalid table integer"))?;
    if value > 2147483647 {
        return Err(fail("table integer outside HWPUNIT range"));
    }
    Ok(value)
}
fn set(d: &XmlDoc, i: usize, a: &str, v: impl ToString, cuts: &mut Vec<Splice>) -> Result<()> {
    let original = attr(d, i, a)?;
    let want = v.to_string();
    if original.value != want {
        cuts.push((original.value_span.clone(), want.into_bytes()));
    }
    Ok(())
}
struct Region {
    element: usize,
    size: usize,
    y: usize,
    x: usize,
    dy: usize,
    dx: usize,
    width: u32,
    height: u32,
}
struct Source {
    table: usize,
    size: usize,
    regions: Vec<Region>,
    columns: Vec<u32>,
    rows: Vec<u32>,
}
fn tracks(regions: &[Region], count: usize, whole: u32, is_x: bool) -> Result<Vec<u32>> {
    // Prefix-coordinate constraint graph. Every known interval is an edge;
    // disconnected boundaries and contradictory cycles are refused.
    let mut graph = vec![Vec::<(usize, i64)>::new(); count + 1];
    graph[0].push((count, i64::from(whole)));
    graph[count].push((0, -i64::from(whole)));
    for r in regions {
        let (a, b, v) = if is_x {
            (r.x, r.x + r.dx, r.width)
        } else {
            (r.y, r.y + r.dy, r.height)
        };
        graph[a].push((b, i64::from(v)));
        graph[b].push((a, -i64::from(v)));
    }
    let mut coord = vec![None; count + 1];
    coord[0] = Some(0i64);
    let mut queue = VecDeque::from([0]);
    while let Some(a) = queue.pop_front() {
        for &(b, delta) in &graph[a] {
            let v = coord[a].unwrap() + delta;
            match coord[b] {
                Some(old) if old != v => {
                    return Err(fail("inconsistent table dimension constraints"))
                }
                None => {
                    coord[b] = Some(v);
                    queue.push_back(b);
                }
                _ => {}
            }
        }
    }
    let coordinates = coord
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| unsupported("underdetermined merged track"))?;
    coordinates
        .windows(2)
        .map(|p| {
            let n = p[1] - p[0];
            if !(1..=2147483647).contains(&n) {
                Err(fail("non-positive table track"))
            } else {
                Ok(n as u32)
            }
        })
        .collect()
}
fn source(d: &XmlDoc, t: usize) -> Result<Source> {
    if !hp(d.el(t), "tbl") {
        return Err(fail("table operation requires tbl"));
    }
    let nr = num(d, t, "rowCnt")? as usize;
    let nc = num(d, t, "colCnt")? as usize;
    if !(1..=1024).contains(&nr) || !(1..=1024).contains(&nc) || num(d, t, "cellSpacing")? != 0 {
        return Err(unsupported("table grid limits or spacing"));
    }
    let size = one(d, t, "sz")?;
    for field in ["widthRelTo", "heightRelTo"] {
        if attr(d, size, field)?.value != "ABSOLUTE" {
            return Err(unsupported("relative table extent"));
        }
    }
    let rows: Vec<_> = d
        .el(t)
        .children
        .iter()
        .copied()
        .filter(|&i| hp(d.el(i), "tr"))
        .collect();
    if rows.len() != nr {
        return Err(fail("table row count mismatch"));
    }
    let mut regions = vec![];
    for (y, &row) in rows.iter().enumerate() {
        for &c in &d.el(row).children {
            if !hp(d.el(c), "tc") || d.descendants(c).any(|i| hp(d.el(i), "tbl")) {
                return Err(unsupported("non-cell row or nested table"));
            }
            let address = one(d, c, "cellAddr")?;
            let span = one(d, c, "cellSpan")?;
            let size = one(d, c, "cellSz")?;
            let r = Region {
                element: c,
                size,
                y: num(d, address, "rowAddr")? as usize,
                x: num(d, address, "colAddr")? as usize,
                dy: num(d, span, "rowSpan")? as usize,
                dx: num(d, span, "colSpan")? as usize,
                width: num(d, size, "width")?,
                height: num(d, size, "height")?,
            };
            if r.y != y || r.dy == 0 || r.dx == 0 || r.y + r.dy > nr || r.x + r.dx > nc {
                return Err(fail("invalid cell bounds"));
            }
            regions.push(r);
        }
    }
    // Sweep row intervals independently instead of trusting a model occupancy map.
    for y in 0..nr {
        let mut intervals: Vec<_> = regions
            .iter()
            .filter(|r| r.y <= y && y < r.y + r.dy)
            .map(|r| (r.x, r.x + r.dx))
            .collect();
        intervals.sort_unstable();
        let mut end = 0;
        for (a, b) in intervals {
            if a != end {
                return Err(fail("table grid overlap/hole"));
            }
            end = b;
        }
        if end != nc {
            return Err(fail("incomplete table row"));
        }
    }
    let columns = tracks(&regions, nc, num(d, size, "width")?, true)?;
    let rows = tracks(&regions, nr, num(d, size, "height")?, false)?;
    Ok(Source {
        table: t,
        size,
        regions,
        columns,
        rows,
    })
}
fn sum(values: &[u32]) -> Result<u32> {
    let mut n = 0u64;
    for &v in values {
        if v == 0 {
            return Err(fail("zero table dimension"));
        }
        n += u64::from(v);
    }
    if n > 2147483647 {
        return Err(fail("table dimension overflow"));
    }
    Ok(n as u32)
}
fn requested(values: Option<&[u32]>, same: Option<u32>, count: usize) -> Result<Vec<u32>> {
    let values = match (values, same) {
        (Some(v), None) if v.len() == count => v.to_vec(),
        (None, Some(v)) => vec![v; count],
        _ => return Err(fail("invalid track request shape")),
    };
    sum(&values)?;
    Ok(values)
}
fn enough(width: u32, height: u32, margin: [u32; 4]) -> Result<()> {
    if u64::from(margin[0]) + u64::from(margin[1]) >= u64::from(width)
        || u64::from(margin[2]) + u64::from(margin[3]) >= u64::from(height)
    {
        return Err(fail("cell extent is less than margin minimum"));
    }
    Ok(())
}
fn edges(d: &XmlDoc, e: usize) -> Result<[u32; 4]> {
    Ok([
        num(d, e, "left")?,
        num(d, e, "right")?,
        num(d, e, "top")?,
        num(d, e, "bottom")?,
    ])
}
fn cell_minimum(d: &XmlDoc, s: &Source, r: &Region, w: u32, h: u32) -> Result<()> {
    let m = match attr(d, r.element, "hasMargin")?.value.as_str() {
        "0" => one(d, s.table, "inMargin")?,
        "1" => one(d, r.element, "cellMargin")?,
        _ => return Err(fail("invalid cell margin flag")),
    };
    enough(w, h, edges(d, m)?)
}
fn border_signature(d: &XmlDoc, i: usize) -> Result<Vec<u8>> {
    let outer = d.el(i).outer();
    let mut exclude = vec![attr(d, i, "id")?.value_span.clone()];
    for &c in &d.el(i).children {
        if d.el(c).is(NS_HC, "fillBrush") {
            exclude.push(d.el(c).outer());
        }
    }
    patched(
        d.raw(outer.clone()),
        exclude
            .into_iter()
            .map(|r| (r.start - outer.start..r.end - outer.start, vec![]))
            .collect(),
    )
}
fn background(d: &XmlDoc, i: usize) -> Option<String> {
    let b: Vec<_> = d
        .el(i)
        .children
        .iter()
        .copied()
        .filter(|&c| d.el(c).is(NS_HC, "fillBrush"))
        .collect();
    match b.as_slice() {
        [] => Some("none".into()),
        [b] => {
            let c = &d.el(*b).children;
            if c.len() != 1 {
                return None;
            }
            let w = d.el(c[0]);
            if !w.is(NS_HC, "winBrush")
                || w.attr("hatchStyle").is_some()
                || !matches!(w.attr_value("alpha"), None | Some("0"))
            {
                return None;
            }
            w.attr_value("faceColor").map(|s| {
                if s.eq_ignore_ascii_case("none") {
                    "none".into()
                } else {
                    s.to_ascii_uppercase()
                }
            })
        }
        _ => None,
    }
}
fn formatting(
    state: &PackageState,
    info: &PackageInfo,
    d: &XmlDoc,
    t: usize,
    c: usize,
    f: &CellFormat,
    limits: &Limits,
) -> Result<Vec<Splice>> {
    if f.border_fill_id.is_none() && f.background.is_none() && f.cell_margin.is_none() {
        return Err(fail("empty cell format"));
    }
    let mut cuts = vec![];
    if f.border_fill_id.is_some() || f.background.is_some() {
        let h = parse(
            state,
            info.header.as_deref().ok_or_else(|| fail("no header"))?,
            limits,
        )?;
        let definitions: Vec<_> = h
            .iter()
            .filter(|(_, e)| e.is(NS_HH, "borderFill"))
            .map(|(i, _)| i)
            .collect();
        let mut ids = BTreeSet::new();
        for &i in &definitions {
            if !ids.insert(attr(&h, i, "id")?.value.as_str()) {
                return Err(fail("duplicate borderFill"));
            }
        }
        let base = f
            .border_fill_id
            .as_deref()
            .unwrap_or(&attr(d, c, "borderFillIDRef")?.value);
        let source = *definitions
            .iter()
            .find(|&&i| h.el(i).attr_value("id") == Some(base))
            .ok_or_else(|| fail("missing borderFillId"))?;
        let selected = match &f.background {
            None => source,
            Some(color) => {
                if color != "none"
                    && (color.len() != 7
                        || !color.starts_with('#')
                        || !color.bytes().skip(1).all(|b| b.is_ascii_hexdigit()))
                {
                    return Err(fail("invalid cell background"));
                }
                let color = if color == "none" {
                    color.clone()
                } else {
                    color.to_ascii_uppercase()
                };
                let signature = border_signature(&h, source)?;
                let mut found = None;
                for &i in &definitions {
                    if background(&h, i).as_deref() == Some(color.as_str())
                        && border_signature(&h, i)? == signature
                    {
                        found = Some(i);
                        break;
                    }
                }
                found.ok_or_else(|| unsupported("no existing background resource"))?
            }
        };
        set(
            d,
            c,
            "borderFillIDRef",
            &attr(&h, selected, "id")?.value,
            &mut cuts,
        )?;
    }
    if let Some(m) = &f.cell_margin {
        if m.left.is_none() && m.right.is_none() && m.top.is_none() && m.bottom.is_none() {
            return Err(fail("empty cellMargin"));
        }
        let grid = source(d, t)?;
        let region = grid
            .regions
            .iter()
            .find(|r| r.element == c)
            .ok_or_else(|| fail("cell absent"))?;
        let margin = one(d, c, "cellMargin")?;
        let mut v = edges(d, margin)?;
        for (index, (name, value)) in [
            ("left", m.left),
            ("right", m.right),
            ("top", m.top),
            ("bottom", m.bottom),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(value) = value {
                if value > 2147483647 {
                    return Err(fail("margin overflow"));
                }
                v[index] = value;
                set(d, margin, name, value, &mut cuts)?;
            }
        }
        enough(region.width, region.height, v)?;
        set(d, c, "hasMargin", 1, &mut cuts)?;
    }
    Ok(cuts)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    state: &mut PackageState,
    info: &PackageInfo,
    entry: &str,
    d: &XmlDoc,
    target: usize,
    op: &VerifyOperation,
    limits: &Limits,
) -> Result<Option<usize>> {
    let t = if hp(d.el(target), "tbl") {
        target
    } else if hp(d.el(target), "tc") {
        d.ancestors(target)
            .find(|&i| hp(d.el(i), "tbl"))
            .ok_or_else(|| fail("cell not in table"))?
    } else {
        return Err(fail("wrong table/cell target kind"));
    };
    let cuts = if let VerifyOperation::CellSetFormat { format, .. } = op {
        if !hp(d.el(target), "tc") {
            return Err(fail("format target not cell"));
        }
        formatting(state, info, d, t, target, format, limits)?
    } else {
        let s = source(d, t)?;
        let mut w = s.columns.clone();
        let mut h = s.rows.clone();
        match op {
            VerifyOperation::TableSetColumnWidths { widths, width, .. } => {
                if t != target {
                    return Err(fail("column target not table"));
                }
                w = requested(widths.as_deref(), *width, w.len())?;
            }
            VerifyOperation::TableSetRowHeights {
                heights, height, ..
            } => {
                if t != target {
                    return Err(fail("row target not table"));
                }
                h = requested(heights.as_deref(), *height, h.len())?;
            }
            VerifyOperation::CellSetSize { width, height, .. } => {
                if width.is_none() && height.is_none() {
                    return Err(fail("empty cell size"));
                }
                let cell = s
                    .regions
                    .iter()
                    .find(|r| r.element == target)
                    .ok_or_else(|| fail("size target not cell"))?;
                if cell.dx != 1 || cell.dy != 1 {
                    return Err(unsupported("merged cell direct size"));
                }
                if let Some(v) = width {
                    w[cell.x] = *v;
                }
                if let Some(v) = height {
                    h[cell.y] = *v;
                }
            }
            _ => return Err(fail("wrong table operation")),
        }
        let mut changes = vec![];
        let width = sum(&w)?;
        let height = sum(&h)?;
        for r in &s.regions {
            let cw = sum(&w[r.x..r.x + r.dx])?;
            let ch = sum(&h[r.y..r.y + r.dy])?;
            cell_minimum(d, &s, r, cw, ch)?;
            set(d, r.size, "width", cw, &mut changes)?;
            set(d, r.size, "height", ch, &mut changes)?;
        }
        set(d, s.size, "width", width, &mut changes)?;
        set(d, s.size, "height", height, &mut changes)?;
        changes
    };
    if cuts.is_empty() {
        return Ok(None);
    }
    change(state, entry, d, cuts)?;
    Ok(Some(t))
}
