//! Views of existing equation nodes; XML lexical source is never normalized.
use crate::{is_hp, Document, Kind, Node};
use hae_diag::Result;
use hae_xml::{Item, XmlDoc};
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquationInfo {
    pub node_id: String,
    pub paragraph_node_id: Option<String>,
    pub id: Option<String>,
    /// Decoded script characters; None when absent or containing opaque markup.
    pub script: Option<String>,
    /// Exact inner XML, including entity spellings (Some("") for empty script).
    pub script_raw: Option<String>,
    pub shape_comment: Option<String>,
    pub size: Option<BTreeMap<String, String>>,
    pub base_line: Option<String>,
    pub text_color: Option<String>,
    pub font: Option<String>,
    pub base_unit: Option<String>,
    pub line_mode: Option<String>,
    pub version: Option<String>,
    pub text_wrap: Option<String>,
    pub text_flow: Option<String>,
    pub treat_as_char: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_text_preview: Option<String>,
}

fn child(d: &XmlDoc, i: usize, name: &str) -> Option<usize> {
    let mut found = d.el(i).children.iter().copied().filter(|&c| {
        let e = d.el(c);
        is_hp(e.ns.as_deref()) && e.local == name
    });
    let first = found.next()?;
    if found.next().is_some() {
        None
    } else {
        Some(first)
    }
}
fn text(d: &XmlDoc, i: usize) -> Option<String> {
    let mut value = String::new();
    for item in &d.el(i).items {
        match item {
            Item::Text { text, .. } | Item::Ref { text, .. } | Item::CData { text, .. } => {
                value.push_str(text)
            }
            _ => return None,
        }
    }
    Some(value)
}
impl Document {
    pub fn equation_paragraph(&self, n: &Node) -> Option<&Node> {
        let mut parent = n.parent.as_deref();
        while let Some(id) = parent {
            let node = self.node(id)?;
            if node.kind == Kind::Paragraph {
                return Some(node);
            }
            parent = node.parent.as_deref();
        }
        None
    }
    pub fn equation_info(&self, target: &str, preview: bool) -> Result<EquationInfo> {
        let n = self.require(target, Some(Kind::Equation))?;
        let d = self.doc_of(n);
        let e = d.el(n.el);
        let p = self.equation_paragraph(n);
        let s = child(d, n.el, "script");
        let a = |name| e.attr_value(name).map(str::to_owned);
        Ok(EquationInfo {
            node_id: n.id.clone(),
            paragraph_node_id: p.map(|p| p.id.clone()),
            id: a("id"),
            script: s.and_then(|i| text(d, i)),
            script_raw: s.map(|i| String::from_utf8_lossy(d.raw(d.el(i).inner())).into_owned()),
            shape_comment: child(d, n.el, "shapeComment").and_then(|i| text(d, i)),
            size: child(d, n.el, "sz").map(|i| {
                d.el(i)
                    .attrs
                    .iter()
                    .map(|a| (a.qname.clone(), a.value.clone()))
                    .collect()
            }),
            base_line: a("baseLine"),
            text_color: a("textColor"),
            font: a("font"),
            base_unit: a("baseUnit"),
            line_mode: a("lineMode"),
            version: a("version"),
            text_wrap: a("textWrap"),
            text_flow: a("textFlow"),
            treat_as_char: child(d, n.el, "pos")
                .and_then(|i| d.el(i).attr_value("treatAsChar").map(str::to_owned)),
            paragraph_text_preview: if preview {
                p.map(|p| {
                    self.node_text(p)
                        .graphemes(true)
                        .take(crate::query::PREVIEW_GRAPHEMES)
                        .collect()
                })
            } else {
                None
            },
        })
    }
    pub fn equation_nodes(&self, scope: Option<&str>) -> Result<Vec<&Node>> {
        if let Some(s) = scope {
            self.require(s, None)?;
        }
        Ok(self
            .nodes
            .iter()
            .filter(|n| {
                n.kind == Kind::Equation
                    && scope.is_none_or(|s| n.id == s || self.is_descendant(n, s))
            })
            .collect())
    }
    /// Admission is lexical, not a renderer or a complete Hancom script parser.
    pub fn equation_script_block_reason(&self, n: &Node) -> Option<String> {
        let d = self.doc_of(n);
        let Some(s) = child(d, n.el, "script") else {
            return Some("equation_script_shape".into());
        };
        if !d.el(s).attrs.is_empty()
            || d.el(s)
                .items
                .iter()
                .any(|i| !matches!(i, Item::Text { .. } | Item::Ref { .. }))
        {
            return Some("equation_script_markup".into());
        }
        None
    }
}
