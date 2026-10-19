//! Request/source-only verifier for a newly inserted native table.
//!
//! The verifier re-derives the expected `<hp:tbl>` bytes from the pre-request
//! package, the pre-request target section and the request alone. It never
//! consumes an editor plan, editor bytes or a node declaration, and it does
//! not depend on `hae-edit`/`hae-model`. The editor module
//! (`hae-edit::table_insert`) is a separate implementation of the same
//! contract; see `docs/decisions/new-node-table-notes.md`.
//!
//! Like the editor, this module returns only the table element bytes. The
//! carrier paragraph/run, splice point, ID remap and cache changes are the
//! caller's responsibility.

use hae_diag::{Code, EngineError, Result, Stage};
use hae_package::{PackageState, NS_HH, NS_HP, NS_HP10};
use hae_xml::{escape_text, is_xml_char, XmlDoc};
use std::{collections::BTreeSet, fmt::Write as _};

const MAX_HWPUNIT: u32 = 1_000_000;
const MAX_CELLS: u64 = 1024;
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

fn fail(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::VerifyFailed, Stage::Save, message)
}

fn request(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::InvalidArgument, Stage::Validate, message)
}

fn unavailable(reason: &str, message: impl Into<String>) -> EngineError {
    fail(message).with_reason(reason)
}

struct SourceRefs {
    border_fill: String,
    char_pr: String,
    para_pr: String,
    style: String,
}

fn ids_of(header: &XmlDoc, local: &str) -> Result<Vec<(String, usize)>> {
    let mut found = Vec::new();
    let mut distinct = BTreeSet::new();
    for (i, e) in header.iter() {
        if !e.is(NS_HH, local) {
            continue;
        }
        let Some(id) = e.attr_value("id") else {
            return Err(unavailable(
                "table_insert_style_resource_unavailable",
                format!("hh:{local} lacks an id"),
            ));
        };
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || id.parse::<u32>().is_err() {
            return Err(unavailable(
                "table_insert_style_resource_unavailable",
                format!("hh:{local} id is not numeric"),
            ));
        }
        if !distinct.insert(id.to_string()) {
            return Err(unavailable(
                "table_insert_style_resource_unavailable",
                format!("hh:{local} id {id} is duplicated"),
            ));
        }
        found.push((id.to_string(), i));
    }
    Ok(found)
}

fn is_visible(header: &XmlDoc, fill: usize) -> bool {
    ["leftBorder", "rightBorder", "topBorder", "bottomBorder"]
        .iter()
        .all(|name| {
            header
                .el(fill)
                .children
                .iter()
                .copied()
                .find(|&c| header.el(c).is(NS_HH, name))
                .and_then(|c| header.el(c).attr_value("type"))
                .is_some_and(|t| t != "NONE")
        })
}

fn find_header(state: &PackageState) -> Result<&str> {
    if state.get("Contents/header.xml").is_some() {
        return Ok("Contents/header.xml");
    }
    for entry in &state.entries {
        if entry.name.rsplit('/').next() == Some("header.xml") {
            return Ok(entry.name.as_str());
        }
    }
    Err(unavailable(
        "table_insert_style_resource_unavailable",
        "package has no header.xml",
    ))
}

fn source_refs(state: &PackageState) -> Result<SourceRefs> {
    let name = find_header(state)?;
    let payload = state.get(name).cloned().ok_or_else(|| {
        unavailable(
            "table_insert_style_resource_unavailable",
            "header payload missing",
        )
    })?;
    let header = XmlDoc::parse(payload).map_err(|e| {
        unavailable(
            "table_insert_style_resource_unavailable",
            format!("header.xml cannot be parsed: {}", e.diagnostic().message),
        )
    })?;

    let borders = ids_of(&header, "borderFill")?;
    let chars = ids_of(&header, "charPr")?;
    let paras = ids_of(&header, "paraPr")?;
    let styles: Vec<_> = ids_of(&header, "style")?
        .into_iter()
        .filter(|(_, el)| header.el(*el).attr_value("type") == Some("PARA"))
        .collect();
    if borders.is_empty() || chars.is_empty() || paras.is_empty() || styles.is_empty() {
        return Err(unavailable(
            "table_insert_style_resource_unavailable",
            "header needs at least one borderFill, charPr, paraPr and PARA style",
        ));
    }

    let border_fill = borders
        .iter()
        .find(|(_, fill)| is_visible(&header, *fill))
        .map_or_else(|| borders[0].0.clone(), |(id, _)| id.clone());

    let style = styles[0].0.clone();
    let style_el = header.el(styles[0].1);
    let choose = |preferred: Option<&str>, list: &[(String, usize)]| {
        preferred
            .filter(|p| list.iter().any(|(id, _)| id.as_str() == *p))
            .map_or_else(|| list[0].0.clone(), |p| p.to_string())
    };
    Ok(SourceRefs {
        border_fill,
        char_pr: choose(style_el.attr_value("charPrIDRef"), &chars),
        para_pr: choose(style_el.attr_value("paraPrIDRef"), &paras),
        style,
    })
}

fn is_xml(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".xml")
}

fn collect_ids(doc: &XmlDoc, shape: &mut Option<u32>, paragraph: &mut Option<u32>) {
    for (_, e) in doc.iter() {
        if !matches!(e.ns.as_deref(), Some(NS_HP) | Some(NS_HP10)) {
            continue;
        }
        let Some(id) = e.attr_value("id").and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        if e.local == "p" {
            *paragraph = Some((*paragraph).map_or(id, |old| old.max(id)));
        } else if SHAPES.contains(&e.local.as_str()) {
            *shape = Some((*shape).map_or(id, |old| old.max(id)));
        }
    }
}

fn next(max: Option<u32>) -> Result<u32> {
    match max {
        None => Ok(1),
        Some(v) => v.checked_add(1).ok_or_else(|| {
            unavailable(
                "table_insert_id_exhausted",
                "no unused numeric object or paragraph id remains",
            )
        }),
    }
}

fn fresh_ids(state: &PackageState, source: &XmlDoc) -> Result<(u32, u32)> {
    let mut shape = None;
    let mut paragraph = None;
    collect_ids(source, &mut shape, &mut paragraph);
    for entry in &state.entries {
        if !is_xml(&entry.name) {
            continue;
        }
        if let Ok(doc) = XmlDoc::parse(entry.payload.clone()) {
            collect_ids(&doc, &mut shape, &mut paragraph);
        }
    }
    Ok((next(shape)?, next(paragraph)?))
}

fn paragraph_prefix(doc: &XmlDoc, at: usize) -> Result<String> {
    let mut ancestors = Vec::new();
    let mut walk = Some(at);
    while let Some(i) = walk {
        ancestors.push(i);
        walk = doc.el(i).parent;
    }
    let mut bound = std::collections::BTreeMap::<String, String>::new();
    for i in ancestors.into_iter().rev() {
        for attr in &doc.el(i).attrs {
            if let Some(prefix) = attr.qname.strip_prefix("xmlns:") {
                bound.insert(prefix.to_string(), attr.value.clone());
            }
        }
    }

    let mut other = None;
    for (prefix, uri) in &bound {
        if uri != NS_HP {
            continue;
        }
        if prefix == "hp" {
            return Ok(prefix.clone());
        }
        if other.is_none() {
            other = Some(prefix.clone());
        }
    }
    if let Some(prefix) = other.take() {
        return Ok(prefix);
    }
    for (prefix, uri) in &bound {
        if uri != NS_HP10 {
            continue;
        }
        if prefix == "hp" || prefix == "hp10" {
            return Ok(prefix.clone());
        }
        if other.is_none() {
            other = Some(prefix.clone());
        }
    }
    other.ok_or_else(|| {
        unavailable(
            "table_insert_namespace_unavailable",
            "target paragraph has no declared non-empty paragraph prefix",
        )
    })
}

fn text_ok(text: &str) -> Result<()> {
    if text.contains(['\t', '\n', '\r']) {
        return Err(request(
            "table.insert cell text must not contain tabs or line breaks",
        ));
    }
    if let Some(c) = text.chars().find(|&c| !is_xml_char(c) || c.is_control()) {
        return Err(request(format!(
            "table.insert cell text contains forbidden character U+{:04X}",
            c as u32
        )));
    }
    Ok(())
}

/// Independently derive the expected table object bytes.
///
/// `state` and `xml` must be the **pre-request** package and target section;
/// `paragraph` is the target paragraph element index in `xml`. The result is
/// the complete `<hp:tbl>` element and nothing else.
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
        return Err(request("table.insert rows and columns must be positive"));
    }
    let total_cells = u64::from(rows) * u64::from(columns);
    if total_cells > MAX_CELLS {
        return Err(request("table.insert cell count exceeds 1024"));
    }
    if !(1..=MAX_HWPUNIT).contains(&width) {
        return Err(request("table.insert width must be in 1..=1000000 HWPUNIT"));
    }
    if !(1..=MAX_HWPUNIT).contains(&row_height) {
        return Err(request(
            "table.insert rowHeight must be in 1..=1000000 HWPUNIT",
        ));
    }
    if columns > width {
        return Err(request(
            "table.insert width cannot give every column a positive HWPUNIT width",
        ));
    }
    if paragraph >= xml.elements().len() {
        return Err(request(
            "table.insert target paragraph is outside the section document",
        ));
    }
    let target = xml.el(paragraph);
    if target.local != "p" || !matches!(target.ns.as_deref(), Some(NS_HP) | Some(NS_HP10)) {
        return Err(request(
            "table.insert target must be an HWPX paragraph element",
        ));
    }
    if !cells.is_empty() {
        if cells.len() as u64 != u64::from(rows) {
            return Err(request(
                "table.insert cells must be empty or have exactly rows inner arrays",
            ));
        }
        if cells
            .iter()
            .any(|row| row.len() as u64 != u64::from(columns))
        {
            return Err(request(
                "table.insert every cells row must have exactly columns entries",
            ));
        }
    }
    for text in cells.iter().flatten() {
        text_ok(text)?;
    }

    let height = u64::from(rows) * u64::from(row_height);
    let height: u32 = height
        .try_into()
        .map_err(|_| request("table.insert total height exceeds the HWPUNIT range"))?;

    let refs = source_refs(state)?;
    let prefix = paragraph_prefix(xml, paragraph)?;
    let (shape_id, first_pid) = fresh_ids(state, xml)?;
    first_pid
        .checked_add((total_cells - 1) as u32)
        .ok_or_else(|| {
            unavailable(
                "table_insert_id_exhausted",
                "no unused numeric paragraph id range remains",
            )
        })?;

    let mut widths = Vec::with_capacity(columns as usize);
    let share = width / columns;
    let remainder = width - share * columns;
    for c in 0..columns {
        widths.push(if c < remainder { share + 1 } else { share });
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
         <{p}:sz width=\"{width}\" widthRelTo=\"ABSOLUTE\" height=\"{height}\" \
         heightRelTo=\"ABSOLUTE\" protect=\"0\"/>\
         <{p}:pos treatAsChar=\"1\" affectLSpacing=\"0\" flowWithText=\"1\" \
         allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" \
         horzRelTo=\"PARA\" vertAlign=\"TOP\" horzAlign=\"LEFT\" \
         vertOffset=\"0\" horzOffset=\"0\"/>\
         <{p}:outMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>\
         <{p}:inMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>"
    )
    .expect("String writes cannot fail");

    for r in 0..rows {
        write!(out, "<{p}:tr>").expect("String writes cannot fail");
        for c in 0..columns {
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
            .expect("String writes cannot fail");
            if text.is_empty() {
                write!(out, "<{p}:t/>").expect("String writes cannot fail");
            } else {
                write!(out, "<{p}:t>{}</{p}:t>", escape_text(text))
                    .expect("String writes cannot fail");
            }
            write!(
                out,
                "</{p}:run></{p}:p></{p}:subList>\
                 <{p}:cellAddr colAddr=\"{c}\" rowAddr=\"{r}\"/>\
                 <{p}:cellSpan colSpan=\"1\" rowSpan=\"1\"/>\
                 <{p}:cellSz width=\"{}\" height=\"{row_height}\"/>\
                 <{p}:cellMargin left=\"0\" right=\"0\" top=\"0\" bottom=\"0\"/>\
                 </{p}:tc>",
                widths[c as usize]
            )
            .expect("String writes cannot fail");
        }
        write!(out, "</{p}:tr>").expect("String writes cannot fail");
    }
    write!(out, "</{p}:tbl>").expect("String writes cannot fail");
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
             <hh:borderFills itemCnt=\"1\"><hh:borderFill id=\"1\">{NONE_EDGES}\
             </hh:borderFill></hh:borderFills>\
             <hh:charProperties itemCnt=\"1\"><hh:charPr id=\"0\"/></hh:charProperties>\
             <hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\"/></hh:paraProperties>\
             <hh:styles itemCnt=\"1\"><hh:style id=\"0\" paraPrIDRef=\"0\" \
             charPrIDRef=\"0\" type=\"PARA\"/></hh:styles></hh:refList></hh:head>"
        )
    }

    fn header_visible() -> String {
        format!(
            "<hh:head xmlns:hh=\"{NS_HH}\"><hh:refList>\
             <hh:borderFills itemCnt=\"2\">\
             <hh:borderFill id=\"1\">{NONE_EDGES}</hh:borderFill>\
             <hh:borderFill id=\"2\">{SOLID_EDGES}</hh:borderFill>\
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
        (state, XmlDoc::parse(section.as_bytes()).unwrap())
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
    fn exact_expected_bytes_for_one_cell() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        let out = build(
            &state,
            &doc,
            p,
            1,
            1,
            6000,
            1000,
            &[vec!["가<&".to_string()]],
        )
        .unwrap();
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
    fn blank_grid_from_a_document_without_tables() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        let out = build(&state, &doc, p, 2, 3, 6000, 1000, &[]).unwrap();
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.starts_with("<hp:tbl "));
        assert!(text.ends_with("</hp:tbl>"));
        let d = wrap("hp", &out);
        let (table, _) = d.iter().find(|(_, e)| e.local == "tbl").unwrap();
        assert_eq!(d.el(table).attr_value("rowCnt"), Some("2"));
        assert_eq!(d.el(table).attr_value("colCnt"), Some("3"));
        assert_eq!(d.iter().filter(|(_, e)| e.local == "tr").count(), 2);
        assert_eq!(d.iter().filter(|(_, e)| e.local == "tc").count(), 6);
        assert_eq!(d.iter().filter(|(_, e)| e.local == "t").count(), 6);
        let ids: BTreeSet<&str> = d
            .iter()
            .filter(|(_, e)| e.local == "p")
            .filter_map(|(_, e)| e.attr_value("id"))
            .collect();
        assert_eq!(ids, BTreeSet::from(["1", "2", "3", "4", "5", "6"]));
    }

    #[test]
    fn unicode_markup_and_prefix() {
        let body = "<hpx:p id=\"0\"><hpx:run charPrIDRef=\"0\"/></hpx:p>";
        let source = format!("<hs:sec xmlns:hs=\"{HS}\" xmlns:hpx=\"{NS_HP}\">{body}</hs:sec>");
        let (state, doc) = package(&header(), &source);
        let p = paragraph(&doc);
        let out = build(
            &state,
            &doc,
            p,
            1,
            2,
            6001,
            1200,
            &[vec!["<&\"한글😀".to_string(), "a&b".to_string()]],
        )
        .unwrap();
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.starts_with("<hpx:tbl "));
        assert!(text.contains("<hpx:t>&lt;&amp;\"한글😀</hpx:t>"));
        assert!(text.contains("<hpx:t>a&amp;b</hpx:t>"));
        let d = wrap("hpx", &out);
        let widths: Vec<u32> = d
            .iter()
            .filter(|(_, e)| e.local == "cellSz")
            .map(|(_, e)| e.attr_value("width").unwrap().parse().unwrap())
            .collect();
        assert_eq!(widths, vec![3001, 3000]);
    }

    #[test]
    fn missing_paragraph_namespace_is_refused() {
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
    fn invalid_requests_are_rejected() {
        let (state, doc) = package(&header(), &base_section());
        let p = paragraph(&doc);
        for (rows, columns, width, height) in [
            (0, 1, 3000, 900),
            (1, 0, 3000, 900),
            (1, 1, 0, 900),
            (1, 1, 1_000_001, 900),
            (1, 1, 3000, 0),
            (1, 1, 3000, 1_000_001),
            (1, 3, 2, 900),
            (1, 1025, 3000, 900),
            (1025, 1, 3000, 900),
        ] {
            let err = build(&state, &doc, p, rows, columns, width, height, &[]).unwrap_err();
            assert_eq!(err.code(), Code::InvalidArgument);
        }
        for cells in [
            vec![vec!["a".to_string()], vec!["b".to_string()]],
            vec![
                vec!["a".to_string()],
                vec!["b".to_string(), "c".to_string()],
            ],
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
                vec!["e".to_string(), "f".to_string()],
            ],
        ] {
            let err = build(&state, &doc, p, 2, 2, 3000, 900, &cells).unwrap_err();
            assert_eq!(err.code(), Code::InvalidArgument);
        }
        for text in ["tab\there", "line\nbreak", "ctl\u{1}"] {
            let err =
                build(&state, &doc, p, 1, 1, 3000, 900, &[vec![text.to_string()]]).unwrap_err();
            assert_eq!(err.code(), Code::InvalidArgument);
        }
        let err = build(&state, &doc, doc.elements().len(), 1, 1, 3000, 900, &[]).unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);

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
    fn ids_scan_every_xml_part_not_only_section_names() {
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
                    name: "Contents/annex-body.xml".into(),
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
        let (table, _) = d.iter().find(|(_, e)| e.local == "tbl").unwrap();
        assert_eq!(d.el(table).attr_value("id"), Some("901"));
        let cell_p = d
            .iter()
            .find(|(_, e)| e.local == "p")
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(d.el(cell_p).attr_value("id"), Some("501"));
    }

    #[test]
    fn source_references_must_resolve_and_ids_stay_fresh() {
        let (state, doc) = package(&header_visible(), &base_section());
        let p = paragraph(&doc);
        let out = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap();
        let d = wrap("hp", &out);
        let (table, _) = d.iter().find(|(_, e)| e.local == "tbl").unwrap();
        assert_eq!(d.el(table).attr_value("borderFillIDRef"), Some("2"));

        let no_char = format!(
            "<hh:head xmlns:hh=\"{NS_HH}\"><hh:refList>\
             <hh:borderFills itemCnt=\"1\"><hh:borderFill id=\"1\">{NONE_EDGES}\
             </hh:borderFill></hh:borderFills>\
             <hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\"/></hh:paraProperties>\
             <hh:styles itemCnt=\"1\"><hh:style id=\"0\" paraPrIDRef=\"0\" \
             charPrIDRef=\"0\" type=\"PARA\"/></hh:styles></hh:refList></hh:head>"
        );
        let (state, doc) = package(&no_char, &base_section());
        let p = paragraph(&doc);
        let err = build(&state, &doc, p, 1, 1, 3000, 900, &[]).unwrap_err();
        assert_eq!(
            err.reason(),
            Some("table_insert_style_resource_unavailable")
        );
    }
}
