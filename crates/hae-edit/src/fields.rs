//! Fields are addressed semantically; only their existing text carriers change.
use crate::{Draft, Intent};
use hae_diag::{Code, EngineError, Result, Stage};
use hae_model::{Kind, PieceKind};
use hae_xml::escape_text;

pub(crate) fn set_text(
    d: &mut Draft,
    target: &str,
    text: &str,
    name: Option<&str>,
    fieldid: Option<&str>,
    occurrence: Option<usize>,
) -> Result<()> {
    crate::text::check_new_text(text)?;
    let para = d.node(target, Some(Kind::Paragraph))?;
    let f = d.doc.select_field(target, name, fieldid, occurrence)?;
    if let Some(reason) = &f.reason {
        return Err(EngineError::new(
            Code::UnsupportedContentInRange,
            Stage::Validate,
            "field value cannot be filled safely",
        )
        .with_target(target)
        .with_reason(reason));
    }
    if f.text == text {
        return Ok(());
    }
    let x = d.doc.doc_of(&para);
    let pt = d.doc.para_text(&para)?;
    let selected: Vec<_> = pt
        .pieces
        .iter()
        .filter(|p| p.t.is_some_and(|t| f.text_elements.contains(&t)))
        .collect();
    // Reuse text.replace's actual fragment distribution, admission and shape diagnostic.
    let (cuts, diag) = if let (Some(first), Some(last)) = (selected.first(), selected.last()) {
        if selected
            .iter()
            .any(|p| !matches!(p.kind, PieceKind::Text { .. } | PieceKind::Ref { .. }))
        {
            return Err(EngineError::new(
                Code::UnsupportedContentInRange,
                Stage::Validate,
                "unsupported field fragment",
            )
            .with_reason("field_contains_control_or_opaque"));
        }
        crate::text::plan_replace(
            d,
            &para,
            &pt,
            first.cp_start..last.cp_start + last.cp_len,
            text,
            false,
        )?
    } else if f.text_elements.is_empty() {
        let (run, at) = f.insertion.expect("admitted insertion site");
        let q = &x.el(run).qname;
        let prefix = q
            .rsplit_once(':')
            .map_or(String::new(), |(p, _)| format!("{p}:"));
        (
            vec![(
                at..at,
                format!("<{prefix}t>{}</{prefix}t>", escape_text(text)).into_bytes(),
            )],
            None,
        )
    } else {
        let t = x.el(f.text_elements[0]);
        let cut = if t.end.is_some() {
            (t.inner(), escape_text(text).into_bytes())
        } else {
            let tag = std::str::from_utf8(x.raw(t.start.clone())).unwrap();
            (
                t.start.clone(),
                format!(
                    "{}>{}</{}>",
                    tag.trim_end_matches('>').trim_end_matches('/').trim_end(),
                    escape_text(text),
                    t.qname
                )
                .into_bytes(),
            )
        };
        (vec![cut], None)
    };
    // An empty first carrier still owns the replacement's original character shape.
    let mut cuts = cuts;
    if let Some(&first) = f.text_elements.first() {
        if x.el(first).items.is_empty() && selected.first().is_some_and(|p| p.t != Some(first)) {
            for (_, v) in &mut cuts {
                v.clear();
            }
            let e = x.el(first);
            cuts.push(if e.end.is_some() {
                (e.inner(), escape_text(text).into_bytes())
            } else {
                let tag = std::str::from_utf8(x.raw(e.start.clone())).unwrap();
                (
                    e.start.clone(),
                    format!(
                        "{}>{}</{}>",
                        tag.trim_end_matches('>').trim_end_matches('/').trim_end(),
                        escape_text(text),
                        e.qname
                    )
                    .into_bytes(),
                )
            });
        }
    }
    let path = x.path(para.el);
    d.splice_entry(&para.entry, cuts)?;
    let next = d.doc.require(target, Some(Kind::Paragraph))?.clone();
    let new = d.doc.para_text(&next)?.text;
    d.anchor(&next);
    d.prv.push((pt.text, new));
    if let Some(diag) = diag {
        d.diags.push(diag);
    }
    d.changed.push(target.into());
    d.intents.push(Intent::FieldSetText {
        entry: para.entry,
        path,
        text: text.into(),
        name: name.map(str::to_string),
        fieldid: fieldid.map(str::to_string),
        occurrence,
    });
    Ok(())
}
