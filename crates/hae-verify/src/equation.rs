//! Independent equation expectation from original XML + declarative requests.
//! No dependency on hae-model/hae-edit; never read candidate equation values.
use crate::{change, fail, hp, unsupported, EquationProps};
use hae_diag::Result;
use hae_package::PackageState;
use hae_xml::{escape_attr, escape_text, is_xml_char, Item, XmlDoc};

pub(crate) fn check(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    target: usize,
    script: Option<&str>,
    props: Option<&EquationProps>,
) -> Result<bool> {
    let eq = doc.el(target);
    if !hp(eq, "equation") {
        return Err(fail("equation request target has wrong kind"));
    }
    let mut patches = Vec::new();
    if let Some(value) = script {
        if value.trim().is_empty() || !value.chars().all(is_xml_char) {
            return Err(fail("empty or invalid XML script"));
        }
        if value
            .chars()
            .any(|c| c == '\\' || c == '"' || c.is_control())
            || value.contains("<!")
            || value.contains("<?")
        {
            return Err(unsupported(
                "unverified script escape/quoted/markup/control notation",
            ));
        }
        let mut groups = 0i64;
        for c in value.chars() {
            match c {
                '{' => groups += 1,
                '}' => groups -= 1,
                _ => {}
            }
            if groups < 0 {
                return Err(fail("script closing group without opening group"));
            }
        }
        if groups != 0 {
            return Err(fail("unterminated script group"));
        }
        let scripts: Vec<_> = eq
            .children
            .iter()
            .copied()
            .filter(|&i| hp(doc.el(i), "script"))
            .collect();
        if scripts.len() != 1 {
            return Err(unsupported("equation requires a unique script"));
        }
        let el = doc.el(scripts[0]);
        if !el.attrs.is_empty()
            || el
                .items
                .iter()
                .any(|i| !matches!(i, Item::Text { .. } | Item::Ref { .. }))
        {
            return Err(unsupported("opaque script markup"));
        }
        let before: String = el.items.iter().filter_map(Item::text).collect();
        if before != value {
            let encoded = escape_text(value);
            if el.end.is_none() {
                let bytes = doc.raw(el.start.clone());
                let mut replacement = bytes[..bytes.len() - 2].to_vec();
                // Match the documented empty-element expansion, retaining all
                // token bytes except trailing whitespace before the slash.
                while replacement.last().is_some_and(u8::is_ascii_whitespace) {
                    replacement.pop();
                }
                replacement.extend_from_slice(format!(">{encoded}</{}>", el.qname).as_bytes());
                patches.push((el.outer(), replacement));
            } else {
                patches.push((el.inner(), encoded.into_bytes()));
            }
        }
    }
    if let Some(p) = props {
        let fields = [
            ("baseLine", p.base_line.map(|n| n.to_string())),
            ("textColor", p.text_color.clone()),
            ("font", p.font.clone()),
            ("baseUnit", p.base_unit.map(|n| n.to_string())),
            ("lineMode", p.line_mode.clone()),
            ("version", p.version.clone()),
        ];
        if fields.iter().all(|(_, v)| v.is_none())
            || p.base_unit == Some(0)
            || p.line_mode
                .as_deref()
                .is_some_and(|v| v != "CHAR" && v != "LINE")
        {
            return Err(fail("invalid equation properties"));
        }
        if let Some(color) = &p.text_color {
            if color.len() != 7
                || color.as_bytes()[0] != b'#'
                || !color.chars().skip(1).all(|c| c.is_ascii_hexdigit())
            {
                return Err(fail("invalid equation color"));
            }
        }
        for v in [&p.font, &p.line_mode, &p.version].into_iter().flatten() {
            if v.trim().is_empty() || !v.chars().all(|c| is_xml_char(c) && !c.is_control()) {
                return Err(fail("invalid equation string property"));
            }
        }
        let mut additions = Vec::new();
        for (name, value) in fields {
            if let Some(v) = value {
                match eq.attr(name) {
                    Some(a) if a.value == v => {}
                    Some(a) => patches.push((a.value_span.clone(), escape_attr(&v).into_bytes())),
                    None => additions
                        .extend_from_slice(format!(" {name}=\"{}\"", escape_attr(&v)).as_bytes()),
                }
            }
        }
        if !additions.is_empty() {
            let insert = eq.start.end - if eq.end.is_some() { 1 } else { 2 };
            patches.push((insert..insert, additions));
        }
    }
    if patches.is_empty() {
        return Ok(false);
    }
    change(state, entry, doc, patches)?;
    Ok(true)
}
