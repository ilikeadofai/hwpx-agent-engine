//! Independent field selector and byte derivation. Only original XML + request inputs.
use crate::{change, fail, hp, unsupported};
use hae_diag::Result;
use hae_package::PackageState;
use hae_xml::{escape_text, is_xml_char, Item, XmlDoc};

#[allow(clippy::too_many_arguments)]
pub(crate) fn set_text(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    p: usize,
    text: &str,
    name: Option<&str>,
    fieldid: Option<&str>,
    occurrence: Option<usize>,
) -> Result<Option<(String, String)>> {
    if !hp(doc.el(p), "p")
        || text
            .chars()
            .any(|c| !is_xml_char(c) || ['\t', '\n', '\r'].contains(&c))
    {
        return Err(fail("invalid field target or text"));
    }
    let owner = |i| doc.ancestors(i).find(|&a| hp(doc.el(a), "p"));
    let stream = |i| owner(i).and_then(|p| doc.el(p).parent);
    let all: Vec<_> = doc
        .iter()
        .filter(|(_, e)| hp(e, "fieldBegin"))
        .map(|(i, _)| i)
        .collect();
    let matches: Vec<_> = all
        .iter()
        .copied()
        .filter(|&i| {
            owner(i) == Some(p)
                && name.is_none_or(|n| doc.el(i).attr_value("name").unwrap_or("") == n)
                && fieldid.is_none_or(|id| {
                    doc.el(i).attr_value("fieldid") == Some(id)
                        || doc.el(i).attr_value("id") == Some(id)
                })
        })
        .collect();
    if matches.len() > 1 && occurrence.is_none() {
        return Err(fail("ambiguous field selector"));
    }
    let begin = *matches
        .get(occurrence.unwrap_or(0))
        .ok_or_else(|| fail("field selector not found"))?;
    let start = doc.el(begin);
    if !matches!(
        start.attr_value("type"),
        Some("CLICK_HERE" | "CLICKHERE" | "HYPERLINK")
    ) {
        return Err(unsupported("unsupported field type"));
    }
    let id = start
        .attr_value("id")
        .ok_or_else(|| unsupported("field lacks id"))?;
    if all
        .iter()
        .filter(|&&b| stream(b) == stream(begin) && doc.el(b).attr_value("id") == Some(id))
        .count()
        != 1
    {
        return Err(unsupported("duplicate field begin ID"));
    }
    let ends: Vec<_> = doc
        .iter()
        .filter(|(i, e)| hp(e, "fieldEnd") && stream(*i) == stream(begin))
        .map(|(i, _)| i)
        .collect();
    let matching: Vec<_> = ends
        .iter()
        .copied()
        .filter(|&i| doc.el(i).attr_value("beginIDRef") == Some(id))
        .collect();
    if matching.len() != 1 {
        return Err(unsupported("unpaired field"));
    }
    let finish = matching[0];
    let region = start.outer().end..doc.el(finish).start.start;
    if region.start > region.end || owner(finish) != Some(p) {
        return Err(unsupported(
            "field crosses paragraphs or has reversed markers",
        ));
    }
    for &other in &all {
        if other == begin || stream(other) != stream(begin) {
            continue;
        }
        let marker = doc.el(other);
        if region.contains(&marker.start.start)
            || (marker.start.start < start.start.start
                && ends.iter().any(|&e| {
                    doc.el(e).attr_value("beginIDRef") == marker.attr_value("id")
                        && doc.el(e).start.start > start.start.start
                }))
        {
            return Err(unsupported("nested fields"));
        }
    }
    if ends
        .iter()
        .any(|&e| region.contains(&doc.el(e).start.start))
    {
        return Err(unsupported("crossed field markers"));
    }
    let mut carriers = vec![];
    let mut value = String::new();
    for (i, e) in doc.iter() {
        for token in &e.items {
            if !hp(e, "t")
                && token
                    .raw()
                    .is_some_and(|r| r.start >= region.start && r.end <= region.end)
                && token.text().is_some_and(|s| !s.trim().is_empty())
            {
                return Err(unsupported("non-text run payload in field"));
            }
            if token
                .raw()
                .is_some_and(|r| r.start >= region.start && r.end <= region.end)
                && matches!(token, Item::CData { .. } | Item::Comment(_) | Item::Pi(_))
            {
                return Err(unsupported("field lexical boundary"));
            }
        }
        if e.start.start < region.start || e.outer().end > region.end {
            continue;
        }
        if hp(e, "linesegarray") {
            continue;
        }
        if !hp(e, "t") {
            if !hp(e, "run") && !hp(e, "p") {
                return Err(unsupported("field contains non-text object"));
            }
            continue;
        }
        if owner(i) != Some(p) || e.parent.is_none_or(|r| !hp(doc.el(r), "run")) {
            return Err(unsupported("field text is not direct run content"));
        }
        if e.attrs.iter().any(|a| {
            a.qname != "xmlns"
                && !a.qname.starts_with("xmlns:")
                && (a.ns.is_some() || a.local != "charPrIDRef")
        }) {
            return Err(unsupported("opaque field text attributes"));
        }
        for token in &e.items {
            if !matches!(token, Item::Text { .. } | Item::Ref { .. }) {
                return Err(unsupported("non-text field value"));
            }
            let s = token.text().unwrap();
            if s.contains(['\t', '\n', '\r']) {
                return Err(unsupported("literal control in field value"));
            }
            value.push_str(s);
        }
        carriers.push(i);
    }
    let insertion = if carriers.is_empty() {
        let run = doc
            .ancestors(begin)
            .find(|&a| hp(doc.el(a), "run"))
            .ok_or_else(|| unsupported("field lacks an existing run"))?;
        if doc.el(run).parent != Some(p) {
            return Err(unsupported("field run is not direct paragraph child"));
        }
        let mut unit = begin;
        while doc.el(unit).parent != Some(run) {
            unit = doc
                .el(unit)
                .parent
                .ok_or_else(|| unsupported("invalid field ancestry"))?;
        }
        if unit != begin && !hp(doc.el(unit), "ctrl") {
            return Err(unsupported("opaque field wrapper"));
        }
        let at = doc.el(unit).outer().end;
        if at > region.end {
            return Err(unsupported("no safe field insertion site"));
        }
        let prefix = doc
            .el(run)
            .qname
            .split_once(':')
            .map_or(String::new(), |(n, _)| format!("{n}:"));
        Some((
            at..at,
            format!("<{prefix}t>{}</{prefix}t>", escape_text(text)).into_bytes(),
        ))
    } else {
        None
    };
    if value == text {
        return Ok(None);
    }
    let old = crate::text::paragraph_display(doc, p);
    let mut cuts: Vec<_> = insertion.into_iter().collect();
    for (j, &i) in carriers.iter().enumerate() {
        let t = doc.el(i);
        let replacement = if j == 0 {
            escape_text(text)
        } else {
            String::new()
        };
        if t.end.is_some() {
            cuts.push((t.inner(), replacement.into_bytes()));
        } else if !replacement.is_empty() {
            let raw = std::str::from_utf8(doc.raw(t.start.clone()))
                .map_err(|_| unsupported("non-UTF8 text tag"))?;
            let open = raw
                .strip_suffix("/>")
                .ok_or_else(|| unsupported("invalid empty text tag"))?;
            cuts.push((
                t.start.clone(),
                format!("{}>{replacement}</{}>", open.trim_end(), t.qname).into_bytes(),
            ));
        }
    }
    let next = doc.splice(cuts.clone(), hae_xml::ParseOptions::default())?;
    let new = crate::text::paragraph_display(&next, p);
    change(state, entry, doc, cuts)?;
    Ok(Some((old, new)))
}
