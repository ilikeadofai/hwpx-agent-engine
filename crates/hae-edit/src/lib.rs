//! hae-edit: The four supported edits, lineseg cache policy and PrvText update.
//!
//! Edits are applied in order to a candidate [`Document`]; each step is a set
//! of byte splices followed by [`Document::rebuild`] (the commit gate). Cache
//! invalidation and PrvText run once at the end of a batch (spec §8, §11).

mod format;
mod image;
mod text;

use hae_diag::{Code, Diagnostic, EngineError, Result, Stage};
use hae_model::{Document, Kind, Node};
use hae_xml::{ParseOptions, XmlDoc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

pub use format::{Alignment, LineSpacing};
pub use image::ImageBytes;

/// One edit command (spec §6). Ranges are grapheme-cluster indices.
#[derive(Debug, Clone)]
pub enum EditOp {
    TextReplace {
        target: String,
        range: Range<usize>,
        text: String,
    },
    ParagraphFormat {
        target: String,
        alignment: Option<Alignment>,
        line_spacing: Option<LineSpacing>,
    },
    CellSetText {
        target: String,
        text: String,
    },
    ImageReplace {
        target: String,
        image: ImageBytes,
    },
}

impl EditOp {
    pub fn target(&self) -> &str {
        match self {
            EditOp::TextReplace { target, .. }
            | EditOp::ParagraphFormat { target, .. }
            | EditOp::CellSetText { target, .. }
            | EditOp::ImageReplace { target, .. } => target,
        }
    }
}

/// Structural description of an applied edit, independent of nodeIds.
/// hae-verify and the test oracle consume this (spec §10).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Intent {
    #[serde(rename = "text.replace")]
    TextReplace {
        entry: String,
        path: Vec<usize>,
        #[serde(rename = "expectedText")]
        expected_text: String,
    },
    #[serde(rename = "cell.setText")]
    CellSetText {
        entry: String,
        path: Vec<usize>,
        #[serde(rename = "expectedText")]
        expected_text: String,
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
}

/// Work-in-progress state while applying a batch.
pub(crate) struct Draft {
    pub doc: Document,
    pub changed: Vec<String>,
    pub resources: Vec<ResourceChange>,
    pub diags: Vec<Diagnostic>,
    pub intents: Vec<Intent>,
    /// Section entry → (lowest top-level paragraph ordinal to clear, extra nested paragraph paths)
    pub anchors: BTreeMap<String, (usize, Vec<Vec<usize>>)>,
    /// (old display text, new display text) of edited paragraphs for PrvText.
    pub prv: Vec<(String, String)>,
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

    pub fn node(&self, id: &str, kind: Option<Kind>) -> Result<Node> {
        let n = self.doc.require(id, kind)?.clone();
        self.doc.check_editable(&n)?;
        Ok(n)
    }

    /// Record a lineseg anchor for an edited paragraph (spec §11 per-command rule).
    pub fn anchor(&mut self, para: &Node) {
        let doc: &Arc<XmlDoc> = self.doc.doc_of(para);
        let root = doc.root();
        let path = doc.path(para.el);
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
            e.1.push(path);
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
    };
    for (i, op) in ops.iter().enumerate() {
        let r = match op {
            EditOp::TextReplace {
                target,
                range,
                text,
            } => text::text_replace(&mut d, target, range.clone(), text),
            EditOp::CellSetText { target, text } => text::cell_set_text(&mut d, target, text),
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
            diagnostics: Vec::new(),
            intents: Vec::new(),
            stale: Staleness::default(),
            cache_removals: BTreeMap::new(),
        });
    }
    let cache_removals = clear_caches(&mut d)?;
    let stale = update_previews(&mut d, &cache_removals)?;
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
        for path in nested {
            if let Some(p) = doc.iter().map(|(i, _)| i).find(|&i| doc.path(i) == path) {
                cuts.extend(lsa_of(p));
            }
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
