//! Independent scalar-source mapping for paragraph requests.
use crate::{change, fail, hp, unsupported, Splice};
use hae_diag::Result;
use hae_package::PackageState;
use hae_xml::{escape_text, is_xml_char, Item, XmlDoc};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

struct Scalar {
    raw: Option<Range<usize>>,
    t: Option<usize>,
    character: char,
    barrier: bool,
    control: bool,
    position: usize,
}
struct View {
    para: usize,
    text: String,
    scalars: Vec<Scalar>,
    barriers: Vec<usize>,
}
fn scalar_before(s: &Scalar, at: usize) -> bool {
    s.raw.as_ref().map_or(s.position < at, |r| r.start < at)
}
fn placeholder(doc: &XmlDoc, i: usize) -> char {
    match doc.el(i).local.as_str() {
        "tab" => '\t',
        "lineBreak" => '\n',
        "fwSpace" => '\u{3000}',
        "nbSpace" => '\u{a0}',
        "hyphen" => '-',
        _ => '\u{fffc}',
    }
}
fn view(doc: &XmlDoc, p: usize) -> View {
    let mut out = View {
        para: p,
        text: String::new(),
        scalars: Vec::new(),
        barriers: Vec::new(),
    };
    for &run in &doc.el(p).children {
        if !hp(doc.el(run), "run") {
            continue;
        }
        for item in &doc.el(run).items {
            let Item::Element(c) = item else {
                // Whitespace between run elements is lexical formatting. Any
                // non-whitespace direct text is unsupported if a range crosses it.
                if item.text().is_some_and(|s| !s.trim().is_empty())
                    || matches!(item, Item::Comment(_) | Item::Pi(_))
                {
                    out.barriers.push(out.scalars.len());
                }
                continue;
            };
            let c = *c;
            if hp(doc.el(c), "t") {
                for item in &doc.el(c).items {
                    match item {
                        Item::Text { raw, text } => {
                            for (offset, ch) in text.char_indices() {
                                out.text.push(ch);
                                out.scalars.push(Scalar {
                                    raw: Some(
                                        raw.start + offset..raw.start + offset + ch.len_utf8(),
                                    ),
                                    t: Some(c),
                                    character: ch,
                                    barrier: false,
                                    control: false,
                                    position: 0,
                                });
                            }
                        }
                        Item::Ref { raw, text } => {
                            for ch in text.chars() {
                                out.text.push(ch);
                                out.scalars.push(Scalar {
                                    raw: Some(raw.clone()),
                                    t: Some(c),
                                    character: ch,
                                    barrier: false,
                                    control: false,
                                    position: 0,
                                });
                            }
                        }
                        Item::CData { text, .. } => {
                            for ch in text.chars() {
                                out.text.push(ch);
                                out.scalars.push(Scalar {
                                    raw: None,
                                    t: Some(c),
                                    character: ch,
                                    barrier: true,
                                    control: false,
                                    position: 0,
                                });
                            }
                            // Empty CDATA also represents an unsupported boundary.
                            out.barriers.push(out.scalars.len());
                        }
                        Item::Element(e) => {
                            let ch = placeholder(doc, *e);
                            out.text.push(ch);
                            out.scalars.push(Scalar {
                                raw: None,
                                t: Some(c),
                                character: ch,
                                barrier: true,
                                control: hp(doc.el(*e), "tab") || hp(doc.el(*e), "lineBreak"),
                                position: doc.el(*e).start.start,
                            });
                        }
                        Item::Comment(_) | Item::Pi(_) => out.barriers.push(out.scalars.len()),
                    }
                }
            } else if !hp(doc.el(c), "secPr") {
                out.text.push('\u{fffc}');
                out.scalars.push(Scalar {
                    raw: None,
                    t: None,
                    character: '\u{fffc}',
                    barrier: true,
                    control: true,
                    position: doc.el(c).start.start,
                });
            }
        }
    }
    out
}
fn new_text(s: &str) -> Result<()> {
    if s.chars()
        .any(|c| !is_xml_char(c) || ['\t', '\n', '\r'].contains(&c))
    {
        return Err(unsupported(
            "replacement contains control/non-XML characters",
        ));
    }
    Ok(())
}
fn plain_attrs(doc: &XmlDoc, t: usize) -> Result<()> {
    for a in &doc.el(t).attrs {
        if a.qname == "xmlns" || a.qname.starts_with("xmlns:") {
            continue;
        }
        if a.ns.is_some() || a.local != "charPrIDRef" {
            return Err(unsupported("touched text has opaque attributes"));
        }
    }
    Ok(())
}

pub(crate) fn replace(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    p: usize,
    range: Range<usize>,
    replacement: &str,
    preserve: bool,
) -> Result<Option<(String, String)>> {
    new_text(replacement)?;
    let v = view(doc, p);
    let boundaries: Vec<usize> = std::iter::once(0)
        .chain(v.text.graphemes(true).scan(0, |cp, g| {
            *cp += g.chars().count();
            Some(*cp)
        }))
        .collect();
    if range.start > range.end
        || range.end >= boundaries.len()
        || (range.start == range.end && replacement.is_empty())
    {
        return Err(fail("invalid original grapheme range"));
    }
    if range.start == range.end {
        return insert(
            state,
            entry,
            doc,
            p,
            v,
            boundaries[range.start],
            replacement,
        );
    }
    replace_scalars(
        state,
        entry,
        doc,
        v,
        boundaries[range.start]..boundaries[range.end],
        replacement,
        preserve,
    )
}
fn replace_scalars(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    v: View,
    range: Range<usize>,
    replacement: &str,
    preserve: bool,
) -> Result<Option<(String, String)>> {
    let covered = &v.scalars[range.clone()];
    if v.barriers.iter().any(|&b| range.start < b && b < range.end) {
        return Err(unsupported("range crosses comments/PI/CDATA boundaries"));
    }
    let mut cuts: Vec<Splice> = Vec::new();
    for s in covered {
        if s.control && preserve {
            if let Some(t) = s.t {
                plain_attrs(doc, t)?;
            }
            continue;
        }
        if s.barrier || ['\t', '\n', '\r'].contains(&s.character) {
            return Err(unsupported(
                "range covers control, opaque content, or CDATA",
            ));
        }
        plain_attrs(doc, s.t.ok_or_else(|| unsupported("text outside hp:t"))?)?;
        let r = s.raw.as_ref().ok_or_else(|| unsupported("unmapped text"))?;
        if let Some((last, _)) = cuts.last_mut() {
            if last.end == r.start {
                last.end = r.end;
                continue;
            }
        }
        cuts.push((r.clone(), Vec::new()));
    }
    if cuts.is_empty() {
        if !preserve {
            return Err(unsupported("empty replacement source"));
        }
        // Existing empty hp:t, including a self-closing one, is a valid
        // zero-width fragment. Never create one in a non-empty paragraph.
        let mut found = None;
        for &run in &doc.el(v.para).children {
            if !hp(doc.el(run), "run") {
                continue;
            }
            for &t in &doc.el(run).children {
                let e = doc.el(t);
                let cp = v
                    .scalars
                    .iter()
                    .filter(|s| scalar_before(s, e.start.start))
                    .count();
                if hp(e, "t") && e.items.is_empty() && range.start <= cp && cp <= range.end {
                    plain_attrs(doc, t)?;
                    found = Some((t, cp));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        if let Some((t, _)) = found {
            let e = doc.el(t);
            let cp = v
                .scalars
                .iter()
                .filter(|s| scalar_before(s, e.start.start))
                .count();
            let mut wanted: String = v.text.chars().take(cp).collect();
            wanted.push_str(replacement);
            wanted.extend(v.text.chars().skip(cp));
            if replacement.is_empty() {
                return Ok(None);
            }
            let cut = if e.end.is_some() {
                (e.inner(), escape_text(replacement).into_bytes())
            } else {
                let tag =
                    std::str::from_utf8(doc.raw(e.start.clone())).map_err(|_| fail("invalid t"))?;
                (
                    e.outer(),
                    format!(
                        "{}>{}</{}>",
                        tag.trim_end_matches('>').trim_end_matches('/').trim_end(),
                        escape_text(replacement),
                        e.qname
                    )
                    .into_bytes(),
                )
            };
            change(state, entry, doc, vec![cut])?;
            return Ok(Some((v.text, wanted)));
        }
        return Err(unsupported("preserve range has no text fragment"));
    }
    cuts[0].1 = escape_text(replacement).into_bytes();
    let mut wanted: String = v.scalars[..range.start]
        .iter()
        .map(|s| s.character)
        .collect();
    let mut inserted = false;
    for s in covered {
        if preserve && s.control {
            wanted.push(s.character);
        } else if !inserted {
            wanted.push_str(replacement);
            inserted = true;
        }
    }
    wanted.extend(v.scalars[range.end..].iter().map(|s| s.character));
    if v.text == wanted {
        return Ok(None);
    }
    change(state, entry, doc, cuts)?;
    Ok(Some((v.text, wanted)))
}

fn insert(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    p: usize,
    v: View,
    cp: usize,
    text: &str,
) -> Result<Option<(String, String)>> {
    if !v.scalars.is_empty() {
        let valid = |s: &Scalar| {
            !s.barrier && !['\t', '\n', '\r'].contains(&s.character) && s.raw.is_some()
        };
        let right = v.scalars.get(cp).filter(|s| valid(s));
        let left = cp
            .checked_sub(1)
            .and_then(|i| v.scalars.get(i))
            .filter(|s| valid(s));
        let s = right
            .or(left)
            .ok_or_else(|| unsupported("insertion has no adjacent text"))?;
        // The selected adjacent grapheme must be entirely supported.
        let offsets: Vec<usize> = std::iter::once(0)
            .chain(v.text.graphemes(true).scan(0, |n, g| {
                *n += g.chars().count();
                Some(*n)
            }))
            .collect();
        let g = offsets
            .binary_search(&cp)
            .map_err(|_| fail("not a grapheme boundary"))?;
        let (a, b) = if right.is_some() {
            (cp, offsets[g + 1])
        } else {
            (offsets[g - 1], cp)
        };
        if v.barriers.iter().any(|&i| a < i && i < b) || v.scalars[a..b].iter().any(|s| !valid(s)) {
            return Err(unsupported("insertion outside safe span"));
        }
        for s in &v.scalars[a..b] {
            plain_attrs(doc, s.t.ok_or_else(|| unsupported("missing t"))?)?;
        }
        let r = s.raw.as_ref().unwrap();
        let at = if right.is_some() { r.start } else { r.end };
        let mut wanted: String = v.text.chars().take(cp).collect();
        wanted.push_str(text);
        wanted.extend(v.text.chars().skip(cp));
        change(
            state,
            entry,
            doc,
            vec![(at..at, escape_text(text).into_bytes())],
        )?;
        return Ok(Some((v.text, wanted)));
    }
    let run = doc
        .el(p)
        .children
        .iter()
        .copied()
        .find(|&i| hp(doc.el(i), "run"))
        .ok_or_else(|| unsupported("empty cell lacks first run"))?;
    if !v.barriers.is_empty() {
        return Err(unsupported("empty cell has opaque content"));
    }
    let t = doc
        .el(run)
        .children
        .iter()
        .copied()
        .find(|&i| hp(doc.el(i), "t"));
    let cuts = if let Some(t) = t {
        plain_attrs(doc, t)?;
        let e = doc.el(t);
        if !e.items.is_empty() {
            return Err(unsupported("empty text element has non-text content"));
        }
        if e.end.is_some() {
            vec![(e.inner(), escape_text(text).into_bytes())]
        } else {
            let tag =
                std::str::from_utf8(doc.raw(e.start.clone())).map_err(|_| fail("non-UTF-8 t"))?;
            let open = tag
                .strip_suffix('>')
                .and_then(|s| s.strip_suffix('/'))
                .ok_or_else(|| fail("invalid empty t"))?;
            vec![(
                e.outer(),
                format!("{}>{}</{}>", open.trim_end(), escape_text(text), e.qname).into_bytes(),
            )]
        }
    } else {
        let r = doc.el(run);
        if r.end.is_none() {
            let tag =
                std::str::from_utf8(doc.raw(r.start.clone())).map_err(|_| fail("non-UTF-8 run"))?;
            let open = tag.trim_end_matches('>').trim_end_matches('/').trim_end();
            let prefix = r
                .qname
                .split_once(':')
                .map_or(String::new(), |(p, _)| format!("{p}:"));
            change(
                state,
                entry,
                doc,
                vec![(
                    r.start.clone(),
                    format!(
                        "{open}><{prefix}t>{}</{prefix}t></{}>",
                        escape_text(text),
                        r.qname
                    )
                    .into_bytes(),
                )],
            )?;
            return Ok(Some((String::new(), text.into())));
        }
        let prefix = r
            .qname
            .split_once(':')
            .map_or(String::new(), |(p, _)| format!("{p}:"));
        vec![(
            r.inner().end..r.inner().end,
            format!("<{prefix}t>{}</{prefix}t>", escape_text(text)).into_bytes(),
        )]
    };
    change(state, entry, doc, cuts)?;
    Ok(Some((String::new(), text.into())))
}

type CellChange = (usize, Option<(String, String)>);
pub(crate) fn cell(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    tc: usize,
    text: Option<&str>,
    paragraphs: Option<&[String]>,
    preserve: bool,
) -> Result<Vec<CellChange>> {
    if !hp(doc.el(tc), "tc") {
        return Err(fail("cell target is not hp:tc"));
    }
    if doc.ancestors(tc).any(|i| hp(doc.el(i), "tc")) {
        return Err(unsupported("nested cell"));
    }
    let mut paras = Vec::new();
    for i in doc.descendants(tc) {
        let e = doc.el(i);
        if hp(e, "tbl") {
            return Err(unsupported("nested table"));
        }
        if hp(e, "p") {
            paras.push(i);
        }
    }
    if paras.is_empty() {
        return Err(unsupported("cell has no paragraphs"));
    }
    let values: Vec<&str> = match (text, paragraphs) {
        (Some(s), None) if paras.len() == 1 => vec![s],
        (None, Some(ss)) if paras.len() == ss.len() => ss.iter().map(String::as_str).collect(),
        _ => return Err(unsupported("invalid cell paragraph arguments")),
    };
    let mut changes = Vec::new();
    for (p, text) in paras.into_iter().zip(values) {
        new_text(text)?;
        let current = XmlDoc::parse(state.get(entry).unwrap().clone())?;
        let path = doc.path(p);
        let now = crate::at(&current, &path)?;
        let v = view(&current, now);
        let changed = if v.scalars.is_empty() {
            if text.is_empty() {
                None
            } else {
                insert(state, entry, &current, now, v, 0, text)?
            }
        } else {
            let len = v.scalars.len();
            replace_scalars(state, entry, &current, v, 0..len, text, preserve)?
        };
        changes.push((p, changed));
    }
    Ok(changes)
}
