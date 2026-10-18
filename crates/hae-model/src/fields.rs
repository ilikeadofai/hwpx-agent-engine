//! Source-addressed fields. Markers remain opaque, paired within one paragraph flow.
use crate::{is_hp, Document, Kind};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_xml::{Item, XmlDoc};
use schemars::JsonSchema;
use serde::Serialize;
use std::ops::Range;

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo {
    pub node_id: String,
    pub r#type: String,
    pub name: String,
    pub fieldid: Option<String>,
    pub id: Option<String>,
    pub command: Option<String>,
    /// Original inner XML of the Command parameter (entities unnormalized).
    pub command_raw: Option<String>,
    pub text: String,
    pub empty: bool,
    pub multi_paragraph: bool,
    pub nested: bool,
    pub in_cell: bool,
    pub set_text_supported: bool,
    pub reason: Option<String>,
    pub paragraph_ids: Vec<String>,
    #[serde(skip)]
    pub begin: usize,
    #[serde(skip)]
    pub end: Option<usize>,
    #[serde(skip)]
    pub entry: String,
    #[serde(skip)]
    pub value: Range<usize>,
    #[serde(skip)]
    pub text_elements: Vec<usize>,
    #[serde(skip)]
    pub insertion: Option<(usize, usize)>,
}
fn hp(x: &XmlDoc, i: usize, name: &str) -> bool {
    is_hp(x.el(i).ns.as_deref()) && x.el(i).local == name
}
fn paragraph(x: &XmlDoc, i: usize) -> Option<usize> {
    x.ancestors(i).find(|&p| hp(x, p, "p"))
}
fn flow(x: &XmlDoc, i: usize) -> Option<usize> {
    paragraph(x, i).and_then(|p| x.el(p).parent)
}

impl Document {
    pub fn fields(&self, scope: Option<&str>) -> Result<Vec<FieldInfo>> {
        if let Some(id) = scope {
            let n = self.require(id, None)?;
            if !matches!(
                n.kind,
                Kind::Paragraph | Kind::Cell | Kind::Table | Kind::Section
            ) {
                return Err(EngineError::new(
                    Code::InvalidArgument,
                    Stage::Query,
                    "scope must be paragraph, cell, table or section",
                ));
            }
        }
        let mut out = vec![];
        for entry in &self.info.sections {
            let x = &self.xml[entry];
            let begins: Vec<_> = x
                .iter()
                .filter(|(i, _)| hp(x, *i, "fieldBegin"))
                .map(|(i, _)| i)
                .collect();
            let ends: Vec<_> = x
                .iter()
                .filter(|(i, _)| hp(x, *i, "fieldEnd"))
                .map(|(i, _)| i)
                .collect();
            for &begin in &begins {
                let Some(p) = paragraph(x, begin) else {
                    continue;
                };
                let Some(node) = self.nodes.iter().find(|n| n.entry == *entry && n.el == p) else {
                    continue;
                };
                let e = x.el(begin);
                let id = e.attr_value("id");
                let matching: Vec<_> = ends
                    .iter()
                    .copied()
                    .filter(|&i| {
                        id.is_some()
                            && x.el(i).attr_value("beginIDRef") == id
                            && flow(x, i) == flow(x, begin)
                    })
                    .collect();
                let end = matching.first().copied();
                let paired = matching.len() == 1
                    && end.is_some_and(|i| x.el(i).start.start >= e.outer().end)
                    && begins
                        .iter()
                        .filter(|&&i| {
                            flow(x, i) == flow(x, begin) && x.el(i).attr_value("id") == id
                        })
                        .count()
                        == 1;
                let value = e.outer().end
                    ..end
                        .filter(|_| paired)
                        .map_or(e.outer().end, |i| x.el(i).start.start);
                let mut paras: Vec<_> = self
                    .nodes
                    .iter()
                    .filter(|n| {
                        n.entry == *entry
                            && n.kind == Kind::Paragraph
                            && x.el(n.el).parent == flow(x, begin)
                            && x.el(n.el).outer().end > e.start.start
                            && x.el(n.el).start.start
                                <= end
                                    .filter(|_| paired)
                                    .map_or(e.start.start, |i| x.el(i).start.start)
                    })
                    .collect();
                paras.sort_by_key(|n| x.el(n.el).start.start);
                if let Some(s) = scope {
                    if !paras.iter().any(|n| n.id == s || self.is_descendant(n, s)) {
                        continue;
                    }
                }
                let multi = paras.len() > 1;
                let nested = paired
                    && begins.iter().copied().any(|b| {
                        b != begin && flow(x, b) == flow(x, begin) && {
                            let pos = x.el(b).start.start;
                            let within = value.contains(&pos);
                            let enclosing = pos < e.start.start
                                && ends.iter().any(|&en| {
                                    x.el(en).attr_value("beginIDRef") == x.el(b).attr_value("id")
                                        && flow(x, en) == flow(x, b)
                                        && x.el(en).start.start > e.start.start
                                });
                            within || enclosing
                        }
                    });
                let crossing = paired
                    && ends.iter().any(|&i| {
                        flow(x, i) == flow(x, begin) && value.contains(&x.el(i).start.start)
                    });
                let mut texts = vec![];
                let mut text = String::new();
                for (j, para) in paras.iter().enumerate() {
                    if j > 0 {
                        text.push('\n');
                    }
                    for (i, t) in x.iter().filter(|(i, t)| {
                        hp(x, *i, "t")
                            && paragraph(x, *i) == Some(para.el)
                            && value.start <= t.start.start
                            && t.outer().end <= value.end
                    }) {
                        texts.push(i);
                        for item in &t.items {
                            if let Some(v) = item.text() {
                                text.push_str(v)
                            } else if let Item::Element(c) = item {
                                match x.el(*c).local.as_str() {
                                    "tab" => text.push('\t'),
                                    "lineBreak" => text.push('\n'),
                                    _ => text.push('\u{fffc}'),
                                }
                            }
                        }
                    }
                }
                let lexical = x.iter().any(|(_, el)| {
                    el.items.iter().any(|item| {
                        item.raw()
                            .is_some_and(|r| r.start >= value.start && r.end <= value.end)
                            && matches!(item, Item::CData { .. } | Item::Comment(_) | Item::Pi(_))
                    })
                });
                let direct_payload = x.iter().any(|(i, el)| {
                    !hp(x, i, "t")
                        && el.items.iter().any(|v| {
                            v.raw()
                                .is_some_and(|r| r.start >= value.start && r.end <= value.end)
                                && v.text().is_some_and(|s| !s.trim().is_empty())
                        })
                });
                let opaque = x.iter().any(|(i, el)| {
                    value.start <= el.start.start
                        && el.outer().end <= value.end
                        && !(is_hp(el.ns.as_deref())
                            && matches!(el.local.as_str(), "p" | "run" | "t"))
                        && !hp(x, i, "linesegarray")
                });
                let bad_text = texts.iter().any(|&i| {
                    !crate::para::text_element_supported(x, i)
                        || x.el(i).parent.is_none_or(|r| !hp(x, r, "run"))
                        || x.el(i).items.iter().any(|v| {
                            !matches!(v, Item::Text { .. } | Item::Ref { .. })
                                || v.text().is_some_and(|s| s.contains(['\t', '\r', '\n']))
                        })
                });
                let insertion = x
                    .ancestors(begin)
                    .find(|&i| hp(x, i, "run"))
                    .and_then(|run| {
                        if x.el(run).parent != Some(p) {
                            return None;
                        }
                        let mut unit = begin;
                        while x.el(unit).parent != Some(run) {
                            unit = x.el(unit).parent?;
                        }
                        let at = x.el(unit).outer().end;
                        (at <= value.end && (unit == begin || hp(x, unit, "ctrl")))
                            .then_some((run, at))
                    });
                let reason = if !paired {
                    Some("unpaired_field")
                } else if multi {
                    Some("multi_paragraph_field")
                } else if nested || crossing {
                    Some("nested_field")
                } else if !matches!(e.attr_value("type"), Some("CLICKHERE" | "HYPERLINK")) {
                    Some("unsupported_field_type")
                } else if lexical {
                    Some("field_lexical_markup")
                } else if opaque || bad_text || direct_payload {
                    Some("field_contains_control_or_opaque")
                } else if texts.is_empty() && insertion.is_none() {
                    Some("no_text_fragment")
                } else {
                    None
                };
                let command = x.descendants(begin).find(|&i| {
                    hp(x, i, "stringParam") && x.el(i).attr_value("name") == Some("Command")
                });
                let reason = reason.map(str::to_string).or_else(|| {
                    self.check_editable(node)
                        .err()
                        .map(|_| "paragraph_not_editable".into())
                });
                out.push(FieldInfo {
                    node_id: node.id.clone(),
                    r#type: e.attr_value("type").unwrap_or("").into(),
                    name: e.attr_value("name").unwrap_or("").into(),
                    fieldid: e.attr_value("fieldid").map(str::to_string),
                    id: id.map(str::to_string),
                    command: command.map(|i| x.el(i).items.iter().filter_map(Item::text).collect()),
                    command_raw: command
                        .map(|i| String::from_utf8_lossy(x.raw(x.el(i).inner())).into_owned()),
                    empty: text.is_empty(),
                    text,
                    multi_paragraph: multi,
                    nested,
                    in_cell: x.ancestors(p).any(|i| hp(x, i, "tc")),
                    set_text_supported: reason.is_none(),
                    reason,
                    paragraph_ids: paras.iter().map(|p| p.id.clone()).collect(),
                    begin,
                    end,
                    entry: entry.clone(),
                    value,
                    text_elements: texts,
                    insertion,
                });
            }
        }
        Ok(out)
    }
    pub fn select_field(
        &self,
        target: &str,
        name: Option<&str>,
        fieldid: Option<&str>,
        occurrence: Option<usize>,
    ) -> Result<FieldInfo> {
        self.require(target, Some(Kind::Paragraph))?;
        let fields: Vec<_> = self
            .fields(Some(target))?
            .into_iter()
            .filter(|f| {
                f.node_id == target
                    && name.is_none_or(|n| f.name == n)
                    && fieldid.is_none_or(|id| {
                        f.fieldid.as_deref() == Some(id) || f.id.as_deref() == Some(id)
                    })
            })
            .collect();
        if fields.len() > 1 && occurrence.is_none() {
            return Err(EngineError::new(
                Code::AmbiguousTarget,
                Stage::Validate,
                "multiple matching fields; supply occurrence",
            )
            .with_target(target));
        }
        fields
            .into_iter()
            .nth(occurrence.unwrap_or(0))
            .ok_or_else(|| {
                EngineError::new(
                    Code::TargetNotFound,
                    Stage::Validate,
                    "field not found in target paragraph",
                )
                .with_target(target)
            })
    }
}
