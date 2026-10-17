//! Single-source command registry and JSON Schema definitions (spec §6, plan 1-17).

use hae_diag::{Code, EngineError, Result, Stage};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CommandCategory {
    Introspection,
    Lifecycle,
    Query,
    Mutation,
    History,
    Snapshot,
    Persistence,
    Preview,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommandDescriptor {
    pub name: &'static str,
    pub cli_name: &'static str,
    pub mcp_tool_name: &'static str,
    pub summary: &'static str,
    pub category: CommandCategory,
    pub mutates_content: bool,
    pub requires_document: bool,
    pub requires_expected_version: bool,
    pub uses_request_sequence: bool,
    pub supports_dry_run: bool,
    pub records_history: bool,
    pub supported_in_stage1: bool,
    /// Available with the current adapter configuration. Runtime discovery
    /// updates preview availability without changing historical stage metadata.
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<&'static str>,
    pub input_schema: Value,
    pub output_schema: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmptyParams {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandDescribeParams {
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentOpenParams {
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentInspectParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementQueryParams {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub text_contains: Option<String>,
    #[serde(default)]
    pub ancestor: Option<String>,
    #[serde(default)]
    pub table: Option<String>,
    #[serde(default)]
    pub row: Option<u32>,
    #[serde(default)]
    pub col: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElementGetParams {
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRangeParam {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextReplaceParams {
    pub target: String,
    pub range: TextRangeParam,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineSpacingParam {
    pub kind: String,
    pub value: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphFormatParams {
    pub target: String,
    #[serde(default)]
    pub alignment: Option<String>,
    #[serde(default)]
    pub line_spacing: Option<LineSpacingParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellSetTextParams {
    pub target: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageReplaceParams {
    pub target: String,
    pub image_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchStepParam {
    pub command: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandBatchParams {
    pub steps: Vec<BatchStepParam>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotCreateParams {
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotReleaseParams {
    pub snapshot_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentDiffParams {
    #[serde(default)]
    pub base_snapshot_id: Option<String>,
    #[serde(default)]
    pub target_snapshot_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentPreviewParams {
    /// 1-based pages to return inline; default `[1]`.
    #[serde(default)]
    pub pages: Option<Vec<u32>>,
    /// Include each returned page's PNG as base64 (default true).
    #[serde(default)]
    pub include_png: Option<bool>,
    /// Include each returned page's checked SVG text (default false).
    #[serde(default)]
    pub include_svg: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSaveParams {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentExportParams {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MutationResultShape {
    pub outcome: String,
    pub before_version: u64,
    pub after_version: u64,
    pub changed_ids: Vec<String>,
    pub history_evicted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenericResultShape {
    pub revision: Option<u64>,
}

/// Discovery schemas describe the command result inside ResponseEnvelope.result.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InspectResultShape {
    pub document_id: String,
    pub revision: u64,
    /// Document-wide permission; node editability is in each Outline.edits.
    pub editable: bool,
    pub readonly_reasons: Vec<String>,
    pub items: Vec<hae_model::query::Outline>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryResultShape {
    pub document_id: String,
    pub revision: u64,
    pub matches: Vec<hae_model::query::Outline>,
    pub unaligned_matches: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetResultShape {
    pub document_id: String,
    pub revision: u64,
    pub outline: hae_model::query::Outline,
    pub text: String,
    pub grapheme_count: Option<usize>,
    pub alignment: Option<String>,
    pub line_spacing_percent: Option<u32>,
    pub children: Vec<String>,
}

/// Parsed, strongly typed command enum. No arbitrary string-to-function dispatch.
#[derive(Debug, Clone)]
pub enum ParsedCommand {
    CommandList(EmptyParams),
    CommandDescribe(CommandDescribeParams),
    DocumentOpen(DocumentOpenParams),
    DocumentInspect(DocumentInspectParams),
    DocumentClose(EmptyParams),
    ElementQuery(ElementQueryParams),
    ElementGet(ElementGetParams),
    TextReplace(TextReplaceParams),
    ParagraphFormat(ParagraphFormatParams),
    CellSetText(CellSetTextParams),
    ImageReplace(ImageReplaceParams),
    CommandBatch(CommandBatchParams),
    HistoryUndo(EmptyParams),
    HistoryRedo(EmptyParams),
    HistoryClear(EmptyParams),
    DocumentSnapshot(SnapshotCreateParams),
    SnapshotRelease(SnapshotReleaseParams),
    DocumentDiff(DocumentDiffParams),
    DocumentPreview(DocumentPreviewParams),
    ProjectSave(ProjectSaveParams),
    DocumentExport(DocumentExportParams),
}

fn schema_json<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).unwrap_or(Value::Null)
}

fn parse_params<T: for<'de> Deserialize<'de>>(params: Value, cmd: &str) -> Result<T> {
    let v = if params.is_null() {
        serde_json::json!({})
    } else {
        params
    };
    serde_json::from_value(v).map_err(|e| {
        EngineError::new(
            Code::InvalidRequest,
            Stage::Request,
            format!("invalid params for {cmd}: {e}"),
        )
    })
}

macro_rules! define_commands {
    ($(
        $variant:ident {
            name: $name:literal,
            cli: $cli:literal,
            mcp: $mcp:literal,
            summary: $summary:literal,
            category: $cat:ident,
            mutates: $mutates:literal,
            doc: $doc:literal,
            ver: $ver:literal,
            seq: $seq:literal,
            dry: $dry:literal,
            hist: $hist:literal,
            stage1: $s1:literal,
            reason: $reason:expr,
            params: $params:ty,
            output: $out:ty,
        }
    ),* $(,)?) => {
        pub const COMMAND_NAMES: &[&str] = &[ $( $name ),* ];

        pub fn command_list() -> Vec<CommandDescriptor> {
            vec![
                $(
                    CommandDescriptor {
                        name: $name,
                        cli_name: $cli,
                        mcp_tool_name: $mcp,
                        summary: $summary,
                        category: CommandCategory::$cat,
                        mutates_content: $mutates,
                        requires_document: $doc,
                        requires_expected_version: $ver,
                        uses_request_sequence: $seq,
                        supports_dry_run: $dry,
                        records_history: $hist,
                        supported_in_stage1: $s1,
                        available: $s1,
                        unsupported_reason: $reason,
                        input_schema: schema_json::<$params>(),
                        output_schema: schema_json::<$out>(),
                    }
                ),*
            ]
        }

        pub fn command_describe(name: &str) -> Result<CommandDescriptor> {
            command_list()
                .into_iter()
                .find(|c| c.name == name)
                .ok_or_else(|| {
                    EngineError::new(
                        Code::UnknownCommand,
                        Stage::Request,
                        format!("unknown command: {name}"),
                    )
                })
        }

        impl ParsedCommand {
            pub fn parse(name: &str, params: Value) -> Result<(Self, CommandDescriptor)> {
                let desc = command_describe(name)?;
                let parsed = match name {
                    $(
                        $name => ParsedCommand::$variant(parse_params::<$params>(params, $name)?),
                    )*
                    _ => unreachable!(),
                };
                Ok((parsed, desc))
            }
        }
    };
}

define_commands! {
    CommandList {
        name: "command.list",
        cli: "command-list",
        mcp: "command_list",
        summary: "List all registered commands and their capabilities",
        category: Introspection,
        mutates: false, doc: false, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: EmptyParams, output: GenericResultShape,
    },
    CommandDescribe {
        name: "command.describe",
        cli: "command-describe",
        mcp: "command_describe",
        summary: "Describe one command's input/output JSON Schema and policies",
        category: Introspection,
        mutates: false, doc: false, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: CommandDescribeParams, output: CommandDescriptor,
    },
    DocumentOpen {
        name: "document.open",
        cli: "document-open",
        mcp: "document_open",
        summary: "Open an .hwpx document or .hwpwork project into the session",
        category: Lifecycle,
        mutates: false, doc: false, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: DocumentOpenParams, output: GenericResultShape,
    },
    DocumentInspect {
        name: "document.inspect",
        cli: "document-inspect",
        mcp: "document_inspect",
        summary: "Return paged structural outline of the open document",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: DocumentInspectParams, output: InspectResultShape,
    },
    DocumentClose {
        name: "document.close",
        cli: "document-close",
        mcp: "document_close",
        summary: "Close the open document and release any bound project lock",
        category: Lifecycle,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: EmptyParams, output: GenericResultShape,
    },
    ElementQuery {
        name: "element.query",
        cli: "element-query",
        mcp: "element_query",
        summary: "Query elements by kind, codepoint substring, ancestor, or table cell",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: ElementQueryParams, output: QueryResultShape,
    },
    ElementGet {
        name: "element.get",
        cli: "element-get",
        mcp: "element_get",
        summary: "Get detailed properties and full text of one element by nodeId",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: ElementGetParams, output: GetResultShape,
    },
    TextReplace {
        name: "text.replace",
        cli: "text-replace",
        mcp: "text_replace",
        summary: "Replace a non-empty grapheme range within a paragraph",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: TextReplaceParams, output: MutationResultShape,
    },
    ParagraphFormat {
        name: "paragraph.format",
        cli: "paragraph-format",
        mcp: "paragraph_format",
        summary: "Change paragraph alignment and/or percent line spacing",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: ParagraphFormatParams, output: MutationResultShape,
    },
    CellSetText {
        name: "cell.setText",
        cli: "cell-set-text",
        mcp: "cell_set_text",
        summary: "Replace the full text of an unmerged single-paragraph table cell",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: CellSetTextParams, output: MutationResultShape,
    },
    ImageReplace {
        name: "image.replace",
        cli: "image-replace",
        mcp: "image_replace",
        summary: "Replace a picture's media with a validated static PNG or JPEG",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: ImageReplaceParams, output: MutationResultShape,
    },
    CommandBatch {
        name: "command.batch",
        cli: "command-batch",
        mcp: "command_batch",
        summary: "Apply multiple edit steps atomically as one revision and history unit",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: CommandBatchParams, output: MutationResultShape,
    },
    HistoryUndo {
        name: "history.undo",
        cli: "history-undo",
        mcp: "history_undo",
        summary: "Restore the previous committed document snapshot and advance revision",
        category: History,
        mutates: true, doc: true, ver: true, seq: true, dry: false, hist: false,
        stage1: true, reason: None,
        params: EmptyParams, output: MutationResultShape,
    },
    HistoryRedo {
        name: "history.redo",
        cli: "history-redo",
        mcp: "history_redo",
        summary: "Restore the next undone document snapshot without re-running commands",
        category: History,
        mutates: true, doc: true, ver: true, seq: true, dry: false, hist: false,
        stage1: true, reason: None,
        params: EmptyParams, output: MutationResultShape,
    },
    HistoryClear {
        name: "history.clear",
        cli: "history-clear",
        mcp: "history_clear",
        summary: "Clear all undo and redo history units without changing content revision",
        category: History,
        mutates: false, doc: true, ver: false, seq: true, dry: false, hist: false,
        stage1: true, reason: None,
        params: EmptyParams, output: GenericResultShape,
    },
    DocumentSnapshot {
        name: "document.snapshot",
        cli: "document-snapshot",
        mcp: "document_snapshot",
        summary: "Capture an explicit in-session snapshot for later diffing",
        category: Snapshot,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: SnapshotCreateParams, output: GenericResultShape,
    },
    SnapshotRelease {
        name: "snapshot.release",
        cli: "snapshot-release",
        mcp: "snapshot_release",
        summary: "Release an explicit snapshot by snapshotId using requestSequence",
        category: Snapshot,
        mutates: false, doc: true, ver: false, seq: true, dry: false, hist: false,
        stage1: true, reason: None,
        params: SnapshotReleaseParams, output: GenericResultShape,
    },
    DocumentDiff {
        name: "document.diff",
        cli: "document-diff",
        mcp: "document_diff",
        summary: "Compare semantic content, media, and cache changes between snapshots",
        category: Snapshot,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: DocumentDiffParams, output: GenericResultShape,
    },
    DocumentPreview {
        name: "document.preview",
        cli: "document-preview",
        mcp: "document_preview",
        summary: "Render SVG/PNG previews with a configured adapter; otherwise RENDERER_UNAVAILABLE (renderer_not_configured)",
        category: Preview,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: false, reason: Some("renderer_not_configured"),
        params: DocumentPreviewParams, output: GenericResultShape,
    },
    ProjectSave {
        name: "project.save",
        cli: "project-save",
        mcp: "project_save",
        summary: "Atomically save and verify a .hwpwork project file",
        category: Persistence,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: ProjectSaveParams, output: GenericResultShape,
    },
    DocumentExport {
        name: "document.export",
        cli: "document-export",
        mcp: "document_export",
        summary: "Export the current document to a new .hwpx file after independent verification",
        category: Persistence,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: DocumentExportParams, output: GenericResultShape,
    },
}
