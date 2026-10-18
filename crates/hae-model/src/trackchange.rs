//! `trackchange.list` view: revision marks and header track-change records.
//!
//! Documents carrying track changes stay locked read-only by
//! `Document::compute_permissions` (`readonlyReasons: ["track_changes"]`); this
//! module only reads them.
//!
//! Body marks follow KS X 6101 (`hp:insertBegin`/`hp:insertEnd`/
//! `hp:deleteBegin`/`hp:deleteEnd`, inline children of `hp:t`, attributes
//! `Id`/`TcId`/`paraend`). A body-level `hp:trackChange` is not part of the
//! published section schema, so it is accepted defensively and reported with
//! its element name rather than dropped. The author and kind of a mark come
//! from the header record whose `id` equals the mark's `TcId`
//! (`hh:trackChanges/hh:trackChange`, attributes `id`/`type`/`date`/`authorID`)
//! and from `hh:trackChangeAuthors/hh:trackChangeAuthor` (`id`/`name`/`mark`/
//! `color`).

use crate::{is_hp, Document};
use hae_diag::Result;
use hae_package::NS_HH;
use hae_xml::{Item, XmlDoc};
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::HashMap;

/// Inline mark element names read from section XML, in schema order.
const MARK_KINDS: [&str; 4] = ["insertBegin", "insertEnd", "deleteBegin", "deleteEnd"];

/// One `hh:trackChange` header record: kind, time, and author reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackChangeRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub change_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    /// `name` of the author record with `id == authorID`, when resolvable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_shape_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub para_shape_id: Option<String>,
}

/// One `hh:trackChangeAuthor` record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackChangeAuthorRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mark: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// One revision mark with its position and resolved header record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackChangeMark {
    /// Element local name: `insertBegin`, `insertEnd`, `deleteBegin`,
    /// `deleteEnd`, or the defensive `trackChange`.
    pub kind: String,
    /// Paragraph nodeId holding the mark, when the containing `hp:p` is an
    /// indexed node (paragraphs inside table cells included).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub section: usize,
    /// Package entry the mark lives in (usually `Contents/sectionN.xml`).
    pub entry: String,
    /// `Id` attribute of the mark.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `TcId` attribute: the `hh:trackChange` record this mark belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tc_id: Option<String>,
    /// `paraend` attribute: the mark covers the paragraph end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub para_end: Option<bool>,
    /// Codepoint offset of the mark inside its `hp:t` text; absent when the
    /// mark is not inside an `hp:t`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_offset: Option<usize>,
    #[serde(rename = "changeType", skip_serializing_if = "Option::is_none")]
    pub change_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// The full track-change view before pagination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackChangeReport {
    /// True when the document carries marks, header change records, or is
    /// locked read-only for track changes.
    pub has_track_changes: bool,
    /// The document's read-only lock reasons (`track_changes` when locked).
    pub readonly_reasons: Vec<String>,
    /// Marks in document order.
    pub marks: Vec<TrackChangeMark>,
    /// `hh:trackChange` records in header order.
    pub changes: Vec<TrackChangeRecord>,
    /// `hh:trackChangeAuthor` records in header order.
    pub authors: Vec<TrackChangeAuthorRecord>,
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

/// Attribute lookup that tolerates the two spellings seen in the wild
/// (`charShapeID` in KS X 6101, `charshapeID` in hwpxlib read/write paths).
fn attr_any(doc: &XmlDoc, i: usize, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| doc.el(i).attr_value(n))
        .map(str::to_string)
}

/// Codepoint offset of the mark inside the nearest enclosing `hp:t`.
fn text_offset(doc: &XmlDoc, mark: usize) -> Option<usize> {
    let t = doc.ancestors(mark).find(|&a| {
        let e = doc.el(a);
        is_hp(e.ns.as_deref()) && e.local == "t"
    })?;
    let t_end = doc.el(t).outer().end;
    let mark_start = doc.el(mark).start.start;
    let mut offset = 0usize;
    // Element indices are document order, so the whole `hp:t` subtree is a
    // contiguous range; only character data ending before the mark counts.
    for j in t..doc.elements().len() {
        if doc.el(j).start.start >= t_end {
            break;
        }
        for item in &doc.el(j).items {
            match item {
                Item::Text { raw, text } | Item::Ref { raw, text } | Item::CData { raw, text } => {
                    if raw.end <= mark_start {
                        offset += text.chars().count();
                    }
                }
                _ => {}
            }
        }
    }
    Some(offset)
}

impl Document {
    /// Marks and header records; `scope` restricts marks to those whose
    /// containing paragraph is the scope node or a descendant of it.
    pub fn track_change_report(&self, scope: Option<&str>) -> Result<TrackChangeReport> {
        let scope_node = scope.map(|id| self.require(id, None)).transpose()?;
        let (changes, authors) = self.track_change_header();
        let by_id: HashMap<&str, &TrackChangeRecord> = changes
            .iter()
            .filter_map(|c| c.id.as_deref().map(|id| (id, c)))
            .collect();
        let mut marks = Vec::new();
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
                if !MARK_KINDS.contains(&el.local.as_str()) && el.local != "trackChange" {
                    continue;
                }
                let paragraph = doc.ancestors(i).find(|&a| {
                    let e = doc.el(a);
                    is_hp(e.ns.as_deref()) && e.local == "p"
                });
                let node_id = paragraph.and_then(|p| node_ids.get(&p).map(|id| id.to_string()));
                if let Some(scope_node) = scope_node {
                    let Some(node_id) = &node_id else { continue };
                    let node = self.node(node_id).expect("collected nodeId resolves");
                    if node.id != scope_node.id && !self.is_descendant(node, &scope_node.id) {
                        continue;
                    }
                }
                let tc_id = attr_any(doc, i, &["TcId", "tcId"]);
                let change = tc_id.as_deref().and_then(|id| by_id.get(id).copied());
                marks.push(TrackChangeMark {
                    kind: el.local.clone(),
                    node_id,
                    section,
                    entry: entry.clone(),
                    id: attr_any(doc, i, &["Id", "id"]),
                    tc_id,
                    para_end: doc
                        .el(i)
                        .attr_value("paraend")
                        .or_else(|| doc.el(i).attr_value("paraEnd"))
                        .and_then(parse_bool),
                    text_offset: text_offset(doc, i),
                    change_type: change.and_then(|c| c.change_type.clone()),
                    author_id: change.and_then(|c| c.author_id.clone()),
                    author_name: change.and_then(|c| c.author_name.clone()),
                    date: change.and_then(|c| c.date.clone()),
                });
            }
        }
        let locked = self
            .permissions
            .readonly_reasons
            .iter()
            .any(|r| r == "track_changes");
        Ok(TrackChangeReport {
            has_track_changes: locked || !marks.is_empty() || !changes.is_empty(),
            readonly_reasons: self.permissions.readonly_reasons.clone(),
            marks,
            changes,
            authors,
        })
    }

    /// `hh:trackChanges`/`hh:trackChangeAuthors` records from the header.
    fn track_change_header(&self) -> (Vec<TrackChangeRecord>, Vec<TrackChangeAuthorRecord>) {
        let mut changes = Vec::new();
        let mut authors = Vec::new();
        let Some(header) = self.header() else {
            return (changes, authors);
        };
        for (i, el) in header.iter() {
            if el.is(NS_HH, "trackChangeAuthor") {
                authors.push(TrackChangeAuthorRecord {
                    id: attr_any(header, i, &["id"]),
                    name: attr_any(header, i, &["name"]),
                    mark: el.attr_value("mark").and_then(parse_bool),
                    color: attr_any(header, i, &["color"]),
                });
            } else if el.is(NS_HH, "trackChange") {
                changes.push(TrackChangeRecord {
                    id: attr_any(header, i, &["id"]),
                    change_type: attr_any(header, i, &["type"]),
                    date: attr_any(header, i, &["date"]),
                    author_id: attr_any(header, i, &["authorID", "authorId", "authorid"]),
                    author_name: None,
                    hide: el.attr_value("hide").and_then(parse_bool),
                    char_shape_id: attr_any(header, i, &["charShapeID", "charshapeID"]),
                    para_shape_id: attr_any(header, i, &["paraShapeID", "parashapeID"]),
                });
            }
        }
        let names: HashMap<&str, &str> = authors
            .iter()
            .filter_map(|a| a.id.as_deref().zip(a.name.as_deref()))
            .collect();
        for c in &mut changes {
            c.author_name = c
                .author_id
                .as_deref()
                .and_then(|id| names.get(id))
                .map(|name| (*name).to_string());
        }
        (changes, authors)
    }
}
