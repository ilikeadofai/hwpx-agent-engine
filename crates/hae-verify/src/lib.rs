//! Independent, source-preserving save gate (design §4, §6, §10, §11).
//!
//! Capture [`VerifyOperation`] from the input requests before editing. Pass
//! the pre-edit package and the bytes of the temporary output to
//! [`verify_candidate`]. This crate never consumes model/editor outcomes,
//! expectedText, or editor-generated allowed-change lists. It derives text
//! splices, style clones, cache removals and preview changes independently and
//! requires exact payload equality everywhere else. Opaque data thus retains
//! both its original spelling and payload hash, including inside cloned styles.
//!
//! [`verify_archive`] additionally enforces entry methods and raw compressed
//! bytes/local headers for unchanged entries. Use that entry point whenever
//! the pre-edit archive is available. Neither function publishes a file.
//!
//! Bounded support: text ranges may span plain hp:t text, references and runs;
//! touched CDATA, comments/PIs, controls, unknown hp:t attributes and ambiguous
//! cell structures fail closed. Empty cells require one paragraph and a first
//! run. Styles must have unique numeric IDs and existing requested fields;
//! cloning preserves every other byte and reuses a byte-identical definition
//! when available. Image replacement requires a simple picture with one
//! hc:img; clipping/effects and repeated replacement of the same picture in a
//! batch are unsupported. New media may use arbitrary fresh manifest IDs and
//! safe BinData names; only a single validated item token may be appended.
//! Image decoding (including compressed pixel validity and PNG CRCs) is the
//! caller's input-validation responsibility; the gate checks bounded static
//! PNG/JPEG structure, dimensions, and the exact frozen request bytes.
//!
//! Known UTF-8 Preview/PrvText.txt: uniquely occurring old paragraph text is
//! replaced in request order (tabs/newlines become spaces). Ambiguous, empty
//! or non-UTF-8 previews stay unchanged and are reported stale. No other
//! preview entry may change. Thumbnails remain byte-identical and are stale
//! after a material edit. PrvText is not regenerated from a full-document
//! extraction, since its producer's serialization is not known.
mod char_format;
mod contract;
mod equation;
mod fit;
mod form_fill;
mod format;
mod image;
mod narrow;
mod section;
mod table;
mod text;

pub use contract::{
    Alignment, CellFormat, CellMargin, CharProps, EquationProps, FitMode, LineSpacing,
    LineSpacingKind, Margins, PageNumbering, PageSettings, VerifyOperation, VerifyReport,
    VerifyScope, Visibility,
};
use hae_diag::{Budget, Code, EngineError, Limits, Result, Stage};
use hae_package::{PackageInfo, PackageState, XmlMap, NS_HC, NS_HH, NS_HP, NS_HP10, NS_HS};
use hae_xml::{Element, ParseOptions, XmlDoc};
use hae_zip::Archive;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

type Splice = (Range<usize>, Vec<u8>);
fn carrier(doc: &XmlDoc, i: usize) -> bool {
    hp(doc.el(i), "run") || hp(doc.el(i), "t")
}
fn logical_children(doc: &XmlDoc, i: usize) -> Vec<usize> {
    doc.el(i)
        .children
        .iter()
        .flat_map(|&c| {
            if carrier(doc, c) {
                logical_children(doc, c)
            } else {
                vec![c]
            }
        })
        .collect()
}
fn stable_path(doc: &XmlDoc, mut i: usize) -> Vec<usize> {
    let mut result = Vec::new();
    while let Some(mut parent) = doc.el(i).parent {
        while carrier(doc, parent) {
            parent = doc.el(parent).parent.unwrap();
        }
        result.push(
            logical_children(doc, parent)
                .iter()
                .position(|&c| c == i)
                .unwrap(),
        );
        i = parent;
    }
    result.reverse();
    result
}
fn stable_at(doc: &XmlDoc, path: &[usize]) -> Result<usize> {
    path.iter().try_fold(doc.root(), |i, &n| {
        logical_children(doc, i)
            .get(n)
            .copied()
            .ok_or_else(|| fail("stable target path missing"))
    })
}
type Anchors = BTreeMap<String, (usize, BTreeSet<Vec<usize>>)>;

pub(crate) fn fail(message: impl Into<String>) -> EngineError {
    EngineError::new(Code::VerifyFailed, Stage::Save, message)
}
pub(crate) fn unsupported(message: impl Into<String>) -> EngineError {
    fail(message).with_reason("unsupported_verification")
}
pub(crate) fn hp(e: &Element, name: &str) -> bool {
    e.local == name && matches!(e.ns.as_deref(), Some(NS_HP) | Some(NS_HP10))
}
pub(crate) fn at(doc: &XmlDoc, path: &[usize]) -> Result<usize> {
    let mut i = doc.root();
    for &n in path {
        i = *doc
            .el(i)
            .children
            .get(n)
            .ok_or_else(|| fail("target structural path does not exist"))?;
    }
    Ok(i)
}
pub(crate) fn attr<'a>(doc: &'a XmlDoc, i: usize, name: &str) -> Result<&'a hae_xml::Attr> {
    doc.el(i)
        .attr(name)
        .ok_or_else(|| unsupported(format!("{} lacks {name}", doc.el(i).qname)))
}
pub(crate) fn parse(state: &PackageState, entry: &str, limits: &Limits) -> Result<XmlDoc> {
    XmlDoc::parse_with(
        state
            .get(entry)
            .ok_or_else(|| fail(format!("missing entry {entry}")))?
            .clone(),
        ParseOptions {
            max_depth: limits.max_xml_depth as usize,
        },
    )
}
// This assembler is deliberately local: no editor/model splice planner is used.
pub(crate) fn patched(bytes: &[u8], mut cuts: Vec<Splice>) -> Result<Vec<u8>> {
    cuts.sort_by_key(|(r, _)| (r.start, r.end));
    let mut out = Vec::new();
    let mut pos = 0;
    for (r, rep) in cuts {
        if r.start < pos || r.start > r.end || r.end > bytes.len() {
            return Err(fail(
                "overlapping or invalid independently calculated changes",
            ));
        }
        out.extend_from_slice(&bytes[pos..r.start]);
        out.extend(rep);
        pos = r.end;
    }
    out.extend_from_slice(&bytes[pos..]);
    Ok(out)
}
pub(crate) fn change(
    state: &mut PackageState,
    entry: &str,
    doc: &XmlDoc,
    cuts: Vec<Splice>,
) -> Result<()> {
    let bytes = patched(doc.bytes(), cuts)?;
    if !state.set(entry, Arc::from(bytes)) {
        return Err(fail("missing changed entry"));
    }
    Ok(())
}

/// Re-open the temporary candidate and independently derive allowed changes.
/// `original` must be the trusted PRE-EDIT state, not the edited state.
/// For a batch, ranges refer to the independently derived pre-step text.
/// The gate checks cumulative original-payload + candidate inflate bytes.
pub fn verify_candidate(
    original: &PackageState,
    candidate_bytes: &[u8],
    requests: &[VerifyOperation],
    limits: &Limits,
) -> Result<VerifyReport> {
    verify_inner(original, candidate_bytes, requests, limits).map_err(|e| {
        if e.code() == Code::VerifyFailed {
            e
        } else {
            fail(format!("candidate or request rejected: {e}"))
                .with_reason(e.reason().unwrap_or("invalid_candidate"))
        }
    })
}

/// As [`verify_candidate`], with the ZIP preservation contract as well.
pub fn verify_archive(
    original: &Archive,
    candidate_bytes: &[u8],
    requests: &[VerifyOperation],
    limits: &Limits,
) -> Result<VerifyReport> {
    let state = PackageState::from_archive(original);
    let report = verify_candidate(&state, candidate_bytes, requests, limits)?;
    let candidate =
        Archive::open(candidate_bytes.to_vec(), limits).map_err(|e| fail(e.to_string()))?;
    for old in original.entries() {
        let new = candidate
            .get(&old.name)
            .ok_or_else(|| fail("original ZIP entry deleted"))?;
        if old.method != new.method {
            return Err(fail(format!("ZIP method changed: {}", old.name)));
        }
        if old.payload() == new.payload()
            && (original.raw_data(&old.name) != candidate.raw_data(&old.name)
                || original.raw_local_header(&old.name) != candidate.raw_local_header(&old.name))
        {
            return Err(fail(format!(
                "unchanged ZIP entry was re-encoded: {}",
                old.name
            )));
        }
    }
    Ok(report)
}

fn verify_inner(
    original: &PackageState,
    bytes: &[u8],
    requests: &[VerifyOperation],
    limits: &Limits,
) -> Result<VerifyReport> {
    if original.entries.len() as u64 > limits.max_entries
        || original.total_bytes() > limits.max_session_memory
    {
        return Err(fail("original state exceeds resource limits"));
    }
    let mut names = BTreeSet::new();
    for e in &original.entries {
        if !names.insert(e.name.to_lowercase()) {
            return Err(fail("duplicate original entry"));
        }
    }
    let mut budget = Budget::new(limits.max_inflate_total);
    budget.try_reserve(original.total_bytes(), "original package payloads")?;
    let archive = Archive::open_with_budget(bytes.to_vec(), limits, &mut budget)?;
    let candidate = PackageState::from_archive(&archive);
    let (info, xml) = hae_package::analyze(original, true, limits)?;
    let (candidate_info, candidate_xml) = hae_package::analyze(
        &candidate,
        archive.entries().first().is_some_and(|e| e.is_stored()),
        limits,
    )?;
    if !requests.is_empty() {
        permissions(&info, &xml)?;
    }
    let mut expected = original.clone();
    let mut anchors = Anchors::new();
    let mut preview = Vec::new();
    let mut material = false;
    let mut seen_images = BTreeSet::new();
    for op in requests {
        if let VerifyOperation::ParagraphSetStyle { targets, style_id } = op {
            if targets.is_empty() {
                return Err(fail("empty style targets"));
            }
            for t in targets {
                if !info.sections.contains(&t.entry) {
                    return Err(fail("style target not in a section"));
                }
                let doc = parse(&expected, &t.entry, limits)?;
                let source = at(&xml[&t.entry], &t.path)?;
                let p = stable_at(&doc, &stable_path(&xml[&t.entry], source))?;
                check_opaque_refs(&xml, &t.entry, &doc, p)?;
                if narrow::style(&mut expected, &info, &t.entry, &doc, p, *style_id, limits)? {
                    anchor(&mut anchors, &t.entry, &doc, p)?;
                    material = true;
                }
            }
            if expected.total_bytes() > limits.max_session_memory {
                return Err(fail("derived package exceeds memory limit"));
            }
            continue;
        }
        if let VerifyOperation::TextReplaceAll {
            find,
            replacement,
            scope,
            max_matches,
        } = op
        {
            let sections: Vec<String> = match scope {
                Some(s) => vec![s.entry.clone()],
                None => info.sections.clone(),
            };
            let mut total = 0usize;
            for entry in sections {
                if !info.sections.contains(&entry) {
                    return Err(unsupported("targets must belong to a spine section"));
                }
                let doc = parse(&expected, &entry, limits)?;
                let scope_el = match scope {
                    Some(s) => {
                        let original_target = at(&xml[&entry], &s.path)?;
                        Some(stable_at(
                            &doc,
                            &stable_path(&xml[&entry], original_target),
                        )?)
                    }
                    None => None,
                };
                for (para, old, new) in text::replace_all(
                    &mut expected,
                    &entry,
                    &doc,
                    scope_el,
                    find,
                    replacement,
                    *max_matches,
                    &mut total,
                )? {
                    anchor(&mut anchors, &entry, &doc, para)?;
                    preview.push((old, new));
                    material = true;
                }
            }
            if expected.total_bytes() > limits.max_session_memory {
                return Err(fail("derived package exceeds memory limit"));
            }
            continue;
        }
        if let VerifyOperation::FormFill {
            values,
            scope,
            strict,
        } = op
        {
            if form_fill::check(
                &mut expected,
                &info,
                &xml,
                values,
                scope,
                *strict,
                limits,
                &mut anchors,
                &mut preview,
            )? {
                material = true;
            }
            continue;
        }
        let (entry, path) = op.target();
        if !info.sections.iter().any(|s| s == entry) {
            return Err(unsupported("targets must belong to a spine section"));
        }
        let doc = parse(&expected, entry, limits)?;
        let original_target = at(&xml[entry], path)?;
        let target = stable_at(&doc, &stable_path(&xml[entry], original_target))?;
        // Header requests name a section for lookup, not as the edited subtree.
        // Check only the independently selected paragraph below.
        if !matches!(op, VerifyOperation::HeaderSetText { .. }) {
            check_opaque_refs(&xml, entry, &doc, target)?;
        }
        match op {
            VerifyOperation::TableSetColumnWidths { .. }
            | VerifyOperation::TableSetRowHeights { .. }
            | VerifyOperation::CellSetFormat { .. }
            | VerifyOperation::CellSetSize { .. } => {
                if let Some(table) =
                    table::check(&mut expected, &info, entry, &doc, target, op, limits)?
                {
                    check_opaque_refs(&xml, entry, &doc, table)?;
                    for p in doc.descendants(table).filter(|&i| hp(doc.el(i), "p")) {
                        anchor(&mut anchors, entry, &doc, p)?;
                    }
                    let carrier = doc
                        .ancestors(table)
                        .find(|&i| hp(doc.el(i), "p"))
                        .ok_or_else(|| unsupported("table has no paragraph carrier"))?;
                    anchor(&mut anchors, entry, &doc, carrier)?;
                }
            }
            VerifyOperation::EquationSetScript { script, .. } => {
                if equation::check(&mut expected, entry, &doc, target, Some(script), None)? {
                    let para = doc
                        .ancestors(target)
                        .find(|&i| hp(doc.el(i), "p"))
                        .ok_or_else(|| unsupported("equation lacks paragraph"))?;
                    anchor(&mut anchors, entry, &doc, para)?;
                    material = true;
                }
            }
            VerifyOperation::EquationSetProperties { props, .. } => {
                if equation::check(&mut expected, entry, &doc, target, None, Some(props))? {
                    let para = doc
                        .ancestors(target)
                        .find(|&i| hp(doc.el(i), "p"))
                        .ok_or_else(|| unsupported("equation lacks paragraph"))?;
                    anchor(&mut anchors, entry, &doc, para)?;
                    material = true;
                }
            }
            VerifyOperation::ParagraphSetStyle { .. } => unreachable!("handled above"),
            VerifyOperation::HeaderSetText {
                kind,
                paragraph_index,
                text,
                ..
            } => {
                let p = narrow::header(&doc, target, kind, *paragraph_index)?;
                check_opaque_refs(&xml, entry, &doc, p)?;
                let len = text::paragraph_len(&doc, p);
                if len != 0 || !text.is_empty() {
                    if let Some(pair) =
                        text::replace(&mut expected, entry, &doc, p, 0..len, text, false)?
                    {
                        anchor(&mut anchors, entry, &doc, p)?;
                        preview.push(pair);
                        material = true;
                    }
                }
            }
            VerifyOperation::ImageFit { mode, .. } => {
                let (step_info, _) = hae_package::analyze(&expected, true, limits)?;
                if fit::check(
                    &mut expected,
                    &step_info,
                    entry,
                    &doc,
                    target,
                    *mode,
                    limits,
                )? {
                    let para = doc
                        .ancestors(target)
                        .find(|&p| hp(doc.el(p), "p"))
                        .ok_or_else(|| fail("picture lacks containing paragraph"))?;
                    anchor(&mut anchors, entry, &doc, para)?;
                    material = true;
                }
            }
            VerifyOperation::SectionSetPage { .. }
            | VerifyOperation::SectionSetPageNumbering { .. }
            | VerifyOperation::SectionSetVisibility { .. } => {
                if section::check(&mut expected, entry, &doc, target, op)? {
                    let nested: BTreeSet<_> = doc
                        .iter()
                        .filter(|(_, e)| hp(e, "p") && e.parent != Some(doc.root()))
                        .map(|(i, _)| stable_path(&doc, i))
                        .collect();
                    let slot = anchors.entry(entry.into()).or_insert((0, BTreeSet::new()));
                    slot.0 = 0;
                    slot.1.extend(nested);
                    material = true;
                }
            }
            VerifyOperation::TextFormat {
                range,
                props,
                preserve,
                ..
            } => {
                if !hp(doc.el(target), "p") {
                    return Err(fail("text.format target is not paragraph"));
                }
                if char_format::check(
                    &mut expected,
                    &info,
                    entry,
                    &doc,
                    target,
                    range.clone(),
                    props,
                    *preserve,
                    limits,
                )? {
                    anchor(&mut anchors, entry, &doc, target)?;
                    material = true;
                }
            }
            VerifyOperation::TextReplace {
                range,
                replacement,
                preserve,
                ..
            } => {
                if !hp(doc.el(target), "p") {
                    return Err(fail("text target is not a paragraph"));
                }
                if let Some((old, new)) = text::replace(
                    &mut expected,
                    entry,
                    &doc,
                    target,
                    range.clone(),
                    replacement,
                    *preserve,
                )? {
                    anchor(&mut anchors, entry, &doc, target)?;
                    preview.push((old, new));
                    material = true;
                }
            }
            VerifyOperation::TextReplaceAll { .. } => unreachable!("handled before target lookup"),
            VerifyOperation::FormFill { .. } => unreachable!("handled before target lookup"),
            VerifyOperation::CellSetText { .. } | VerifyOperation::CellSetParagraphs { .. } => {
                let (text, paragraphs, preserve) = match op {
                    VerifyOperation::CellSetText { text, preserve, .. } => {
                        (Some(text.as_str()), None, *preserve)
                    }
                    VerifyOperation::CellSetParagraphs {
                        paragraphs,
                        preserve,
                        ..
                    } => (None, Some(paragraphs.as_slice()), *preserve),
                    _ => unreachable!(),
                };
                for (para, changed) in text::cell(
                    &mut expected,
                    entry,
                    &doc,
                    target,
                    text,
                    paragraphs,
                    preserve,
                )? {
                    if let Some(pair) = changed {
                        anchor(&mut anchors, entry, &doc, para)?;
                        preview.push(pair);
                        material = true;
                    }
                }
            }
            VerifyOperation::ParagraphFormat {
                alignment,
                line_spacing,
                ..
            } => {
                if !hp(doc.el(target), "p") {
                    return Err(fail("format target is not a paragraph"));
                }
                if format::check(
                    &mut expected,
                    &info,
                    entry,
                    &doc,
                    target,
                    *alignment,
                    *line_spacing,
                    limits,
                )? {
                    anchor(&mut anchors, entry, &doc, target)?;
                    material = true;
                }
            }
            VerifyOperation::ImageReplace { bytes, .. } => {
                if !seen_images.insert((entry.to_string(), path.to_vec())) {
                    return Err(unsupported("repeated image replacement in a batch"));
                }
                material |= image::check(
                    &mut expected,
                    &info,
                    &candidate,
                    &candidate_info,
                    &candidate_xml,
                    entry,
                    &doc,
                    target,
                    path,
                    bytes,
                    limits,
                )?;
            }
        }
        if expected.total_bytes() > limits.max_session_memory {
            return Err(fail("derived package exceeds memory limit"));
        }
    }
    let mut report = VerifyReport::default();
    for (entry, (from, nested)) in anchors {
        let doc = parse(&expected, &entry, limits)?;
        let mut cuts = Vec::new();
        for (ord, &p) in doc.el(doc.root()).children.iter().enumerate() {
            if ord >= from && hp(doc.el(p), "p") {
                cache_cuts(&doc, p, &mut cuts);
            }
        }
        for path in nested {
            cache_cuts(&doc, stable_at(&doc, &path)?, &mut cuts);
        }
        cuts.sort_by_key(|(r, _)| r.start);
        cuts.dedup_by(|a, b| a.0 == b.0);
        report.cache_removals.insert(entry.clone(), cuts.len());
        change(&mut expected, &entry, &doc, cuts)?;
    }
    if let Some(name) = &info.prv_text {
        let payload = original.get(name).ok_or_else(|| fail("missing PrvText"))?;
        match std::str::from_utf8(payload) {
            Ok(s) => {
                let mut updated = s.to_string();
                for (old, new) in preview {
                    let old = old.replace(['\t', '\n'], " ");
                    let new = new.replace(['\t', '\n'], " ");
                    if old == new {
                        continue;
                    }
                    if !old.trim().is_empty() && updated.matches(&old).count() == 1 {
                        updated = updated.replacen(&old, &new, 1);
                    } else {
                        report.prv_text_stale = true;
                    }
                }
                expected.set(name, Arc::from(updated.into_bytes()));
            }
            Err(_) => report.prv_text_stale = material,
        }
    }
    report.thumbnail_stale = material && info.prv_image.is_some();
    if candidate.entries.len() != expected.entries.len() {
        return Err(fail("candidate has missing or unrequested entries"));
    }
    for (want, got) in expected.entries.iter().zip(&candidate.entries) {
        if want.name != got.name {
            return Err(fail("entry order/name changed outside request"));
        }
        if want.payload != got.payload {
            return Err(fail(format!("unrequested or missing change in {}",want.name)).with_detail(serde_json::json!({"entry":want.name,"expectedSha256":hae_package::sha256_hex(&want.payload),"candidateSha256":hae_package::sha256_hex(&got.payload)})));
        }
        if original.get(&want.name) != Some(&got.payload) {
            report.changed_entries.push(want.name.clone());
        }
    }
    // Relations were parsed independently before comparison; exact expected
    // bytes also constrain all original opaque nodes and media references.
    validate_refs(&info, &xml)?;
    validate_refs(&candidate_info, &candidate_xml)?;
    Ok(report)
}
fn cache_cuts(doc: &XmlDoc, p: usize, cuts: &mut Vec<Splice>) {
    for &c in &doc.el(p).children {
        if hp(doc.el(c), "linesegarray") {
            cuts.push((doc.el(c).outer(), Vec::new()));
        }
    }
}
fn anchor(anchors: &mut Anchors, entry: &str, doc: &XmlDoc, p: usize) -> Result<()> {
    let path = doc.path(p);
    let &top = path
        .first()
        .ok_or_else(|| unsupported("paragraph cannot be section root"))?;
    let top_el = at(doc, &[top])?;
    if !hp(doc.el(top_el), "p") {
        return Err(unsupported("nested target has no top-level paragraph"));
    }
    let slot = anchors
        .entry(entry.into())
        .or_insert((top, BTreeSet::new()));
    slot.0 = slot.0.min(top);
    if path.len() > 1 {
        slot.1.insert(stable_path(doc, p));
    }
    Ok(())
}
fn permissions(info: &PackageInfo, xml: &XmlMap) -> Result<()> {
    if !info.signature_entries.is_empty() {
        return Err(unsupported("signed document is read-only"));
    }
    for doc in xml.values() {
        for (_, e) in doc.iter() {
            if (matches!(e.ns.as_deref(), Some(NS_HP) | Some(NS_HP10))
                && ["insertBegin", "insertEnd", "deleteBegin", "deleteEnd"]
                    .contains(&e.local.as_str()))
                || (e.ns.as_deref() == Some(NS_HH)
                    && ["trackChange", "trackChangeAuthor"].contains(&e.local.as_str()))
            {
                return Err(unsupported("tracked document is read-only"));
            }
        }
    }
    Ok(())
}
fn check_opaque_refs(xml: &XmlMap, entry: &str, doc: &XmlDoc, target: usize) -> Result<()> {
    // Include descendants and ancestors: a reference to a containing table or
    // a descendant run also constrains edits to their content.
    let mut ids = BTreeSet::new();
    for i in std::iter::once(target)
        .chain(doc.ancestors(target))
        .chain(doc.descendants(target))
    {
        if let Some(id) = doc.el(i).attr_value("id") {
            ids.insert(id);
        }
    }
    if ids.is_empty() {
        return Ok(());
    }
    let source = xml.get(entry).ok_or_else(|| fail("missing source XML"))?;
    for (_, e) in source.iter() {
        let known = matches!(
            e.ns.as_deref(),
            Some(NS_HP) | Some(NS_HP10) | Some(NS_HC) | Some(NS_HH) | Some(NS_HS)
        );
        for a in &e.attrs {
            let opaque = !known
                || a.ns.as_deref().is_some_and(|ns| {
                    ![
                        NS_HP,
                        NS_HP10,
                        NS_HC,
                        NS_HH,
                        NS_HS,
                        "http://www.w3.org/2000/xmlns/",
                    ]
                    .contains(&ns)
                });
            if opaque
                && !a.qname.starts_with("xmlns")
                && a.value.split_whitespace().any(|s| ids.contains(s))
            {
                return Err(unsupported(
                    "opaque element/attribute references edited identity",
                ));
            }
        }
    }
    Ok(())
}
fn validate_refs(info: &PackageInfo, xml: &XmlMap) -> Result<()> {
    let Some(header) = info.header.as_ref().and_then(|n| xml.get(n)) else {
        return Ok(());
    };
    for (definition, reference) in [
        ("paraPr", "paraPrIDRef"),
        ("charPr", "charPrIDRef"),
        ("style", "styleIDRef"),
        ("borderFill", "borderFillIDRef"),
    ] {
        let mut ids = BTreeSet::new();
        for (_, e) in header.iter().filter(|(_, e)| e.is(NS_HH, definition)) {
            if let Some(id) = e.attr_value("id") {
                if !ids.insert(id) {
                    return Err(fail(format!("duplicate {definition} resource ID")));
                }
            }
        }
        for name in &info.sections {
            for (_, e) in xml[name].iter() {
                if let Some(r) = e.attr_value(reference) {
                    if !ids.contains(r) {
                        return Err(fail(format!("unresolved {reference}: {r}")));
                    }
                }
            }
        }
    }
    Ok(())
}
