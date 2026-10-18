//! Existing equations only. Splice script character data or requested attributes.
use crate::{Draft, Intent};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{is_hp, Kind};
use hae_xml::{escape_attr, escape_text, is_xml_char};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EquationProps {
    pub base_line: Option<u32>,
    pub text_color: Option<String>,
    pub font: Option<String>,
    pub base_unit: Option<u32>,
    pub line_mode: Option<String>,
    pub version: Option<String>,
}
impl EquationProps {
    pub fn validate(&self) -> Result<()> {
        if self == &Self::default()
            || self.base_unit == Some(0)
            || self
                .line_mode
                .as_deref()
                .is_some_and(|s| !matches!(s, "CHAR" | "LINE"))
            || self.text_color.as_ref().is_some_and(|c| {
                c.len() != 7
                    || !c.starts_with('#')
                    || !c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
            })
            || [&self.font, &self.version, &self.line_mode]
                .into_iter()
                .flatten()
                .any(|s| {
                    s.trim().is_empty() || s.chars().any(|c| !is_xml_char(c) || c.is_control())
                })
        {
            return Err(EngineError::new(Code::InvalidRequest, Stage::Validate, "equation props requires nonempty properties, positive baseUnit, lineMode CHAR/LINE, #RRGGBB color and nonempty XML strings without controls"));
        }
        Ok(())
    }
    fn values(&self) -> Vec<(&str, String)> {
        [
            ("baseLine", self.base_line.map(|v| v.to_string())),
            ("textColor", self.text_color.clone()),
            ("font", self.font.clone()),
            ("baseUnit", self.base_unit.map(|v| v.to_string())),
            ("lineMode", self.line_mode.clone()),
            ("version", self.version.clone()),
        ]
        .into_iter()
        .filter_map(|(n, v)| v.map(|s| (n, s)))
        .collect()
    }
}
fn invalid_script(message: &str) -> EngineError {
    EngineError::new(Code::Unsupported, Stage::Validate, message)
        .with_reason("equation_script_notation")
}
fn validate_script(s: &str) -> Result<()> {
    if s.trim().is_empty() || s.chars().any(|c| !is_xml_char(c)) {
        return Err(EngineError::new(
            Code::InvalidRequest,
            Stage::Validate,
            "script must be nonempty XML character data",
        ));
    }
    // No translation of LaTeX/MathML or unverified equation escape syntax.
    if s.contains('\\')
        || s.contains('"')
        || s.contains("<!")
        || s.contains("<?")
        || s.chars().any(char::is_control)
    {
        return Err(invalid_script(
            "equation backslash/quoted escapes, markup and control notation are not verified",
        ));
    }
    let mut depth = 0usize;
    for c in s.chars() {
        if c == '{' {
            depth += 1;
        }
        if c == '}' {
            depth = depth
                .checked_sub(1)
                .ok_or_else(|| invalid_script("unbalanced script groups"))?;
        }
    }
    if depth != 0 {
        return Err(invalid_script("unbalanced script groups"));
    }
    Ok(())
}

pub(crate) fn set(
    d: &mut Draft,
    target: &str,
    script: Option<&str>,
    props: Option<&EquationProps>,
) -> Result<()> {
    if let Some(s) = script {
        validate_script(s)?;
    }
    if let Some(p) = props {
        p.validate()?;
    }
    let n = d.node(target, Some(Kind::Equation))?;
    let para = d.doc.equation_paragraph(&n).cloned().ok_or_else(|| {
        EngineError::new(
            Code::Unsupported,
            Stage::Validate,
            "equation has no containing paragraph",
        )
    })?;
    let x = d.doc.doc_of(&n).clone();
    let e = x.el(n.el);
    let mut cuts = Vec::new();
    if let Some(s) = script {
        if let Some(reason) = d.doc.equation_script_block_reason(&n) {
            return Err(EngineError::new(
                Code::Unsupported,
                Stage::Validate,
                "script must be one existing plain-text element",
            )
            .with_reason(reason));
        }
        let i = *e
            .children
            .iter()
            .find(|&&i| is_hp(x.el(i).ns.as_deref()) && x.el(i).local == "script")
            .unwrap();
        let se = x.el(i);
        let old: String = se.items.iter().filter_map(|i| i.text()).collect();
        if old == s {
            return Ok(());
        }
        if se.end.is_some() {
            cuts.push((se.inner(), escape_text(s).into_bytes()));
        } else {
            // Expand the existing empty script; no model/identity node is added.
            let open = std::str::from_utf8(x.raw(se.start.clone())).unwrap();
            let end = open.rfind('/').unwrap();
            cuts.push((
                se.outer(),
                format!(
                    "{}>{}</{}>",
                    open[..end].trim_end(),
                    escape_text(s),
                    se.qname
                )
                .into_bytes(),
            ));
        }
    }
    if let Some(p) = props {
        let mut extra = String::new();
        for (name, v) in p.values() {
            if let Some(a) = e.attr(name) {
                if a.value != v {
                    cuts.push((a.value_span.clone(), escape_attr(&v).into_bytes()));
                }
            } else {
                extra.push_str(&format!(" {name}=\"{}\"", escape_attr(&v)));
            }
        }
        if !extra.is_empty() {
            let at = if e.end.is_some() {
                e.start.end - 1
            } else {
                e.start.end - 2
            };
            cuts.push((at..at, extra.into_bytes()));
        }
    }
    if cuts.is_empty() {
        return Ok(());
    }
    let path = x.path(n.el);
    d.splice_entry(&n.entry, cuts)?;
    let p = d.doc.require(&para.id, Some(Kind::Paragraph))?.clone();
    d.anchor(&p);
    d.changed.push(target.into());
    d.intents.push(if let Some(s) = script {
        Intent::EquationSetScript {
            entry: n.entry,
            path,
            script: s.into(),
        }
    } else {
        Intent::EquationSetProperties {
            entry: n.entry,
            path,
            props: props.unwrap().clone(),
        }
    });
    Ok(())
}
