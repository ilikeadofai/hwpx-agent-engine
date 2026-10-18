//! Narrow edits reuse existing identities, resources and text carriers only.
use crate::{Draft, Intent};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{is_hp, Kind};
use hae_package::NS_HH;

fn unsupported(reason: &str, message: &str) -> EngineError {
    EngineError::new(Code::Unsupported, Stage::Validate, message).with_reason(reason)
}

pub(crate) fn set_style(d: &mut Draft, targets: &[String], style_id: u32) -> Result<()> {
    if targets.is_empty() {
        return Err(EngineError::new(
            Code::InvalidRequest,
            Stage::Validate,
            "targets must not be empty",
        ));
    }
    let id = style_id.to_string();
    let styles: Vec<_> = d
        .doc
        .header()
        .into_iter()
        .flat_map(|h| h.iter())
        .filter(|(_, e)| e.is(NS_HH, "style") && e.attr_value("id") == Some(&id))
        .collect();
    if styles.is_empty() {
        return Err(EngineError::new(
            Code::TargetNotFound,
            Stage::Validate,
            "styleId is not defined in header",
        ));
    }
    if styles.len() != 1 {
        return Err(unsupported("style_shape", "duplicate style ID"));
    }
    for target in targets {
        let p = d.node(target, Some(Kind::Paragraph))?;
        let xml = d.doc.doc_of(&p);
        let a = xml
            .el(p.el)
            .attr("styleIDRef")
            .ok_or_else(|| unsupported("style_shape", "paragraph has no styleIDRef"))?;
        if a.value == id {
            continue;
        }
        let path = xml.path(p.el);
        d.splice_entry(
            &p.entry,
            vec![(a.value_span.clone(), id.as_bytes().to_vec())],
        )?;
        let p2 = d.doc.require(target, Some(Kind::Paragraph))?.clone();
        d.anchor(&p2);
        d.changed.push(target.clone());
        d.intents.push(Intent::ParagraphSetStyle {
            entry: p.entry,
            path,
            style_id,
        });
    }
    Ok(())
}

pub(crate) fn header_text(
    d: &mut Draft,
    scope: Option<&str>,
    kind: &str,
    index: usize,
    text: &str,
) -> Result<()> {
    crate::text::check_new_text(text)?;
    if !["header", "footer"].contains(&kind) {
        return Err(EngineError::new(
            Code::InvalidRequest,
            Stage::Validate,
            "kind must be header or footer",
        ));
    }
    let section = match scope {
        Some(s) => d.node(s, Some(Kind::Section))?,
        None => d
            .doc
            .nodes
            .iter()
            .find(|n| n.kind == Kind::Section)
            .cloned()
            .ok_or_else(|| {
                EngineError::new(Code::TargetNotFound, Stage::Validate, "no first section")
            })?,
    };
    let doc = d.doc.doc_of(&section);
    // Paragraphs directly in the header/footer subList, never nested table/caption paragraphs.
    let mut paragraphs = Vec::new();
    for (i, e) in doc.iter() {
        if !is_hp(e.ns.as_deref()) || e.local != kind {
            continue;
        }
        for &sub in &doc.el(i).children {
            if is_hp(doc.el(sub).ns.as_deref()) && doc.el(sub).local == "subList" {
                paragraphs.extend(
                    doc.el(sub)
                        .children
                        .iter()
                        .copied()
                        .filter(|&p| is_hp(doc.el(p).ns.as_deref()) && doc.el(p).local == "p"),
                );
            }
        }
    }
    let p = *paragraphs.get(index).ok_or_else(|| {
        EngineError::new(
            Code::TargetNotFound,
            Stage::Validate,
            "header/footer paragraph does not exist",
        )
    })?;
    for i in doc.descendants(p) {
        let e = doc.el(i);
        if !is_hp(e.ns.as_deref())
            || !["run", "t", "linesegarray", "lineseg"].contains(&e.local.as_str())
        {
            return Err(unsupported(
                "header_control",
                "header/footer paragraph contains margin, numbering, field or other control",
            ));
        }
    }
    if !doc
        .descendants(p)
        .any(|i| is_hp(doc.el(i).ns.as_deref()) && doc.el(i).local == "t")
    {
        return Err(unsupported(
            "no_text_fragment",
            "header/footer needs an existing text carrier",
        ));
    }
    let para = d
        .doc
        .nodes
        .iter()
        .find(|n| n.entry == section.entry && n.el == p && n.kind == Kind::Paragraph)
        .unwrap()
        .clone();
    d.doc.check_editable(&para)?;
    let pt = d.doc.para_text(&para)?;
    if pt.text == text {
        return Ok(());
    }
    let before = d.intents.len();
    crate::text::text_replace(d, &para.id, 0..pt.grapheme_len(), text, false)?;
    if d.intents.len() > before {
        d.intents.pop();
        d.intents.push(Intent::HeaderSetText {
            entry: section.entry,
            kind: kind.into(),
            paragraph_index: index,
        });
    }
    Ok(())
}
