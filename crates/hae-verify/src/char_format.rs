//! Independent expected-byte derivation for text.format. No model/edit dependency.
use crate::{attr, change, fail, parse, patched, unsupported, CharProps, Splice};
use hae_diag::{Limits, Result};
use hae_package::{PackageInfo, PackageState, NS_HH};
use hae_xml::{Item, XmlDoc};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

fn validate(p: &CharProps) -> Result<()> {
    let has = p.bold.is_some()
        || p.italic.is_some()
        || p.underline.is_some()
        || p.strikeout.is_some()
        || p.height_hwpunit.is_some()
        || p.text_color.is_some();
    if !has
        || p.height_hwpunit
            .is_some_and(|v| !(100..=100000).contains(&v))
    {
        return Err(fail("invalid character props"));
    }
    if let Some(c) = &p.text_color {
        if c.len() != 7
            || c.as_bytes()[0] != b'#'
            || !c.chars().skip(1).all(|v| v.is_ascii_hexdigit())
        {
            return Err(fail("invalid textColor"));
        }
    }
    Ok(())
}
fn local_patch(doc: &XmlDoc, i: usize, cuts: Vec<Splice>) -> Result<Vec<u8>> {
    let base = doc.el(i).outer();
    patched(
        doc.raw(base.clone()),
        cuts.into_iter()
            .map(|(r, v)| (r.start - base.start..r.end - base.start, v))
            .collect(),
    )
}
fn blank_id(doc: &XmlDoc, i: usize) -> Result<Vec<u8>> {
    local_patch(
        doc,
        i,
        vec![(attr(doc, i, "id")?.value_span.clone(), vec![])],
    )
}
fn requested_shape(doc: &XmlDoc, i: usize, id: &str, p: &CharProps) -> Result<Vec<u8>> {
    let root = doc.el(i);
    let end = root
        .end
        .as_ref()
        .ok_or_else(|| unsupported("empty charPr"))?;
    let mut cuts = vec![(
        attr(doc, i, "id")?.value_span.clone(),
        id.as_bytes().to_vec(),
    )];
    let mut attrs = Vec::new();
    for (name, value) in [
        ("height", p.height_hwpunit.map(|v| v.to_string())),
        ("textColor", p.text_color.as_ref().map(|s| s.to_uppercase())),
    ] {
        if let Some(v) = value {
            match root.attr(name) {
                Some(a) => cuts.push((a.value_span.clone(), v.into_bytes())),
                None => attrs.extend_from_slice(format!(" {name}=\"{v}\"").as_bytes()),
            };
        }
    }
    if !attrs.is_empty() {
        cuts.push((root.start.end - 1..root.start.end - 1, attrs));
    }
    let prefix = root
        .qname
        .split_once(':')
        .map_or(String::new(), |(n, _)| format!("{n}:"));
    let rank = |n: &str| match n {
        "fontRef" => 0,
        "ratio" => 1,
        "spacing" => 2,
        "relSz" => 3,
        "offset" => 4,
        "italic" => 5,
        "bold" => 6,
        "underline" => 7,
        "strikeout" => 8,
        "outline" => 9,
        "shadow" => 10,
        "emboss" => 11,
        "engrave" => 12,
        "supscript" => 13,
        "subscript" => 14,
        _ => usize::MAX,
    };
    let mut insert = BTreeMap::<usize, Vec<u8>>::new();
    for (name, enabled) in [
        ("italic", p.italic),
        ("bold", p.bold),
        ("underline", p.underline),
        ("strikeout", p.strikeout),
    ] {
        let Some(on) = enabled else { continue };
        let nodes: Vec<_> = root
            .children
            .iter()
            .copied()
            .filter(|&c| doc.el(c).is(NS_HH, name))
            .collect();
        if nodes.len() > 1 {
            return Err(unsupported("ambiguous char property"));
        }
        if let Some(&c) = nodes.first() {
            let node = doc.el(c);
            match name {
                "italic" | "bold" => {
                    if !node.items.is_empty() || !node.attrs.is_empty() {
                        return Err(unsupported("opaque boolean property"));
                    }
                    if !on {
                        cuts.push((node.outer(), vec![]));
                    }
                }
                "underline" => {
                    cuts.push((
                        attr(doc, c, "type")?.value_span.clone(),
                        if on {
                            b"BOTTOM".to_vec()
                        } else {
                            b"NONE".to_vec()
                        },
                    ));
                    if on && node.attr_value("shape") == Some("NONE") {
                        cuts.push((attr(doc, c, "shape")?.value_span.clone(), b"SOLID".to_vec()));
                    }
                }
                _ => cuts.push((
                    attr(doc, c, "shape")?.value_span.clone(),
                    if on {
                        b"SOLID".to_vec()
                    } else {
                        b"NONE".to_vec()
                    },
                )),
            }
        } else if on {
            let position = root
                .children
                .iter()
                .copied()
                .find(|&c| {
                    doc.el(c).ns.as_deref() == Some(NS_HH)
                        && rank(&doc.el(c).local) != usize::MAX
                        && rank(&doc.el(c).local) > rank(name)
                })
                .map_or(end.start, |c| doc.el(c).start.start);
            let extra = match name {
                "underline" => " type=\"BOTTOM\" shape=\"SOLID\" color=\"#000000\"",
                "strikeout" => " shape=\"SOLID\" color=\"#000000\"",
                _ => "",
            };
            insert
                .entry(position)
                .or_default()
                .extend_from_slice(format!("<{prefix}{name}{extra}/>").as_bytes());
        }
    }
    cuts.extend(insert.into_iter().map(|(n, v)| (n..n, v)));
    local_patch(doc, i, cuts)
}

/// Derive wrapper boundaries from scalar coverage, keeping all original tokens.
fn formatted_run(
    doc: &XmlDoc,
    run: usize,
    coverage: &[(usize, Range<usize>)],
    id: &str,
) -> Result<Vec<u8>> {
    let root = doc.el(run);
    let close = root
        .end
        .as_ref()
        .ok_or_else(|| unsupported("run without closing tag"))?;
    let reference = attr(doc, run, "charPrIDRef")?;
    let mut selected_open = doc.raw(root.start.clone()).to_vec();
    let offset = reference.value_span.start - root.start.start;
    let finish = reference.value_span.end - root.start.start;
    selected_open.splice(offset..finish, id.bytes());
    let mut groups: Vec<(bool, Vec<u8>)> = vec![];
    let mut append = |flag: bool, data: Vec<u8>| {
        if let Some((last, v)) = groups.last_mut() {
            if *last == flag {
                v.extend(data);
                return;
            }
        }
        groups.push((flag, data));
    };
    for item in &root.items {
        match item {
            Item::Element(t) => {
                let e = doc.el(*t);
                let spans: Vec<_> = coverage
                    .iter()
                    .filter(|(i, _)| i == t)
                    .map(|(_, s)| s)
                    .collect();
                if spans.is_empty() {
                    append(false, doc.raw(e.outer()).to_vec());
                    continue;
                }
                let tail = e
                    .end
                    .as_ref()
                    .ok_or_else(|| unsupported("text without end"))?;
                let mut positions = BTreeSet::from([e.inner().start, e.inner().end]);
                for s in &spans {
                    positions.insert(s.start);
                    positions.insert(s.end);
                }
                let positions: Vec<_> = positions.into_iter().collect();
                let mut pieces: Vec<(bool, Vec<u8>)> = vec![];
                for pair in positions.windows(2) {
                    let on = spans.iter().any(|s| s.contains(&pair[0]));
                    let b = doc.raw(pair[0]..pair[1]);
                    if let Some((f, v)) = pieces.last_mut() {
                        if *f == on {
                            v.extend_from_slice(b);
                            continue;
                        }
                    }
                    pieces.push((on, b.to_vec()));
                }
                for (on, value) in pieces {
                    let mut content = doc.raw(e.start.clone()).to_vec();
                    content.extend(value);
                    content.extend_from_slice(doc.raw(tail.clone()));
                    append(on, content);
                }
            }
            _ => append(
                false,
                doc.raw(
                    item.raw()
                        .ok_or_else(|| unsupported("unknown run item"))?
                        .clone(),
                )
                .to_vec(),
            ),
        }
    }
    let mut result = vec![];
    for (on, content) in groups {
        result.extend_from_slice(if on {
            &selected_open
        } else {
            doc.raw(root.start.clone())
        });
        result.extend(content);
        result.extend_from_slice(doc.raw(close.clone()));
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    state: &mut PackageState,
    info: &PackageInfo,
    entry: &str,
    section: &XmlDoc,
    p: usize,
    range: Range<usize>,
    props: &CharProps,
    preserve: bool,
    limits: &Limits,
) -> Result<bool> {
    validate(props)?;
    let coverage = crate::text::format_spans(section, p, range, preserve)?;
    let mut by_run = BTreeMap::<usize, Vec<_>>::new();
    for (t, span) in coverage {
        let r = section
            .el(t)
            .parent
            .ok_or_else(|| unsupported("text lacks run"))?;
        by_run.entry(r).or_default().push((t, span));
    }
    let name = info
        .header
        .as_ref()
        .ok_or_else(|| unsupported("missing header"))?;
    let header = parse(state, name, limits)?;
    let containers: Vec<_> = header
        .iter()
        .filter(|(_, e)| e.is(NS_HH, "charProperties"))
        .map(|(i, _)| i)
        .collect();
    if containers.len() != 1 {
        return Err(unsupported("ambiguous charProperties"));
    }
    let container = containers[0];
    let defs: Vec<_> = header
        .el(container)
        .children
        .iter()
        .copied()
        .filter(|&i| header.el(i).is(NS_HH, "charPr"))
        .collect();
    let count = attr(&header, container, "itemCnt")?;
    if count.value.parse::<usize>().ok() != Some(defs.len()) {
        return Err(unsupported("bad charProperties count"));
    }
    let mut ids = BTreeSet::new();
    let mut max = 0u64;
    for &i in &defs {
        let id = &attr(&header, i, "id")?.value;
        if !ids.insert(id) {
            return Err(fail("duplicate charPr ID"));
        }
        max = max.max(
            id.parse()
                .map_err(|_| unsupported("non-numeric charPr ID"))?,
        );
    }
    let mut created: Option<(String, Vec<u8>, Vec<u8>)> = None;
    let mut body = vec![];
    for (run, coverage) in by_run {
        let source = &attr(section, run, "charPrIDRef")?.value;
        let original = *defs
            .iter()
            .find(|&&i| header.el(i).attr_value("id") == Some(source))
            .ok_or_else(|| fail("unresolved source charPr"))?;
        let want = requested_shape(&header, original, "", props)?;
        let mut use_id = None;
        for &i in &defs {
            if blank_id(&header, i)? == want {
                use_id = Some(attr(&header, i, "id")?.value.clone());
                break;
            }
        }
        let id = if let Some(id) = use_id {
            id
        } else if let Some((id, prior, _)) = &created {
            if prior != &want {
                return Err(unsupported("multiple charPr required"));
            }
            id.clone()
        } else {
            let id = max
                .checked_add(1)
                .ok_or_else(|| unsupported("charPr ID overflow"))?
                .to_string();
            created = Some((
                id.clone(),
                want,
                requested_shape(&header, original, &id, props)?,
            ));
            id
        };
        if id != *source {
            body.push((
                section.el(run).outer(),
                formatted_run(section, run, &coverage, &id)?,
            ));
        }
    }
    if body.is_empty() {
        return Ok(false);
    }
    if let Some((_, _, data)) = created {
        let end = header
            .el(container)
            .end
            .as_ref()
            .ok_or_else(|| unsupported("empty charProperties"))?;
        change(
            state,
            name,
            &header,
            vec![
                (
                    count.value_span.clone(),
                    (defs.len() + 1).to_string().into_bytes(),
                ),
                (end.start..end.start, data),
            ],
        )?;
    }
    change(state, entry, section, body)?;
    Ok(true)
}
