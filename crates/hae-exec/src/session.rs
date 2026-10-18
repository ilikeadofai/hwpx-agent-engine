//! Session executor: envelope validation, writer/sequence table, transactions,
//! batch, dryRun, 100-unit Undo/Redo history, snapshots, and resource budgets.

use crate::registry::{
    command_describe, command_list, BatchStepParam, CellSetFormatParams, CellSetSizeParams,
    CellSetTextParams, CommandBatchParams, CommandDescriptor, DocumentDiffParams,
    DocumentExportParams, DocumentOpenParams, DocumentPreviewParams, EquationSetPropertiesParams,
    EquationSetScriptParams, FormFillParams, HeaderSetTextParams, ImageFitParams,
    ImageReplaceParams, LineSpacingParam, LinkListParams, ParagraphFormatParams,
    ParagraphSetStyleParams, ParsedCommand, ProjectSaveParams, SectionSetPageNumberingParams,
    SectionSetPageParams, SectionSetVisibilityParams, SnapshotCreateParams, SnapshotReleaseParams,
    TableSetColumnWidthsParams, TableSetRowHeightsParams, TextFormatParams, TextReplaceAllParams,
    TextReplaceParams, TrackChangeListParams,
};
use hae_diag::{Code, Diagnostic, EngineError, Limits, Result, Stage};
use hae_edit::{Alignment, Applied, EditOp, ImageBytes, LineSpacing};
use hae_model::{query::Query, Document, Kind};
use hae_package::{sha256_hex, PackageState, NS_HH};
use hae_project::{IoRoots, ProjectBinding};
use hae_render_rhwp::{AdapterConfig, RhwpAdapter};
use hae_verify::{
    verify_archive, verify_candidate, Alignment as VerifyAlignment,
    LineSpacing as VerifyLineSpacing, LineSpacingKind as VerifyLineSpacingKind, VerifyOperation,
    VerifyScope,
};
use hae_zip::Archive;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestEnvelope {
    pub schema_version: u32,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub writer_id: Option<String>,
    #[serde(default)]
    pub request_sequence: Option<u64>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub document_id: Option<String>,
    #[serde(default)]
    pub expected_version: Option<u64>,
    pub command: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub schema_version: u32,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub command: String,
    pub ok: bool,
    #[serde(default)]
    pub replayed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone)]
struct VerifyStepRecord {
    pre_state: PackageState,
    ops: Vec<VerifyOperation>,
}

#[derive(Clone)]
struct CommittedState {
    doc: Document,
    verify_steps: Arc<Vec<VerifyStepRecord>>,
}

#[derive(Clone)]
struct HistoryUnit {
    before: CommittedState,
    after: CommittedState,
    changed_ids: Vec<String>,
    resource_changes: Vec<hae_edit::ResourceChange>,
}

#[derive(Clone)]
struct ExplicitSnapshot {
    doc: Document,
}

#[derive(Default)]
struct WriterRecord {
    last_seq: u64,
    last_hash: Option<String>,
    last_response: Option<ResponseEnvelope>,
}

struct OpenDocSession {
    initial_doc: Document,
    session_baseline: Arc<Archive>,
    current: CommittedState,
    revision: u64,
    binding: Option<ProjectBinding>,
    undo_stack: Vec<HistoryUnit>,
    redo_stack: Vec<HistoryUnit>,
    snapshots: BTreeMap<String, ExplicitSnapshot>,
}

pub struct SessionExecutor {
    session_id: String,
    default_writer_id: String,
    default_next_seq: u64,
    roots: IoRoots,
    limits: Limits,
    writers: HashMap<String, WriterRecord>,
    open_doc: Option<OpenDocSession>,
    /// rhwp preview adapter; `None` keeps `document.preview` at RENDERER_UNAVAILABLE.
    renderer: Option<Arc<RhwpAdapter>>,
}

/// Attach per-step `text.replaceAll` reports to a mutation result.
fn with_replace_all(mut result: Value, reports: &[hae_edit::ReplaceAllReport]) -> Value {
    if !reports.is_empty() {
        result["replaceAll"] = serde_json::to_value(reports).expect("reports serialize");
    }
    result
}

/// Attach per-step `form.fill` reports to a mutation result.
fn with_form_fill(mut result: Value, reports: &[hae_edit::FormFillReport]) -> Value {
    if !reports.is_empty() {
        result["formFill"] = serde_json::to_value(reports).expect("reports serialize");
    }
    result
}

fn canonical_json(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                sorted.insert(k.clone(), canonical_json(&map[k]));
            }
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

fn request_fingerprint(req: &RequestEnvelope) -> String {
    let payload = json!({
        "command": req.command,
        "documentId": req.document_id,
        "expectedVersion": req.expected_version,
        "dryRun": req.dry_run,
        "params": canonical_json(&req.params),
    });
    sha256_hex(&serde_json::to_vec(&payload).unwrap_or_default())
}

fn parse_kind(s: &str) -> Result<Kind> {
    serde_json::from_value(Value::String(s.to_ascii_lowercase())).map_err(|_| {
        EngineError::new(
            Code::InvalidRequest,
            Stage::Request,
            format!("unknown element kind: {s}"),
        )
    })
}

fn parse_alignment(s: &str) -> Result<(Alignment, VerifyAlignment)> {
    let up = s.to_ascii_uppercase();
    let a: Alignment = serde_json::from_value(Value::String(up.clone())).map_err(|_| {
        EngineError::new(
            Code::InvalidRequest,
            Stage::Validate,
            format!("unsupported alignment: {s}"),
        )
    })?;
    let va: VerifyAlignment = serde_json::from_value(Value::String(up)).map_err(|_| {
        EngineError::new(
            Code::InvalidRequest,
            Stage::Validate,
            format!("unsupported alignment: {s}"),
        )
    })?;
    Ok((a, va))
}

fn parse_line_spacing(ls: LineSpacingParam) -> Result<(LineSpacing, VerifyLineSpacing)> {
    if !ls.kind.eq_ignore_ascii_case("percent") {
        return Err(EngineError::new(
            Code::Unsupported,
            Stage::Validate,
            format!("unsupported lineSpacing kind: {}", ls.kind),
        )
        .with_reason("line_spacing_kind"));
    }
    let edit_ls: LineSpacing = serde_json::from_value(json!({
        "kind": "percent",
        "value": ls.value,
    }))
    .map_err(|e| EngineError::new(Code::InvalidRequest, Stage::Validate, e.to_string()))?;
    Ok((
        edit_ls,
        VerifyLineSpacing {
            kind: VerifyLineSpacingKind::Percent,
            value: ls.value,
        },
    ))
}

fn add_arc_bytes(seen: &mut HashSet<(usize, usize)>, total: &mut u64, buf: &Arc<[u8]>) {
    let key = (buf.as_ptr() as usize, buf.len());
    if seen.insert(key) {
        *total = total.saturating_add(buf.len() as u64);
    }
}

/// Account a byte slice by identity. Small text fields are cheap but the
/// `image.replace` payload is not, so counting them keeps the session budget
/// honest without needing a second bookkeeping structure.
fn add_slice_bytes(seen: &mut HashSet<(usize, usize)>, total: &mut u64, buf: &[u8]) {
    if buf.is_empty() {
        return;
    }
    let key = (buf.as_ptr() as usize, buf.len());
    if seen.insert(key) {
        *total = total.saturating_add(buf.len() as u64);
    }
}

fn add_state_bytes(seen: &mut HashSet<(usize, usize)>, total: &mut u64, state: &PackageState) {
    for e in &state.entries {
        add_arc_bytes(seen, total, &e.payload);
    }
}

fn add_op_bytes(seen: &mut HashSet<(usize, usize)>, total: &mut u64, op: &VerifyOperation) {
    match op {
        VerifyOperation::EquationSetScript { entry, script, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            add_slice_bytes(seen, total, script.as_bytes());
        }
        VerifyOperation::EquationSetProperties { entry, props, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            for v in [
                &props.text_color,
                &props.font,
                &props.line_mode,
                &props.version,
            ]
            .into_iter()
            .flatten()
            {
                add_slice_bytes(seen, total, v.as_bytes());
            }
        }
        VerifyOperation::TextReplace {
            entry, replacement, ..
        } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            add_slice_bytes(seen, total, replacement.as_bytes());
        }
        VerifyOperation::TextReplaceAll {
            find,
            replacement,
            scope,
            ..
        } => {
            add_slice_bytes(seen, total, find.as_bytes());
            add_slice_bytes(seen, total, replacement.as_bytes());
            if let Some(s) = scope {
                add_slice_bytes(seen, total, s.entry.as_bytes());
            }
        }
        VerifyOperation::CellSetText { entry, text, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            add_slice_bytes(seen, total, text.as_bytes());
        }
        VerifyOperation::CellSetParagraphs {
            entry, paragraphs, ..
        } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            for p in paragraphs {
                add_slice_bytes(seen, total, p.as_bytes());
            }
        }
        VerifyOperation::TableSetColumnWidths { entry, widths, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            *total = total.saturating_add(widths.as_ref().map_or(0, |v| v.len() as u64 * 4));
        }
        VerifyOperation::TableSetRowHeights { entry, heights, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            *total = total.saturating_add(heights.as_ref().map_or(0, |v| v.len() as u64 * 4));
        }
        VerifyOperation::CellSetSize { entry, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes())
        }
        VerifyOperation::CellSetFormat { entry, format, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            for value in [&format.border_fill_id, &format.background]
                .into_iter()
                .flatten()
            {
                add_slice_bytes(seen, total, value.as_bytes());
            }
        }
        VerifyOperation::FormFill { values, scope, .. } => {
            for (label, value) in values {
                add_slice_bytes(seen, total, label.as_bytes());
                add_slice_bytes(seen, total, value.as_bytes());
            }
            if let Some(s) = scope {
                add_slice_bytes(seen, total, s.entry.as_bytes());
            }
        }
        VerifyOperation::TextFormat { entry, props, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            if let Some(color) = &props.text_color {
                add_slice_bytes(seen, total, color.as_bytes());
            }
        }
        VerifyOperation::ParagraphSetStyle { targets, .. } => {
            for t in targets {
                add_slice_bytes(seen, total, t.entry.as_bytes());
                *total = total.saturating_add((t.path.len() * std::mem::size_of::<usize>()) as u64);
            }
        }
        VerifyOperation::HeaderSetText {
            entry, kind, text, ..
        } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            add_slice_bytes(seen, total, kind.as_bytes());
            add_slice_bytes(seen, total, text.as_bytes());
        }
        VerifyOperation::SectionSetPage {
            entry, settings, ..
        } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            for s in [&settings.landscape, &settings.gutter_type]
                .into_iter()
                .flatten()
            {
                add_slice_bytes(seen, total, s.as_bytes());
            }
        }
        VerifyOperation::SectionSetPageNumbering {
            entry, settings, ..
        } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            if let Some(s) = &settings.page_starts_on {
                add_slice_bytes(seen, total, s.as_bytes());
            }
        }
        VerifyOperation::SectionSetVisibility {
            entry, settings, ..
        } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            for s in [&settings.border, &settings.fill].into_iter().flatten() {
                add_slice_bytes(seen, total, s.as_bytes());
            }
        }
        VerifyOperation::ImageFit { entry, .. }
        | VerifyOperation::ParagraphFormat { entry, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
        }
        VerifyOperation::ImageReplace { entry, bytes, .. } => {
            add_slice_bytes(seen, total, entry.as_bytes());
            add_slice_bytes(seen, total, bytes);
        }
    }
}

/// Verification logs keep the pre-edit package state and the fixed request
/// bytes for every commit that is still reachable from the current state or
/// from an Undo/Redo unit. They were previously left out of the session
/// budget entirely, so a long edit run could retain many XML generations while
/// `memory_used_bytes` stayed flat.
fn add_steps_bytes(
    seen: &mut HashSet<(usize, usize)>,
    total: &mut u64,
    steps: &[VerifyStepRecord],
) {
    for step in steps {
        add_state_bytes(seen, total, &step.pre_state);
        for op in &step.ops {
            add_op_bytes(seen, total, op);
        }
    }
}

fn add_doc_bytes(seen: &mut HashSet<(usize, usize)>, total: &mut u64, doc: &Document) {
    let raw = doc.original.raw_bytes();
    let raw_key = (raw.as_ptr() as usize, raw.len());
    if seen.insert(raw_key) {
        *total = total.saturating_add(raw.len() as u64);
    }
    for e in doc.original.entries() {
        add_arc_bytes(seen, total, e.payload());
    }
    for e in &doc.state.entries {
        add_arc_bytes(seen, total, &e.payload);
    }
}

impl SessionExecutor {
    pub fn new(
        input_root: impl AsRef<Path>,
        output_root: impl AsRef<Path>,
        limits: Limits,
    ) -> Result<Self> {
        let roots = IoRoots::new(input_root, output_root, limits)?;
        let session_id = hae_model::new_id("ses-")?;
        let default_writer_id = hae_model::new_id("wrt-")?;
        let mut writers = HashMap::new();
        writers.insert(default_writer_id.clone(), WriterRecord::default());
        Ok(Self {
            session_id,
            default_writer_id,
            default_next_seq: 1,
            roots,
            limits,
            writers,
            open_doc: None,
            renderer: None,
        })
    }

    /// Enable `document.preview` with the given adapter.
    pub fn set_renderer(&mut self, adapter: Arc<RhwpAdapter>) {
        self.renderer = Some(adapter);
    }

    fn configured_descriptor(&self, mut descriptor: CommandDescriptor) -> CommandDescriptor {
        if descriptor.name == "document.preview" {
            descriptor.available = self.renderer.is_some();
            descriptor.unsupported_reason = if descriptor.available {
                None
            } else {
                Some("renderer_not_configured")
            };
        }
        descriptor
    }

    /// Enable `document.preview` from `HAE_RHWP` (+ `HAE_BWRAP`, `HAE_FONT_DIR`).
    /// Returns whether an adapter was configured; unset means unconfigured.
    pub fn configure_renderer_from_env(&mut self) -> bool {
        match AdapterConfig::from_env() {
            Some(mut cfg) => {
                cfg.limits = self.limits;
                self.renderer = Some(Arc::new(RhwpAdapter::new(cfg)));
                true
            }
            None => false,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn default_writer_id(&self) -> &str {
        &self.default_writer_id
    }

    pub fn issue_writer(&mut self) -> Result<String> {
        let id = hae_model::new_id("wrt-")?;
        self.writers.insert(id.clone(), WriterRecord::default());
        Ok(id)
    }

    pub fn current_document_id(&self) -> Option<&str> {
        self.open_doc
            .as_ref()
            .map(|d| d.current.doc.document_id.as_str())
    }

    pub fn current_revision(&self) -> Option<u64> {
        self.open_doc.as_ref().map(|d| d.revision)
    }

    pub fn roots(&self) -> &IoRoots {
        &self.roots
    }

    /// Unique session memory currently held across original, current state,
    /// Undo/Redo history units, explicit snapshots, and the retained
    /// verification logs of every reachable committed state (shared buffers
    /// counted once).
    pub fn memory_used_bytes(&self) -> u64 {
        let Some(s) = &self.open_doc else {
            return 0;
        };
        Self::compute_memory(s, None)
    }

    fn compute_memory(s: &OpenDocSession, candidate: Option<&Document>) -> u64 {
        let mut seen = HashSet::new();
        let mut total = 0u64;
        add_doc_bytes(&mut seen, &mut total, &s.initial_doc);
        add_doc_bytes(&mut seen, &mut total, &s.current.doc);
        add_steps_bytes(&mut seen, &mut total, &s.current.verify_steps);
        for u in &s.undo_stack {
            add_doc_bytes(&mut seen, &mut total, &u.before.doc);
            add_doc_bytes(&mut seen, &mut total, &u.after.doc);
            add_steps_bytes(&mut seen, &mut total, &u.before.verify_steps);
            add_steps_bytes(&mut seen, &mut total, &u.after.verify_steps);
        }
        for u in &s.redo_stack {
            add_doc_bytes(&mut seen, &mut total, &u.before.doc);
            add_doc_bytes(&mut seen, &mut total, &u.after.doc);
            add_steps_bytes(&mut seen, &mut total, &u.before.verify_steps);
            add_steps_bytes(&mut seen, &mut total, &u.after.verify_steps);
        }
        for snap in s.snapshots.values() {
            add_doc_bytes(&mut seen, &mut total, &snap.doc);
        }
        if let Some(c) = candidate {
            add_doc_bytes(&mut seen, &mut total, c);
        }
        total
    }

    /// Convenience in-process call for CLI script mode / adapter callers:
    /// fills sessionId, default writerId, documentId, and requestSequence automatically.
    pub fn call(
        &mut self,
        command: &str,
        params: Value,
        expected_version: Option<u64>,
        dry_run: bool,
    ) -> ResponseEnvelope {
        let uses_seq = command_describe(command)
            .map(|d| d.uses_request_sequence && !dry_run)
            .unwrap_or(false);
        let seq = if uses_seq {
            let s = self.default_next_seq;
            self.default_next_seq += 1;
            Some(s)
        } else {
            None
        };
        let req = RequestEnvelope {
            schema_version: 1,
            session_id: Some(self.session_id.clone()),
            writer_id: Some(self.default_writer_id.clone()),
            request_sequence: seq,
            request_id: None,
            document_id: self.current_document_id().map(str::to_owned),
            expected_version,
            command: command.to_owned(),
            dry_run,
            params,
        };
        self.execute(req)
    }

    fn err_response(&self, req: &RequestEnvelope, err: EngineError) -> ResponseEnvelope {
        ResponseEnvelope {
            schema_version: 1,
            session_id: self.session_id.clone(),
            request_id: req.request_id.clone(),
            command: req.command.clone(),
            ok: false,
            replayed: false,
            result: None,
            diagnostics: vec![err.diagnostic().clone()],
        }
    }

    fn ok_response(
        &self,
        req: &RequestEnvelope,
        result: Value,
        diagnostics: Vec<Diagnostic>,
    ) -> ResponseEnvelope {
        ResponseEnvelope {
            schema_version: 1,
            session_id: self.session_id.clone(),
            request_id: req.request_id.clone(),
            command: req.command.clone(),
            ok: true,
            replayed: false,
            result: Some(result),
            diagnostics,
        }
    }

    pub fn execute(&mut self, req: RequestEnvelope) -> ResponseEnvelope {
        if req.schema_version != 1 {
            return self.err_response(
                &req,
                EngineError::new(
                    Code::InvalidRequest,
                    Stage::Request,
                    format!("unsupported schemaVersion {}", req.schema_version),
                ),
            );
        }
        if let Some(sid) = &req.session_id {
            if sid != &self.session_id {
                return self.err_response(
                    &req,
                    EngineError::new(
                        Code::SessionMismatch,
                        Stage::Request,
                        "request sessionId does not match current session",
                    ),
                );
            }
        } else {
            return self.err_response(
                &req,
                EngineError::new(
                    Code::InvalidRequest,
                    Stage::Request,
                    "sessionId is required in RequestEnvelope",
                ),
            );
        }

        if let Some(wid) = &req.writer_id {
            if !self.writers.contains_key(wid) {
                return self.err_response(
                    &req,
                    EngineError::new(
                        Code::UnknownWriter,
                        Stage::Request,
                        format!("writerId '{wid}' was not issued by this session"),
                    ),
                );
            }
        }

        let desc = match command_describe(&req.command) {
            Ok(d) => d,
            Err(e) => return self.err_response(&req, e),
        };

        if req.dry_run && !desc.supports_dry_run {
            return self.err_response(
                &req,
                EngineError::new(
                    Code::InvalidRequest,
                    Stage::Request,
                    format!("command '{}' does not support dryRun", req.command),
                ),
            );
        }

        let track_seq = desc.uses_request_sequence && !req.dry_run;
        let mut seq_ticket: Option<(String, u64, String)> = None;
        if track_seq {
            let Some(wid) = req.writer_id.clone() else {
                return self.err_response(
                    &req,
                    EngineError::new(
                        Code::InvalidRequest,
                        Stage::Request,
                        "writerId is required for sequence-tracked commands",
                    ),
                );
            };
            let Some(seq) = req.request_sequence else {
                return self.err_response(
                    &req,
                    EngineError::new(
                        Code::InvalidRequest,
                        Stage::Request,
                        "requestSequence is required for sequence-tracked commands",
                    ),
                );
            };
            let fp = request_fingerprint(&req);
            let w = self.writers.get(&wid).unwrap();
            if seq == 0 || seq < w.last_seq {
                return self.err_response(
                    &req,
                    EngineError::new(
                        Code::ReplayTooOld,
                        Stage::Request,
                        format!("requestSequence {seq} is older than {}", w.last_seq),
                    ),
                );
            }
            if seq == w.last_seq {
                if w.last_hash.as_deref() == Some(fp.as_str()) {
                    let mut saved = w.last_response.clone().unwrap();
                    saved.replayed = true;
                    saved.request_id = req.request_id.clone();
                    return saved;
                } else {
                    return self.err_response(
                        &req,
                        EngineError::new(
                            Code::RequestIdConflict,
                            Stage::Request,
                            format!(
                                "requestSequence {seq} reused with different payload on writer '{wid}'"
                            ),
                        ),
                    );
                }
            }
            if seq > w.last_seq + 1 {
                return self.err_response(
                    &req,
                    EngineError::new(
                        Code::RequestOutOfOrder,
                        Stage::Request,
                        format!("expected requestSequence {}, got {seq}", w.last_seq + 1),
                    ),
                );
            }
            seq_ticket = Some((wid, seq, fp));
        }

        let resp = match self.dispatch(&req, &desc) {
            Ok((val, diags)) => self.ok_response(&req, val, diags),
            Err(e) => self.err_response(&req, e),
        };

        if let Some((wid, seq, fp)) = seq_ticket {
            if let Some(w) = self.writers.get_mut(&wid) {
                w.last_seq = seq;
                w.last_hash = Some(fp);
                w.last_response = Some(resp.clone());
            }
        }
        resp
    }

    fn require_doc_mut<'a>(
        open_doc: &'a mut Option<OpenDocSession>,
        req_doc_id: Option<&str>,
    ) -> Result<&'a mut OpenDocSession> {
        let Some(s) = open_doc.as_mut() else {
            return Err(EngineError::new(
                Code::SessionRequired,
                Stage::Request,
                "no document is currently open in this session",
            ));
        };
        if let Some(id) = req_doc_id {
            if id != s.current.doc.document_id {
                return Err(EngineError::new(
                    Code::TargetNotFound,
                    Stage::Request,
                    format!("documentId '{id}' does not match open document"),
                ));
            }
        }
        Ok(s)
    }

    fn check_version(s: &OpenDocSession, expected: Option<u64>) -> Result<u64> {
        let Some(v) = expected else {
            return Err(EngineError::new(
                Code::InvalidRequest,
                Stage::Validate,
                "expectedVersion is required for content mutations and Undo/Redo",
            ));
        };
        if v != s.revision {
            return Err(EngineError::new(
                Code::VersionConflict,
                Stage::Validate,
                format!(
                    "expectedVersion {v} does not match current revision {}",
                    s.revision
                ),
            )
            .with_detail(json!({
                "expectedVersion": v,
                "currentVersion": s.revision,
            })));
        }
        Ok(s.revision)
    }

    fn dispatch(
        &mut self,
        req: &RequestEnvelope,
        _desc: &CommandDescriptor,
    ) -> Result<(Value, Vec<Diagnostic>)> {
        let (parsed, _) = ParsedCommand::parse(&req.command, req.params.clone())?;
        match parsed {
            ParsedCommand::CommandList(_) => Ok((
                json!({
                    "commands": command_list().into_iter().map(|d| self.configured_descriptor(d)).collect::<Vec<_>>(),
                }),
                Vec::new(),
            )),
            ParsedCommand::CommandDescribe(p) => {
                let d = self.configured_descriptor(command_describe(&p.command)?);
                Ok((serde_json::to_value(d).unwrap(), Vec::new()))
            }
            ParsedCommand::DocumentOpen(p) => self.handle_open(&p),
            ParsedCommand::DocumentClose(_) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                let doc_id = s.current.doc.document_id.clone();
                self.open_doc = None;
                Ok((json!({ "closedDocumentId": doc_id }), Vec::new()))
            }
            ParsedCommand::DocumentInspect(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                if let Some(ev) = req.expected_version {
                    if ev != s.revision {
                        return Err(EngineError::new(
                            Code::VersionConflict,
                            Stage::Query,
                            "expectedVersion mismatch on document.inspect",
                        ));
                    }
                }
                let offset = if let Some(c) = &p.cursor {
                    let Some((rev_s, idx_s)) = c.split_once(':') else {
                        return Err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Query,
                            "invalid inspect cursor format",
                        ));
                    };
                    let cursor_rev: u64 = rev_s.parse().map_err(|_| {
                        EngineError::new(Code::InvalidRequest, Stage::Query, "invalid cursor rev")
                    })?;
                    if cursor_rev != s.revision {
                        return Err(EngineError::new(
                            Code::VersionConflict,
                            Stage::Query,
                            format!(
                                "inspect cursor belongs to revision {cursor_rev}, current is {}",
                                s.revision
                            ),
                        ));
                    }
                    idx_s.parse::<usize>().map_err(|_| {
                        EngineError::new(Code::InvalidRequest, Stage::Query, "invalid cursor index")
                    })?
                } else {
                    0
                };
                let limit = p.limit.unwrap_or(100).clamp(1, 500);
                let (items, next) = s.current.doc.inspect(offset, limit);
                let next_cursor = next.map(|n| format!("{}:{n}", s.revision));
                Ok((
                    json!({
                        "documentId": s.current.doc.document_id,
                        "revision": s.revision,
                        "editable": s.current.doc.permissions.editable(),
                        "readonlyReasons": s.current.doc.permissions.readonly_reasons,
                        "items": items,
                        "nextCursor": next_cursor,
                    }),
                    Vec::new(),
                ))
            }
            ParsedCommand::DocumentStats(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                if let Some(ev) = req.expected_version {
                    if ev != s.revision {
                        return Err(EngineError::new(
                            Code::VersionConflict,
                            Stage::Query,
                            "expectedVersion mismatch on document.stats",
                        ));
                    }
                }
                let res = s.current.doc.stats(s.revision, p.scope.as_deref())?;
                let val = serde_json::to_value(res).map_err(|e| {
                    EngineError::new(Code::StateInvariantError, Stage::Query, e.to_string())
                })?;
                Ok((val, Vec::new()))
            }
            ParsedCommand::SectionList(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                if let Some(ev) = req.expected_version {
                    if ev != s.revision {
                        return Err(EngineError::new(
                            Code::VersionConflict,
                            Stage::Query,
                            "expectedVersion mismatch on section.list",
                        ));
                    }
                }
                let res = s
                    .current
                    .doc
                    .section_list(s.revision, p.limit, p.cursor.as_deref())?;
                let val = serde_json::to_value(res).map_err(|e| {
                    EngineError::new(Code::StateInvariantError, Stage::Query, e.to_string())
                })?;
                Ok((val, Vec::new()))
            }
            ParsedCommand::NoteList(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                if let Some(ev) = req.expected_version {
                    if ev != s.revision {
                        return Err(EngineError::new(
                            Code::VersionConflict,
                            Stage::Query,
                            "expectedVersion mismatch on note.list",
                        ));
                    }
                }
                let res = s.current.doc.note_list(
                    s.revision,
                    p.kind.as_deref(),
                    p.limit,
                    p.cursor.as_deref(),
                )?;
                let val = serde_json::to_value(res).map_err(|e| {
                    EngineError::new(Code::StateInvariantError, Stage::Query, e.to_string())
                })?;
                Ok((val, Vec::new()))
            }
            ParsedCommand::ElementQuery(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                let q = Query {
                    kind: p.kind.as_deref().map(parse_kind).transpose()?,
                    text_contains: p.text_contains,
                    ancestor: p.ancestor,
                    table: p.table,
                    row: p.row,
                    col: p.col,
                };
                let qr = s.current.doc.query(&q)?;
                Ok((
                    json!({
                        "documentId": s.current.doc.document_id,
                        "revision": s.revision,
                        "matches": qr.matches,
                        "unalignedMatches": qr.unaligned_matches,
                    }),
                    Vec::new(),
                ))
            }
            ParsedCommand::ElementGet(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                let doc = &s.current.doc;
                let n = doc.require(&p.target, None)?;
                let outline = doc.outline(n);
                let text = doc.node_text(n);
                let grapheme_count = if n.kind == Kind::Paragraph {
                    Some(doc.para_text(n)?.grapheme_len())
                } else {
                    None
                };
                let (alignment, line_spacing_percent) = if n.kind == Kind::Paragraph {
                    Self::read_para_format_props(doc, n)
                } else {
                    (None, None)
                };
                let children: Vec<String> = doc.children(&n.id).map(|c| c.id.clone()).collect();
                Ok((
                    json!({
                        "documentId": doc.document_id,
                        "revision": s.revision,
                        "outline": outline,
                        "text": text,
                        "graphemeCount": grapheme_count,
                        "alignment": alignment,
                        "lineSpacingPercent": line_spacing_percent,
                        "children": children,
                    }),
                    Vec::new(),
                ))
            }
            ParsedCommand::TableSetColumnWidths(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "table.setColumnWidths".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::TableSetRowHeights(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "table.setRowHeights".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::CellSetFormat(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "cell.setFormat".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::CellSetSize(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "cell.setSize".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::EquationList(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                if req.expected_version.is_some_and(|v| v != s.revision) {
                    return Err(EngineError::new(
                        Code::VersionConflict,
                        Stage::Query,
                        "expectedVersion mismatch",
                    ));
                }
                let doc = &s.current.doc;
                let nodes = doc.equation_nodes(p.scope.as_deref())?;
                let limit = p.limit.unwrap_or(100);
                if !(1..=500).contains(&limit) {
                    return Err(EngineError::new(
                        Code::InvalidRequest,
                        Stage::Query,
                        "equation limit must be 1..500",
                    ));
                }
                let binding = sha256_hex(
                    format!("{}:{}", doc.document_id, p.scope.as_deref().unwrap_or("")).as_bytes(),
                );
                let offset = if let Some(c) = p.cursor {
                    let parts: Vec<_> = c.split(':').collect();
                    if parts.len() != 3 || parts[1] != binding {
                        return Err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Query,
                            "invalid equation cursor or changed scope/document",
                        ));
                    }
                    let rev = parts[0].parse::<u64>().map_err(|_| {
                        EngineError::new(
                            Code::InvalidRequest,
                            Stage::Query,
                            "invalid cursor revision",
                        )
                    })?;
                    if rev != s.revision {
                        return Err(EngineError::new(
                            Code::VersionConflict,
                            Stage::Query,
                            "equation cursor revision is stale",
                        ));
                    }
                    parts[2].parse::<usize>().map_err(|_| {
                        EngineError::new(
                            Code::InvalidRequest,
                            Stage::Query,
                            "invalid cursor offset",
                        )
                    })?
                } else {
                    0
                };
                if offset > nodes.len() {
                    return Err(EngineError::new(
                        Code::InvalidRequest,
                        Stage::Query,
                        "cursor outside equation list",
                    ));
                }
                let items = nodes
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|n| doc.equation_info(&n.id, false))
                    .collect::<Result<Vec<_>>>()?;
                let next = offset + items.len();
                let cursor =
                    (next < nodes.len()).then(|| format!("{}:{binding}:{next}", s.revision));
                Ok((
                    json!({"documentId":doc.document_id,"revision":s.revision,"items":items,"nextCursor":cursor}),
                    Vec::new(),
                ))
            }
            ParsedCommand::EquationGet(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                if req.expected_version.is_some_and(|v| v != s.revision) {
                    return Err(EngineError::new(
                        Code::VersionConflict,
                        Stage::Query,
                        "expectedVersion mismatch",
                    ));
                }
                Ok((
                    json!({"documentId":s.current.doc.document_id,"revision":s.revision,"equation":s.current.doc.equation_info(&p.target,true)?}),
                    Vec::new(),
                ))
            }
            ParsedCommand::EquationSetScript(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "equation.setScript".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::EquationSetProperties(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "equation.setProperties".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::LinkList(p) => self.handle_link_list(req, &p),
            ParsedCommand::TrackChangeList(p) => self.handle_track_change_list(req, &p),
            ParsedCommand::SectionGetPage(p) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                Ok((
                    crate::section::get_page(
                        &s.current.doc,
                        p.scope.as_deref(),
                        p.section_index,
                        s.revision,
                    )?,
                    Vec::new(),
                ))
            }
            ParsedCommand::SectionSetPage(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "section.setPage".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::SectionSetPageNumbering(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "section.setPageNumbering".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::SectionSetVisibility(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "section.setVisibility".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::TextFormat(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "text.format".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::TextReplace(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "text.replace".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::TextReplaceAll(p) => {
                let (mut result, diags) = self.handle_edit_steps(
                    req,
                    &[BatchStepParam {
                        command: "text.replaceAll".into(),
                        params: serde_json::to_value(p).unwrap(),
                    }],
                )?;
                // A single command reports its one step's summary at top level.
                if let Some(Value::Array(mut reports)) =
                    result.as_object_mut().and_then(|o| o.remove("replaceAll"))
                {
                    if let (Some(Value::Object(report)), Some(out)) =
                        (reports.pop(), result.as_object_mut())
                    {
                        out.extend(report);
                    }
                }
                Ok((result, diags))
            }
            ParsedCommand::ParagraphFormat(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "paragraph.format".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::CellSetText(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "cell.setText".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::FormFill(p) => {
                let (mut result, diags) = self.handle_edit_steps(
                    req,
                    &[BatchStepParam {
                        command: "form.fill".into(),
                        params: serde_json::to_value(p).unwrap(),
                    }],
                )?;
                // A single command reports its one step's summary at top level.
                if let Some(Value::Array(mut reports)) =
                    result.as_object_mut().and_then(|o| o.remove("formFill"))
                {
                    if let (Some(Value::Object(report)), Some(out)) =
                        (reports.pop(), result.as_object_mut())
                    {
                        out.extend(report);
                    }
                }
                Ok((result, diags))
            }
            ParsedCommand::ImageReplace(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "image.replace".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::ParagraphSetStyle(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "paragraph.setStyle".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::ImageFit(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "image.fit".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::HeaderSetText(p) => self.handle_edit_steps(
                req,
                &[BatchStepParam {
                    command: "header.setText".into(),
                    params: serde_json::to_value(p).unwrap(),
                }],
            ),
            ParsedCommand::CommandBatch(CommandBatchParams { steps }) => {
                if steps.is_empty() {
                    return Err(EngineError::new(
                        Code::InvalidRequest,
                        Stage::Request,
                        "command.batch requires at least one step",
                    ));
                }
                self.handle_edit_steps(req, &steps)
            }
            ParsedCommand::HistoryUndo(_) => self.handle_undo(req),
            ParsedCommand::HistoryRedo(_) => self.handle_redo(req),
            ParsedCommand::HistoryClear(_) => {
                if req.expected_version.is_some() {
                    return Err(EngineError::new(
                        Code::InvalidRequest,
                        Stage::Request,
                        "history.clear does not accept expectedVersion",
                    ));
                }
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                let had_history = !s.undo_stack.is_empty() || !s.redo_stack.is_empty();
                s.undo_stack.clear();
                s.redo_stack.clear();
                Ok((
                    json!({
                        "controlChanged": had_history,
                        "revision": s.revision,
                    }),
                    Vec::new(),
                ))
            }
            ParsedCommand::DocumentSnapshot(SnapshotCreateParams { label }) => {
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                let id = hae_model::new_id("snap-")?;
                s.snapshots.insert(
                    id.clone(),
                    ExplicitSnapshot {
                        doc: s.current.doc.clone(),
                    },
                );
                if Self::compute_memory(s, None) > self.limits.max_session_memory {
                    s.snapshots.remove(&id);
                    return Err(EngineError::new(
                        Code::ResourceLimit,
                        Stage::Validate,
                        "session memory limit exceeded while creating snapshot",
                    ));
                }
                Ok((
                    json!({
                        "snapshotId": id,
                        "revision": s.revision,
                        "label": label,
                    }),
                    Vec::new(),
                ))
            }
            ParsedCommand::SnapshotRelease(SnapshotReleaseParams { snapshot_id }) => {
                if req.expected_version.is_some() {
                    return Err(EngineError::new(
                        Code::InvalidRequest,
                        Stage::Request,
                        "snapshot.release does not accept expectedVersion",
                    ));
                }
                let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
                if s.snapshots.remove(&snapshot_id).is_none() {
                    return Err(EngineError::new(
                        Code::SnapshotUnavailable,
                        Stage::Validate,
                        format!("snapshot '{snapshot_id}' is unavailable or already released"),
                    ));
                }
                Ok((
                    json!({
                        "controlChanged": true,
                        "snapshotId": snapshot_id,
                        "revision": s.revision,
                    }),
                    Vec::new(),
                ))
            }
            ParsedCommand::DocumentDiff(p) => self.handle_diff(req, &p),
            ParsedCommand::DocumentPreview(p) => self.handle_preview(req, &p),
            ParsedCommand::ProjectSave(p) => self.handle_project_save(req, &p),
            ParsedCommand::DocumentExport(p) => self.handle_document_export(req, &p),
        }
    }

    /// document.preview: read-only. The snapshot is the current committed
    /// state; a rendering failure returns an error and changes nothing.
    fn handle_preview(
        &mut self,
        req: &RequestEnvelope,
        p: &DocumentPreviewParams,
    ) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        if let Some(ev) = req.expected_version {
            if ev != s.revision {
                return Err(EngineError::new(
                    Code::VersionConflict,
                    Stage::Render,
                    "expectedVersion mismatch on document.preview",
                ));
            }
        }
        let Some(adapter) = self.renderer.clone() else {
            return Err(EngineError::new(
                Code::RendererUnavailable,
                Stage::Render,
                "no rhwp preview adapter is configured",
            )
            .with_reason("renderer_not_configured"));
        };
        let mut preview_limits = self.limits;
        preview_limits.max_session_memory = self
            .limits
            .max_session_memory
            .saturating_sub(Self::compute_memory(s, None));
        if preview_limits.max_session_memory == 0 {
            return Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Render,
                "no session memory remains for preview",
            ));
        }
        crate::preview::render(
            &adapter,
            &s.current.doc,
            s.revision,
            p,
            &preview_limits,
            Some(&self.roots),
        )
    }

    fn read_para_format_props(
        doc: &Document,
        n: &hae_model::Node,
    ) -> (Option<String>, Option<u32>) {
        let Some(pref) = doc.doc_of(n).el(n.el).attr_value("paraPrIDRef") else {
            return (None, None);
        };
        let Some(header) = doc.header() else {
            return (None, None);
        };
        let Some((p_el, _)) = header
            .iter()
            .find(|(_, e)| e.is(NS_HH, "paraPr") && e.attr_value("id") == Some(pref))
        else {
            return (None, None);
        };
        let mut align = None;
        let mut spacing = None;
        for d in header.descendants(p_el) {
            let e = header.el(d);
            if e.is(NS_HH, "align") {
                align = e.attr_value("horizontal").map(str::to_owned);
            }
            if e.is(NS_HH, "lineSpacing") && e.attr_value("type") == Some("PERCENT") {
                spacing = e.attr_value("value").and_then(|v| v.parse::<u32>().ok());
            }
        }
        (align, spacing)
    }

    /// Optional `expectedVersion` on a read-only query: it must match the
    /// current revision when supplied, and never advances anything.
    fn check_query_version(s: &OpenDocSession, req: &RequestEnvelope, command: &str) -> Result<()> {
        if let Some(ev) = req.expected_version {
            if ev != s.revision {
                return Err(EngineError::new(
                    Code::VersionConflict,
                    Stage::Query,
                    format!("expectedVersion mismatch on {command}"),
                ));
            }
        }
        Ok(())
    }

    /// Parse the `revision:offset` cursor shared with `document.inspect`.
    fn query_cursor(cursor: &Option<String>, revision: u64, command: &str) -> Result<usize> {
        let Some(c) = cursor else {
            return Ok(0);
        };
        let Some((rev, offset)) = c.split_once(':') else {
            return Err(EngineError::new(
                Code::InvalidRequest,
                Stage::Query,
                format!("invalid {command} cursor format"),
            ));
        };
        let cursor_rev: u64 = rev.parse().map_err(|_| {
            EngineError::new(
                Code::InvalidRequest,
                Stage::Query,
                format!("invalid {command} cursor revision"),
            )
        })?;
        if cursor_rev != revision {
            return Err(EngineError::new(
                Code::VersionConflict,
                Stage::Query,
                format!("{command} cursor belongs to revision {cursor_rev}, current is {revision}"),
            ));
        }
        offset.parse::<usize>().map_err(|_| {
            EngineError::new(
                Code::InvalidRequest,
                Stage::Query,
                format!("invalid {command} cursor offset"),
            )
        })
    }

    /// `link.list`: read-only. The document, revision, and history are unchanged.
    fn handle_link_list(
        &mut self,
        req: &RequestEnvelope,
        p: &LinkListParams,
    ) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        Self::check_query_version(s, req, "link.list")?;
        let offset = Self::query_cursor(&p.cursor, s.revision, "link.list")?;
        let limit = p.limit.unwrap_or(100).clamp(1, 500);
        let links = s.current.doc.link_fields(p.scope.as_deref())?;
        let total = links.len();
        let start = offset.min(total);
        let end = start.saturating_add(limit).min(total);
        let next = (end < total).then(|| format!("{}:{end}", s.revision));
        let page: Vec<_> = links.into_iter().skip(start).take(limit).collect();
        Ok((
            json!({
                "documentId": s.current.doc.document_id,
                "revision": s.revision,
                "links": page,
                "total": total,
                "nextCursor": next,
            }),
            Vec::new(),
        ))
    }

    /// `trackchange.list`: read-only. A track-change document stays locked
    /// read-only; this view only reports the marks and the lock reasons.
    fn handle_track_change_list(
        &mut self,
        req: &RequestEnvelope,
        p: &TrackChangeListParams,
    ) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        Self::check_query_version(s, req, "trackchange.list")?;
        let offset = Self::query_cursor(&p.cursor, s.revision, "trackchange.list")?;
        let limit = p.limit.unwrap_or(100).clamp(1, 500);
        let report = s.current.doc.track_change_report(p.scope.as_deref())?;
        let total = report.marks.len();
        let start = offset.min(total);
        let end = start.saturating_add(limit).min(total);
        let next = (end < total).then(|| format!("{}:{end}", s.revision));
        let marks: Vec<_> = report.marks.into_iter().skip(start).take(limit).collect();
        Ok((
            json!({
                "documentId": s.current.doc.document_id,
                "revision": s.revision,
                "hasTrackChanges": report.has_track_changes,
                "readonlyReasons": report.readonly_reasons,
                "marks": marks,
                "markCount": total,
                "nextCursor": next,
                "changes": report.changes,
                "authors": report.authors,
            }),
            Vec::new(),
        ))
    }

    fn handle_open(&mut self, p: &DocumentOpenParams) -> Result<(Value, Vec<Diagnostic>)> {
        let path = PathBuf::from(&p.path);
        let is_project = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("hwpwork"));

        let session = if is_project {
            let opened = self.roots.open_project(&path)?;
            let baseline = Arc::new(Archive::open(opened.current_hwpx.to_vec(), &self.limits)?);
            let doc = opened.document;
            OpenDocSession {
                initial_doc: doc.clone(),
                session_baseline: baseline,
                current: CommittedState {
                    doc,
                    verify_steps: Arc::new(Vec::new()),
                },
                revision: opened.revision,
                binding: Some(opened.binding),
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
                snapshots: BTreeMap::new(),
            }
        } else {
            let input = self.roots.read_input(&path)?;
            let doc = Document::open(input.bytes, &self.limits)?;
            let baseline = doc.original.clone();
            OpenDocSession {
                initial_doc: doc.clone(),
                session_baseline: baseline,
                current: CommittedState {
                    doc,
                    verify_steps: Arc::new(Vec::new()),
                },
                revision: 0,
                binding: None,
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
                snapshots: BTreeMap::new(),
            }
        };

        if Self::compute_memory(&session, None) > self.limits.max_session_memory {
            return Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Open,
                "opened document exceeds max_session_memory",
            ));
        }

        let res = json!({
            "documentId": session.current.doc.document_id,
            "revision": session.revision,
            "editable": session.current.doc.permissions.editable(),
            "readonlyReasons": session.current.doc.permissions.readonly_reasons,
            "nodeCount": session.current.doc.nodes.len(),
            "boundProjectPath": session.binding.as_ref().map(|b| b.path()),
        });
        self.open_doc = Some(session);
        Ok((res, Vec::new()))
    }

    fn build_step_ops(
        roots: &IoRoots,
        doc: &Document,
        step_index: usize,
        total_steps: usize,
        step: &BatchStepParam,
    ) -> Result<(EditOp, VerifyOperation)> {
        let wrap_err = |e: EngineError| {
            if total_steps > 1 {
                e.with_detail(json!({ "failedStep": step_index }))
            } else {
                e
            }
        };
        if step.params.get("expectedVersion").is_some()
            || step.params.get("requestSequence").is_some()
        {
            return Err(wrap_err(EngineError::new(
                Code::InvalidRequest,
                Stage::Request,
                "per-step expectedVersion or requestSequence is not allowed inside command.batch",
            )));
        }
        crate::registry::reject_structure_change(&step.command).map_err(&wrap_err)?;
        match step.command.as_str() {
            "table.setColumnWidths" => {
                let p: TableSetColumnWidthsParams = serde_json::from_value(step.params.clone())
                    .map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                Ok((
                    EditOp::TableSetColumnWidths {
                        target: p.target,
                        widths: p.widths.clone(),
                        width: p.width,
                    },
                    VerifyOperation::TableSetColumnWidths {
                        entry,
                        path,
                        widths: p.widths,
                        width: p.width,
                    },
                ))
            }
            "table.setRowHeights" => {
                let p: TableSetRowHeightsParams = serde_json::from_value(step.params.clone())
                    .map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                Ok((
                    EditOp::TableSetRowHeights {
                        target: p.target,
                        heights: p.heights.clone(),
                        height: p.height,
                    },
                    VerifyOperation::TableSetRowHeights {
                        entry,
                        path,
                        heights: p.heights,
                        height: p.height,
                    },
                ))
            }
            "cell.setSize" => {
                let p: CellSetSizeParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                Ok((
                    EditOp::CellSetSize {
                        target: p.target,
                        width: p.width,
                        height: p.height,
                    },
                    VerifyOperation::CellSetSize {
                        entry,
                        path,
                        width: p.width,
                        height: p.height,
                    },
                ))
            }
            "cell.setFormat" => {
                let p: CellSetFormatParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                let border_fill_id = p.border_fill_id.as_ref().map(|v| v.value());
                let format = hae_edit::CellFormat {
                    border_fill_id: border_fill_id.clone(),
                    background: p.background.clone(),
                    cell_margin: p.cell_margin.as_ref().map(|m| hae_edit::CellMargin {
                        left: m.left,
                        right: m.right,
                        top: m.top,
                        bottom: m.bottom,
                    }),
                };
                let independent = hae_verify::CellFormat {
                    border_fill_id,
                    background: p.background,
                    cell_margin: p.cell_margin.map(|m| hae_verify::CellMargin {
                        left: m.left,
                        right: m.right,
                        top: m.top,
                        bottom: m.bottom,
                    }),
                };
                Ok((
                    EditOp::CellSetFormat {
                        target: p.target,
                        format,
                    },
                    VerifyOperation::CellSetFormat {
                        entry,
                        path,
                        format: independent,
                    },
                ))
            }
            "equation.setScript" => {
                let p: EquationSetScriptParams = serde_json::from_value(step.params.clone())
                    .map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let n = doc
                    .require(&p.target, Some(Kind::Equation))
                    .map_err(&wrap_err)?;
                Ok((
                    EditOp::EquationSetScript {
                        target: p.target,
                        script: p.script.clone(),
                    },
                    VerifyOperation::EquationSetScript {
                        entry: n.entry.clone(),
                        path: doc.doc_of(n).path(n.el),
                        script: p.script,
                    },
                ))
            }
            "equation.setProperties" => {
                let p: EquationSetPropertiesParams = serde_json::from_value(step.params.clone())
                    .map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let n = doc
                    .require(&p.target, Some(Kind::Equation))
                    .map_err(&wrap_err)?;
                let props = serde_json::to_value(p.props).unwrap();
                Ok((
                    EditOp::EquationSetProperties {
                        target: p.target,
                        props: serde_json::from_value(props.clone()).map_err(|e| {
                            wrap_err(EngineError::new(
                                Code::InvalidRequest,
                                Stage::Request,
                                e.to_string(),
                            ))
                        })?,
                    },
                    VerifyOperation::EquationSetProperties {
                        entry: n.entry.clone(),
                        path: doc.doc_of(n).path(n.el),
                        props: serde_json::from_value(props).map_err(|e| {
                            wrap_err(EngineError::new(
                                Code::InvalidRequest,
                                Stage::Request,
                                e.to_string(),
                            ))
                        })?,
                    },
                ))
            }
            "paragraph.setStyle" => {
                let p: ParagraphSetStyleParams = serde_json::from_value(step.params.clone())
                    .map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let targets = match (p.target, p.targets) {
                    (Some(t), None) => vec![t],
                    (None, Some(ts)) if !ts.is_empty() => ts,
                    _ => {
                        return Err(wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            "supply exactly one of target or non-empty targets",
                        )))
                    }
                };
                let vtargets = targets
                    .iter()
                    .map(|t| {
                        let n = doc.require(t, Some(Kind::Paragraph))?;
                        Ok(VerifyScope {
                            entry: n.entry.clone(),
                            path: doc.doc_of(n).path(n.el),
                        })
                    })
                    .collect::<Result<Vec<_>>>()
                    .map_err(&wrap_err)?;
                Ok((
                    EditOp::ParagraphSetStyle {
                        targets,
                        style_id: p.style_id,
                    },
                    VerifyOperation::ParagraphSetStyle {
                        targets: vtargets,
                        style_id: p.style_id,
                    },
                ))
            }
            "image.fit" => {
                let p: ImageFitParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let n = doc
                    .require(&p.target, Some(Kind::Picture))
                    .map_err(&wrap_err)?;
                let mode = serde_json::to_value(p.mode).unwrap();
                Ok((
                    EditOp::ImageFit {
                        target: p.target,
                        mode: serde_json::from_value(mode.clone()).unwrap(),
                    },
                    VerifyOperation::ImageFit {
                        entry: n.entry.clone(),
                        path: doc.doc_of(n).path(n.el),
                        mode: serde_json::from_value(mode).unwrap(),
                    },
                ))
            }
            "header.setText" => {
                let p: HeaderSetTextParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let n = match &p.scope {
                    Some(t) => doc.require(t, Some(Kind::Section)).map_err(&wrap_err)?,
                    None => doc
                        .nodes
                        .iter()
                        .find(|n| n.kind == Kind::Section)
                        .ok_or_else(|| {
                            wrap_err(EngineError::new(
                                Code::TargetNotFound,
                                Stage::Validate,
                                "no first section",
                            ))
                        })?,
                };
                let kind = serde_json::to_value(p.kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string();
                Ok((
                    EditOp::HeaderSetText {
                        scope: p.scope,
                        kind: kind.clone(),
                        paragraph_index: p.paragraph_index,
                        text: p.text.clone(),
                    },
                    VerifyOperation::HeaderSetText {
                        entry: n.entry.clone(),
                        path: doc.doc_of(n).path(n.el),
                        kind,
                        paragraph_index: p.paragraph_index,
                        text: p.text,
                    },
                ))
            }
            "section.setPage" => {
                let p: SectionSetPageParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let node = hae_edit::resolve_section(doc, p.scope.as_deref(), p.section_index)
                    .map_err(&wrap_err)?;
                let entry = node.entry.clone();
                let path = doc.doc_of(&node).path(node.el);
                let mut settings = serde_json::to_value(p).unwrap();
                let obj = settings.as_object_mut().unwrap();
                obj.remove("scope");
                obj.remove("sectionIndex");
                Ok((
                    EditOp::SectionSetPage {
                        target: node.id,
                        settings: serde_json::from_value(settings.clone()).unwrap(),
                    },
                    VerifyOperation::SectionSetPage {
                        entry,
                        path,
                        settings: serde_json::from_value(settings).unwrap(),
                    },
                ))
            }
            "section.setPageNumbering" => {
                let p: SectionSetPageNumberingParams = serde_json::from_value(step.params.clone())
                    .map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let node = hae_edit::resolve_section(doc, p.scope.as_deref(), p.section_index)
                    .map_err(&wrap_err)?;
                let entry = node.entry.clone();
                let path = doc.doc_of(&node).path(node.el);
                let mut settings = serde_json::to_value(p).unwrap();
                let obj = settings.as_object_mut().unwrap();
                obj.remove("scope");
                obj.remove("sectionIndex");
                Ok((
                    EditOp::SectionSetPageNumbering {
                        target: node.id,
                        settings: serde_json::from_value(settings.clone()).unwrap(),
                    },
                    VerifyOperation::SectionSetPageNumbering {
                        entry,
                        path,
                        settings: serde_json::from_value(settings).unwrap(),
                    },
                ))
            }
            "section.setVisibility" => {
                let p: SectionSetVisibilityParams = serde_json::from_value(step.params.clone())
                    .map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            e.to_string(),
                        ))
                    })?;
                let node = hae_edit::resolve_section(doc, p.scope.as_deref(), p.section_index)
                    .map_err(&wrap_err)?;
                let entry = node.entry.clone();
                let path = doc.doc_of(&node).path(node.el);
                let mut settings = serde_json::to_value(p).unwrap();
                let obj = settings.as_object_mut().unwrap();
                obj.remove("scope");
                obj.remove("sectionIndex");
                Ok((
                    EditOp::SectionSetVisibility {
                        target: node.id,
                        settings: serde_json::from_value(settings.clone()).unwrap(),
                    },
                    VerifyOperation::SectionSetVisibility {
                        entry,
                        path,
                        settings: serde_json::from_value(settings).unwrap(),
                    },
                ))
            }

            "text.format" => {
                let p: TextFormatParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            format!("invalid text.format params: {e}"),
                        ))
                    })?;
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                let props = serde_json::to_value(&p.props).unwrap();
                Ok((
                    EditOp::TextFormat {
                        target: p.target,
                        range: p.range.start..p.range.end,
                        props: serde_json::from_value(props.clone()).map_err(|e| {
                            wrap_err(EngineError::new(
                                Code::InvalidRequest,
                                Stage::Request,
                                e.to_string(),
                            ))
                        })?,
                        preserve: p.controls.preserving(),
                    },
                    VerifyOperation::TextFormat {
                        entry,
                        path,
                        range: p.range.start..p.range.end,
                        props: serde_json::from_value(props).map_err(|e| {
                            wrap_err(EngineError::new(
                                Code::InvalidRequest,
                                Stage::Request,
                                e.to_string(),
                            ))
                        })?,
                        preserve: p.controls.preserving(),
                    },
                ))
            }
            "text.replace" => {
                let p: TextReplaceParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            format!("invalid text.replace params: {e}"),
                        ))
                    })?;
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                Ok((
                    EditOp::TextReplace {
                        target: p.target,
                        range: p.range.start..p.range.end,
                        text: p.text.clone(),
                        preserve: p.controls.preserving(),
                    },
                    VerifyOperation::TextReplace {
                        entry,
                        path,
                        range: p.range.start..p.range.end,
                        replacement: p.text,
                        preserve: p.controls.preserving(),
                    },
                ))
            }
            "text.replaceAll" => {
                let p: TextReplaceAllParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            format!("invalid text.replaceAll params: {e}"),
                        ))
                    })?;
                let max_matches = p.max_matches.unwrap_or(hae_edit::DEFAULT_MAX_MATCHES);
                let scope = p.scope.as_ref().map(|id| {
                    let (entry, path) = doc
                        .node(id)
                        .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                        .unwrap_or_default();
                    VerifyScope { entry, path }
                });
                Ok((
                    EditOp::TextReplaceAll {
                        find: p.find.clone(),
                        text: p.text.clone(),
                        scope: p.scope,
                        max_matches,
                    },
                    VerifyOperation::TextReplaceAll {
                        find: p.find,
                        replacement: p.text,
                        scope,
                        max_matches,
                    },
                ))
            }
            "paragraph.format" => {
                let p: ParagraphFormatParams = serde_json::from_value(step.params.clone())
                    .map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            format!("invalid paragraph.format params: {e}"),
                        ))
                    })?;
                let (align, v_align) = match p.alignment.as_deref() {
                    Some(a) => {
                        let (x, y) = parse_alignment(a).map_err(&wrap_err)?;
                        (Some(x), Some(y))
                    }
                    None => (None, None),
                };
                let (ls, v_ls) = match p.line_spacing {
                    Some(l) => {
                        let (x, y) = parse_line_spacing(l).map_err(&wrap_err)?;
                        (Some(x), Some(y))
                    }
                    None => (None, None),
                };
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                Ok((
                    EditOp::ParagraphFormat {
                        target: p.target,
                        alignment: align,
                        line_spacing: ls,
                    },
                    VerifyOperation::ParagraphFormat {
                        entry,
                        path,
                        alignment: v_align,
                        line_spacing: v_ls,
                    },
                ))
            }
            "cell.setText" => {
                let p: CellSetTextParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            format!("invalid cell.setText params: {e}"),
                        ))
                    })?;
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                Ok((
                    match (&p.text, &p.paragraphs) {
                        (Some(text), None) => EditOp::CellSetText {
                            target: p.target.clone(),
                            text: text.clone(),
                            preserve: p.controls.preserving(),
                        },
                        (None, Some(paragraphs)) => EditOp::CellSetParagraphs {
                            target: p.target.clone(),
                            paragraphs: paragraphs.clone(),
                            preserve: p.controls.preserving(),
                        },
                        _ => {
                            return Err(wrap_err(EngineError::new(
                                Code::InvalidArgument,
                                Stage::Validate,
                                "supply exactly one of text or paragraphs",
                            )))
                        }
                    },
                    match (p.text, p.paragraphs) {
                        (Some(text), None) => VerifyOperation::CellSetText {
                            entry,
                            path,
                            text,
                            preserve: p.controls.preserving(),
                        },
                        (None, Some(paragraphs)) => VerifyOperation::CellSetParagraphs {
                            entry,
                            path,
                            paragraphs,
                            preserve: p.controls.preserving(),
                        },
                        _ => unreachable!(),
                    },
                ))
            }
            "form.fill" => {
                let p: FormFillParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            format!("invalid form.fill params: {e}"),
                        ))
                    })?;
                let scope = match &p.scope {
                    Some(id) => {
                        let n = doc.node(id).ok_or_else(|| {
                            wrap_err(
                                EngineError::new(
                                    Code::TargetNotFound,
                                    Stage::Validate,
                                    format!("no node {id}"),
                                )
                                .with_target(id),
                            )
                        })?;
                        if !matches!(n.kind, Kind::Table | Kind::Section) {
                            return Err(wrap_err(
                                EngineError::new(
                                    Code::Unsupported,
                                    Stage::Validate,
                                    "form.fill scope must be a table or section nodeId",
                                )
                                .with_reason("form_scope")
                                .with_target(id),
                            ));
                        }
                        Some(VerifyScope {
                            entry: n.entry.clone(),
                            path: doc.doc_of(n).path(n.el),
                        })
                    }
                    None => None,
                };
                Ok((
                    EditOp::FormFill {
                        values: p.values.clone(),
                        scope: p.scope,
                        strict: p.strict,
                    },
                    VerifyOperation::FormFill {
                        values: p.values,
                        scope,
                        strict: p.strict,
                    },
                ))
            }
            "image.replace" => {
                let p: ImageReplaceParams =
                    serde_json::from_value(step.params.clone()).map_err(|e| {
                        wrap_err(EngineError::new(
                            Code::InvalidRequest,
                            Stage::Request,
                            format!("invalid image.replace params: {e}"),
                        ))
                    })?;
                let input = roots.read_input(&p.image_path).map_err(&wrap_err)?;
                let image = ImageBytes::from_bytes(input.bytes.clone(), &doc.limits)
                    .map_err(|e| wrap_err(e.with_target(&p.target)))?;
                let (entry, path) = doc
                    .node(&p.target)
                    .map(|n| (n.entry.clone(), doc.doc_of(n).path(n.el)))
                    .unwrap_or_default();
                Ok((
                    EditOp::ImageReplace {
                        target: p.target,
                        image,
                    },
                    VerifyOperation::ImageReplace {
                        entry,
                        path,
                        bytes: input.bytes,
                    },
                ))
            }
            other => Err(wrap_err(EngineError::new(
                Code::InvalidRequest,
                Stage::Request,
                format!("command '{other}' is not permitted inside an edit or command.batch"),
            ))),
        }
    }

    fn handle_edit_steps(
        &mut self,
        req: &RequestEnvelope,
        steps: &[BatchStepParam],
    ) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        let before_version = Self::check_version(s, req.expected_version)?;

        let mut edit_ops = Vec::with_capacity(steps.len());
        let mut verify_ops = Vec::with_capacity(steps.len());
        for (i, step) in steps.iter().enumerate() {
            let (eop, vop) =
                Self::build_step_ops(&self.roots, &s.current.doc, i, steps.len(), step)?;
            edit_ops.push(eop);
            verify_ops.push(vop);
        }

        let applied: Applied = hae_edit::apply(&s.current.doc, &edit_ops)?;

        let is_noop =
            applied.doc.state.payload_set_sha256() == s.current.doc.state.payload_set_sha256();

        if !is_noop {
            // Independently verify candidate via hae-verify before committing
            let candidate_bytes = applied.doc.to_hwpx()?;
            let _ = verify_candidate(
                &s.current.doc.state,
                &candidate_bytes,
                &verify_ops,
                &self.limits,
            )?;
        }

        let used = Self::compute_memory(s, Some(&applied.doc));
        if used > self.limits.max_session_memory {
            return Err(EngineError::new(
                Code::ResourceLimit,
                Stage::Validate,
                format!(
                    "edit exceeds session memory limit ({used} bytes used, limit {} bytes)",
                    self.limits.max_session_memory
                ),
            ));
        }

        let diags = applied.diagnostics.clone();
        if req.dry_run {
            let outcome = if is_noop { "noop" } else { "applied" };
            return Ok((
                with_form_fill(
                    with_replace_all(
                        json!({
                            "outcome": outcome,
                            "dryRun": true,
                            "beforeVersion": before_version,
                            "afterVersion": before_version,
                            "predictedAfterVersion": if is_noop { before_version } else { before_version + 1 },
                            "changedIds": applied.changed_ids,
                            "resourceChanges": applied.resource_changes,
                            "intents": applied.intents,
                            "stale": applied.stale,
                            "cacheRemovals": applied.cache_removals,
                            "historyEvicted": false,
                        }),
                        &applied.replace_all,
                    ),
                    &applied.form_fill,
                ),
                diags,
            ));
        }

        if is_noop {
            return Ok((
                with_form_fill(
                    with_replace_all(
                        json!({
                            "outcome": "noop",
                            "beforeVersion": before_version,
                            "afterVersion": before_version,
                            "changedIds": Vec::<String>::new(),
                            "resourceChanges": Vec::<hae_edit::ResourceChange>::new(),
                            "intents": Vec::<hae_edit::Intent>::new(),
                            "stale": applied.stale,
                            "cacheRemovals": applied.cache_removals,
                            "historyEvicted": false,
                        }),
                        &applied.replace_all,
                    ),
                    &applied.form_fill,
                ),
                diags,
            ));
        }

        // Only the newest record is ever consumed: `verify_temporary_hwpx`
        // reads `last()` once the state has more than one commit, and its
        // single-entry case compares against the session baseline. Retaining
        // the earlier records cloned every pre-edit package state per commit
        // and never read them back.
        let next_steps = vec![VerifyStepRecord {
            pre_state: s.current.doc.state.clone(),
            ops: verify_ops,
        }];
        let next_state = CommittedState {
            doc: applied.doc,
            verify_steps: Arc::new(next_steps),
        };

        let mut history_evicted = false;
        let max_units = self.limits.max_history_units as usize;
        if max_units > 0 && s.undo_stack.len() >= max_units {
            s.undo_stack.remove(0);
            history_evicted = true;
        }
        s.redo_stack.clear();
        s.undo_stack.push(HistoryUnit {
            before: s.current.clone(),
            after: next_state.clone(),
            changed_ids: applied.changed_ids.clone(),
            resource_changes: applied.resource_changes.clone(),
        });
        s.current = next_state;
        s.revision += 1;

        Ok((
            with_form_fill(
                with_replace_all(
                    json!({
                        "outcome": "applied",
                        "beforeVersion": before_version,
                        "afterVersion": s.revision,
                        "changedIds": applied.changed_ids,
                        "resourceChanges": applied.resource_changes,
                        "intents": applied.intents,
                        "stale": applied.stale,
                        "cacheRemovals": applied.cache_removals,
                        "historyEvicted": history_evicted,
                    }),
                    &applied.replace_all,
                ),
                &applied.form_fill,
            ),
            diags,
        ))
    }

    fn handle_undo(&mut self, req: &RequestEnvelope) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        let before_version = Self::check_version(s, req.expected_version)?;
        let Some(unit) = s.undo_stack.pop() else {
            return Err(EngineError::new(
                Code::HistoryEmpty,
                Stage::Validate,
                "undo history is empty",
            ));
        };
        let changed_ids = unit.changed_ids.clone();
        let resource_changes = unit.resource_changes.clone();
        s.current = unit.before.clone();
        s.redo_stack.push(unit);
        s.revision += 1;
        Ok((
            json!({
                "outcome": "applied",
                "beforeVersion": before_version,
                "afterVersion": s.revision,
                "changedIds": changed_ids,
                "resourceChanges": resource_changes,
                "historyEvicted": false,
            }),
            Vec::new(),
        ))
    }

    fn handle_redo(&mut self, req: &RequestEnvelope) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        let before_version = Self::check_version(s, req.expected_version)?;
        let Some(unit) = s.redo_stack.pop() else {
            return Err(EngineError::new(
                Code::HistoryEmpty,
                Stage::Validate,
                "redo history is empty",
            ));
        };
        let changed_ids = unit.changed_ids.clone();
        let resource_changes = unit.resource_changes.clone();
        s.current = unit.after.clone();
        s.undo_stack.push(unit);
        s.revision += 1;
        Ok((
            json!({
                "outcome": "applied",
                "beforeVersion": before_version,
                "afterVersion": s.revision,
                "changedIds": changed_ids,
                "resourceChanges": resource_changes,
                "historyEvicted": false,
            }),
            Vec::new(),
        ))
    }

    fn handle_diff(
        &mut self,
        req: &RequestEnvelope,
        p: &DocumentDiffParams,
    ) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        let base_doc = match &p.base_snapshot_id {
            None => &s.initial_doc,
            Some(id) => {
                &s.snapshots
                    .get(id)
                    .ok_or_else(|| {
                        EngineError::new(
                            Code::SnapshotUnavailable,
                            Stage::Query,
                            format!("base snapshot '{id}' is unavailable"),
                        )
                    })?
                    .doc
            }
        };
        let target_doc = match &p.target_snapshot_id {
            None => &s.current.doc,
            Some(id) => {
                &s.snapshots
                    .get(id)
                    .ok_or_else(|| {
                        EngineError::new(
                            Code::SnapshotUnavailable,
                            Stage::Query,
                            format!("target snapshot '{id}' is unavailable"),
                        )
                    })?
                    .doc
            }
        };

        let mut changed_nodes = Vec::new();
        for (a, b) in base_doc.nodes.iter().zip(&target_doc.nodes) {
            let oa = base_doc.outline(a);
            let ob = target_doc.outline(b);
            let ta = base_doc.node_text(a);
            let tb = target_doc.node_text(b);
            if ta != tb || oa.media != ob.media {
                changed_nodes.push(json!({
                    "nodeId": b.id,
                    "kind": b.kind,
                    "beforeText": ta,
                    "afterText": tb,
                    "beforeMedia": oa.media,
                    "afterMedia": ob.media,
                }));
            }
        }
        let mut changed_entries = Vec::new();
        let mut media_changes = Vec::new();
        for e in &target_doc.state.entries {
            let old = base_doc.state.get(&e.name);
            if old != Some(&e.payload) {
                if e.name.starts_with("BinData/") {
                    media_changes.push(e.name.clone());
                } else {
                    changed_entries.push(e.name.clone());
                }
            }
        }
        let identical =
            base_doc.state.payload_set_sha256() == target_doc.state.payload_set_sha256();
        Ok((
            json!({
                "identical": identical,
                "changedNodes": changed_nodes,
                "changedEntries": changed_entries,
                "mediaChanges": media_changes,
                "staleLayoutSections": target_doc.stale_layout,
            }),
            Vec::new(),
        ))
    }

    fn verify_temporary_hwpx(
        s: &OpenDocSession,
        limits: &Limits,
        candidate_bytes: &[u8],
    ) -> Result<()> {
        if s.current.verify_steps.is_empty() {
            let _ = verify_archive(&s.session_baseline, candidate_bytes, &[], limits)?;
            return Ok(());
        }
        if s.current.verify_steps.len() == 1 {
            let step = &s.current.verify_steps[0];
            if step.pre_state.payload_set_sha256()
                == PackageState::from_archive(&s.session_baseline).payload_set_sha256()
            {
                let _ = verify_archive(&s.session_baseline, candidate_bytes, &step.ops, limits)?;
                return Ok(());
            }
        }
        let last = s.current.verify_steps.last().unwrap();
        let _ = verify_candidate(&last.pre_state, candidate_bytes, &last.ops, limits)?;
        // Also enforce unchanged raw ZIP entry preservation against doc.original
        let cand_arch = Archive::open(candidate_bytes.to_vec(), limits)
            .map_err(|e| EngineError::new(Code::VerifyFailed, Stage::Save, e.to_string()))?;
        for old in s.current.doc.original.entries() {
            let Some(new_e) = cand_arch.get(&old.name) else {
                return Err(EngineError::new(
                    Code::VerifyFailed,
                    Stage::Save,
                    "original ZIP entry deleted",
                ));
            };
            if old.method != new_e.method {
                return Err(EngineError::new(
                    Code::VerifyFailed,
                    Stage::Save,
                    format!("ZIP method changed: {}", old.name),
                ));
            }
            if old.payload() == new_e.payload()
                && (s.current.doc.original.raw_data(&old.name) != cand_arch.raw_data(&old.name)
                    || s.current.doc.original.raw_local_header(&old.name)
                        != cand_arch.raw_local_header(&old.name))
            {
                return Err(EngineError::new(
                    Code::VerifyFailed,
                    Stage::Save,
                    format!("unchanged ZIP entry was re-encoded: {}", old.name),
                ));
            }
        }
        Ok(())
    }

    fn handle_project_save(
        &mut self,
        req: &RequestEnvelope,
        p: &ProjectSaveParams,
    ) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        let target_path = match (&p.path, &s.binding) {
            (Some(path), _) => PathBuf::from(path),
            (None, Some(b)) => b.path().to_path_buf(),
            (None, None) => {
                return Err(EngineError::new(
                    Code::InvalidRequest,
                    Stage::Save,
                    "first project.save requires a target path",
                ));
            }
        };
        let limits = self.limits;
        let baseline = s.session_baseline.clone();
        let steps = s.current.verify_steps.clone();
        let doc_clone = s.current.doc.clone();
        let rev = s.revision;
        let receipt = self.roots.save_project(
            &mut s.binding,
            &target_path,
            &doc_clone,
            rev,
            |cand_bytes| {
                let dummy = OpenDocSession {
                    initial_doc: doc_clone.clone(),
                    session_baseline: baseline,
                    current: CommittedState {
                        doc: doc_clone.clone(),
                        verify_steps: steps,
                    },
                    revision: rev,
                    binding: None,
                    undo_stack: Vec::new(),
                    redo_stack: Vec::new(),
                    snapshots: BTreeMap::new(),
                };
                Self::verify_temporary_hwpx(&dummy, &limits, cand_bytes)
            },
        )?;
        Ok((
            json!({
                "path": receipt.path,
                "sha256": receipt.sha256,
                "savedVersion": receipt.revision,
                "revision": s.revision,
            }),
            Vec::new(),
        ))
    }

    fn handle_document_export(
        &mut self,
        req: &RequestEnvelope,
        p: &DocumentExportParams,
    ) -> Result<(Value, Vec<Diagnostic>)> {
        let s = Self::require_doc_mut(&mut self.open_doc, req.document_id.as_deref())?;
        let target_path = PathBuf::from(&p.path);
        let limits = self.limits;
        let baseline = s.session_baseline.clone();
        let steps = s.current.verify_steps.clone();
        let doc_clone = s.current.doc.clone();
        let rev = s.revision;
        let receipt = self
            .roots
            .export_hwpx(&target_path, &doc_clone, |cand_bytes| {
                let dummy = OpenDocSession {
                    initial_doc: doc_clone.clone(),
                    session_baseline: baseline,
                    current: CommittedState {
                        doc: doc_clone.clone(),
                        verify_steps: steps,
                    },
                    revision: rev,
                    binding: None,
                    undo_stack: Vec::new(),
                    redo_stack: Vec::new(),
                    snapshots: BTreeMap::new(),
                };
                Self::verify_temporary_hwpx(&dummy, &limits, cand_bytes)
            })?;
        Ok((
            json!({
                "path": receipt.path,
                "sha256": receipt.sha256,
                "exportedVersion": s.revision,
                "unmodifiedPassthrough": s.current.doc.is_unmodified(),
            }),
            Vec::new(),
        ))
    }
}
