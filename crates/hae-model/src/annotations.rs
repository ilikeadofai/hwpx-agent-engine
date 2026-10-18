//! `bookmark.list` and `memo.list`: read-only annotation views over section XML.
//!
//! Both views only read the document. They report what the markup states and
//! leave everything else `null` instead of guessing: an unnamed bookmark stays
//! unnamed, an unstated memo flag stays unstated, and raw
//! `hp:parameters` values are passed through undecoded.
//!
//! # Bookmarks
//!
//! Sections carry bookmarks in two shapes and both are listed:
//!
//! * a point marker, an `hp:bookmark@name` element (Hancom writes it as the
//!   only child of `hp:ctrl`),
//! * a span pair, `hp:fieldBegin type="BOOKMARK"` … `hp:fieldEnd
//!   beginIDRef=<begin id>`, the shape Hancom writes for a bookmark covering a
//!   range (hwpforge's `BookmarkType::SpanStart`/`SpanEnd`).
//!
//! Hancom writes nameless markers too, so `name` stays `null` rather than being
//! filled from a parameter or nearby text. Duplicate names are reported as many
//! times as they occur.
//!
//! # Memos
//!
//! Real Hancom files (checked against the local `04_complex.hwpx` input) keep a
//! memo in its `hp:fieldBegin type="MEMO"` control: `hp:parameters` carry
//! `ID`/`Number`/`Author`/`CreateDateTime`/`MemoShapeIDRef`/`Command`, and the
//! memo body is the control's own `hp:subList`. `python-hwpx` additionally
//! writes and reads a grouped `hp:memo` element with `id`/`memoShapeIDRef`
//! attributes (anchored by the MEMO field whose `ID` equals the memo id), so
//! that shape is read as well. `Contents/memoExtended.xml`
//! (`he:memosEx/he:memo@id,parentId`) records replies; a reply is reported as
//! the anchoring field id in `parentBeginId`.
//!
//! `hidden` is only reported when the marker states it (a boolean `hidden`
//! attribute); no published HWPX memo attribute set is known to carry a hidden
//! flag, so the field is `null` for ordinary documents instead of being assumed
//! false. `hh:refList/hh:memoProperties/hh:memoPr` shapes are resolved by exact
//! `id` match against the memo's `MemoShapeIDRef`; an unmatched reference stays
//! unresolved and keeps the raw value.

use crate::{is_hp, Document, ParaText};
use hae_diag::Result;
use hae_package::NS_HH;
use hae_xml::XmlDoc;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

/// Grapheme count kept from each memo paragraph as `preview` (the full text is
/// in `text`).
pub const MEMO_PREVIEW_GRAPHEMES: usize = 120;

/// One `hp:parameters` child (`stringParam`, `integerParam`, ...), raw text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RawParameter {
    /// Element local name (`stringParam`, `integerParam`, `booleanParam`,
    /// `floatParam`, `listParam`).
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Character data of the parameter element, unmodified.
    pub value: String,
}

/// One bookmark marker, in document order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkInfo {
    /// Paragraph nodeId holding the marker, when the containing `hp:p` is an
    /// indexed node (paragraphs inside table cells included).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub section: usize,
    /// Package entry the marker lives in (usually `Contents/sectionN.xml`).
    pub entry: String,
    /// `bookmark` for an `hp:bookmark` element (Hancom writes it as the only
    /// child of `hp:ctrl`), `field` for `hp:fieldBegin type="BOOKMARK"`.
    pub marker: String,
    /// Non-empty `name` attribute of the marker; `null` when the marker carries
    /// no name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `id` attribute of the marker (`hp:bookmark@id` or `fieldBegin@id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `fieldid` attribute; field form only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_id: Option<String>,
    /// Grapheme index of the marker inside its paragraph text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grapheme_offset: Option<usize>,
    /// Paragraph holding the `hp:fieldEnd` whose `beginIDRef` names this
    /// marker; field form only, absent when no matching end exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_node_id: Option<String>,
    /// Grapheme index of the matching `hp:fieldEnd` inside its paragraph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_grapheme_offset: Option<usize>,
    /// Raw `hp:parameters` children; field form only, empty otherwise.
    pub parameters: Vec<RawParameter>,
}

/// One memo body paragraph: nodeId, full text, and a bounded preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoParagraph {
    /// nodeId of the memo body paragraph, when it is an indexed node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Full text of the paragraph.
    pub text: String,
    /// First [`MEMO_PREVIEW_GRAPHEMES`] graphemes of `text`.
    pub preview: String,
    pub preview_truncated: bool,
    pub grapheme_count: usize,
}

/// One `hh:memoPr` shape definition, resolved by `MemoShapeIDRef`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoShape {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_width: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_color: Option<String>,
    /// Stored spelling, e.g. `NOMAL` in real Hancom files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo_type: Option<String>,
}

/// One memo, in document order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoInfo {
    /// Memo id: the `ID` parameter of a MEMO field, or `hp:memo@id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `id` attribute of the anchoring `hp:fieldBegin`; field form only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub begin_id: Option<String>,
    /// `fieldid` attribute of the anchoring `hp:fieldBegin`; field form only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_id: Option<String>,
    /// `Number` parameter as an integer; `null` when absent or unparsable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    /// `Number` parameter, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_number: Option<String>,
    /// `Author` parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// `CreateDateTime` parameter, verbatim (no reformatting).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// `MemoShapeIDRef` parameter or `hp:memo@memoShapeIDRef`, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo_shape_id_ref: Option<String>,
    /// `hh:memoPr` with the same `id`, when the header defines it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<MemoShape>,
    /// Boolean `hidden` attribute of the marker; `null` when the marker does
    /// not state one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// `field` for `hp:fieldBegin type="MEMO"`, `memo` for an `hp:memo`
    /// element.
    pub source: String,
    /// Paragraph the memo hangs off (the paragraph holding its anchor control).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_node_id: Option<String>,
    /// Grapheme index of the anchoring control inside `paragraphNodeId`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grapheme_offset: Option<usize>,
    /// `id` from `Contents/memoExtended.xml` (`he:memosEx`) naming this memo's
    /// parent in a reply thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_begin_id: Option<String>,
    pub paragraph_count: usize,
    pub paragraphs: Vec<MemoParagraph>,
    /// Body paragraphs joined by `\n`.
    pub text: String,
    pub section: usize,
    pub entry: String,
    /// Raw `hp:parameters` children; field form only, empty otherwise.
    pub parameters: Vec<RawParameter>,
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

fn parameter_kind(local: &str) -> bool {
    matches!(
        local,
        "booleanParam" | "integerParam" | "floatParam" | "stringParam" | "listParam"
    )
}

fn raw_parameters(doc: &XmlDoc, i: usize) -> Vec<RawParameter> {
    doc.descendants(i)
        .filter(|&d| parameter_kind(&doc.el(d).local))
        .map(|d| RawParameter {
            kind: doc.el(d).local.clone(),
            name: doc.el(d).attr_value("name").map(str::to_string),
            value: doc.direct_text(d),
        })
        .collect()
}

fn parameter<'a>(params: &'a [RawParameter], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|p| p.name.as_deref() == Some(name))
        .map(|p| p.value.as_str())
}

/// Paragraph text and a bounded preview, counted in graphemes.
fn memo_paragraph(doc: &XmlDoc, paragraph: usize, node_id: Option<String>) -> MemoParagraph {
    let text = ParaText::build(doc, paragraph);
    let preview: String = text
        .text
        .graphemes(true)
        .take(MEMO_PREVIEW_GRAPHEMES)
        .collect();
    MemoParagraph {
        node_id,
        preview_truncated: preview != text.text,
        preview,
        text: text.text.clone(),
        grapheme_count: text.grapheme_len(),
    }
}

/// Direct `hp:p` children of `parent`, optionally under a named wrapper first.
fn child_paragraphs(doc: &XmlDoc, parent: usize, wrapper: &str) -> Vec<usize> {
    let holder = doc
        .el(parent)
        .children
        .iter()
        .copied()
        .find(|&c| is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == wrapper)
        .unwrap_or(parent);
    doc.el(holder)
        .children
        .iter()
        .copied()
        .filter(|&c| is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "p")
        .collect()
}

/// The paragraph that directly hosts `el`, and its nodeId when indexed.
fn host_paragraph(
    doc: &XmlDoc,
    el: usize,
    node_ids: &HashMap<usize, &str>,
) -> Option<(usize, Option<String>)> {
    let paragraph = doc
        .ancestors(el)
        .find(|&a| is_hp(doc.el(a).ns.as_deref()) && doc.el(a).local == "p")?;
    Some((
        paragraph,
        node_ids.get(&paragraph).map(|id| (*id).to_string()),
    ))
}

/// `he:memosEx/he:memo@id → parentId` reply links, when the package carries
/// them. The entry is found by root element name, not by a fixed path.
fn reply_parents(doc: &Document) -> HashMap<String, String> {
    let mut parents = HashMap::new();
    for entry in doc.xml.values() {
        let root = entry.root();
        if entry.el(root).local != "memosEx" {
            continue;
        }
        for (_, el) in entry.iter() {
            if el.local != "memo" {
                continue;
            }
            if let (Some(id), Some(parent)) = (el.attr_value("id"), el.attr_value("parentId")) {
                parents.insert(id.to_string(), parent.to_string());
            }
        }
    }
    parents
}

impl Document {
    /// `hh:memoPr` definitions by `id`, in header order.
    fn memo_shapes(&self) -> HashMap<String, MemoShape> {
        let mut shapes = HashMap::new();
        let Some(header) = self.header() else {
            return shapes;
        };
        for (i, el) in header.iter() {
            if !el.is(NS_HH, "memoPr") {
                continue;
            }
            let attr = |name: &str| header.el(i).attr_value(name).map(str::to_string);
            let shape = MemoShape {
                id: attr("id"),
                width: attr("width"),
                line_width: attr("lineWidth"),
                line_type: attr("lineType"),
                line_color: attr("lineColor"),
                fill_color: attr("fillColor"),
                active_color: attr("activeColor"),
                memo_type: attr("memoType"),
            };
            if let Some(id) = shape.id.clone() {
                shapes.insert(id, shape);
            }
        }
        shapes
    }

    /// Every bookmark marker in spine order, in document order per section.
    ///
    /// `scope` restricts the result to markers whose containing paragraph is
    /// the scope node or a descendant of it. Nothing is dropped or merged:
    /// duplicate names are reported once per marker.
    pub fn bookmark_list(&self, scope: Option<&str>) -> Result<Vec<BookmarkInfo>> {
        let scope_node = scope.map(|id| self.require(id, None)).transpose()?;
        let mut bookmarks = Vec::new();
        for (section, entry) in self.info.sections.iter().enumerate() {
            let doc = &self.xml[entry];
            let node_ids: HashMap<usize, &str> = self
                .nodes
                .iter()
                .filter(|n| &n.entry == entry)
                .map(|n| (n.el, n.id.as_str()))
                .collect();
            for (i, el) in doc.iter() {
                if !is_hp(el.ns.as_deref()) {
                    continue;
                }
                let marker = match el.local.as_str() {
                    "bookmark" => "bookmark",
                    "fieldBegin" if el.attr_value("type") == Some("BOOKMARK") => "field",
                    _ => continue,
                };
                let Some((paragraph, node_id)) = host_paragraph(doc, i, &node_ids) else {
                    continue;
                };
                if let Some(scope_node) = scope_node {
                    let Some(node_id) = &node_id else { continue };
                    let node = self.node(node_id).expect("collected nodeId resolves");
                    if node.id != scope_node.id && !self.is_descendant(node, &scope_node.id) {
                        continue;
                    }
                }
                let text = ParaText::build(doc, paragraph);
                let (end_node_id, end_grapheme_offset) = if marker == "field" {
                    match_field_end(doc, i, &node_ids)
                } else {
                    (None, None)
                };
                bookmarks.push(BookmarkInfo {
                    node_id,
                    section,
                    entry: entry.clone(),
                    marker: marker.to_string(),
                    name: el
                        .attr_value("name")
                        .filter(|n| !n.is_empty())
                        .map(str::to_string),
                    id: el.attr_value("id").map(str::to_string),
                    field_id: el.attr_value("fieldid").map(str::to_string),
                    grapheme_offset: Some(text.grapheme_offset_of(doc, i)),
                    end_node_id,
                    end_grapheme_offset,
                    parameters: if marker == "field" {
                        raw_parameters(doc, i)
                    } else {
                        Vec::new()
                    },
                });
            }
        }
        Ok(bookmarks)
    }

    /// Every memo in spine order, in document order per section.
    ///
    /// `scope` restricts the result to memos anchored in the scope node or a
    /// descendant of it.
    pub fn memo_list(&self, scope: Option<&str>) -> Result<Vec<MemoInfo>> {
        let scope_node = scope.map(|id| self.require(id, None)).transpose()?;
        let shapes = self.memo_shapes();
        let parents = reply_parents(self);
        let mut memos = Vec::new();
        for (section, entry) in self.info.sections.iter().enumerate() {
            let doc = &self.xml[entry];
            let node_ids: HashMap<usize, &str> = self
                .nodes
                .iter()
                .filter(|n| &n.entry == entry)
                .map(|n| (n.el, n.id.as_str()))
                .collect();
            for (i, el) in doc.iter() {
                if !is_hp(el.ns.as_deref()) {
                    continue;
                }
                let is_field = el.local == "fieldBegin" && el.attr_value("type") == Some("MEMO");
                let is_memo_element = el.local == "memo";
                if !is_field && !is_memo_element {
                    continue;
                }
                let parameters = raw_parameters(doc, i);
                // Field memos are anchored by their own control; a grouped
                // `hp:memo` is anchored by the MEMO field naming its id.
                let (anchor, paragraphs, id, memo_shape_id_ref) = if is_field {
                    (
                        Some(i),
                        child_paragraphs(doc, i, "subList"),
                        parameter(&parameters, "ID").map(str::to_string),
                        parameter(&parameters, "MemoShapeIDRef").map(str::to_string),
                    )
                } else {
                    let id = el.attr_value("id").map(str::to_string);
                    let anchor = id
                        .as_deref()
                        .and_then(|id| find_memo_field(doc, id))
                        .or(Some(i));
                    (
                        anchor,
                        child_paragraphs(doc, i, "paraList"),
                        id,
                        el.attr_value("memoShapeIDRef").map(str::to_string),
                    )
                };
                // `Contents/memoExtended.xml` names the anchoring MEMO field,
                // so a grouped memo is looked up through its anchor's id.
                let parent_key = anchor.and_then(|anchor| doc.el(anchor).attr_value("id"));
                let (paragraph_node_id, grapheme_offset) = match anchor {
                    Some(anchor) => match host_paragraph(doc, anchor, &node_ids) {
                        Some((paragraph, node_id)) => {
                            let text = ParaText::build(doc, paragraph);
                            (node_id, Some(text.grapheme_offset_of(doc, anchor)))
                        }
                        None => (None, None),
                    },
                    None => (None, None),
                };
                if let Some(scope_node) = scope_node {
                    let Some(node_id) = &paragraph_node_id else {
                        continue;
                    };
                    let node = self.node(node_id).expect("collected nodeId resolves");
                    if node.id != scope_node.id && !self.is_descendant(node, &scope_node.id) {
                        continue;
                    }
                }
                let items: Vec<MemoParagraph> = paragraphs
                    .iter()
                    .map(|&p| {
                        let node_id = node_ids.get(&p).map(|id| (*id).to_string());
                        memo_paragraph(doc, p, node_id)
                    })
                    .collect();
                let text = items
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let raw_number = parameter(&parameters, "Number").map(str::to_string);
                memos.push(MemoInfo {
                    id,
                    begin_id: if is_field {
                        el.attr_value("id").map(str::to_string)
                    } else {
                        None
                    },
                    field_id: if is_field {
                        el.attr_value("fieldid").map(str::to_string)
                    } else {
                        None
                    },
                    number: raw_number.as_deref().and_then(|n| n.parse().ok()),
                    raw_number,
                    author: parameter(&parameters, "Author").map(str::to_string),
                    created_at: parameter(&parameters, "CreateDateTime").map(str::to_string),
                    shape: memo_shape_id_ref
                        .as_deref()
                        .and_then(|id| shapes.get(id))
                        .cloned(),
                    memo_shape_id_ref,
                    hidden: el.attr_value("hidden").and_then(parse_bool),
                    source: if is_field { "field" } else { "memo" }.to_string(),
                    parent_begin_id: parent_key.and_then(|id| parents.get(id)).cloned(),
                    paragraph_node_id,
                    grapheme_offset,
                    paragraph_count: items.len(),
                    paragraphs: items,
                    text,
                    section,
                    entry: entry.clone(),
                    parameters: if is_field {
                        raw_parameters(doc, i)
                    } else {
                        Vec::new()
                    },
                });
            }
        }
        Ok(memos)
    }
}

/// A field-form bookmark's matching `hp:fieldEnd` and its position.
fn match_field_end(
    doc: &XmlDoc,
    begin: usize,
    node_ids: &HashMap<usize, &str>,
) -> (Option<String>, Option<usize>) {
    let Some(id) = doc.el(begin).attr_value("id") else {
        return (None, None);
    };
    let end = doc.iter().skip(begin + 1).find_map(|(j, e)| {
        (is_hp(e.ns.as_deref()) && e.local == "fieldEnd" && e.attr_value("beginIDRef") == Some(id))
            .then_some(j)
    });
    let Some((paragraph, node_id)) = end.and_then(|end| host_paragraph(doc, end, node_ids)) else {
        return (None, None);
    };
    let offset = ParaText::build(doc, paragraph);
    let end = end.expect("the end element was just found");
    (node_id, Some(offset.grapheme_offset_of(doc, end)))
}

/// The `hp:fieldBegin type="MEMO"` whose `ID` parameter equals `id`.
fn find_memo_field(doc: &XmlDoc, id: &str) -> Option<usize> {
    doc.iter()
        .filter(|(_, e)| is_hp(e.ns.as_deref()) && e.local == "fieldBegin")
        .map(|(i, _)| i)
        .find(|&i| {
            doc.el(i).attr_value("type") == Some("MEMO")
                && doc.descendants(i).any(|d| {
                    doc.el(d).local.ends_with("Param")
                        && doc.el(d).attr_value("name") == Some("ID")
                        && doc.direct_text(d) == id
                })
        })
}
