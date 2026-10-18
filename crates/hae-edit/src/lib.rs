//! hae-edit: Bounded edits, lineseg cache policy and PrvText update.
//!
//! Edits are applied in order to a candidate [`Document`]; each step is a set
//! of byte splices followed by [`Document::rebuild`] (the commit gate). Cache
//! invalidation and PrvText run once at the end of a batch (spec §8, §11).

mod char_format;
mod equation;
mod fields;
mod fit;
mod form_fill;
mod format;
mod image;
mod narrow;
mod replace_all;
mod section;
mod table;
mod table_structure;
mod text;

use hae_diag::{Code, Diagnostic, EngineError, Result, Stage};
use hae_model::{Document, Kind, Node, StructureDelta, TableRemap};
use hae_xml::{ParseOptions, XmlDoc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

pub use char_format::CharProps;
pub use equation::EquationProps;
pub use fit::FitMode;
pub use form_fill::{
    FilledCell, FormFillIntent, FormFillReport, SkippedFill, SKIP_AMBIGUOUS_LABEL,
    SKIP_AMBIGUOUS_TARGET, SKIP_CONFLICTING_TARGET, SKIP_LABEL_NOT_FOUND, SKIP_NO_CELL_GEOMETRY,
    SKIP_NO_TARGET_CELL, SKIP_TARGET_NOT_EMPTY,
};
pub use format::{Alignment, LineSpacing};
pub use image::ImageBytes;
pub use replace_all::{
    GraphemeRange, MatchPreview, ReplaceAllReport, SkippedGroup, SkippedLocation,
    DEFAULT_MAX_MATCHES, HARD_MAX_MATCHES,
};
pub use section::{resolve_section, Margins, PageNumbering, PageSettings, Visibility};
pub use table::{CellFormat, CellMargin};

/// One edit command (spec §6). Ranges are grapheme-cluster indices.
#[derive(Debug, Clone)]
pub enum EditOp {
    TableSetColumnWidths {
        target: String,
        widths: Option<Vec<u32>>,
        width: Option<u32>,
    },
    TableSetRowHeights {
        target: String,
        heights: Option<Vec<u32>>,
        height: Option<u32>,
    },
    /// Insert `count` (default 1) empty rows so the first new row gets address `index`.
    TableInsertRow {
        target: String,
        index: usize,
        count: Option<usize>,
    },
    /// Delete `count` (default 1) rows starting at address `index`.
    TableDeleteRow {
        target: String,
        index: usize,
        count: Option<usize>,
    },
    TableInsertColumn {
        target: String,
        index: usize,
        count: Option<usize>,
    },
    TableDeleteColumn {
        target: String,
        index: usize,
        count: Option<usize>,
    },
    CellSetFormat {
        target: String,
        format: CellFormat,
    },
    CellSetSize {
        target: String,
        width: Option<u32>,
        height: Option<u32>,
    },
    EquationSetScript {
        target: String,
        script: String,
    },
    EquationSetProperties {
        target: String,
        props: EquationProps,
    },
    ParagraphSetStyle {
        targets: Vec<String>,
        style_id: u32,
    },
    ImageFit {
        target: String,
        mode: FitMode,
    },
    HeaderSetText {
        scope: Option<String>,
        kind: String,
        paragraph_index: usize,
        text: String,
    },
    SectionSetPage {
        target: String,
        settings: PageSettings,
    },
    SectionSetPageNumbering {
        target: String,
        settings: PageNumbering,
    },
    SectionSetVisibility {
        target: String,
        settings: Visibility,
    },

    FieldSetText {
        target: String,
        text: String,
        name: Option<String>,
        fieldid: Option<String>,
        occurrence: Option<usize>,
    },
    TextFormat {
        target: String,
        range: Range<usize>,
        props: CharProps,
        preserve: bool,
    },
    TextReplace {
        target: String,
        range: Range<usize>,
        text: String,
        preserve: bool,
    },
    /// Exact-match replacement over the document or a `scope` subtree.
    TextReplaceAll {
        find: String,
        text: String,
        scope: Option<String>,
        max_matches: usize,
    },
    ParagraphFormat {
        target: String,
        alignment: Option<Alignment>,
        line_spacing: Option<LineSpacing>,
    },
    CellSetText {
        target: String,
        text: String,
        preserve: bool,
    },
    CellSetParagraphs {
        target: String,
        paragraphs: Vec<String>,
        preserve: bool,
    },
    /// Label-based form fill over tables (0.3 TB-07).
    FormFill {
        values: BTreeMap<String, String>,
        scope: Option<String>,
        strict: bool,
    },
    ImageReplace {
        target: String,
        image: ImageBytes,
    },
}

impl EditOp {
    pub fn target(&self) -> &str {
        match self {
            EditOp::ParagraphSetStyle { targets, .. } => targets.first().map_or("", String::as_str),
            EditOp::HeaderSetText { scope, .. } => scope.as_deref().unwrap_or(""),
            EditOp::TableSetColumnWidths { target, .. }
            | EditOp::TableSetRowHeights { target, .. }
            | EditOp::TableInsertRow { target, .. }
            | EditOp::TableDeleteRow { target, .. }
            | EditOp::TableInsertColumn { target, .. }
            | EditOp::TableDeleteColumn { target, .. }
            | EditOp::CellSetFormat { target, .. }
            | EditOp::CellSetSize { target, .. }
            | EditOp::EquationSetScript { target, .. }
            | EditOp::EquationSetProperties { target, .. }
            | EditOp::ImageFit { target, .. }
            | EditOp::SectionSetPage { target, .. }
            | EditOp::SectionSetPageNumbering { target, .. }
            | EditOp::SectionSetVisibility { target, .. }
            | EditOp::FieldSetText { target, .. }
            | EditOp::TextFormat { target, .. }
            | EditOp::TextReplace { target, .. }
            | EditOp::ParagraphFormat { target, .. }
            | EditOp::CellSetText { target, .. }
            | EditOp::CellSetParagraphs { target, .. }
            | EditOp::ImageReplace { target, .. } => target,
            EditOp::TextReplaceAll { scope, .. } | EditOp::FormFill { scope, .. } => {
                scope.as_deref().unwrap_or("")
            }
        }
    }
}

/// Structural description of an applied edit, independent of nodeIds.
/// hae-verify and the test oracle consume this (spec §10).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Intent {
    #[serde(rename = "table.geometry")]
    TableGeometry {
        command: String,
        entry: String,
        path: Vec<usize>,
        widths: Vec<u32>,
        heights: Vec<u32>,
        #[serde(rename = "tableSize")]
        table_size: (u32, u32),
        #[serde(rename = "changedCells")]
        changed_cells: Vec<String>,
    },
    #[serde(rename = "table.structure")]
    TableStructure {
        command: String,
        entry: String,
        path: Vec<usize>,
        index: usize,
        count: usize,
    },
    #[serde(rename = "cell.setFormat")]
    CellFormat {
        entry: String,
        path: Vec<usize>,
        format: CellFormat,
    },
    #[serde(rename = "equation.setScript")]
    EquationSetScript {
        entry: String,
        path: Vec<usize>,
        script: String,
    },
    #[serde(rename = "equation.setProperties")]
    EquationSetProperties {
        entry: String,
        path: Vec<usize>,
        props: EquationProps,
    },
    #[serde(rename = "paragraph.setStyle")]
    ParagraphSetStyle {
        entry: String,
        path: Vec<usize>,
        #[serde(rename = "styleId")]
        style_id: u32,
    },
    #[serde(rename = "image.fit")]
    ImageFit {
        entry: String,
        path: Vec<usize>,
        mode: FitMode,
    },
    #[serde(rename = "header.setText")]
    HeaderSetText {
        entry: String,
        kind: String,
        #[serde(rename = "paragraphIndex")]
        paragraph_index: usize,
    },
    Section {
        command: String,
        entry: String,
        path: Vec<usize>,
        request: serde_json::Value,
    },
    #[serde(rename = "field.setText")]
    FieldSetText {
        entry: String,
        path: Vec<usize>,
        text: String,
        name: Option<String>,
        fieldid: Option<String>,
        occurrence: Option<usize>,
    },
    #[serde(rename = "text.format")]
    TextFormat {
        entry: String,
        path: Vec<usize>,
        range: Range<usize>,
        props: CharProps,
    },
    #[serde(rename = "text.replace")]
    TextReplace {
        entry: String,
        path: Vec<usize>,
        #[serde(rename = "expectedText")]
        expected_text: String,
    },
    #[serde(rename = "text.replaceAll")]
    TextReplaceAll {
        find: String,
        replacement: String,
        #[serde(rename = "replacementCount")]
        replacement_count: usize,
        #[serde(rename = "paragraphCount")]
        paragraph_count: usize,
    },
    #[serde(rename = "cell.setText")]
    CellSetText {
        entry: String,
        path: Vec<usize>,
        #[serde(rename = "expectedText")]
        expected_text: String,
    },
    #[serde(rename = "cell.setText")]
    CellSetParagraphs {
        entry: String,
        path: Vec<usize>,
        #[serde(rename = "expectedParagraphs")]
        expected_paragraphs: Vec<String>,
    },
    #[serde(rename = "paragraph.format")]
    ParagraphFormat {
        entry: String,
        path: Vec<usize>,
        alignment: Option<String>,
        #[serde(rename = "lineSpacingPercent")]
        line_spacing_percent: Option<u32>,
    },
    #[serde(rename = "image.replace")]
    ImageReplace {
        entry: String,
        path: Vec<usize>,
        #[serde(rename = "imageSha256")]
        image_sha256: String,
    },
    /// One entry per `form.fill` step: every label → value applied to a cell.
    #[serde(rename = "form.fill")]
    FormFill { fills: Vec<FormFillIntent> },
}

/// A node created by a table structure edit. Cells carry their new address.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreatedNode {
    pub node_id: String,
    pub kind: &'static str,
    pub row: Option<usize>,
    pub col: Option<usize>,
}

/// What one table structure step created, removed and moved.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StructureReport {
    pub command: String,
    pub table: String,
    pub index: usize,
    pub count: usize,
    pub rows_before: usize,
    pub rows_after: usize,
    pub columns_before: usize,
    pub columns_after: usize,
    pub created: Vec<CreatedNode>,
    pub removed: Vec<String>,
    /// Surviving cells whose address moved.
    pub shifted: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceChange {
    pub kind: &'static str,
    pub resource_id: String,
    pub action: &'static str,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Staleness {
    pub layout_sections: Vec<String>,
    pub prv_text: bool,
    pub thumbnail: bool,
}

/// Result of applying a batch to a candidate document.
#[derive(Debug, Clone)]
pub struct Applied {
    pub doc: Document,
    pub changed_ids: Vec<String>,
    pub resource_changes: Vec<ResourceChange>,
    pub diagnostics: Vec<Diagnostic>,
    pub intents: Vec<Intent>,
    pub stale: Staleness,
    /// Lineseg arrays removed per section entry (separate from body changes, spec §11).
    pub cache_removals: BTreeMap<String, usize>,
    /// One report per `text.replaceAll` step, in step order.
    pub replace_all: Vec<ReplaceAllReport>,
    /// One report per `form.fill` step, in step order.
    pub form_fill: Vec<FormFillReport>,
    /// One report per table structure step, in step order.
    pub structure: Vec<StructureReport>,
}

/// Work-in-progress state while applying a batch.
pub(crate) struct Draft {
    pub doc: Document,
    pub changed: Vec<String>,
    pub resources: Vec<ResourceChange>,
    pub diags: Vec<Diagnostic>,
    pub intents: Vec<Intent>,
    /// Section entry → (lowest top-level paragraph ordinal to clear, extra nested paragraph paths)
    pub anchors: BTreeMap<String, (usize, Vec<String>)>,
    /// (old display text, new display text) of edited paragraphs for PrvText.
    pub prv: Vec<(String, String)>,
    pub replace_all: Vec<ReplaceAllReport>,
    pub form_fill: Vec<FormFillReport>,
    pub structure: Vec<StructureReport>,
    /// Text vanished without a PrvText line to edit (deleted table cells).
    pub prv_extra: bool,
}

impl Draft {
    pub fn opts(&self) -> ParseOptions {
        ParseOptions {
            max_depth: self.doc.limits.max_xml_depth as usize,
        }
    }

    /// Replace one entry's payload via splices and rebuild the candidate.
    pub fn splice_entry(&mut self, entry: &str, edits: Vec<(Range<usize>, Vec<u8>)>) -> Result<()> {
        if edits.is_empty() {
            return Ok(());
        }
        let doc = self.doc.xml.get(entry).ok_or_else(|| {
            EngineError::new(
                Code::StateInvariantError,
                Stage::Apply,
                format!("no parsed entry {entry}"),
            )
        })?;
        let new = doc.splice(edits, self.opts())?;
        let mut st = self.doc.state.clone();
        st.set(entry, new.bytes().clone());
        let stale = self.doc.stale_layout.clone();
        self.doc = self.doc.rebuild(st, stale)?;
        Ok(())
    }

    /// Splice one entry and run the structure-aware commit gate.
    pub fn splice_structural(
        &mut self,
        entry: &str,
        edits: Vec<(Range<usize>, Vec<u8>)>,
        change: &TableRemap,
    ) -> Result<StructureDelta> {
        let doc = self.doc.xml.get(entry).ok_or_else(|| {
            EngineError::new(
                Code::StateInvariantError,
                Stage::Apply,
                format!("no parsed entry {entry}"),
            )
        })?;
        let new = doc.splice(edits, self.opts())?;
        let mut st = self.doc.state.clone();
        st.set(entry, new.bytes().clone());
        let stale = self.doc.stale_layout.clone();
        let (doc, delta) = self.doc.rebuild_table_structure(st, stale, change)?;
        self.doc = doc;
        Ok(delta)
    }

    pub fn node(&self, id: &str, kind: Option<Kind>) -> Result<Node> {
        let n = self.doc.require(id, kind)?.clone();
        self.doc.check_editable(&n)?;
        Ok(n)
    }

    /// Record a lineseg anchor for an edited paragraph (spec §11 per-command rule).
    pub fn anchor(&mut self, para: &Node) {
        let doc: &Arc<XmlDoc> = self.doc.doc_of(para);
        let root = doc.root();
        // Top-level paragraph: the ancestor-or-self whose parent is the root.
        let mut top = para.el;
        while let Some(p) = doc.el(top).parent {
            if p == root {
                break;
            }
            top = p;
        }
        let ord = doc
            .el(root)
            .children
            .iter()
            .position(|&c| c == top)
            .unwrap_or(0);
        let e = self
            .anchors
            .entry(para.entry.clone())
            .or_insert((usize::MAX, Vec::new()));
        e.0 = e.0.min(ord);
        if top != para.el {
            e.1.push(para.id.clone());
        }
    }
}

/// Apply edits in order to a candidate copy of `doc`. On error nothing of
/// `doc` changes (the candidate is dropped).
pub fn apply(doc: &Document, ops: &[EditOp]) -> Result<Applied> {
    if !doc.permissions.editable() {
        return Err(EngineError::new(
            Code::Unsupported,
            Stage::Validate,
            format!(
                "document is read-only: {}",
                doc.permissions.readonly_reasons.join(", ")
            ),
        )
        .with_reason("document_readonly"));
    }
    let mut d = Draft {
        doc: doc.clone(),
        changed: Vec::new(),
        resources: Vec::new(),
        diags: Vec::new(),
        intents: Vec::new(),
        anchors: BTreeMap::new(),
        prv: Vec::new(),
        replace_all: Vec::new(),
        form_fill: Vec::new(),
        structure: Vec::new(),
        prv_extra: false,
    };
    for (i, op) in ops.iter().enumerate() {
        let r = match op {
            EditOp::TableSetColumnWidths {
                target,
                widths,
                width,
            } => table::tracks(&mut d, target, widths.as_deref(), *width, true),
            EditOp::TableSetRowHeights {
                target,
                heights,
                height,
            } => table::tracks(&mut d, target, heights.as_deref(), *height, false),
            EditOp::TableInsertRow {
                target,
                index,
                count,
            } => table_structure::structure(
                &mut d,
                "table.insertRow",
                target,
                table_structure::Axis::Row,
                table_structure::Mode::Insert,
                *index,
                *count,
            ),
            EditOp::TableDeleteRow {
                target,
                index,
                count,
            } => table_structure::structure(
                &mut d,
                "table.deleteRow",
                target,
                table_structure::Axis::Row,
                table_structure::Mode::Delete,
                *index,
                *count,
            ),
            EditOp::TableInsertColumn {
                target,
                index,
                count,
            } => table_structure::structure(
                &mut d,
                "table.insertColumn",
                target,
                table_structure::Axis::Column,
                table_structure::Mode::Insert,
                *index,
                *count,
            ),
            EditOp::TableDeleteColumn {
                target,
                index,
                count,
            } => table_structure::structure(
                &mut d,
                "table.deleteColumn",
                target,
                table_structure::Axis::Column,
                table_structure::Mode::Delete,
                *index,
                *count,
            ),
            EditOp::CellSetFormat { target, format } => table::cell_format(&mut d, target, format),
            EditOp::CellSetSize {
                target,
                width,
                height,
            } => table::cell_size(&mut d, target, *width, *height),
            EditOp::EquationSetScript { target, script } => {
                equation::set(&mut d, target, Some(script), None)
            }
            EditOp::EquationSetProperties { target, props } => {
                equation::set(&mut d, target, None, Some(props))
            }
            EditOp::ParagraphSetStyle { targets, style_id } => {
                narrow::set_style(&mut d, targets, *style_id)
            }
            EditOp::ImageFit { target, mode } => fit::fit(&mut d, target, *mode),
            EditOp::HeaderSetText {
                scope,
                kind,
                paragraph_index,
                text,
            } => narrow::header_text(&mut d, scope.as_deref(), kind, *paragraph_index, text),
            EditOp::SectionSetPage { target, settings } => {
                section::set_page(&mut d, target, settings)
            }
            EditOp::SectionSetPageNumbering { target, settings } => {
                section::set_numbering(&mut d, target, settings)
            }
            EditOp::SectionSetVisibility { target, settings } => {
                section::set_visibility(&mut d, target, settings)
            }
            EditOp::FieldSetText {
                target,
                text,
                name,
                fieldid,
                occurrence,
            } => fields::set_text(
                &mut d,
                target,
                text,
                name.as_deref(),
                fieldid.as_deref(),
                *occurrence,
            ),
            EditOp::TextFormat {
                target,
                range,
                props,
                preserve,
            } => char_format::text_format(&mut d, target, range.clone(), props, *preserve),
            EditOp::TextReplace {
                target,
                range,
                text,
                preserve,
            } => text::text_replace(&mut d, target, range.clone(), text, *preserve),
            EditOp::TextReplaceAll {
                find,
                text,
                scope,
                max_matches,
            } => replace_all::text_replace_all(&mut d, find, text, scope.as_deref(), *max_matches),
            EditOp::CellSetText {
                target,
                text,
                preserve,
            } => text::cell_set_text(&mut d, target, Some(text), None, *preserve),
            EditOp::CellSetParagraphs {
                target,
                paragraphs,
                preserve,
            } => text::cell_set_text(&mut d, target, None, Some(paragraphs), *preserve),
            EditOp::FormFill {
                values,
                scope,
                strict,
            } => form_fill::form_fill(&mut d, values, scope.as_deref(), *strict),
            EditOp::ParagraphFormat {
                target,
                alignment,
                line_spacing,
            } => format::paragraph_format(&mut d, target, *alignment, *line_spacing),
            EditOp::ImageReplace { target, image } => image::image_replace(&mut d, target, image),
        };
        r.map_err(|mut e| {
            if ops.len() > 1 {
                let mut detail = e.0.detail.take().unwrap_or_else(|| serde_json::json!({}));
                detail["failedStep"] = serde_json::json!(i);
                e.with_detail(detail)
            } else {
                e
            }
        })?;
    }
    if d.doc.state.payload_set_sha256() == doc.state.payload_set_sha256() {
        return Ok(Applied {
            doc: doc.clone(),
            changed_ids: Vec::new(),
            resource_changes: Vec::new(),
            // Only text.replaceAll and form.fill can report (skipped matches
            // or labels) without a change.
            diagnostics: d.diags,
            intents: Vec::new(),
            stale: Staleness::default(),
            cache_removals: BTreeMap::new(),
            replace_all: d.replace_all,
            form_fill: d.form_fill,
            structure: Vec::new(),
        });
    }
    let prv_extra = d.prv_extra;
    let cache_removals = clear_caches(&mut d)?;
    let mut stale = update_previews(&mut d, &cache_removals)?;
    if prv_extra && d.doc.info.prv_text.is_some() {
        stale.prv_text = true;
    }
    let mut changed = d.changed.clone();
    changed.dedup();
    Ok(Applied {
        doc: d.doc,
        changed_ids: changed,
        resource_changes: d.resources,
        diagnostics: d.diags,
        intents: d.intents,
        stale,
        cache_removals,
        replace_all: d.replace_all,
        form_fill: d.form_fill,
        structure: d.structure,
    })
}

/// LinesegPolicy (spec §11): remove `hp:linesegarray` of top-level paragraphs
/// from each section's anchor to its end, plus the edited nested paragraphs'.
fn clear_caches(d: &mut Draft) -> Result<BTreeMap<String, usize>> {
    let mut removed = BTreeMap::new();
    let anchors = std::mem::take(&mut d.anchors);
    for (entry, (from, nested)) in anchors {
        let doc = d.doc.xml[&entry].clone();
        let root = doc.root();
        let mut cuts = Vec::new();
        let lsa_of = |p: usize| -> Vec<Range<usize>> {
            doc.el(p)
                .children
                .iter()
                .filter(|&&c| {
                    hae_model::is_hp(doc.el(c).ns.as_deref()) && doc.el(c).local == "linesegarray"
                })
                .map(|&c| doc.el(c).outer())
                .collect()
        };
        for (ord, &c) in doc.el(root).children.iter().enumerate() {
            let el = doc.el(c);
            if ord >= from && hae_model::is_hp(el.ns.as_deref()) && el.local == "p" {
                cuts.extend(lsa_of(c));
            }
        }
        for id in nested {
            let p = d.doc.require(&id, Some(Kind::Paragraph))?;
            cuts.extend(lsa_of(p.el));
        }
        cuts.sort_by_key(|r| r.start);
        cuts.dedup();
        let n = cuts.len();
        if n > 0 {
            d.splice_entry(&entry, cuts.into_iter().map(|r| (r, Vec::new())).collect())?;
        }
        removed.insert(entry.clone(), n);
        if !d.doc.stale_layout.contains(&entry) {
            d.doc.stale_layout.push(entry);
        }
    }
    Ok(removed)
}

/// PrvText: replace each edited paragraph's old text when it occurs exactly
/// once; otherwise report PrvText as stale. Thumbnails are never regenerated.
fn update_previews(d: &mut Draft, removals: &BTreeMap<String, usize>) -> Result<Staleness> {
    let mut stale = Staleness {
        layout_sections: removals.keys().cloned().collect(),
        prv_text: false,
        thumbnail: false,
    };
    let edits = std::mem::take(&mut d.prv);
    let touched = !d.intents.is_empty();
    if let Some(name) = d.doc.info.prv_text.clone() {
        let cur = d
            .doc
            .payload(&name)
            .cloned()
            .unwrap_or_else(|| Arc::from(Vec::new()));
        match std::str::from_utf8(&cur) {
            Ok(s) => {
                let mut s = s.to_string();
                for (old, new) in &edits {
                    let old_line = old.replace(['\t', '\n'], " ");
                    let new_line = new.replace(['\t', '\n'], " ");
                    if old_line == new_line {
                        continue;
                    }
                    if !old_line.trim().is_empty() && s.matches(old_line.as_str()).count() == 1 {
                        s = s.replacen(old_line.as_str(), &new_line, 1);
                    } else {
                        stale.prv_text = true;
                    }
                }
                if s.as_bytes() != &cur[..] {
                    let mut st = d.doc.state.clone();
                    st.set(&name, Arc::from(s.into_bytes()));
                    let sl = d.doc.stale_layout.clone();
                    d.doc = d.doc.rebuild(st, sl)?;
                }
            }
            Err(_) => stale.prv_text = touched,
        }
    }
    stale.thumbnail = touched && d.doc.info.prv_image.is_some();
    Ok(stale)
}
