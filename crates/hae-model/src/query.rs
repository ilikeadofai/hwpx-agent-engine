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
    pub requires: Option<BTreeMap<String, String>>,
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
                if e.available && spans.text_spans.is_empty() {
                    e.requires = Some(BTreeMap::from([("controls".into(), "preserve".into())]));
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
            Kind::Cell => {
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
                    e.requires = Some(BTreeMap::from([("controls".into(), "preserve".into())]));
                }
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
        let size = if n.kind == Kind::Picture || n.kind == Kind::Table {
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
    pub fn inspect(&self, cursor: usize, limit: usize) -> (Vec<Outline>, Option<usize>) {
        let end = (cursor + limit).min(self.nodes.len());
        let items = self.nodes[cursor.min(end)..end]
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
}
