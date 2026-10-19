//! Native rectangular table object generation for `table.insert`.
//!
//! This module owns object XML only. [`build`] returns the bytes of exactly
//! one `<hp:tbl>` element: no carrier paragraph or run, no XML declaration, no
//! splice plan and no node remap. The caller keeps the source bytes, chooses
//! the insertion point, adds the carrier `hp:p`/`hp:run`, records the declared
//! node change and invalidates layout caches.
//!
//! The element/attribute order and the defaults follow public HWPX samples and
//! the MIT-licensed `hwpx-workbench` reference (`vendor/hwpx_utils.py::table_xml`,
//! `build.py::Builder.table`). No template is copied verbatim: this module
//! writes a native table from scratch and never synthesizes a style, character
//! shape, paragraph shape or borderFill. Every reference it emits must already
//! exist in the source header.
//!
//! Contract and remaining uncertainty: `docs/decisions/new-node-table-notes.md`.

use hae_diag::{Code, EngineError, Result, Stage};
use hae_package::{PackageState, NS_HH, NS_HP, NS_HP10};
use hae_xml::{escape_text, is_xml_char, XmlDoc};
use std::{collections::BTreeSet, fmt::Write as _};

/// HWPUNIT value bound for explicit `width`/`rowHeight` requests.
const MAX_HWPUNIT: u32 = 1_000_000;
/// Bound on rows × columns, checked before any cell-sized allocation.
const MAX_CELLS: u64 = 1024;

/// Element names that own a numeric shape/object id in a section document.
/// Paragraph ids are a separate space and are handled explicitly.
const SHAPES: [&str; 19] = [
    "tbl",
    "pic",
    "equation",
    "ole",
    "container",
    "chart",
    "textart",
    "shape",
    "group",
    "rect",
    "ellipse",
    "line",
    "arc",
    "polygon",
    "curve",
    "connectLine",
    "button",
    "cellzone",
    "video",
];

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::InvalidArgument, Stage::Validate, message)
}

fn unavailable(reason: &str, message: impl Into<String>) -> EngineError {
    EngineError::new(Code::Unsupported, Stage::Validate, message).with_reason(reason)
}

/// Resolved, already validated header references. The strings are the exact
/// `id` attribute spellings found in the source header.
struct Refs {
    border_fill: String,
    char_pr: String,
    para_pr: String,
    style: String,
}

fn definitions(header: &XmlDoc, local: &str) -> Result<Vec<(String, usize)>> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, e) in header.iter() {
        if !e.is(NS_HH, local) {
            continue;
        }
        let id = e.attr_value("id").ok_or_else(|| {
            unavailable(
                "table_insert_style_resource_unavailable",
                format!("hh:{local} has no id"),
            )
        })?;
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || id.parse::<u32>().is_err() {
            return Err(unavailable(
                "table_insert_style_resource_unavailable",
                format!("hh:{local} has a non-numeric id"),
            ));
        }
        if !seen.insert(id.to_string()) {
            return Err(unavailable(
                "table_insert_style_resource_unavailable",
                format!("duplicate hh:{local} id {id}"),
            ));
        }
        out.push((id.to_string(), i));
    }
    Ok(out)
}

/// A borderFill is visually usable when all four cell edges are drawn. This is
/// only a preference among existing definitions; nothing is created.
fn visible_border(header: &XmlDoc, el: usize) -> bool {
    ["leftBorder", "rightBorder", "topBorder", "bottomBorder"]
        .iter()
        .all(|name| {
            header
                .el(el)
                .children
                .iter()
                .copied()
                .find(|&c| header.el(c).is(NS_HH, name))
                .and_then(|c| header.el(c).attr_value("type"))
                .is_some_and(|kind| kind != "NONE")
        })
}

fn header_name(state: &PackageState) -> Result<&str> {
    if state.get("Contents/header.xml").is_some() {
        return Ok("Contents/header.xml");
    }
    state
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .find(|name| name.rsplit('/').next() == Some("header.xml"))
        .ok_or_else(|| {
            unavailable(
                "table_insert_style_resource_unavailable",
                "package has no header.xml",
            )
        })
}

fn refs(state: &PackageState) -> Result<Refs> {
    let name = header_name(state)?;
    let payload = state.get(name).cloned().ok_or_else(|| {
        unavailable(
            "table_insert_style_resource_unavailable",
            "header.xml payload is missing",
        )
    })?;
    let header = XmlDoc::parse(payload).map_err(|e| {
        unavailable(
            "table_insert_style_resource_unavailable",
            format!("header.xml is not usable: {}", e.diagnostic().message),
        )
    })?;

    let borders = definitions(&header, "borderFill")?;
    let chars = definitions(&header, "charPr")?;
    let paras = definitions(&header, "paraPr")?;
    let styles: Vec<_> = definitions(&header, "style")?
        .into_iter()
        .filter(|(_, el)| header.el(*el).attr_value("type") == Some("PARA"))
        .collect();
    if borders.is_empty() || chars.is_empty() || paras.is_empty() || styles.is_empty() {
        return Err(unavailable(
            "table_insert_style_resource_unavailable",
            "header must define at least one borderFill, charPr, paraPr and PARA style",
        ));
    }

    let border_fill = borders
        .iter()
        .find(|(_, el)| visible_border(&header, *el))
        .unwrap_or(&borders[0])
        .0
        .clone();

    let style = styles[0].0.clone();
    let style_el = header.el(styles[0].1);
    let pick = |preferred: Option<&str>, list: &[(String, usize)]| -> String {
        if let Some(p) = preferred {
            if list.iter().any(|(id, _)| id.as_str() == p) {
                return p.to_string();
            }
        }
        list[0].0.clone()
    };

    Ok(Refs {
        border_fill,
        char_pr: pick(style_el.attr_value("charPrIDRef"), &chars),
        para_pr: pick(style_el.attr_value("paraPrIDRef"), &paras),
        style,
    })
}

/// Every XML part may hold paragraph/shape ids, regardless of its filename.
fn xml_entry(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".xml")
}

fn scan_ids(doc: &XmlDoc, shapes: &mut Option<u32>, paragraphs: &mut Option<u32>) {
    for (_, e) in doc.iter() {
        if !matches!(e.ns.as_deref(), Some(NS_HP) | Some(NS_HP10)) {
            continue;
        }
        let value = e.attr_value("id").and_then(|v| v.parse::<u32>().ok());
        if e.local == "p" {
            if let Some(v) = value {
                *paragraphs = Some((*paragraphs).map_or(v, |m| m.max(v)));
            }
        } else if SHAPES.contains(&e.local.as_str()) {
            if let Some(v) = value {
                *shapes = Some((*shapes).map_or(v, |m| m.max(v)));
            }
        }
    }
}

fn bump(max: Option<u32>) -> Result<u32> {
    max.map_or(Ok(1), |v| {
        v.checked_add(1).ok_or_else(|| {
            unavailable(
                "table_insert_id_exhausted",
                "no unused numeric object or paragraph id remains",
            )
        })
    })
}

/// Numeric id allocation is document-wide and deterministic: scan the target
/// section plus every parseable `.xml` part in `state`, then use max + 1.
/// This keeps insertion from reusing an id even when the source has no table
/// and when another section or XML part has a nonstandard filename.
fn ids(state: &PackageState, xml: &XmlDoc) -> Result<(u32, u32)> {
    let mut shapes = None;
    let mut paragraphs = None;
    scan_ids(xml, &mut shapes, &mut paragraphs);
    for entry in &state.entries {
        if !xml_entry(&entry.name) {
            continue;
        }
        if let Ok(doc) = XmlDoc::parse(entry.payload.clone()) {
            scan_ids(&doc, &mut shapes, &mut paragraphs);
        }
    }
    Ok((bump(shapes)?, bump(paragraphs)?))
}

/// Resolve a non-empty prefix bound to the paragraph namespace in the target
/// paragraph's scope. The chosen prefix is guaranteed to be declared, so the
/// generated fragment never relies on an unbound name.
fn scope_prefix(xml: &XmlDoc, at: usize) -> Result<String> {
    let mut chain = Vec::new();
    let mut current = Some(at);
    while let Some(i) = current {
        chain.push(i);
        current = xml.el(i).parent;
    }
    let mut bindings = std::collections::BTreeMap::<String, String>::new();
    for i in chain.into_iter().rev() {
        for a in &xml.el(i).attrs {
            if let Some(prefix) = a.qname.strip_prefix("xmlns:") {
                bindings.insert(prefix.to_string(), a.value.clone());
            }
        }
    }

    let mut fallback: Option<String> = None;
    for (prefix, uri) in &bindings {
        if uri == NS_HP {
            if prefix == "hp" {
                return Ok(prefix.clone());
            }
            if fallback.is_none() {
                fallback = Some(prefix.clone());
            }
        }
    }
    if let Some(prefix) = fallback.take() {
        return Ok(prefix);
    }
    for (prefix, uri) in &bindings {
        if uri == NS_HP10 {
            if prefix == "hp" || prefix == "hp10" {
                return Ok(prefix.clone());
            }
            if fallback.is_none() {
                fallback = Some(prefix.clone());
            }
        }
    }
    fallback.ok_or_else(|| {
        unavailable(
            "table_insert_namespace_unavailable",
            "target paragraph has no non-empty prefix bound to the HWPX paragraph namespace",
        )
    })
}

/// Cell text follows the paragraph-insertion rule: no tabs or breaks, valid
/// XML characters only, and no other control characters.
fn check_text(text: &str) -> Result<()> {
    if text.contains(['\t', '\n', '\r']) {
        return Err(invalid(
            "table.insert cell text must not contain tabs or line breaks",
        ));
    }
    if let Some(c) = text.chars().find(|&c| !is_xml_char(c) || c.is_control()) {
        return Err(invalid(format!(
            "table.insert cell text contains forbidden character U+{:04X}",
            c as u32
        )));
    }
    Ok(())
}

/// Build one native, rectangular, non-merged table object.
///
/// `state` and `xml` are the **pre-request** package and target section.
/// `paragraph` is the element index of the target paragraph in `xml`; it is
/// used only to resolve the in-scope paragraph prefix and to keep id scanning
/// anchored to the actual insertion document. The return value is only the
/// `<hp:tbl>` bytes, never a carrier paragraph or run.
///
/// `cells` is empty for a blank grid, or exactly `rows` inner vectors of
/// exactly `columns` strings each (`cells[r][c]`). `width` is the total table
/// width; it is divided across
/// columns with the remainder going to the leftmost columns, so every column
/// receives a positive HWPUNIT width and the column widths sum exactly to
/// `width`. Every row has `row_height`, and the table height is their checked
/// sum.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    state: &PackageState,
    xml: &XmlDoc,
    paragraph: usize,
    rows: u32,
    columns: u32,
    width: u32,
    row_height: u32,
    cells: &[Vec<String>],
) -> Result<Vec<u8>> {
    if rows == 0 || columns == 0 {
        return Err(invalid("table.insert requires positive rows and columns"));
    }
    let total_cells = u64::from(rows) * u64::from(columns);
    if total_cells > MAX_CELLS {
        return Err(invalid("table.insert supports at most 1024 cells"));
    }
    if !(1..=MAX_HWPUNIT).contains(&width) {
        return Err(invalid("table.insert width must be in 1..=1000000 HWPUNIT"));
    }
    if !(1..=MAX_HWPUNIT).contains(&row_height) {
        return Err(invalid(
            "table.insert rowHeight must be in 1..=1000000 HWPUNIT",
        ));
    }
    if width < columns {
        return Err(invalid(
            "table.insert width must give every column a positive HWPUNIT width",
        ));
    }
    if paragraph >= xml.elements().len() {
        return Err(invalid(
            "table.insert target paragraph is outside the section document",
        ));
    }
    let target = xml.el(paragraph);
    if target.local != "p" || !matches!(target.ns.as_deref(), Some(NS_HP) | Some(NS_HP10)) {
        return Err(invalid(
            "table.insert target must be an HWPX paragraph element",
        ));
    }
    if !cells.is_empty() {
        if cells.len() as u64 != u64::from(rows) {
            return Err(invalid(
                "table.insert cells must be empty or have exactly rows inner arrays",
            ));
        }
        if cells
            .iter()
            .any(|row| row.len() as u64 != u64::from(columns))
        {
            return Err(invalid(
                "table.insert every cells row must have exactly columns entries",
            ));
        }
    }
    for text in cells.iter().flatten() {
        check_text(text)?;
    }

    let total_height = u64::from(rows) * u64::from(row_height);
    let total_height: u32 = total_height
        .try_into()
        .map_err(|_| invalid("table.insert total table height exceeds the HWPUNIT range"))?;

    let refs = refs(state)?;
    let prefix = scope_prefix(xml, paragraph)?;
    let (shape_id, first_pid) = ids(state, xml)?;
    first_pid
        .checked_add((total_cells - 1) as u32)
        .ok_or_else(|| {
            unavailable(
                "table_insert_id_exhausted",
                "no unused numeric paragraph id range remains",
            )
        })?;

    let base = width / columns;
    let extra = width % columns;
    let mut column_widths = Vec::with_capacity(columns as usize);
    for c in 0..columns {
        column_widths.push(base + u32::from(c < extra));
    }

    let p = prefix.as_str();
    let bf = refs.border_fill.as_str();
    let cp = refs.char_pr.as_str();
    let pp = refs.para_pr.as_str();
    let st = refs.style.as_str();
    let mut out = String::with_capacity(512 + total_cells as usize * 448);

    write!(
        out,
        "<{p}:tbl id=\"{shape_id}\" zOrder=\"0\" numberingType=\"TABLE\" \
         textWrap=\"TOP_AND_BOTTOM\" textFlow=\"BOTH_SIDES\" lock=\"0\" \
         dropcapstyle=\"None\" pageBreak=\"CELL\" repeatHeader=\"0\" \
         rowCnt=\"{rows}\" colCnt=\"{columns}\" cellSpacing=\"0\" \
         borderFillIDRef=\"{bf}\" noAdjust=\"0\">\
         <{p}:sz width=\"{width}\" widthRelTo=\"ABSOLUTE\" height=\"{total_height}\" \
         heightRelTo=\"ABSOLUTE\" protect=\"0\"/>\
         <{p}:pos treatAsChar=\"1\" affectLSpacing=\"0\" flowWithText=\"1\" \
         allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" \
         horzRelTo=\"PARA\" vertAlign=\"TOP\" horzAlign=\"LEFT\" \
         vertOffset=\"0\" horzOffset=\"0\"/>\
         <{p}:outMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>\
         <{p}:inMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>"
    )
    .expect("writing to a String cannot fail");

    for r in 0..rows {
        write!(out, "<{p}:tr>").expect("writing to a String cannot fail");
        for c in 0..columns {
            let cell_width = column_widths[c as usize];
            let pid = first_pid + r * columns + c;
            let text = if cells.is_empty() {
                ""
            } else {
                cells[r as usize][c as usize].as_str()
            };
            write!(
                out,
                "<{p}:tc name=\"\" header=\"0\" hasMargin=\"0\" protect=\"0\" \
                 editable=\"0\" dirty=\"0\" borderFillIDRef=\"{bf}\">\
                 <{p}:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" \
                 vertAlign=\"TOP\" linkListIDRef=\"0\" linkListNextIDRef=\"0\" \
                 textWidth=\"0\" textHeight=\"0\" hasTextRef=\"0\" hasNumRef=\"0\">\
                 <{p}:p id=\"{pid}\" paraPrIDRef=\"{pp}\" styleIDRef=\"{st}\" \
                 pageBreak=\"0\" columnBreak=\"0\" merged=\"0\">\
                 <{p}:run charPrIDRef=\"{cp}\">"
            )
            .expect("writing to a String cannot fail");
            if text.is_empty() {
                write!(out, "<{p}:t/>").expect("writing to a String cannot fail");
            } else {
                write!(out, "<{p}:t>{}</{p}:t>", escape_text(text))
                    .expect("writing to a String cannot fail");
            }
            write!(
                out,
                "</{p}:run></{p}:p></{p}:subList>\
                 <{p}:cellAddr colAddr=\"{c}\" rowAddr=\"{r}\"/>\
                 <{p}:cellSpan colSpan=\"1\" rowSpan=\"1\"/>\
                 <{p}:cellSz width=\"{cell_width}\" height=\"{row_height}\"/>\
                 <{p}:cellMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>\
                 </{p}:tc>"
            )
            .expect("writing to a String cannot fail");
        }
        write!(out, "</{p}:tr>").expect("writing to a String cannot fail");
    }
    write!(out, "</{p}:tbl>").expect("writing to a String cannot fail");
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hae_package::{Entry, Origin};
    use std::sync::Arc;

    const HS: &str = "http://www.hancom.co.kr/hwpml/2011/section";
    const NONE_EDGES: &str = "<hh:leftBorder type=\"NONE\" width=\"0.1 mm\" color=\"#000000\"/>\
        <hh:rightBorder type=\"NONE\" width=\"0.1 mm\" color=\"#000000\"/>\
        <hh:topBorder type=\"NONE\" width=\"0.1 mm\" color=\"#000000\"/>\
        <hh:bottomBorder type=\"NONE\" width=\"0.1 mm\" color=\"#000000\"/>";
    const SOLID_EDGES: &str = "<hh:leftBorder type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/>\
        <hh:rightBorder type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/>\
        <hh:topBorder type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/>\
        <hh:bottomBorder type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/>";

    fn header() -> String {
        format!(
            "<hh:head xmlns:hh=\"{NS_HH}\"><hh:refList>\
             <hh:borderFills itemCnt=\"1\"><hh:borderFill id=\"1\" threeD=\"0\" \
             shadow=\"0\" centerLine=\"NONE\" breakCellSeparateLine=\"0\">\
             <hh:slash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/>\
             <hh:backSlash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/>\
             {NONE_EDGES}</hh:borderFill></hh:borderFills>\
             <hh:charProperties itemCnt=\"1\"><hh:charPr id=\"0\"/></hh:charProperties>\
             <hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\"/></hh:paraProperties>\
             <hh:styles itemCnt=\"1\"><hh:style id=\"0\" paraPrIDRef=\"0\" \
             charPrIDRef=\"0\" type=\"PARA\"/></hh:styles></hh:refList></hh:head>"
        )
    }

    fn header_with_visible_border() -> String {
        format!(
            "<hh:head xmlns:hh=\"{NS_HH}\"><hh:refList>\
             <hh:borderFills itemCnt=\"2\">\
             <hh:borderFill id=\"1\" threeD=\"0\" shadow=\"0\" centerLine=\"NONE\" \
             breakCellSeparateLine=\"0\">{NONE_EDGES}</hh:borderFill>\
             <hh:borderFill id=\"2\" threeD=\"0\" shadow=\"0\" centerLine=\"NONE\" \
             breakCellSeparateLine=\"0\">{SOLID_EDGES}</hh:borderFill>\
             </hh:borderFills>\
             <hh:charProperties itemCnt=\"1\"><hh:charPr id=\"0\"/></hh:charProperties>\
             <hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\"/></hh:paraProperties>\
             <hh:styles itemCnt=\"1\"><hh:style id=\"0\" paraPrIDRef=\"0\" \
             charPrIDRef=\"0\" type=\"PARA\"/></hh:styles></hh:refList></hh:head>"
        )
    }

    fn header_with_char_style_first() -> String {
        format!(
            "<hh:head xmlns:hh=\"{NS_HH}\"><hh:refList>\
             <hh:borderFills itemCnt=\"1\"><hh:borderFill id=\"1\">{NONE_EDGES}\
             </hh:borderFill></hh:borderFills>\
             <hh:charProperties itemCnt=\"1\"><hh:charPr id=\"0\"/></hh:charProperties>\
             <hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\"/></hh:paraProperties>\
             <hh:styles itemCnt=\"2\">\
             <hh:style id=\"9\" paraPrIDRef=\"0\" charPrIDRef=\"0\" type=\"CHAR\"/>\
             <hh:style id=\"0\" paraPrIDRef=\"0\" charPrIDRef=\"0\" type=\"PARA\"/>\
             </hh:styles></hh:refList></hh:head>"
        )
    }

    fn section(body: &str) -> String {
        format!("<hs:sec xmlns:hs=\"{HS}\" xmlns:hp=\"{NS_HP}\">{body}</hs:sec>")
    }

    fn package(header: &str, section: &str) -> (PackageState, XmlDoc) {
        let state = PackageState {
            entries: vec![
                Entry {
                    name: "Contents/header.xml".into(),
                    payload: Arc::from(header.as_bytes()),
                    origin: Origin::Original,
                },
                Entry {
                    name: "Contents/section0.xml".into(),
                    payload: Arc::from(section.as_bytes()),
                    origin: Origin::Original,
                },
            ],
        };
        let doc = XmlDoc::parse(section.as_bytes()).unwrap();
        (state, doc)
    }

    fn paragraph(doc: &XmlDoc) -> usize {
        doc.iter()
            .find(|(_, e)| e.is(NS_HP, "p"))
            .map(|(i, _)| i)
            .unwrap()
    }

    fn wrap(prefix: &str, bytes: &[u8]) -> XmlDoc {
        let body = std::str::from_utf8(bytes).unwrap();
        let text = format!("<hs:sec xmlns:hs=\"{HS}\" xmlns:{prefix}=\"{NS_HP}\">{body}</hs:sec>");
        XmlDoc::parse(text.as_bytes()).unwrap()
    }

    fn base_section() -> String {
        section("<hp:p id=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>base</hp:t></hp:run></hp:p>")
    }

    #[test]
    fn blank_table_works_in_a_section_with_no_table() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        let out = build(&state, &doc, p, 2, 3, 6000, 1000, &[]).unwrap();
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.starts_with("<hp:tbl "));
        assert!(text.ends_with("</hp:tbl>"));
        assert!(!text.contains("<hp:p id=\"7\""));

        let d = wrap("hp", &out);
        let (t, _) = d.iter().find(|(_, e)| e.local == "tbl").unwrap();
        assert_eq!(d.el(t).attr_value("rowCnt"), Some("2"));
        assert_eq!(d.el(t).attr_value("colCnt"), Some("3"));
        assert_eq!(d.el(t).attr_value("borderFillIDRef"), Some("1"));

        let trs: Vec<_> = d
            .el(t)
            .children
            .iter()
            .copied()
            .filter(|&c| d.el(c).local == "tr")
            .collect();
        assert_eq!(trs.len(), 2);
        let mut addresses = Vec::new();
        let mut ids = Vec::new();
        for (r, tr) in trs.iter().enumerate() {
            for &tc in &d.el(*tr).children {
                assert_eq!(d.el(tc).local, "tc");
                let addr = d
                    .el(tc)
                    .children
                    .iter()
                    .copied()
                    .find(|&c| d.el(c).local == "cellAddr")
                    .unwrap();
                addresses.push((
                    d.el(addr).attr_value("colAddr").unwrap().to_string(),
                    d.el(addr).attr_value("rowAddr").unwrap().to_string(),
                ));
                assert_eq!(
                    d.el(addr)
                        .attr_value("rowAddr")
                        .unwrap()
                        .parse::<usize>()
                        .unwrap(),
                    r
                );
                let sz = d
                    .el(tc)
                    .children
                    .iter()
                    .copied()
                    .find(|&c| d.el(c).local == "cellSz")
                    .unwrap();
                let cell_p = d.descendants(tc).find(|&i| d.el(i).local == "p").unwrap();
                let id = d.el(cell_p).attr_value("id").unwrap().to_string();
                assert!(!ids.contains(&id));
                ids.push(id);
                if r == 0 {
                    assert_eq!(d.el(sz).attr_value("height"), Some("1000"));
                }
            }
        }
        assert_eq!(addresses.len(), 6);
        assert_eq!(ids.len(), 6);
        let t_count = d.iter().filter(|(_, e)| e.local == "t").count();
        assert_eq!(t_count, 6);
    }

    #[test]
    fn exact_bytes_for_one_cell() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        let cells = vec![vec!["가<&".to_string()]];
        let out = build(&state, &doc, p, 1, 1, 6000, 1000, &cells).unwrap();
        let expected = "<hp:tbl id=\"1\" zOrder=\"0\" numberingType=\"TABLE\" \
textWrap=\"TOP_AND_BOTTOM\" textFlow=\"BOTH_SIDES\" lock=\"0\" dropcapstyle=\"None\" \
pageBreak=\"CELL\" repeatHeader=\"0\" rowCnt=\"1\" colCnt=\"1\" cellSpacing=\"0\" \
borderFillIDRef=\"1\" noAdjust=\"0\">\
<hp:sz width=\"6000\" widthRelTo=\"ABSOLUTE\" height=\"1000\" heightRelTo=\"ABSOLUTE\" protect=\"0\"/>\
<hp:pos treatAsChar=\"1\" affectLSpacing=\"0\" flowWithText=\"1\" allowOverlap=\"0\" \
holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" horzRelTo=\"PARA\" vertAlign=\"TOP\" \
horzAlign=\"LEFT\" vertOffset=\"0\" horzOffset=\"0\"/>\
<hp:outMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>\
<hp:inMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>\
<hp:tr><hp:tc name=\"\" header=\"0\" hasMargin=\"0\" protect=\"0\" editable=\"0\" \
dirty=\"0\" borderFillIDRef=\"1\">\
<hp:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"TOP\" \
linkListIDRef=\"0\" linkListNextIDRef=\"0\" textWidth=\"0\" textHeight=\"0\" \
hasTextRef=\"0\" hasNumRef=\"0\">\
<hp:p id=\"1\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" \
merged=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>가&lt;&amp;</hp:t></hp:run></hp:p>\
</hp:subList>\
<hp:cellAddr colAddr=\"0\" rowAddr=\"0\"/>\
<hp:cellSpan colSpan=\"1\" rowSpan=\"1\"/>\
<hp:cellSz width=\"6000\" height=\"1000\"/>\
<hp:cellMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/></hp:tc></hp:tr></hp:tbl>";
        assert_eq!(std::str::from_utf8(&out).unwrap(), expected);
    }

    #[test]
    fn unicode_and_markup_are_escaped_and_widths_sum() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        let cells = vec![vec!["<&\"한글😀".to_string(), "a&b".to_string()]];
        let out = build(&state, &doc, p, 1, 2, 6001, 1200, &cells).unwrap();
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.contains("<hp:t>&lt;&amp;\"한글😀</hp:t>"));
        assert!(text.contains("<hp:t>a&amp;b</hp:t>"));
        assert!(!text.contains("<hp:t><&"));

        let d = wrap("hp", &out);
        let widths: Vec<u32> = d
            .iter()
            .filter(|(_, e)| e.local == "cellSz")
            .map(|(_, e)| e.attr_value("width").unwrap().parse().unwrap())
            .collect();
        assert_eq!(widths, vec![3001, 3000]);
        assert_eq!(widths.iter().sum::<u32>(), 6001);
    }

    #[test]
    fn source_prefix_is_reused() {
        let body = "<hpx:p id=\"0\"><hpx:run charPrIDRef=\"0\"/></hpx:p>";
        let source = format!("<hs:sec xmlns:hs=\"{HS}\" xmlns:hpx=\"{NS_HP}\">{body}</hs:sec>");
        let (state, doc) = package(&header(), &source);
        let p = paragraph(&doc);
        let out = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap();
        assert!(std::str::from_utf8(&out).unwrap().starts_with("<hpx:tbl "));
        let _ = wrap("hpx", &out);
    }

    #[test]
    fn default_namespace_only_is_refused() {
        let source = format!(
            "<hs:sec xmlns:hs=\"{HS}\" xmlns=\"{NS_HP}\">\
             <p id=\"0\"><run charPrIDRef=\"0\"/></p></hs:sec>"
        );
        let (state, doc) = package(&header(), &source);
        let p = paragraph(&doc);
        let err = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap_err();
        assert_eq!(err.reason(), Some("table_insert_namespace_unavailable"));
    }

    #[test]
    fn visible_border_is_preferred_without_synthesis() {
        let (state, doc) = package(&header_with_visible_border(), &base_section());
        let p = paragraph(&doc);
        let out = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap();
        let d = wrap("hp", &out);
        let (t, _) = d.iter().find(|(_, e)| e.local == "tbl").unwrap();
        assert_eq!(d.el(t).attr_value("borderFillIDRef"), Some("2"));
        let tc = d
            .iter()
            .find(|(_, e)| e.local == "tc")
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(d.el(tc).attr_value("borderFillIDRef"), Some("2"));
    }

    #[test]
    fn paragraph_style_is_selected_not_character_style() {
        let (state, doc) = package(&header_with_char_style_first(), &base_section());
        let p = paragraph(&doc);
        let out = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap();
        let d = wrap("hp", &out);
        let cell_p = d
            .iter()
            .find(|(_, e)| e.local == "p")
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(d.el(cell_p).attr_value("styleIDRef"), Some("0"));
    }

    #[test]
    fn ids_are_document_wide_and_do_not_reuse_existing_ones() {
        let zero = base_section();
        let state = PackageState {
            entries: vec![
                Entry {
                    name: "Contents/header.xml".into(),
                    payload: Arc::from(header().as_bytes()),
                    origin: Origin::Original,
                },
                Entry {
                    name: "Contents/section0.xml".into(),
                    payload: Arc::from(zero.as_bytes()),
                    origin: Origin::Original,
                },
                Entry {
                    name: "Contents/section1.xml".into(),
                    payload: Arc::from(
                        section(
                            "<hp:p id=\"50\"><hp:run charPrIDRef=\"0\"/></hp:p>\
                             <hp:tbl id=\"70\" rowCnt=\"1\" colCnt=\"1\"/>",
                        )
                        .as_bytes(),
                    ),
                    origin: Origin::Original,
                },
                Entry {
                    name: "Contents/other.xml".into(),
                    payload: Arc::from(
                        section(
                            "<hp:p id=\"500\"><hp:run charPrIDRef=\"0\"/></hp:p>\
                             <hp:tbl id=\"700\" rowCnt=\"1\" colCnt=\"1\"/>\
                             <hp:video id=\"900\"/>",
                        )
                        .as_bytes(),
                    ),
                    origin: Origin::Original,
                },
            ],
        };
        let doc = XmlDoc::parse(zero.as_bytes()).unwrap();
        let p = paragraph(&doc);
        let out = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap();
        let d = wrap("hp", &out);
        let (t, _) = d.iter().find(|(_, e)| e.local == "tbl").unwrap();
        assert_eq!(d.el(t).attr_value("id"), Some("901"));
        let cell_p = d
            .iter()
            .find(|(_, e)| e.local == "p")
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(d.el(cell_p).attr_value("id"), Some("501"));
    }

    #[test]
    fn invalid_shapes_and_dimensions_are_rejected() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        for (rows, columns, width, height) in [
            (0, 1, 6000, 1000),
            (1, 0, 6000, 1000),
            (1, 1, 0, 1000),
            (1, 1, 1_000_001, 1000),
            (1, 1, 6000, 0),
            (1, 1, 6000, 1_000_001),
            (1, 3, 2, 1000),
        ] {
            let err = build(&state, &doc, p, rows, columns, width, height, &[]).unwrap_err();
            assert_eq!(err.code(), Code::InvalidArgument);
        }
    }

    #[test]
    fn cell_count_and_cell_array_are_bounded() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        for (rows, columns) in [(1, 1025), (32, 33), (1025, 1)] {
            let err = build(&state, &doc, p, rows, columns, 6000, 1000, &[]).unwrap_err();
            assert_eq!(err.code(), Code::InvalidArgument);
        }
        for cells in [
            vec![vec!["only".to_string()]],
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
                vec!["e".to_string(), "f".to_string()],
            ],
            vec![
                vec!["a".to_string()],
                vec!["b".to_string(), "c".to_string()],
            ],
        ] {
            let err = build(&state, &doc, p, 2, 2, 6000, 1000, &cells).unwrap_err();
            assert_eq!(err.code(), Code::InvalidArgument);
        }

        let out = build(
            &state,
            &doc,
            p,
            2,
            2,
            6000,
            1000,
            &[
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
            ],
        )
        .unwrap();
        let d = wrap("hp", &out);
        let texts: Vec<String> = d
            .iter()
            .filter(|(_, e)| e.local == "t")
            .map(|(i, _)| d.direct_text(i))
            .collect();
        assert_eq!(texts, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn invalid_text_and_paragraph_index_are_rejected() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        for text in ["a\tb", "a\nb", "a\rb", "a\u{1}b", "a\u{7f}b"] {
            let err =
                build(&state, &doc, p, 1, 1, 3000, 900, &[vec![text.to_string()]]).unwrap_err();
            assert_eq!(err.code(), Code::InvalidArgument);
        }
        let err = build(&state, &doc, doc.elements().len(), 1, 1, 3000, 900, &[]).unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn missing_header_resources_are_rejected() {
        let no_style = format!(
            "<hh:head xmlns:hh=\"{NS_HH}\"><hh:refList>\
             <hh:borderFills itemCnt=\"1\"><hh:borderFill id=\"1\">{NONE_EDGES}\
             </hh:borderFill></hh:borderFills>\
             <hh:charProperties itemCnt=\"1\"><hh:charPr id=\"0\"/></hh:charProperties>\
             <hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\"/></hh:paraProperties>\
             </hh:refList></hh:head>"
        );
        let (state, doc) = package(&no_style, &base_section());
        let p = paragraph(&doc);
        let err = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap_err();
        assert_eq!(
            err.reason(),
            Some("table_insert_style_resource_unavailable")
        );

        let duplicate = format!(
            "<hh:head xmlns:hh=\"{NS_HH}\"><hh:refList>\
             <hh:borderFills itemCnt=\"2\"><hh:borderFill id=\"1\">{NONE_EDGES}\
             </hh:borderFill><hh:borderFill id=\"1\">{NONE_EDGES}</hh:borderFill>\
             </hh:borderFills>\
             <hh:charProperties itemCnt=\"1\"><hh:charPr id=\"0\"/></hh:charProperties>\
             <hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\"/></hh:paraProperties>\
             <hh:styles itemCnt=\"1\"><hh:style id=\"0\" paraPrIDRef=\"0\" \
             charPrIDRef=\"0\" type=\"PARA\"/></hh:styles></hh:refList></hh:head>"
        );
        let (state, doc) = package(&duplicate, &base_section());
        let p = paragraph(&doc);
        let err = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap_err();
        assert_eq!(
            err.reason(),
            Some("table_insert_style_resource_unavailable")
        );
    }
}
