//! document.inspect / element.query / element.get views (spec §7).

use crate::para::{text_element_supported, TextSpan, TextSpans};
use crate::{is_hp, Document, Kind, Node, PieceKind};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_package::{NS_HC, NS_HH};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

pub const PREVIEW_GRAPHEMES: usize = 60;

#[derive(Debug, Clone, Serialize, Default, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CellAddr {
    pub row: u32,
    pub col: u32,
    pub row_span: u32,
    pub col_span: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Outline {
    pub node_id: String,
    pub kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub section: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<CellAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<(i64, i64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaRef>,
    /// True iff at least one command in edits is available on this node.
    pub editable: bool,
    pub edits: BTreeMap<String, EditAvailability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_spans: Option<Vec<TextSpan>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_spans_truncated: Option<bool>,
    /// For cells: at least one descendant paragraph accepts text.replace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_paragraphs_editable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_editable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema)]
pub struct EditAvailability {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modes: Option<Vec<String>>,
    /// Additional arguments needed for the advertised edit path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<BTreeMap<String, serde_json::Value>>,
}

impl EditAvailability {
    fn from_reason(reason: Option<String>) -> Self {
        Self {
            available: reason.is_none(),
            reason,
            modes: None,
            requires: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MediaRef {
    pub item_id: String,
    pub entry: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Query {
    #[serde(default)]
    pub kind: Option<Kind>,
    /// Codepoint-exact substring of the node's text.
    #[serde(default)]
    pub text_contains: Option<String>,
    /// Restrict to descendants of this node.
    #[serde(default)]
    pub ancestor: Option<String>,
    /// Table cell lookup: with `table`, find the cell at (row, col).
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub row: Option<u32>,
    #[serde(default)]
    pub col: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub matches: Vec<Outline>,
    /// Matches dropped because their ends are not grapheme boundaries.
    pub unaligned_matches: usize,
}

fn child_attr(doc: &hae_xml::XmlDoc, el: usize, local: &str, attr: &str) -> Option<String> {
    doc.el(el)
        .children
        .iter()
        .map(|&c| doc.el(c))
        .find(|c| is_hp(c.ns.as_deref()) && c.local == local)
        .and_then(|c| c.attr_value(attr).map(str::to_string))
}

fn num(s: Option<String>) -> Option<u32> {
    s.and_then(|v| v.parse().ok())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentStatsParams {
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentStatsResult {
    pub document_id: String,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub sections: usize,
    pub top_level_paragraphs: usize,
    pub paragraphs: usize,
    pub tables: usize,
    pub cells: usize,
    pub pictures: usize,
    pub equations: usize,
    pub fields: usize,
    pub footnotes: usize,
    pub endnotes: usize,
    pub notes: usize,
    pub memos: usize,
    pub total_chars: usize,
    pub total_nodes: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionListParams {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SectionListResult {
    pub document_id: String,
    pub revision: u64,
    pub sections: Vec<SectionInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SectionInfo {
    pub section: usize,
    pub node_id: String,
    pub paragraphs: usize,
    pub top_level_paragraphs: usize,
    pub page: PageSettings,
    pub column: ColumnSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageSettings {
    /// \"explicit\" if found in secPr/pagePr, \"unspecified\" if absent/inherited.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_orientation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<PageMargin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_starts_on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_page_number: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageMargin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gutter: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSettings {
    /// \"explicit\" if colPr found, \"unspecified\" if absent.
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_size: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_gap: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteListParams {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteListResult {
    pub document_id: String,
    pub revision: u64,
    pub notes: Vec<NoteItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteItem {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_number: Option<String>,
    pub paragraph_node_id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_paragraph_node_id: Option<String>,
}

fn scan_para_controls(
    doc: &hae_xml::XmlDoc,
    el_idx: usize,
    is_root: bool,
    fields: &mut usize,
    footnotes: &mut usize,
    endnotes: &mut usize,
    memos: &mut usize,
) {
    if !is_root && is_hp(doc.el(el_idx).ns.as_deref()) && doc.el(el_idx).local == "p" {
        return;
    }
    let el = doc.el(el_idx);
    if is_hp(el.ns.as_deref()) {
        match el.local.as_str() {
            "fieldBegin" => {
                *fields += 1;
                if el.attr_value("type") == Some("MEMO") {
                    *memos += 1;
                }
            }
            "footNote" => *footnotes += 1,
            "endNote" => *endnotes += 1,
            _ => {}
        }
    }
    for &child in &el.children {
        scan_para_controls(doc, child, false, fields, footnotes, endnotes, memos);
    }
}

impl Document {
    /// Deterministic failures for material edits. No-op requests may bypass the
    /// save gate; those do not constitute an editing capability.
    fn verification_block_reason(&self, n: &Node) -> Option<String> {
        if self.invalid_references {
            return Some("verify_invalid_references".into());
        }
        let doc = self.doc_of(n);
        let top = doc
            .ancestors(n.el)
            .find(|&a| doc.el(a).parent == Some(doc.root()));
        let top = top.unwrap_or(n.el);
        if !(n.kind == Kind::Section
            || is_hp(doc.el(top).ns.as_deref()) && doc.el(top).local == "p")
        {
            return Some("verify_no_top_level_paragraph".into());
        }
        None
    }
    /// Text of a node: paragraph text; for cells/tables the text of their paragraphs.
    pub fn node_text(&self, n: &Node) -> String {
        match n.kind {
            Kind::Paragraph => self.para_text(n).map(|p| p.text).unwrap_or_default(),
            Kind::Cell | Kind::Table | Kind::Section => {
                let paras: Vec<&Node> = self
                    .nodes
                    .iter()
                    .filter(|m| m.kind == Kind::Paragraph && self.is_descendant(m, &n.id))
                    .collect();
                paras
                    .iter()
                    .map(|p| self.node_text(p))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => String::new(),
        }
    }

    pub fn is_descendant(&self, n: &Node, ancestor: &str) -> bool {
        let mut cur = n.parent.clone();
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.node(&p).and_then(|x| x.parent.clone());
        }
        false
    }

    pub fn cell_addr(&self, n: &Node) -> Option<CellAddr> {
        if n.kind != Kind::Cell {
            return None;
        }
        let doc = self.doc_of(n);
        Some(CellAddr {
            row: num(child_attr(doc, n.el, "cellAddr", "rowAddr"))?,
            col: num(child_attr(doc, n.el, "cellAddr", "colAddr"))?,
            row_span: num(child_attr(doc, n.el, "cellSpan", "rowSpan")).unwrap_or(1),
            col_span: num(child_attr(doc, n.el, "cellSpan", "colSpan")).unwrap_or(1),
        })
    }

    /// The `hc:img` element of a picture and its binary item.
    pub fn picture_media(&self, n: &Node) -> Option<(usize, MediaRef)> {
        let doc = self.doc_of(n);
        let img = doc.descendants(n.el).find(|&d| {
            let e = doc.el(d);
            e.local == "img" && e.ns.as_deref() == Some(NS_HC)
        })?;
        let item_id = doc.el(img).attr_value("binaryItemIDRef")?.to_string();
        let item = self.info.item_by_id(&item_id)?;
        let sha256 = hae_package::sha256_hex(self.payload(&item.entry)?);
        Some((
            img,
            MediaRef {
                item_id,
                entry: item.entry.clone(),
                sha256,
            },
        ))
    }

    /// Geometry and content preconditions only; parameter validation and resource
    /// budgets still apply when the caller supplies an actual edit.
    pub fn paragraph_text_spans(&self, n: &Node) -> TextSpans {
        if n.kind != Kind::Paragraph
            || self.check_editable(n).is_err()
            || self.verification_block_reason(n).is_some()
        {
            return TextSpans::default();
        }
        self.para_text(n)
            .map(|p| p.replaceable_spans(self.doc_of(n)))
            .unwrap_or_default()
    }

    fn text_replace_reason(&self, n: &Node, spans: &TextSpans) -> Option<String> {
        if let Some(r) = self.verification_block_reason(n) {
            return Some(r);
        }
        if !spans.text_spans.is_empty()
            || self
                .para_text(n)
                .is_ok_and(|p| p.preserve_replaceable(self.doc_of(n)))
        {
            return None;
        }
        Some(
            if self.para_text(n).is_ok_and(|p| p.grapheme_len() == 0) {
                self.para_text(n)
                    .ok()
                    .and_then(|p| p.empty_insert_reason(self.doc_of(n)))
                    .unwrap_or("no_run")
            } else {
                "no_text_span"
            }
            .into(),
        )
    }

    /// At least one of alignment/lineSpacing must be changeable. Availability
    /// does not promise that every combination of formatting parameters works.
    pub fn paragraph_format_block_reason(&self, n: &Node) -> Option<String> {
        if let Some(r) = self.verification_block_reason(n) {
            return Some(r);
        }
        let Some(header) = self.info.header.as_ref().and_then(|h| self.xml.get(h)) else {
            return Some("no_header".into());
        };
        let Some(pref) = self.doc_of(n).el(n.el).attr_value("paraPrIDRef") else {
            return Some("no_paraPr".into());
        };
        let hh =
            |e: &hae_xml::Element, local: &str| e.ns.as_deref() == Some(NS_HH) && e.local == local;
        let Some((props, _)) = header.iter().find(|(_, e)| hh(e, "paraProperties")) else {
            return Some("no_paraPr".into());
        };
        let Some(pp) =
            header.el(props).children.iter().copied().find(|&p| {
                hh(header.el(p), "paraPr") && header.el(p).attr_value("id") == Some(pref)
            })
        else {
            return Some("no_paraPr".into());
        };
        let align: Vec<_> = header
            .descendants(pp)
            .filter(|&p| hh(header.el(p), "align"))
            .collect();
        let spacing: Vec<_> = header
            .descendants(pp)
            .filter(|&p| hh(header.el(p), "lineSpacing"))
            .collect();
        let can_align = !align.is_empty()
            && align
                .iter()
                .all(|&p| header.el(p).attr("horizontal").is_some());
        let can_space = !spacing.is_empty()
            && spacing.iter().all(|&p| {
                header.el(p).attr("type").is_some() && header.el(p).attr("value").is_some()
            });
        if can_align || can_space {
            None
        } else {
            Some("paraPr_shape".into())
        }
    }

    pub fn cell_set_text_block_reason(&self, n: &Node) -> Option<String> {
        let doc = self.doc_of(n);
        if doc
            .ancestors(n.el)
            .any(|a| is_hp(doc.el(a).ns.as_deref()) && doc.el(a).local == "tc")
        {
            return Some("nested_cell".into());
        }
        if let Some(r) = self.cell_block_reason(n) {
            return Some(r);
        }
        if let Some(r) = self.verification_block_reason(n) {
            return Some(r);
        }
        self.children(&n.id)
            .filter(|p| p.kind == Kind::Paragraph)
            .find_map(|p| self.cell_paragraph_block_reason(p))
    }

    fn cell_paragraph_block_reason(&self, para: &Node) -> Option<String> {
        if let Err(e) = self.check_editable(para) {
            return Some(e.reason().unwrap_or("not_editable").into());
        }
        let pt = self.para_text(para).ok()?;
        let doc = self.doc_of(para);
        if !pt.text.is_empty() && pt.preserve_replaceable(doc) {
            return None;
        }
        let len = pt.text.chars().count();
        if pt
            .pieces
            .iter()
            .any(|p| matches!(p.kind, PieceKind::CData { .. }))
            || pt.text.contains(['\t', '\n', '\r'])
                && !pt
                    .pieces
                    .iter()
                    .any(|p| matches!(p.kind, PieceKind::Control { .. }))
            || pt
                .verification_barriers
                .iter()
                .any(|&b| len == 0 || (0 < b && b < len))
        {
            return Some("verify_unsupported_markup".into());
        }
        if pt.pieces.is_empty() {
            let Some(run) = doc
                .el(para.el)
                .children
                .iter()
                .copied()
                .find(|&p| is_hp(doc.el(p).ns.as_deref()) && doc.el(p).local == "run")
            else {
                return Some("no_run".into());
            };
            let t = doc
                .el(run)
                .children
                .iter()
                .copied()
                .find(|&p| is_hp(doc.el(p).ns.as_deref()) && doc.el(p).local == "t");
            return match t {
                Some(t) if !text_element_supported(doc, t) => {
                    Some("unsupported_content_in_range".into())
                }
                _ => None,
            };
        }
        if pt
            .pieces
            .iter()
            .any(|p| matches!(p.kind, PieceKind::Control { .. }))
        {
            return Some("range_crosses_control".into());
        }
        let spans = pt.replaceable_spans(doc);
        if spans.text_spans.len() == 1
            && spans.text_spans[0]
                == (TextSpan {
                    start: 0,
                    end: pt.grapheme_len(),
                })
        {
            None
        } else {
            Some("unsupported_content_in_range".into())
        }
    }

    fn edits(&self, n: &Node, spans: &TextSpans) -> BTreeMap<String, EditAvailability> {
        let global = self
            .check_editable(n)
            .err()
            .map(|e| e.reason().unwrap_or("not_editable").to_string())
            .or_else(|| self.verification_block_reason(n));
        let mut edits = BTreeMap::new();
        let mut add = |name: &str, reason: Option<String>| {
            edits.insert(
                name.into(),
                EditAvailability::from_reason(global.clone().or(reason)),
            );
        };
        match n.kind {
            Kind::Paragraph => {
                add("text.replace", self.text_replace_reason(n, spans));
                add("paragraph.format", self.paragraph_format_block_reason(n));
                let e = edits.get_mut("text.replace").unwrap();
                if e.available
                    && !spans.text_spans.iter().any(|span| span.start < span.end)
                    && self.para_text(n).is_ok_and(|pt| {
                        !pt.text.is_empty() && pt.preserve_replaceable(self.doc_of(n))
                    })
                {
                    e.requires = Some(BTreeMap::from([(
                        "controls".into(),
                        serde_json::json!("preserve"),
                    )]));
                }
                e.modes = Some(if e.available {
                    let mut modes = Vec::new();
                    if spans.text_spans.iter().any(|s| s.start < s.end)
                        || self.para_text(n).is_ok_and(|p| {
                            !p.text.is_empty() && p.preserve_replaceable(self.doc_of(n))
                        })
                    {
                        modes.push("replace".into());
                    }
                    if !spans.text_spans.is_empty() {
                        modes.push("insert".into());
                    }
                    modes
                } else {
                    vec![]
                });
            }
            Kind::Table => {
                let reason = self.table_geometry_block_reason(n);
                add("table.setColumnWidths", reason.clone());
                add("table.setRowHeights", reason);
            }
            Kind::Cell => {
                let xml = self.doc_of(n);
                let owner = xml
                    .ancestors(n.el)
                    .find(|&i| is_hp(xml.el(i).ns.as_deref()) && xml.el(i).local == "tbl")
                    .and_then(|i| self.nodes.iter().find(|m| m.entry == n.entry && m.el == i));
                let geometry = owner.map_or(Some("table_geometry_unsupported".into()), |t| {
                    self.table_geometry_block_reason(t)
                });
                let merged = self
                    .cell_addr(n)
                    .is_some_and(|a| a.row_span != 1 || a.col_span != 1);
                let format = if child_attr(xml, n.el, "cellMargin", "left").is_none()
                    || xml.el(n.el).attr("borderFillIDRef").is_none()
                {
                    Some("table_geometry_unsupported".into())
                } else {
                    None
                };
                add("cell.setFormat", format);
                add(
                    "cell.setSize",
                    geometry.or_else(|| merged.then(|| "merged_cell_size_unsupported".into())),
                );
                add("cell.setText", self.cell_set_text_block_reason(n));
                let e = edits.get_mut("cell.setText").unwrap();
                if e.available
                    && self
                        .children(&n.id)
                        .filter(|p| p.kind == Kind::Paragraph)
                        .any(|p| {
                            self.para_text(p).is_ok_and(|pt| {
                                pt.pieces
                                    .iter()
                                    .any(|piece| matches!(piece.kind, PieceKind::Control { .. }))
                            })
                        })
                {
                    e.requires = Some(BTreeMap::from([(
                        "controls".into(),
                        serde_json::json!("preserve"),
                    )]));
                }
                if e.available {
                    let count = self
                        .children(&n.id)
                        .filter(|p| p.kind == Kind::Paragraph)
                        .count();
                    if count > 1 {
                        e.requires
                            .get_or_insert_with(BTreeMap::new)
                            .insert("paragraphs".into(), serde_json::json!(count));
                    }
                }
            }
            Kind::Equation => {
                add("equation.setScript", self.equation_script_block_reason(n));
                add("equation.setProperties", None);
            }
            Kind::Picture => add("image.replace", self.picture_block_reason(n)),
            _ => {}
        }
        edits
    }

    /// Why no command is available; command-specific reasons are in edits.
    pub fn edit_block_reason(&self, n: &Node) -> Option<String> {
        let spans = self.paragraph_text_spans(n);
        self.edits_block_reason(n, &self.edits(n, &spans))
    }

    fn edits_block_reason(
        &self,
        n: &Node,
        edits: &BTreeMap<String, EditAvailability>,
    ) -> Option<String> {
        if edits.values().any(|e| e.available) {
            return None;
        }
        if let Err(e) = self.check_editable(n) {
            return Some(e.reason().unwrap_or("not_editable").into());
        }
        edits.values().find_map(|e| e.reason.clone()).or_else(|| {
            Some(
                match n.kind {
                    Kind::Equation => "equation_edit_unsupported",
                    Kind::Opaque => "opaque",
                    _ => "no_edit_command",
                }
                .into(),
            )
        })
    }

    pub fn cell_block_reason(&self, n: &Node) -> Option<String> {
        let paras: Vec<&Node> = self.children_deep(n, Kind::Paragraph);
        let direct: Vec<&&Node> = paras
            .iter()
            .filter(|p| p.parent.as_deref() == Some(&n.id))
            .collect();
        if direct.is_empty() {
            return Some("cell_paragraph_count".into());
        }
        if self
            .nodes
            .iter()
            .any(|m| m.kind == Kind::Table && self.is_descendant(m, &n.id))
        {
            return Some("nested_table".into());
        }
        None
    }

    pub fn picture_block_reason(&self, n: &Node) -> Option<String> {
        let doc = self.doc_of(n);
        if self.picture_media(n).is_none() {
            return Some("no_embedded_media".into());
        }
        for d in doc.descendants(n.el) {
            let e = doc.el(d);
            match e.local.as_str() {
                "imgClip" => {
                    // Non-zero left/top or a clip smaller than the image means cropping.
                    let v = |k: &str| {
                        e.attr_value(k)
                            .and_then(|x| x.parse::<i64>().ok())
                            .unwrap_or(0)
                    };
                    if v("left") != 0 || v("top") != 0 {
                        return Some("cropped".into());
                    }
                    if let Some(dim) = doc
                        .el(n.el)
                        .children
                        .iter()
                        .map(|&c| doc.el(c))
                        .find(|c| c.local == "imgDim")
                    {
                        let w = dim
                            .attr_value("dimwidth")
                            .and_then(|x| x.parse::<i64>().ok());
                        let h = dim
                            .attr_value("dimheight")
                            .and_then(|x| x.parse::<i64>().ok());
                        if let (Some(w), Some(h)) = (w, h) {
                            if (v("right") != 0 && v("right") != w)
                                || (v("bottom") != 0 && v("bottom") != h)
                            {
                                return Some("cropped".into());
                            }
                        }
                    }
                }
                "effects" => {
                    // Native Hancom writes an empty effects container for a plain image.
                    if !e.children.is_empty()
                        || !e.attrs.is_empty()
                        || e.items
                            .iter()
                            .filter_map(|i| i.text())
                            .any(|t| !t.trim().is_empty())
                    {
                        return Some("effects".into());
                    }
                }
                "shadow" | "glow" | "softEdge" | "reflection" => return Some("effects".into()),
                _ => {}
            }
        }
        if let Some(img) = doc
            .descendants(n.el)
            .map(|d| doc.el(d))
            .find(|e| e.local == "img")
        {
            let effect = img.attr_value("effect").unwrap_or("REAL_PIC");
            if effect != "REAL_PIC" {
                return Some("effects".into());
            }
            for k in ["bright", "contrast"] {
                if img.attr_value(k).is_some_and(|v| v != "0") {
                    return Some("effects".into());
                }
            }
        }
        None
    }

    /// Discovery preconditions only; the request planner checks the full grid,
    /// resource selection, parameter bounds and effective margin minima.
    fn table_geometry_block_reason(&self, n: &Node) -> Option<String> {
        let doc = self.doc_of(n);
        let bad = || Some("table_geometry_unsupported".into());
        if doc.el(n.el).attr_value("cellSpacing") != Some("0") {
            return bad();
        }
        for a in ["rowCnt", "colCnt"] {
            if !doc
                .el(n.el)
                .attr_value(a)
                .and_then(|s| s.parse::<u32>().ok())
                .is_some_and(|v| (1..=1024).contains(&v))
            {
                return bad();
            }
        }
        for a in ["widthRelTo", "heightRelTo"] {
            if child_attr(doc, n.el, "sz", a).as_deref() != Some("ABSOLUTE") {
                return bad();
            }
        }
        if doc
            .descendants(n.el)
            .any(|i| is_hp(doc.el(i).ns.as_deref()) && doc.el(i).local == "tbl")
        {
            return Some("nested_table".into());
        }
        if self.children_deep(n, Kind::Cell).iter().any(|c| {
            self.cell_addr(c).is_none()
                || child_attr(doc, c.el, "cellSz", "width").is_none()
                || child_attr(doc, c.el, "cellSz", "height").is_none()
        }) {
            return bad();
        }
        None
    }

    fn children_deep(&self, n: &Node, kind: Kind) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|m| m.kind == kind && self.is_descendant(m, &n.id))
            .collect()
    }

    pub fn outline(&self, n: &Node) -> Outline {
        let doc = self.doc_of(n);
        let el = doc.el(n.el);
        let preview = match n.kind {
            Kind::Paragraph | Kind::Cell => {
                let t = self.node_text(n);
                Some(t.graphemes(true).take(PREVIEW_GRAPHEMES).collect())
            }
            _ => None,
        };
        let (rows, cols) = if n.kind == Kind::Table {
            (
                num(el.attr_value("rowCnt").map(str::to_string)),
                num(el.attr_value("colCnt").map(str::to_string)),
            )
        } else {
            (None, None)
        };
        let size = if n.kind == Kind::Cell {
            num(child_attr(doc, n.el, "cellSz", "width"))
                .zip(num(child_attr(doc, n.el, "cellSz", "height")))
                .map(|(w, h)| (i64::from(w), i64::from(h)))
        } else if n.kind == Kind::Picture || n.kind == Kind::Table {
            let w = child_attr(
                doc,
                n.el,
                if n.kind == Kind::Picture {
                    "curSz"
                } else {
                    "sz"
                },
                "width",
            );
            let h = child_attr(
                doc,
                n.el,
                if n.kind == Kind::Picture {
                    "curSz"
                } else {
                    "sz"
                },
                "height",
            );
            match (
                w.and_then(|v| v.parse().ok()),
                h.and_then(|v| v.parse().ok()),
            ) {
                (Some(w), Some(h)) => Some((w, h)),
                _ => None,
            }
        } else {
            None
        };
        let spans = self.paragraph_text_spans(n);
        let edits = self.edits(n, &spans);
        let block = self.edits_block_reason(n, &edits);
        Outline {
            node_id: n.id.clone(),
            kind: n.kind,
            parent: n.parent.clone(),
            section: n.section,
            text_preview: preview,
            rows,
            cols,
            cell: self.cell_addr(n),
            size,
            media: if n.kind == Kind::Picture {
                self.picture_media(n).map(|x| x.1)
            } else {
                None
            },
            editable: edits.values().any(|e| e.available),
            edits,
            text_spans: (n.kind == Kind::Paragraph).then_some(spans.text_spans),
            text_spans_truncated: (n.kind == Kind::Paragraph).then_some(spans.text_spans_truncated),
            child_paragraphs_editable: (n.kind == Kind::Cell).then(|| {
                self.children_deep(n, Kind::Paragraph).iter().any(|p| {
                    self.check_editable(p).is_ok()
                        && self
                            .text_replace_reason(p, &self.paragraph_text_spans(p))
                            .is_none()
                })
            }),
            not_editable_reason: block,
        }
    }

    /// Paged outline (cursor = index into node order).
    ///
    /// A cursor past the end is clamped instead of added to, so an
    /// out-of-range index cannot wrap the offset back to the start of the
    /// document and trap a client in a pagination loop.
    pub fn inspect(&self, cursor: usize, limit: usize) -> (Vec<Outline>, Option<usize>) {
        let start = cursor.min(self.nodes.len());
        let end = start.saturating_add(limit).min(self.nodes.len());
        let items = self.nodes[start..end]
            .iter()
            .map(|n| self.outline(n))
            .collect();
        (items, (end < self.nodes.len()).then_some(end))
    }

    pub fn query(&self, q: &Query) -> Result<QueryResult> {
        if let Some(a) = &q.ancestor {
            self.require(a, None)?;
        }
        if let Some(t) = &q.table {
            self.require(t, Some(Kind::Table))?;
            let (row, col) = match (q.row, q.col) {
                (Some(r), Some(c)) => (r, c),
                _ => {
                    return Err(EngineError::new(
                        Code::InvalidRequest,
                        Stage::Request,
                        "table lookup needs row and col",
                    ));
                }
            };
            let matches: Vec<Outline> = self
                .nodes
                .iter()
                .filter(|m| m.kind == Kind::Cell && self.is_descendant(m, t))
                .filter(|m| {
                    // Nearest table ancestor must be `t` (skip cells of nested tables).
                    let mut cur = m.parent.clone();
                    while let Some(p) = cur {
                        let pn = self.node(&p).unwrap();
                        if pn.kind == Kind::Table {
                            return p == *t;
                        }
                        cur = pn.parent.clone();
                    }
                    false
                })
                .filter(|m| {
                    self.cell_addr(m).is_some_and(|a| {
                        row >= a.row
                            && row < a.row + a.row_span
                            && col >= a.col
                            && col < a.col + a.col_span
                    })
                })
                .map(|m| self.outline(m))
                .collect();
            return Ok(QueryResult {
                matches,
                unaligned_matches: 0,
            });
        }
        let mut unaligned = 0;
        let mut matches = Vec::new();
        for n in &self.nodes {
            if q.kind.is_some_and(|k| k != n.kind) {
                continue;
            }
            if let Some(a) = &q.ancestor {
                if !self.is_descendant(n, a) {
                    continue;
                }
            }
            if let Some(needle) = &q.text_contains {
                if n.kind != Kind::Paragraph {
                    continue;
                }
                let pt = self.para_text(n)?;
                let (found, un) = pt.find_all(needle);
                unaligned += un;
                if found.is_empty() {
                    continue;
                }
            }
            matches.push(self.outline(n));
        }
        Ok(QueryResult {
            matches,
            unaligned_matches: unaligned,
        })
    }

    pub fn stats(&self, revision: u64, scope: Option<&str>) -> Result<DocumentStatsResult> {
        let scope_id = if let Some(target) = scope {
            let n = self.require(target, None)?;
            if !matches!(
                n.kind,
                Kind::Section | Kind::Table | Kind::Cell | Kind::Paragraph
            ) {
                return Err(EngineError::new(
                    Code::InvalidRequest,
                    Stage::Query,
                    format!(
                        "scope must be section, table, cell, or paragraph, found {:?}",
                        n.kind
                    ),
                ));
            }
            Some(target.to_string())
        } else {
            None
        };
        let in_scope = |m: &Node| {
            if let Some(target) = scope {
                m.id == target || self.is_descendant(m, target)
            } else {
                true
            }
        };

        let mut sections = 0;
        let mut top_level_paragraphs = 0;
        let mut paragraphs = 0;
        let mut tables = 0;
        let mut cells = 0;
        let mut pictures = 0;
        let mut equations = 0;
        let mut total_nodes = 0;
        let mut total_chars = 0;
        let mut scope_paras = Vec::new();

        for n in &self.nodes {
            if !in_scope(n) {
                continue;
            }
            total_nodes += 1;
            match n.kind {
                Kind::Section => sections += 1,
                Kind::Paragraph => {
                    paragraphs += 1;
                    if let Some(parent_id) = &n.parent {
                        if self
                            .node(parent_id)
                            .map(|p| p.kind == Kind::Section)
                            .unwrap_or(false)
                        {
                            top_level_paragraphs += 1;
                        }
                    }
                    if let Ok(pt) = self.para_text(n) {
                        total_chars += pt.grapheme_len();
                    }
                    scope_paras.push(n);
                }
                Kind::Table => tables += 1,
                Kind::Cell => cells += 1,
                Kind::Picture => pictures += 1,
                Kind::Equation => equations += 1,
                Kind::Opaque => {}
            }
        }

        let mut fields = 0;
        let mut footnotes = 0;
        let mut endnotes = 0;
        let mut memos = 0;

        for p in scope_paras {
            let doc = self.doc_of(p);
            scan_para_controls(
                doc,
                p.el,
                true,
                &mut fields,
                &mut footnotes,
                &mut endnotes,
                &mut memos,
            );
        }

        Ok(DocumentStatsResult {
            document_id: self.document_id.clone(),
            revision,
            scope: scope_id,
            sections,
            top_level_paragraphs,
            paragraphs,
            tables,
            cells,
            pictures,
            equations,
            fields,
            footnotes,
            endnotes,
            notes: footnotes + endnotes,
            memos,
            total_chars,
            total_nodes,
        })
    }

    pub fn section_list(
        &self,
        revision: u64,
        limit: Option<usize>,
        cursor: Option<&str>,
    ) -> Result<SectionListResult> {
        let mut sections = Vec::new();

        for (si, s_entry) in self.info.sections.iter().enumerate() {
            let doc = &self.xml[s_entry];
            let sec_node = self
                .nodes
                .iter()
                .find(|n| n.entry == *s_entry && n.kind == Kind::Section);
            let node_id = sec_node.map(|n| n.id.clone()).unwrap_or_default();

            let section_paras: Vec<&Node> = self
                .nodes
                .iter()
                .filter(|n| n.section == si && n.kind == Kind::Paragraph)
                .collect();
            let paragraphs = section_paras.len();
            let top_level_paragraphs = section_paras
                .iter()
                .filter(|n| {
                    n.parent
                        .as_ref()
                        .and_then(|p| self.node(p))
                        .map(|pn| pn.kind == Kind::Section)
                        .unwrap_or(false)
                })
                .count();

            let sec_pr_el = doc
                .iter()
                .find_map(|(i, el)| (is_hp(el.ns.as_deref()) && el.local == "secPr").then_some(i));

            let page = if let Some(spi) = sec_pr_el {
                let page_pr_el = doc
                    .el(spi)
                    .children
                    .iter()
                    .copied()
                    .find(|&c| is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "pagePr");

                if let Some(ppi) = page_pr_el {
                    let pel = doc.el(ppi);
                    let raw_orientation = pel.attr_value("landscape").map(str::to_string);
                    let orientation = match raw_orientation.as_deref() {
                        Some(s) if s.eq_ignore_ascii_case("WIDELY") => Some("portrait".to_string()),
                        Some(s) if s.eq_ignore_ascii_case("NARROWLY") => {
                            Some("landscape".to_string())
                        }
                        Some(s) if s.eq_ignore_ascii_case("PORTRAIT") => {
                            Some("portrait".to_string())
                        }
                        Some(s) if s.eq_ignore_ascii_case("LANDSCAPE") => {
                            Some("landscape".to_string())
                        }
                        _ => None,
                    };
                    let width = pel.attr_value("width").and_then(|v| v.parse().ok());
                    let height = pel.attr_value("height").and_then(|v| v.parse().ok());

                    let margin_el =
                        pel.children.iter().copied().find(|&c| {
                            is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "margin"
                        });
                    let margin = margin_el.map(|mi| {
                        let mel = doc.el(mi);
                        PageMargin {
                            left: mel.attr_value("left").and_then(|v| v.parse().ok()),
                            right: mel.attr_value("right").and_then(|v| v.parse().ok()),
                            top: mel.attr_value("top").and_then(|v| v.parse().ok()),
                            bottom: mel.attr_value("bottom").and_then(|v| v.parse().ok()),
                            header: mel.attr_value("header").and_then(|v| v.parse().ok()),
                            footer: mel.attr_value("footer").and_then(|v| v.parse().ok()),
                            gutter: mel.attr_value("gutter").and_then(|v| v.parse().ok()),
                        }
                    });

                    let start_num_el =
                        doc.el(spi).children.iter().copied().find(|&c| {
                            is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "startNum"
                        });
                    let page_starts_on = start_num_el
                        .and_then(|sni| doc.el(sni).attr_value("pageStartsOn").map(str::to_string));
                    let start_page_number = start_num_el.and_then(|sni| {
                        doc.el(sni).attr_value("page").and_then(|v| v.parse().ok())
                    });

                    PageSettings {
                        source: "explicit".to_string(),
                        orientation,
                        raw_orientation,
                        width,
                        height,
                        margin,
                        page_starts_on,
                        start_page_number,
                    }
                } else {
                    PageSettings {
                        source: "unspecified".to_string(),
                        orientation: None,
                        raw_orientation: None,
                        width: None,
                        height: None,
                        margin: None,
                        page_starts_on: None,
                        start_page_number: None,
                    }
                }
            } else {
                PageSettings {
                    source: "unspecified".to_string(),
                    orientation: None,
                    raw_orientation: None,
                    width: None,
                    height: None,
                    margin: None,
                    page_starts_on: None,
                    start_page_number: None,
                }
            };

            let col_pr_el = doc
                .iter()
                .find_map(|(i, el)| (is_hp(el.ns.as_deref()) && el.local == "colPr").then_some(i));

            let column = if let Some(cpi) = col_pr_el {
                let cel = doc.el(cpi);
                ColumnSettings {
                    source: "explicit".to_string(),
                    col_count: cel.attr_value("colCount").and_then(|v| v.parse().ok()),
                    col_type: cel.attr_value("type").map(str::to_string),
                    layout: cel.attr_value("layout").map(str::to_string),
                    same_size: cel.attr_value("sameSz").map(|v| v == "1"),
                    same_gap: cel.attr_value("sameGap").and_then(|v| v.parse().ok()),
                }
            } else {
                ColumnSettings {
                    source: "unspecified".to_string(),
                    col_count: None,
                    col_type: None,
                    layout: None,
                    same_size: None,
                    same_gap: None,
                }
            };

            sections.push(SectionInfo {
                section: si,
                node_id,
                paragraphs,
                top_level_paragraphs,
                page,
                column,
            });
        }

        let total = sections.len();
        let offset = if let Some(c) = cursor {
            let Some((rev_s, idx_s)) = c.split_once(':') else {
                return Err(EngineError::new(
                    Code::InvalidRequest,
                    Stage::Query,
                    "invalid section cursor format",
                ));
            };
            let cursor_rev: u64 = rev_s.parse().map_err(|_| {
                EngineError::new(Code::InvalidRequest, Stage::Query, "invalid cursor rev")
            })?;
            if cursor_rev != revision {
                return Err(EngineError::new(
                    Code::VersionConflict,
                    Stage::Query,
                    format!(
                        "section cursor belongs to revision {cursor_rev}, current is {revision}"
                    ),
                ));
            }
            idx_s.parse::<usize>().map_err(|_| {
                EngineError::new(Code::InvalidRequest, Stage::Query, "invalid cursor index")
            })?
        } else {
            0
        };

        let limit_val = limit.unwrap_or(100).min(1000);
        let start = offset.min(total);
        let end = start.saturating_add(limit_val).min(total);
        let page_items = sections[start..end].to_vec();
        let next_cursor = if end < total {
            Some(format!("{revision}:{end}"))
        } else {
            None
        };

        Ok(SectionListResult {
            document_id: self.document_id.clone(),
            revision,
            sections: page_items,
            next_cursor,
            total,
        })
    }

    pub fn note_list(
        &self,
        revision: u64,
        filter_kind: Option<&str>,
        limit: Option<usize>,
        cursor: Option<&str>,
    ) -> Result<NoteListResult> {
        let mut notes = Vec::new();

        for s_entry in &self.info.sections {
            let doc = &self.xml[s_entry];
            for (i, el) in doc.iter() {
                if !is_hp(el.ns.as_deref()) {
                    continue;
                }
                let kind_str = match el.local.as_str() {
                    "footNote" => "footnote",
                    "endNote" => "endnote",
                    _ => continue,
                };

                if let Some(fk) = filter_kind {
                    if !fk.eq_ignore_ascii_case(kind_str) {
                        continue;
                    }
                }

                let raw_number = el.attr_value("number").map(str::to_string);
                let number = raw_number.as_deref().and_then(|v| v.parse().ok());

                let reference_paragraph_node_id = doc
                    .ancestors(i)
                    .find(|&a| is_hp(doc.el(a).ns.as_deref()) && doc.el(a).local == "p")
                    .and_then(|pa| {
                        self.nodes
                            .iter()
                            .find(|n| n.entry == *s_entry && n.el == pa)
                    })
                    .map(|n| n.id.clone());

                let sub_list_el = el
                    .children
                    .iter()
                    .copied()
                    .find(|&c| is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "subList");

                let mut note_paras = Vec::new();
                if let Some(sli) = sub_list_el {
                    for &c in &doc.el(sli).children {
                        if is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "p" {
                            if let Some(pn) =
                                self.nodes.iter().find(|n| n.entry == *s_entry && n.el == c)
                            {
                                note_paras.push(pn);
                            }
                        }
                    }
                }

                let paragraph_node_id = note_paras
                    .first()
                    .map(|pn| pn.id.clone())
                    .unwrap_or_default();
                let text = if !note_paras.is_empty() {
                    note_paras
                        .iter()
                        .map(|pn| self.node_text(pn))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    doc.descendants(i)
                        .filter_map(|d| {
                            let del = doc.el(d);
                            (is_hp(del.ns.as_deref()) && del.local == "t")
                                .then(|| doc.direct_text(d))
                        })
                        .collect::<Vec<_>>()
                        .join("")
                };

                notes.push(NoteItem {
                    kind: kind_str.to_string(),
                    number,
                    raw_number,
                    paragraph_node_id,
                    text,
                    reference_paragraph_node_id,
                });
            }
        }

        let total = notes.len();
        let offset = if let Some(c) = cursor {
            let Some((rev_s, idx_s)) = c.split_once(':') else {
                return Err(EngineError::new(
                    Code::InvalidRequest,
                    Stage::Query,
                    "invalid note cursor format",
                ));
            };
            let cursor_rev: u64 = rev_s.parse().map_err(|_| {
                EngineError::new(Code::InvalidRequest, Stage::Query, "invalid cursor rev")
            })?;
            if cursor_rev != revision {
                return Err(EngineError::new(
                    Code::VersionConflict,
                    Stage::Query,
                    format!("note cursor belongs to revision {cursor_rev}, current is {revision}"),
                ));
            }
            idx_s.parse::<usize>().map_err(|_| {
                EngineError::new(Code::InvalidRequest, Stage::Query, "invalid cursor index")
            })?
        } else {
            0
        };

        let limit_val = limit.unwrap_or(100).min(1000);
        let start = offset.min(total);
        let end = start.saturating_add(limit_val).min(total);
        let page_items = notes[start..end].to_vec();
        let next_cursor = if end < total {
            Some(format!("{revision}:{end}"))
        } else {
            None
        };

        Ok(NoteListResult {
            document_id: self.document_id.clone(),
            revision,
            notes: page_items,
            next_cursor,
            total,
        })
    }
}
