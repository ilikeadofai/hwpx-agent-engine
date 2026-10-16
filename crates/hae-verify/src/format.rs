//! Permitted byte deltas to paragraph property definitions.
use crate::{attr, change, fail, parse, patched, unsupported, Alignment, LineSpacing, Splice};
use hae_diag::{Limits, Result};
use hae_package::{PackageInfo, PackageState, NS_HH};
use hae_xml::{escape_attr, XmlDoc};
use std::collections::BTreeSet;

fn definition(
    doc: &XmlDoc,
    p: usize,
    id: &str,
    alignment: Option<Alignment>,
    line_spacing: Option<LineSpacing>,
) -> Result<Vec<u8>> {
    let mut cuts = vec![(
        attr(doc, p, "id")?.value_span.clone(),
        escape_attr(id).into_bytes(),
    )];
    let mut align_count = 0;
    let mut line_count = 0;
    for i in doc.descendants(p) {
        let e = doc.el(i);
        if e.is(NS_HH, "align") {
            if let Some(a) = alignment {
                cuts.push((
                    attr(doc, i, "horizontal")?.value_span.clone(),
                    a.value().as_bytes().to_vec(),
                ));
                align_count += 1;
            }
        }
        if e.is(NS_HH, "lineSpacing") {
            if let Some(l) = line_spacing {
                cuts.push((
                    attr(doc, i, "type")?.value_span.clone(),
                    b"PERCENT".to_vec(),
                ));
                cuts.push((
                    attr(doc, i, "value")?.value_span.clone(),
                    l.value.to_string().into_bytes(),
                ));
                line_count += 1;
            }
        }
    }
    if alignment.is_some() && align_count == 0 || line_spacing.is_some() && line_count == 0 {
        return Err(unsupported("style lacks requested fields"));
    }
    let base = doc.el(p).outer();
    let relative: Vec<Splice> = cuts
        .into_iter()
        .map(|(r, b)| (r.start - base.start..r.end - base.start, b))
        .collect();
    patched(doc.raw(base), relative)
}
fn without_id(doc: &XmlDoc, p: usize) -> Result<Vec<u8>> {
    let base = doc.el(p).outer();
    let span = attr(doc, p, "id")?.value_span.clone();
    patched(
        doc.raw(base.clone()),
        vec![(span.start - base.start..span.end - base.start, Vec::new())],
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    state: &mut PackageState,
    info: &PackageInfo,
    entry: &str,
    section: &XmlDoc,
    p: usize,
    alignment: Option<Alignment>,
    line_spacing: Option<LineSpacing>,
    limits: &Limits,
) -> Result<bool> {
    if alignment.is_none() && line_spacing.is_none() {
        return Err(fail("empty format request"));
    }
    if line_spacing.is_some_and(|l| !(1..=1000).contains(&l.value)) {
        return Err(fail("line spacing outside 1..1000 percent"));
    }
    let name = info
        .header
        .as_ref()
        .ok_or_else(|| unsupported("format requires header"))?;
    let header = parse(state, name, limits)?;
    let containers: Vec<_> = header
        .iter()
        .filter(|(_, e)| e.is(NS_HH, "paraProperties"))
        .map(|(i, _)| i)
        .collect();
    if containers.len() != 1 {
        return Err(unsupported("ambiguous paraProperties"));
    }
    let props = containers[0];
    let definitions: Vec<_> = header
        .el(props)
        .children
        .iter()
        .copied()
        .filter(|&i| header.el(i).is(NS_HH, "paraPr"))
        .collect();
    let pref = attr(section, p, "paraPrIDRef")?;
    let mut ids = BTreeSet::new();
    let mut max = 0u64;
    let mut current = None;
    for &i in &definitions {
        let id = &attr(&header, i, "id")?.value;
        if !ids.insert(id) {
            return Err(fail("duplicate paraPr ID"));
        }
        max = max.max(
            id.parse::<u64>()
                .map_err(|_| unsupported("non-numeric paragraph property ID"))?,
        );
        if id == &pref.value {
            current = Some(i);
        }
    }
    let current = current.ok_or_else(|| fail("unresolved original paraPrIDRef"))?;
    let canonical = definition(&header, current, "", alignment, line_spacing)?;
    let mut reuse = None;
    for &i in &definitions {
        if without_id(&header, i)? == canonical {
            reuse = Some(attr(&header, i, "id")?.value.clone());
            break;
        }
    }
    let id = if let Some(id) = reuse {
        id
    } else {
        let id = max
            .checked_add(1)
            .ok_or_else(|| unsupported("paragraph ID exhausted"))?
            .to_string();
        let clone = definition(&header, current, &id, alignment, line_spacing)?;
        let end = header
            .el(props)
            .end
            .as_ref()
            .ok_or_else(|| unsupported("empty paraProperties"))?;
        let mut changes = vec![(end.start..end.start, clone)];
        if let Some(count) = header.el(props).attr("itemCnt") {
            let n = count
                .value
                .parse::<usize>()
                .map_err(|_| unsupported("invalid paraProperties itemCnt"))?;
            if n != definitions.len() {
                return Err(unsupported("inconsistent paraProperties itemCnt"));
            }
            changes.push((count.value_span.clone(), (n + 1).to_string().into_bytes()));
        }
        change(state, name, &header, changes)?;
        id
    };
    if id == pref.value {
        return Ok(false);
    }
    change(
        state,
        entry,
        section,
        vec![(pref.value_span.clone(), escape_attr(&id).into_bytes())],
    )?;
    Ok(true)
}
