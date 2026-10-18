//! `link.list` view: HYPERLINK fields in section XML.
//!
//! A hyperlink is stored as a field pair: `hp:fieldBegin type="HYPERLINK"`
//! (whose `hp:parameters` children carry `Command`/`Path`/`Category`/... as raw
//! text) and the matching `hp:fieldEnd beginIDRef=<begin id>`. This view reads
//! those markers and never changes the document.
//!
//! Reference classification follows the forms Hancom writes (checked against
//! `tests/fixtures/corpus/research-school-plan-reference.hwpx` and the public
//! `python-hwpx` reader): `Category=HWPHYPERLINK_TYPE_HWP` and `Command` heads
//! starting with `?` are in-document targets; `Category` `URL`/`EMAIL` and
//! addressed `Path`/`Command` values are external. Anything else stays
//! `unknown` instead of being guessed.

use crate::{is_hp, Document};
use hae_diag::Result;
use hae_xml::XmlDoc;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::HashMap;

/// One `hp:parameters` child (`stringParam`, `integerParam`, ...), raw text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkParameter {
    /// Element local name (`stringParam`, `integerParam`, `booleanParam`,
    /// `floatParam`, `listParam`).
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Character data of the parameter element, unmodified.
    pub value: String,
}

/// Derived link target classification plus the raw parameter values it uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkReference {
    /// `internal` (target inside this document), `external` (address outside
    /// the document), or `unknown` when the parameters do not identify one.
    pub scope: String,
    /// `Path` if non-empty, otherwise the `Command` head with `\` escapes
    /// resolved; bookmark targets keep Hancom's leading `?`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_open_type: Option<String>,
}

/// One HYPERLINK field, in document order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkInfo {
    /// Paragraph nodeId holding the field, when the containing `hp:p` is an
    /// indexed node (paragraphs inside table cells and headers included).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub section: usize,
    /// Package entry the marker lives in (usually `Contents/sectionN.xml`).
    pub entry: String,
    /// `type` attribute of the fieldBegin (`HYPERLINK` for this view).
    pub field_type: String,
    /// `fieldid` attribute (the control kind, shared by sibling links).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_id: Option<String>,
    /// `id` attribute of the fieldBegin; `fieldEnd beginIDRef` matches it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub begin_id: Option<String>,
    /// `name` attribute when non-empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `editable` attribute when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editable: Option<bool>,
    pub parameters: Vec<LinkParameter>,
    /// `hp:script` text when the field carries one, otherwise the character
    /// data between the fieldBegin and its matching fieldEnd.
    pub display_text: String,
    /// `script`, `between` (a matching fieldEnd was found), or `none`.
    pub display_text_source: String,
    pub reference: LinkReference,
}

fn attr_bool(doc: &XmlDoc, i: usize, name: &str) -> Option<bool> {
    match doc.el(i).attr_value(name) {
        Some("1" | "true") => Some(true),
        Some("0" | "false") => Some(false),
        _ => None,
    }
}

fn parameter_kind(local: &str) -> bool {
    matches!(
        local,
        "booleanParam" | "integerParam" | "floatParam" | "stringParam" | "listParam"
    )
}

/// The field's display text: `hp:script` content when present, else the text
/// inside `hp:t`/`hp:script` elements between the begin and its matching end.
///
/// Markers may sit in different runs (`fieldBegin`, text, `fieldEnd` as three
/// runs is what Hancom writes), so this walks document order instead of
/// assuming adjacency. A field whose markers are adjacent reports empty text
/// with source `between`; a begin without a matching end reports `none`.
fn field_display_text(doc: &XmlDoc, begin: usize) -> (String, &'static str) {
    if let Some(script) = doc
        .descendants(begin)
        .find(|&d| is_hp(doc.el(d).ns.as_deref()) && doc.el(d).local == "script")
    {
        return (doc.direct_text(script), "script");
    }
    let Some(id) = doc.el(begin).attr_value("id") else {
        return (String::new(), "none");
    };
    let begin_end = doc.el(begin).outer().end;
    let end = doc.iter().skip(begin + 1).find_map(|(j, e)| {
        (is_hp(e.ns.as_deref()) && e.local == "fieldEnd" && e.attr_value("beginIDRef") == Some(id))
            .then_some(j)
    });
    let Some(end) = end else {
        return (String::new(), "none");
    };
    let mut text = String::new();
    for j in begin + 1..end {
        let e = doc.el(j);
        if e.start.start < begin_end {
            // A descendant of the fieldBegin itself (`hp:parameters`).
            continue;
        }
        if is_hp(e.ns.as_deref()) && matches!(e.local.as_str(), "t" | "script") {
            text.push_str(&doc.direct_text(j));
        }
    }
    (text, "between")
}

/// Resolve `\x` escapes (`\:`, `\?`, `\;`, `\#`) that Hancom writes inside a
/// link `Command`; the kind tail (`;1;5;-1;`) is left untouched.
fn unescape_command(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn looks_like_address(value: &str) -> bool {
    let value = value.trim();
    if value.starts_with("//") || value.starts_with("\\\\") {
        return true;
    }
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn parameter<'a>(params: &'a [LinkParameter], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|p| p.name.as_deref() == Some(name))
        .map(|p| p.value.as_str())
}

fn classify(params: &[LinkParameter]) -> LinkReference {
    let raw = |name: &str| parameter(params, name).map(str::to_string);
    let command = raw("Command");
    let path = raw("Path");
    let category = raw("Category");
    let target_type = raw("TargetType");
    let doc_open_type = raw("DocOpenType");
    let head = command.as_deref().map(unescape_command);
    let non_empty = |value: &Option<String>| value.as_deref().is_some_and(|v| !v.is_empty());

    // In-document targets first: Hancom writes Category=HWPHYPERLINK_TYPE_HWP
    // and a `?<bookmark>;0;0;0;` Command with no Path.
    let internal = category.as_deref() == Some("HWPHYPERLINK_TYPE_HWP")
        || (non_empty(&path) && path.as_deref().is_some_and(|p| p.starts_with('#')))
        || (non_empty(&head) && head.as_deref().is_some_and(|h| h.starts_with('?')));
    let external = matches!(
        category.as_deref(),
        Some("HWPHYPERLINK_TYPE_URL" | "HWPHYPERLINK_TYPE_EMAIL")
    ) || (non_empty(&path) && path.as_deref().is_some_and(looks_like_address))
        || (non_empty(&head) && head.as_deref().is_some_and(looks_like_address));
    let target_named_internal = target_type
        .as_deref()
        .is_some_and(|t| t.contains("BOOKMARK") || t.contains("OUTLINE") || t.contains("TABLE"));
    let scope = if internal {
        "internal"
    } else if external {
        "external"
    } else if target_named_internal {
        "internal"
    } else if target_type
        .as_deref()
        .is_some_and(|t| t.contains("HYPERLINK"))
    {
        "external"
    } else {
        "unknown"
    };
    let target = path
        .clone()
        .filter(|p| !p.is_empty())
        .or(head.clone())
        .filter(|h| !h.is_empty());
    LinkReference {
        scope: scope.into(),
        target,
        path,
        command,
        category,
        target_type,
        doc_open_type,
    }
}

impl Document {
    /// Every HYPERLINK field in spine order, in document order per section.
    ///
    /// `scope` restricts the result to links whose containing paragraph is the
    /// scope node or a descendant of it.
    pub fn link_fields(&self, scope: Option<&str>) -> Result<Vec<LinkInfo>> {
        let scope_node = scope.map(|id| self.require(id, None)).transpose()?;
        let mut links = Vec::new();
        for (section, entry) in self.info.sections.iter().enumerate() {
            let doc = &self.xml[entry];
            let node_ids: HashMap<usize, &str> = self
                .nodes
                .iter()
                .filter(|n| &n.entry == entry)
                .map(|n| (n.el, n.id.as_str()))
                .collect();
            for (i, el) in doc.iter() {
                if !(is_hp(el.ns.as_deref()) && el.local == "fieldBegin") {
                    continue;
                }
                if el.attr_value("type") != Some("HYPERLINK") {
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
                let parameters: Vec<LinkParameter> = doc
                    .descendants(i)
                    .filter(|&d| parameter_kind(&doc.el(d).local))
                    .map(|d| LinkParameter {
                        kind: doc.el(d).local.clone(),
                        name: doc.el(d).attr_value("name").map(str::to_string),
                        value: doc.direct_text(d),
                    })
                    .collect();
                let (display_text, display_text_source) = field_display_text(doc, i);
                links.push(LinkInfo {
                    node_id,
                    section,
                    entry: entry.clone(),
                    field_type: el.attr_value("type").unwrap_or_default().to_string(),
                    field_id: el.attr_value("fieldid").map(str::to_string),
                    begin_id: el.attr_value("id").map(str::to_string),
                    name: el
                        .attr_value("name")
                        .filter(|n| !n.is_empty())
                        .map(str::to_string),
                    editable: attr_bool(doc, i, "editable"),
                    reference: classify(&parameters),
                    parameters,
                    display_text,
                    display_text_source: display_text_source.to_string(),
                });
            }
        }
        Ok(links)
    }
}
