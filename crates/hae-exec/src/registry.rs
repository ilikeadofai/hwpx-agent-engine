//! Single-source command registry and JSON Schema definitions (spec §6, plan 1-17).

use hae_diag::{Code, EngineError, Result, Stage};
use schemars::{
    generate::SchemaSettings,
    transform::{RecursiveTransform, RestrictFormats},
    JsonSchema, Schema,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use hae_model::query::{
    ColumnSettings, DocumentStatsParams, DocumentStatsResult, NoteItem, NoteListParams,
    NoteListResult, PageMargin, PageSettings, SectionInfo, SectionListParams, SectionListResult,
};

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
    pub schema_version: &'static str,
    pub name: &'static str,
    pub cli_name: &'static str,
    pub mcp_tool_name: &'static str,
    pub summary: &'static str,
    pub category: CommandCategory,
    pub mutates_content: bool,
    pub annotations: ToolAnnotations,
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

/// Conservative MCP hints, generated alongside command policies in the registry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
    pub open_world_hint: bool,
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Controls {
    #[default]
    Reject,
    Preserve,
}
impl Controls {
    pub fn preserving(self) -> bool {
        matches!(self, Self::Preserve)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextReplaceParams {
    #[serde(default)]
    pub controls: Controls,
    pub target: String,
    pub range: TextRangeParam,
    pub text: String,
}

/// Character properties are deliberately bounded; fontRef is not supported.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharPropsParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strikeout: Option<bool>,
    #[schemars(range(min = 100, max = 100000))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height_hwpunit: Option<u32>,
    #[schemars(regex(pattern = r"^#[0-9A-Fa-f]{6}$"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFormatParams {
    pub target: String,
    pub range: TextRangeParam,
    pub props: CharPropsParam,
    #[serde(default)]
    pub controls: Controls,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableSetColumnWidthsParams {
    pub target: String,
    pub widths: Option<Vec<u32>>,
    pub width: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableSetRowHeightsParams {
    pub target: String,
    pub heights: Option<Vec<u32>>,
    pub height: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellSetSizeParams {
    pub target: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BorderFillId {
    String(String),
    Number(u32),
}
impl BorderFillId {
    pub fn value(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Number(n) => n.to_string(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellMarginParam {
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub top: Option<u32>,
    pub bottom: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellSetFormatParams {
    pub target: String,
    pub border_fill_id: Option<BorderFillId>,
    pub background: Option<String>,
    pub cell_margin: Option<CellMarginParam>,
}

pub(crate) fn reject_structure_change(name: &str) -> Result<()> {
    if matches!(
        name,
        "table.addRow"
            | "table.deleteRow"
            | "table.addColumn"
            | "table.deleteColumn"
            | "table.insertRow"
            | "table.insertColumn"
            | "table.insertRows"
            | "table.deleteRows"
            | "table.insertColumns"
            | "table.deleteColumns"
            | "table.addRows"
            | "table.addColumns"
            | "table.removeRow"
            | "table.removeColumn"
            | "table.removeRows"
            | "table.removeColumns"
    ) {
        return Err(EngineError::new(
            Code::Unsupported,
            Stage::Validate,
            "row/column creation or deletion requires a separate identity/history contract",
        )
        .with_reason("structure_change_unsupported"));
    }
    Ok(())
}

/// Exact-match search and replace over the document or one subtree.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextReplaceAllParams {
    /// Non-empty text to find, compared by exact Unicode codepoints.
    pub find: String,
    /// Replacement (no tabs or line breaks); may be empty to delete matches.
    pub text: String,
    /// Optional nodeId; only that node (if a paragraph) and its descendant
    /// paragraphs, including table cells, are searched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Reject before committing when more replaceable matches exist
    /// (default 1000, at most 10000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_matches: Option<usize>,
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
    #[serde(default)]
    pub controls: Controls,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraphs: Option<Vec<String>>,
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
    #[schemars(with = "std::collections::BTreeMap<String, Value>")]
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

/// Result of `text.replaceAll` (batch results carry the same object per step in
/// `replaceAll`). Ranges are grapheme ranges of the paragraph before the command.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceAllResultShape {
    pub outcome: String,
    pub before_version: u64,
    pub after_version: u64,
    pub changed_ids: Vec<String>,
    pub history_evicted: bool,
    pub find: String,
    pub replacement: String,
    pub scope: Option<String>,
    pub max_matches: usize,
    pub replacement_count: usize,
    pub paragraph_count: usize,
    pub skipped_count: usize,
    /// Per reason (`crosses_control`, `grapheme_boundary`, `unsupported_markup`,
    /// `paragraph_not_editable`): `count`, up to 50 `locations` {nodeId, range}.
    pub skipped: Vec<Value>,
    /// Up to 200 {nodeId, range, before, after} previews in document order.
    pub matches: Vec<Value>,
    pub matches_truncated: bool,
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
    DocumentStats(DocumentStatsParams),
    SectionList(SectionListParams),
    NoteList(NoteListParams),
    DocumentClose(EmptyParams),
    ElementQuery(ElementQueryParams),
    ElementGet(ElementGetParams),
    TableSetColumnWidths(TableSetColumnWidthsParams),
    TableSetRowHeights(TableSetRowHeightsParams),
    CellSetFormat(CellSetFormatParams),
    CellSetSize(CellSetSizeParams),
    TextFormat(TextFormatParams),
    TextReplace(TextReplaceParams),
    TextReplaceAll(TextReplaceAllParams),
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

/// Preserve Rust integer ranges as validation keywords before removing the
/// generator's nonstandard format annotations. Walk schema positions only.
fn integer_bounds(schema: &mut Schema) {
    macro_rules! bounds {
        ($ty:ty) => {
            (serde_json::json!(<$ty>::MIN), serde_json::json!(<$ty>::MAX))
        };
    }
    let (minimum, maximum) = match schema.get("format").and_then(Value::as_str) {
        Some("uint") => bounds!(usize),
        Some("uint8") => bounds!(u8),
        Some("uint16") => bounds!(u16),
        Some("uint32") => bounds!(u32),
        Some("uint64") => bounds!(u64),
        Some("uint128") => bounds!(u128),
        Some("int") => bounds!(isize),
        Some("int8") => bounds!(i8),
        Some("int16") => bounds!(i16),
        Some("int32") => bounds!(i32),
        Some("int64") => bounds!(i64),
        Some("int128") => bounds!(i128),
        _ => return,
    };
    let object = schema.ensure_object();
    // Explicit command-specific constraints may be tighter than the Rust type.
    object.entry("minimum").or_insert(minimum);
    object.entry("maximum").or_insert(maximum);
}

fn schema_json<T: JsonSchema>() -> Value {
    let schema = SchemaSettings::draft2020_12()
        .with_transform(RecursiveTransform(integer_bounds))
        .with_transform(RestrictFormats::default())
        .into_generator()
        .into_root_schema_for::<T>();
    let mut schema = serde_json::to_value(schema).unwrap_or(Value::Null);
    if schema["title"] == "CellSetTextParams" {
        schema["oneOf"] = serde_json::json!([
            {"required":["text"],"not":{"required":["paragraphs"]}},
            {"required":["paragraphs"],"not":{"required":["text"]}}
        ]);
        schema["properties"]["text"] = serde_json::json!({"type":"string"});
        schema["properties"]["paragraphs"] =
            serde_json::json!({"type":"array","items":{"type":"string"}});
    }
    if schema["title"] == "TextReplaceAllParams" {
        schema["properties"]["find"] = serde_json::json!({"type":"string","minLength":1});
        schema["properties"]["maxMatches"] =
            serde_json::json!({"type":"integer","minimum":1,"maximum":10000,"default":1000});
    }
    if matches!(
        schema["title"].as_str(),
        Some("TableSetColumnWidthsParams" | "TableSetRowHeightsParams")
    ) {
        let (many, one) = if schema["title"] == "TableSetColumnWidthsParams" {
            ("widths", "width")
        } else {
            ("heights", "height")
        };
        schema["oneOf"] = serde_json::json!([{"required":[many],"not":{"required":[one]}},{"required":[one],"not":{"required":[many]}}]);
        schema["properties"][many] = serde_json::json!({"type":"array","minItems":1,"maxItems":1024,"items":{"type":"integer","minimum":1,"maximum":2147483647}});
        schema["properties"][one] =
            serde_json::json!({"type":"integer","minimum":1,"maximum":2147483647});
    }
    if schema["title"] == "CellSetSizeParams" {
        schema["anyOf"] = serde_json::json!([{"required":["width"]},{"required":["height"]}]);
        for side in ["width", "height"] {
            schema["properties"][side] =
                serde_json::json!({"type":"integer","minimum":1,"maximum":2147483647});
        }
    }
    schema
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn all_tool_annotations_match_behavior_audit() {
        // (readOnly, destructive, idempotent, openWorld), audited against
        // session dispatch, snapshot lifetime, and file publication behavior.
        let expected = [
            ("command_list", [true, false, true, false]),
            ("command_describe", [true, false, true, false]),
            ("document_open", [false, true, false, false]),
            ("document_inspect", [true, false, true, false]),
            ("document_stats", [true, false, true, false]),
            ("section_list", [true, false, true, false]),
            ("note_list", [true, false, true, false]),
            ("document_close", [false, true, false, false]),
            ("element_query", [true, false, true, false]),
            ("element_get", [true, false, true, false]),
            ("text_replace", [false, true, false, false]),
            ("text_format", [false, true, false, false]),
            ("table_set_column_widths", [false, true, false, false]),
            ("table_set_row_heights", [false, true, false, false]),
            ("cell_set_format", [false, true, false, false]),
            ("cell_set_size", [false, true, false, false]),
            ("text_replace_all", [false, true, false, false]),
            ("paragraph_format", [false, true, false, false]),
            ("cell_set_text", [false, true, false, false]),
            ("image_replace", [false, true, false, false]),
            ("command_batch", [false, true, false, false]),
            ("history_undo", [false, true, false, false]),
            ("history_redo", [false, true, false, false]),
            ("history_clear", [false, true, false, false]),
            ("document_snapshot", [false, false, false, false]),
            ("snapshot_release", [false, true, false, false]),
            ("document_diff", [true, false, true, false]),
            ("document_preview", [false, false, false, false]),
            ("project_save", [false, true, false, false]),
            ("document_export", [false, false, false, false]),
        ];
        let commands = command_list();
        assert_eq!(commands.len(), expected.len());
        for (name, hints) in expected {
            let a = &commands
                .iter()
                .find(|d| d.mcp_tool_name == name)
                .unwrap()
                .annotations;
            assert_eq!(
                [
                    a.read_only_hint,
                    a.destructive_hint,
                    a.idempotent_hint,
                    a.open_world_hint
                ],
                hints,
                "{name}"
            );
        }
    }

    #[test]
    fn generated_integer_schemas_express_rust_bounds_with_standard_keywords() {
        for (schema, minimum, maximum) in [
            (schema_json::<usize>(), json!(0), json!(usize::MAX)),
            (schema_json::<u32>(), json!(0), json!(u32::MAX)),
            (schema_json::<u64>(), json!(0), json!(u64::MAX)),
            (schema_json::<i64>(), json!(i64::MIN), json!(i64::MAX)),
        ] {
            assert_eq!(schema["type"], "integer");
            assert!(schema.get("format").is_none(), "{schema}");
            assert_eq!(schema["minimum"], minimum);
            assert_eq!(schema["maximum"], maximum);
        }
    }
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
                        schema_version: "0.2.0",
                        name: $name,
                        cli_name: $cli,
                        mcp_tool_name: $mcp,
                        summary: $summary,
                        category: CommandCategory::$cat,
                        mutates_content: $mutates,
                        annotations: {
                            let read_only = matches!(CommandCategory::$cat, CommandCategory::Introspection | CommandCategory::Query) || $name == "document.diff";
                            ToolAnnotations {
                                read_only_hint: read_only,
                                destructive_hint: $mutates || matches!(CommandCategory::$cat, CommandCategory::History | CommandCategory::Lifecycle) || matches!($name, "project.save" | "snapshot.release"),
                                idempotent_hint: read_only,
                                open_world_hint: false,
                            }
                        },
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
            reject_structure_change(name)?;
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
    DocumentStats {
        name: "document.stats",
        cli: "document-stats",
        mcp: "document_stats",
        summary: "Return document summary metrics across structural elements and character count",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: DocumentStatsParams, output: DocumentStatsResult,
    },
    SectionList {
        name: "section.list",
        cli: "section-list",
        mcp: "section_list",
        summary: "List document sections with paper dimensions, margins, and column layouts",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: SectionListParams, output: SectionListResult,
    },
    NoteList {
        name: "note.list",
        cli: "note-list",
        mcp: "note_list",
        summary: "List footnotes and endnotes with numbering, body paragraphs, and references",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: NoteListParams, output: NoteListResult,
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
    TableSetColumnWidths {
        name: "table.setColumnWidths", cli: "table-set-column-widths", mcp: "table_set_column_widths",
        summary: "Set positive absolute column widths (exact widths array or uniform width); resize merged cells by track sums and report adjusted tableSize/changedCells; no new nodes", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: TableSetColumnWidthsParams, output: MutationResultShape,
    },
    TableSetRowHeights {
        name: "table.setRowHeights", cli: "table-set-row-heights", mcp: "table_set_row_heights",
        summary: "Set positive row minimum heights (exact heights array or uniform height); synchronize cellSz and table height; effective margins must leave content space", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: TableSetRowHeightsParams, output: MutationResultShape,
    },
    CellSetFormat {
        name: "cell.setFormat", cli: "cell-set-format", mcp: "cell_set_format",
        summary: "Set existing borderFillId, background #RRGGBB or none via an existing matching resource, and partial cellMargin; never create or mutate shared resources", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: CellSetFormatParams, output: MutationResultShape,
    },
    CellSetSize {
        name: "cell.setSize", cli: "cell-set-size", mcp: "cell_set_size",
        summary: "Resize a nonmerged cell by changing its whole column/row tracks and table total; report affected cells; merged anchors explicitly unsupported", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: CellSetSizeParams, output: MutationResultShape,
    },
    TextFormat {
        name: "text.format", cli: "text-format", mcp: "text_format",
        summary: "Format a non-empty grapheme range: bold/italic/underline/strikeout, heightHwpunit 100..100000, textColor #RRGGBB; reuse or add at most one charPr; preserve skips controls and rejects field boundaries",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: TextFormatParams, output: MutationResultShape,
    },
    TextReplace {
        name: "text.replace",
        cli: "text-replace",
        mcp: "text_replace",
        summary: "Replace a grapheme range or insert non-empty text at a zero-length range",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: TextReplaceParams, output: MutationResultShape,
    },
    TextReplaceAll {
        name: "text.replaceAll",
        cli: "text-replace-all",
        mcp: "text_replace_all",
        summary: "Replace every exact match of a string in the document or a scope subtree; skips matches that cross objects or graphemes and reports them",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: TextReplaceAllParams, output: ReplaceAllResultShape,
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
        summary: "Replace existing cell paragraphs (merged allowed); use paragraphs with exact count; controls preserve keeps objects",
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

#[cfg(test)]
mod char_format_tests {
    use crate::SessionExecutor;
    use serde_json::json;

    #[test]
    fn text_format_dry_run_undo_redo_and_batch_use_the_session_gate() {
        let root = std::env::temp_dir().join(hae_model::new_id("char-format-").unwrap());
        std::fs::create_dir(&root).unwrap();
        let input = root.join("input.hwpx");
        let bytes = include_bytes!("../../../tests/fixtures/editing/char_format.hwpx");
        std::fs::write(&input, bytes).unwrap();
        let mut exec = SessionExecutor::new(&root, &root, hae_diag::Limits::default()).unwrap();
        assert!(
            exec.call("document.open", json!({"path":input}), None, false)
                .ok
        );
        let found = exec.call("element.query", json!({"kind":"paragraph"}), None, false);
        let id = found.result.unwrap()["matches"][0]["nodeId"].clone();
        let params = json!({"target":id,"range":{"start":1,"end":4},"props":{"bold":true}});
        let dry = exec.call("text.format", params.clone(), Some(0), true);
        assert!(dry.ok, "{:?}", dry.diagnostics);
        let out0 = root.join("dry.hwpx");
        assert!(
            exec.call("document.export", json!({"path":out0}), None, false)
                .ok
        );
        assert_eq!(std::fs::read(out0).unwrap(), bytes);
        let applied = exec.call("text.format", params.clone(), Some(0), false);
        assert!(applied.ok, "{:?}", applied.diagnostics);
        let out1 = root.join("applied.hwpx");
        assert!(
            exec.call("document.export", json!({"path":out1}), None, false)
                .ok
        );
        assert!(exec.call("history.undo", json!({}), Some(1), false).ok);
        let out2 = root.join("undo.hwpx");
        assert!(
            exec.call("document.export", json!({"path":out2}), None, false)
                .ok
        );
        assert_eq!(std::fs::read(out2).unwrap(), bytes);
        assert!(exec.call("history.redo", json!({}), Some(2), false).ok);
        let out3 = root.join("redo.hwpx");
        assert!(
            exec.call("document.export", json!({"path":out3}), None, false)
                .ok
        );
        assert_eq!(std::fs::read(out1).unwrap(), std::fs::read(out3).unwrap());
        let batch = exec.call("command.batch", json!({"steps":[
            {"command":"text.format","params":{"target":id,"range":{"start":0,"end":1},"props":{"italic":true}}},
            {"command":"text.replace","params":{"target":id,"range":{"start":4,"end":6},"text":"XY"}}
        ]}), Some(3), false);
        assert!(batch.ok, "{:?}", batch.diagnostics);
        assert_eq!(batch.result.unwrap()["afterVersion"], 4);
        std::fs::remove_dir_all(root).unwrap();
    }
}
