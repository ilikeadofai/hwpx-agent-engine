//! Independent scalar-source mapping for paragraph requests.
use crate::{attr, change, fail, hp, unsupported, Splice};
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
}
struct View {
    text: String,
    scalars: Vec<Scalar>,
    barriers: Vec<usize>,
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
) -> Result<Option<(String, String)>> {
    new_text(replacement)?;
    let v = view(doc, p);
    let boundaries: Vec<usize> = std::iter::once(0)
        .chain(v.text.graphemes(true).scan(0, |cp, g| {
            *cp += g.chars().count();
            Some(*cp)
        }))
        .collect();
    if range.start >= range.end || range.end >= boundaries.len() {
        return Err(fail("invalid original grapheme range"));
    }
    replace_scalars(
        state,
        entry,
        doc,
        v,
        boundaries[range.start]..boundaries[range.end],
        replacement,
    )
}
fn replace_scalars(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    v: View,
    range: Range<usize>,
    replacement: &str,
) -> Result<Option<(String, String)>> {
    let covered = &v.scalars[range.clone()];
    if v.barriers.iter().any(|&b| range.start < b && b < range.end) {
        return Err(unsupported("range crosses comments/PI/CDATA boundaries"));
    }
    let mut cuts: Vec<Splice> = Vec::new();
    for s in covered {
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
        return Err(unsupported("empty replacement source"));
    }
    let old_selected: String = covered.iter().map(|s| s.character).collect();
    if old_selected == replacement {
        return Ok(None);
    }
    cuts[0].1 = escape_text(replacement).into_bytes();
    let mut wanted: String = v.scalars[..range.start]
        .iter()
        .map(|s| s.character)
        .collect();
    wanted.push_str(replacement);
    wanted.extend(v.scalars[range.end..].iter().map(|s| s.character));
    change(state, entry, doc, cuts)?;
    Ok(Some((v.text, wanted)))
}

type CellChange = (usize, Option<(String, String)>);
pub(crate) fn cell(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    tc: usize,
    text: &str,
) -> Result<CellChange> {
    new_text(text)?;
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
        if hp(e, "cellSpan")
            && (attr(doc, i, "colSpan")?.value != "1" || attr(doc, i, "rowSpan")?.value != "1")
        {
            return Err(unsupported("merged cell"));
        }
        if hp(e, "p") {
            paras.push(i);
        }
    }
    if paras.len() != 1 {
        return Err(unsupported("cell must contain exactly one paragraph"));
    }
    let p = paras[0];
    let v = view(doc, p);
    if !v.scalars.is_empty() {
        let len = v.scalars.len();
        return Ok((p, replace_scalars(state, entry, doc, v, 0..len, text)?));
    }
    let run = doc
        .el(p)
        .children
        .iter()
        .copied()
        .find(|&i| hp(doc.el(i), "run"))
        .ok_or_else(|| unsupported("empty cell lacks first run"))?;
    if text.is_empty() {
        return Ok((p, None));
    }
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
            return Err(unsupported("self-closing empty run"));
        }
        let prefix = r
            .qname
            .split_once(':')
            .map_or(String::new(), |(p, _)| format!("{p}:"));
        vec![(
            r.start.end..r.start.end,
            format!("<{prefix}t>{}</{prefix}t>", escape_text(text)).into_bytes(),
        )]
    };
    change(state, entry, doc, cuts)?;
    Ok((p, Some((String::new(), text.into()))))
}
