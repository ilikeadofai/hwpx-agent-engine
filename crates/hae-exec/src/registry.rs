//! Single-source command registry and JSON Schema definitions (spec §6, plan 1-17).

use hae_diag::{Code, EngineError, Result, Stage};
use schemars::{
    generate::SchemaSettings,
    transform::{RecursiveTransform, RestrictFormats},
    JsonSchema, Schema,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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

/// HWP5 inspection reads a file directly: no open document is required and the
/// file is never modified.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hwp5InspectParams {
    /// Path to a legacy HWP 5.0 (`.hwp`) file inside the input root.
    pub path: String,
    /// Cursor from a previous reply's `nextCursor` (bound to the file SHA-256).
    #[serde(default)]
    pub cursor: Option<String>,
    /// Page size for `paragraphs` (default 100, clamped to 1..=500).
    #[schemars(range(min = 1, max = 500))]
    #[serde(default)]
    pub limit: Option<usize>,
    /// Maximum characters of each paragraph preview (default 120, 1..=1000).
    #[schemars(range(min = 1, max = 1000))]
    #[serde(default)]
    pub preview_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hwp5FileHeaderShape {
    pub version: String,
    pub compressed: bool,
    pub encrypted: bool,
    pub distribution: bool,
    pub drm: bool,
    pub properties: u32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hwp5SectionShape {
    pub index: usize,
    pub paragraph_count: u64,
    pub char_count: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hwp5ParagraphShape {
    pub section_index: usize,
    pub paragraph_index: u64,
    pub level: u16,
    pub char_count: u64,
    pub preview: String,
    pub preview_truncated: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Hwp5InspectResultShape {
    pub path: String,
    pub sha256: String,
    pub file_header: Hwp5FileHeaderShape,
    pub section_count: usize,
    pub paragraph_count: u64,
    pub char_count: u64,
    pub sections: Vec<Hwp5SectionShape>,
    pub paragraphs: Vec<Hwp5ParagraphShape>,
    pub next_cursor: Option<String>,
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

/// Shared paging + subtree scope for read-only list views.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkListParams {
    /// `revision:offset` cursor from the previous reply's `nextCursor`.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Page size (default 100, clamped to 1..=500).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional nodeId; only that node and its descendants are listed.
    #[serde(default)]
    pub scope: Option<String>,
}

/// Same paging/scope contract as `LinkListParams`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackChangeListParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Same paging/scope contract as `LinkListParams` (bookmark markers).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkListParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Same paging/scope contract as `LinkListParams` (memo controls).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoListParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub scope: Option<String>,
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

/// Header resources; count actual XML reference occurrences, not characters.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleListParams {
    pub kind: Option<hae_model::resource_query::ResourceKind>,
    #[schemars(range(min = 1, max = 500))]
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableGetParams {
    /// Section or paragraph nodeId.
    pub scope: Option<String>,
    /// One table nodeId; if scope is given it must contain this table.
    pub table: Option<String>,
    #[schemars(range(min = 1, max = 500))]
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PictureListParams {
    /// Section or paragraph nodeId; duplicate-media references remain document-wide.
    pub scope: Option<String>,
    #[schemars(range(min = 1, max = 500))]
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PagedQueryResult<T> {
    pub document_id: String,
    pub revision: u64,
    pub items: Vec<T>,
    pub total: usize,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StyleListResult {
    #[serde(flatten)]
    pub page: PagedQueryResult<hae_model::resource_query::StyleResource>,
    pub unresolved_reference_count: usize,
    pub unresolved_body_reference_count: usize,
    pub unresolved_examples: Vec<hae_model::resource_query::UnresolvedReference>,
    pub unresolved_examples_truncated: bool,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldListParams {
    #[serde(default)]
    pub scope: Option<String>,
    /// Paging cursor from a previous `nextCursor` (`revision:index`).
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldSetTextParams {
    pub target: String,
    pub text: String,
    pub name: Option<String>,
    pub fieldid: Option<String>,
    pub occurrence: Option<usize>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldListResultShape {
    pub document_id: String,
    pub revision: u64,
    pub fields: Vec<hae_model::fields::FieldInfo>,
    pub next_cursor: Option<String>,
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
macro_rules! structure_params {
    ($($name:ident),*) => {$(
        /// `index` is the 0-based address the first new line gets (insert) or
        /// the first line removed (delete); `count` defaults to 1.
        #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name {
            pub target: String,
            pub index: usize,
            pub count: Option<usize>,
        }
    )*};
}
structure_params!(
    TableInsertRowParams,
    TableDeleteRowParams,
    TableInsertColumnParams,
    TableDeleteColumnParams
);
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
    // `table.insertRow`/`deleteRow`/`insertColumn`/`deleteColumn` exist (0.3);
    // these spellings and the plural/bulk forms do not.
    if matches!(
        name,
        "table.addRow"
            | "table.addColumn"
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
            "no such table structure command; use table.insertRow, table.deleteRow, table.insertColumn or table.deleteColumn with index and count",
        )
        .with_reason("structure_change_unsupported"));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionSetPageParams {
    pub scope: Option<String>,
    pub section_index: Option<usize>,
    #[schemars(regex(pattern = r"^(NARROWLY|WIDELY)$"))]
    pub landscape: Option<String>,
    #[schemars(range(min = 1))]
    pub width: Option<u32>,
    #[schemars(range(min = 1))]
    pub height: Option<u32>,
    #[schemars(regex(pattern = r"^(LEFT_ONLY|LEFT_RIGHT|TOP_BOTTOM)$"))]
    pub gutter_type: Option<String>,
    pub margins: Option<Margins>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Margins {
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub top: Option<u32>,
    pub bottom: Option<u32>,
    pub header: Option<u32>,
    pub footer: Option<u32>,
    pub gutter: Option<u32>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionSetPageNumberingParams {
    pub scope: Option<String>,
    pub section_index: Option<usize>,
    #[schemars(regex(pattern = r"^(BOTH|EVEN|ODD)$"))]
    pub page_starts_on: Option<String>,
    pub page: Option<u32>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionSetVisibilityParams {
    pub scope: Option<String>,
    pub section_index: Option<usize>,
    pub hide_first_header: Option<bool>,
    pub hide_first_footer: Option<bool>,
    pub hide_first_page_num: Option<bool>,
    pub hide_first_empty_line: Option<bool>,
    pub show_line_number: Option<bool>,
    #[schemars(regex(pattern = r"^(SHOW_ALL|HIDE_ALL|SHOW_FIRST|HIDE_FIRST)$"))]
    pub border: Option<String>,
    #[schemars(regex(pattern = r"^(SHOW_ALL|HIDE_ALL|SHOW_FIRST|HIDE_FIRST)$"))]
    pub fill: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionGetPageParams {
    pub scope: Option<String>,
    pub section_index: Option<usize>,
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
pub struct ParagraphSetStyleParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<String>>,
    pub style_id: u32,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FitModeParam {
    Contain,
    Cover,
    Stretch,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageFitParams {
    pub target: String,
    pub mode: FitModeParam,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HeaderKindParam {
    Header,
    Footer,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderSetTextParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub kind: HeaderKindParam,
    #[serde(default)]
    pub paragraph_index: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageReplaceParams {
    pub target: String,
    pub image_path: String,
}

fn strict_default() -> bool {
    true
}

/// Label → value form fill over the document, one table, or one section.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormFillParams {
    /// Label cell text (exact codepoints) → value for the adjacent empty cell.
    pub values: BTreeMap<String, String>,
    /// Optional table or section nodeId; default searches every table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// true (default) rejects every label when one fails; false fills the
    /// resolvable labels and reports the rest.
    #[serde(default = "strict_default")]
    pub strict: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportTextFormat {
    Markdown,
    Html,
}

/// `document.exportText`: write the document as Markdown or HTML to a new file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentExportTextParams {
    /// New file under `--output-root` (absolute path; never overwritten).
    /// Must end in `.md`/`.markdown` for `markdown` or `.html`/`.htm` for `html`.
    pub path: String,
    pub format: ExportTextFormat,
    /// Section nodeId; only that section is exported. Default: every section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Include footnotes and endnotes (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_notes: Option<bool>,
}

/// Result of `document.exportText`; the text itself is only in the file.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportTextResultShape {
    pub path: String,
    pub format: ExportTextFormat,
    pub bytes: u64,
    pub sha256: String,
    /// Document revision that was read; unchanged by this command.
    pub revision: u64,
    pub scope: Option<String>,
    pub include_notes: bool,
    pub counts: hae_model::export_text::ExportCounts,
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

/// Result of the four table structure commands (batch results carry the same
/// objects per step in `structure`). In `dryRun` the created `nodeId`s are
/// null because the candidate is discarded.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TableStructureResultShape {
    pub outcome: String,
    pub before_version: u64,
    pub after_version: u64,
    pub changed_ids: Vec<String>,
    pub history_evicted: bool,
    /// Per step: `command`, `table`, `index`, `count`, `rowsBefore/After`,
    /// `columnsBefore/After`, `created` [{nodeId, kind, row, col}], `removed`
    /// [nodeId], `shifted` [nodeId of surviving cells whose address moved].
    pub structure: Vec<Value>,
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

/// Result of `form.fill` (batch results carry the same object per step in
/// `formFill`). `filled`/`skipped` are arrays of objects.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FormFillResultShape {
    pub outcome: String,
    pub before_version: u64,
    pub after_version: u64,
    pub changed_ids: Vec<String>,
    pub history_evicted: bool,
    pub strict: bool,
    pub scope: Option<String>,
    /// {label, value, nodeId, entry, path} per filled cell.
    pub filled: Vec<Value>,
    /// {label, reason, detail?} per label that could not be filled.
    pub skipped: Vec<Value>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EquationListParams {
    pub scope: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EquationSetScriptParams {
    pub target: String,
    pub script: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EquationPropsParam {
    pub base_line: Option<u32>,
    #[schemars(regex(pattern = r"^#[0-9A-Fa-f]{6}$"))]
    pub text_color: Option<String>,
    pub font: Option<String>,
    #[schemars(range(min = 1))]
    pub base_unit: Option<u32>,
    #[schemars(regex(pattern = r"^(CHAR|LINE)$"))]
    pub line_mode: Option<String>,
    pub version: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EquationSetPropertiesParams {
    pub target: String,
    pub props: EquationPropsParam,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquationListResultShape {
    pub document_id: String,
    pub revision: u64,
    pub items: Vec<hae_model::equation::EquationInfo>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquationGetResultShape {
    pub document_id: String,
    pub revision: u64,
    pub equation: hae_model::equation::EquationInfo,
}

/// One page of HYPERLINK fields (`link.list`); `total` counts the filtered
/// matches, not the page.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkListResultShape {
    pub document_id: String,
    pub revision: u64,
    pub links: Vec<hae_model::links::LinkInfo>,
    pub total: usize,
    pub next_cursor: Option<String>,
}

/// One page of bookmark markers (`bookmark.list`); `total` counts the filtered
/// markers, not the page.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkListResultShape {
    pub document_id: String,
    pub revision: u64,
    pub bookmarks: Vec<hae_model::annotations::BookmarkInfo>,
    pub total: usize,
    pub next_cursor: Option<String>,
}

/// One page of memos (`memo.list`); `total` counts the filtered memos.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoListResultShape {
    pub document_id: String,
    pub revision: u64,
    pub memos: Vec<hae_model::annotations::MemoInfo>,
    pub total: usize,
    pub next_cursor: Option<String>,
}

/// One page of track-change marks plus the header records that name their kind
/// and author (`trackchange.list`). `markCount` counts the filtered marks.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackChangeListResultShape {
    pub document_id: String,
    pub revision: u64,
    pub has_track_changes: bool,
    pub readonly_reasons: Vec<String>,
    pub marks: Vec<hae_model::trackchange::TrackChangeMark>,
    pub mark_count: usize,
    pub next_cursor: Option<String>,
    pub changes: Vec<hae_model::trackchange::TrackChangeRecord>,
    pub authors: Vec<hae_model::trackchange::TrackChangeAuthorRecord>,
}

/// Parsed, strongly typed command enum. No arbitrary string-to-function dispatch.
#[derive(Debug, Clone)]
pub enum ParsedCommand {
    EquationList(EquationListParams),
    EquationGet(ElementGetParams),
    EquationSetScript(EquationSetScriptParams),
    EquationSetProperties(EquationSetPropertiesParams),
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
    TableInsertRow(TableInsertRowParams),
    TableDeleteRow(TableDeleteRowParams),
    TableInsertColumn(TableInsertColumnParams),
    TableDeleteColumn(TableDeleteColumnParams),
    LinkList(LinkListParams),
    TrackChangeList(TrackChangeListParams),
    BookmarkList(BookmarkListParams),
    MemoList(MemoListParams),
    SectionGetPage(SectionGetPageParams),
    SectionSetPage(SectionSetPageParams),
    SectionSetPageNumbering(SectionSetPageNumberingParams),
    SectionSetVisibility(SectionSetVisibilityParams),
    StyleList(StyleListParams),
    TableGet(TableGetParams),
    PictureList(PictureListParams),
    FieldList(FieldListParams),
    FieldSetText(FieldSetTextParams),
    TextFormat(TextFormatParams),
    TextReplace(TextReplaceParams),
    TextReplaceAll(TextReplaceAllParams),
    ParagraphFormat(ParagraphFormatParams),
    CellSetText(CellSetTextParams),
    FormFill(FormFillParams),
    ImageReplace(ImageReplaceParams),
    ParagraphSetStyle(ParagraphSetStyleParams),
    ImageFit(ImageFitParams),
    HeaderSetText(HeaderSetTextParams),
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
    DocumentExportText(DocumentExportTextParams),
    Hwp5Inspect(Hwp5InspectParams),
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
    if schema["title"] == "ParagraphSetStyleParams" {
        schema["oneOf"] = serde_json::json!([
            {"required":["target"],"not":{"required":["targets"]}},
            {"required":["targets"],"not":{"required":["target"]}}
        ]);
        schema["properties"]["target"] = serde_json::json!({"type":"string"});
        schema["properties"]["targets"] =
            serde_json::json!({"type":"array","minItems":1,"items":{"type":"string"}});
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
    if matches!(
        schema["title"].as_str(),
        Some(
            "TableInsertRowParams"
                | "TableDeleteRowParams"
                | "TableInsertColumnParams"
                | "TableDeleteColumnParams"
        )
    ) {
        schema["properties"]["index"] =
            serde_json::json!({"type":"integer","minimum":0,"maximum":1024});
        schema["properties"]["count"] =
            serde_json::json!({"type":"integer","minimum":1,"maximum":1024,"default":1});
    }
    if schema["title"] == "CellSetSizeParams" {
        schema["anyOf"] = serde_json::json!([{"required":["width"]},{"required":["height"]}]);
        for side in ["width", "height"] {
            schema["properties"][side] =
                serde_json::json!({"type":"integer","minimum":1,"maximum":2147483647});
        }
    }
    if schema["title"] == "FormFillParams" {
        schema["properties"]["values"] = serde_json::json!({
            "type":"object","additionalProperties":{"type":"string"},"minProperties":1
        });
        schema["properties"]["strict"] = serde_json::json!({"type":"boolean","default":true});
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
            ("equation_list", [true, false, true, false]),
            ("equation_get", [true, false, true, false]),
            ("equation_set_script", [false, true, false, false]),
            ("equation_set_properties", [false, true, false, false]),
            ("link_list", [true, false, true, false]),
            ("trackchange_list", [true, false, true, false]),
            ("bookmark_list", [true, false, true, false]),
            ("memo_list", [true, false, true, false]),
            ("style_list", [true, false, true, false]),
            ("table_get", [true, false, true, false]),
            ("picture_list", [true, false, true, false]),
            ("field_list", [true, false, true, false]),
            ("hwp5_inspect", [true, false, true, false]),
            ("field_set_text", [false, true, false, false]),
            ("text_replace", [false, true, false, false]),
            ("text_format", [false, true, false, false]),
            ("table_set_column_widths", [false, true, false, false]),
            ("table_set_row_heights", [false, true, false, false]),
            ("cell_set_format", [false, true, false, false]),
            ("cell_set_size", [false, true, false, false]),
            ("table_insert_row", [false, true, false, false]),
            ("table_delete_row", [false, true, false, false]),
            ("table_insert_column", [false, true, false, false]),
            ("table_delete_column", [false, true, false, false]),
            ("section_get_page", [true, false, true, false]),
            ("section_set_page", [false, true, false, false]),
            ("section_set_page_numbering", [false, true, false, false]),
            ("section_set_visibility", [false, true, false, false]),
            ("text_replace_all", [false, true, false, false]),
            ("paragraph_format", [false, true, false, false]),
            ("cell_set_text", [false, true, false, false]),
            ("form_fill", [false, true, false, false]),
            ("image_replace", [false, true, false, false]),
            ("paragraph_set_style", [false, true, false, false]),
            ("image_fit", [false, true, false, false]),
            ("header_set_text", [false, true, false, false]),
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
            // Query category, but it publishes a new file: not read-only/idempotent.
            ("document_export_text", [false, false, false, false]),
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
                            // `document.exportText` is a Query that publishes a file, so it is not read-only.
                            let read_only = (matches!(CommandCategory::$cat, CommandCategory::Introspection | CommandCategory::Query) && $name != "document.exportText") || $name == "document.diff";
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
    EquationList {
        name: "equation.list", cli: "equation-list", mcp: "equation_list",
        summary: "List existing equations with paragraph nodeId, original script/raw XML and geometry/properties; optional scope, limit 1..500 and revision-bound cursor",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: EquationListParams, output: EquationListResultShape,
    },
    EquationGet {
        name: "equation.get", cli: "equation-get", mcp: "equation_get",
        summary: "Read one equation nodeId, original script/raw XML, size, properties and containing paragraph text preview",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: ElementGetParams, output: EquationGetResultShape,
    },
    EquationSetScript {
        name: "equation.setScript", cli: "equation-set-script", mcp: "equation_set_script",
        summary: "Replace only an existing equation script using XML text escaping; preserve equation attributes/geometry/comment; refuse empty, opaque or unverified escape notation",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: EquationSetScriptParams, output: MutationResultShape,
    },
    EquationSetProperties {
        name: "equation.setProperties", cli: "equation-set-properties", mcp: "equation_set_properties",
        summary: "Change only requested equation props: baseLine, textColor #RRGGBB, font, positive baseUnit, lineMode, version; preserve script and size; supports dryRun/batch/undo",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: EquationSetPropertiesParams, output: MutationResultShape,
    },
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
    TableInsertRow {
        name: "table.insertRow", cli: "table-insert-row", mcp: "table_insert_row",
        summary: "Insert `count` (default 1) empty rows into a table so the first new row gets 0-based address `index` (0..=rowCnt); new cells clone a neighbouring row's attributes and borders with empty text; creates new cell/paragraph nodes with new nodeIds; refuses boundaries that split merged cells", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: TableInsertRowParams, output: TableStructureResultShape,
    },
    TableDeleteRow {
        name: "table.deleteRow", cli: "table-delete-row", mcp: "table_delete_row",
        summary: "Delete `count` (default 1) rows starting at 0-based address `index`; removes their cell/paragraph nodes (Undo restores the same nodeIds); refuses merged cells in the range, cells holding objects or fields, and deleting every row", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: TableDeleteRowParams, output: TableStructureResultShape,
    },
    TableInsertColumn {
        name: "table.insertColumn", cli: "table-insert-column", mcp: "table_insert_column",
        summary: "Insert `count` (default 1) empty columns so the first new column gets 0-based address `index` (0..=colCnt); the table widens by the cloned neighbour column's width; refuses boundaries that split merged cells", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: TableInsertColumnParams, output: TableStructureResultShape,
    },
    TableDeleteColumn {
        name: "table.deleteColumn", cli: "table-delete-column", mcp: "table_delete_column",
        summary: "Delete `count` (default 1) columns starting at 0-based address `index`; the table narrows by their widths; refuses merged cells in the range, cells holding objects or fields, and deleting every column", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: TableDeleteColumnParams, output: TableStructureResultShape,
    },
    LinkList {
        name: "link.list",
        cli: "link-list",
        mcp: "link_list",
        summary: "List HYPERLINK fields with their raw parameters, display text, and internal/external target",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: LinkListParams, output: LinkListResultShape,
    },
    TrackChangeList {
        name: "trackchange.list",
        cli: "trackchange-list",
        mcp: "trackchange_list",
        summary: "List track-change marks with position and kind plus the header author records; track-change documents stay read-only",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: TrackChangeListParams, output: TrackChangeListResultShape,
    },
    BookmarkList {
        name: "bookmark.list",
        cli: "bookmark-list",
        mcp: "bookmark_list",
        summary: "List bookmark markers (hp:bookmark controls and BOOKMARK field spans) with name, paragraph nodeId, grapheme offset and raw parameters; unnamed markers stay unnamed",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: BookmarkListParams, output: BookmarkListResultShape,
    },
    MemoList {
        name: "memo.list",
        cli: "memo-list",
        mcp: "memo_list",
        summary: "List memos (MEMO fields and hp:memo controls) with id, author, time, shape reference, anchor paragraph, body paragraphs and text; unstated attributes stay null",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: MemoListParams, output: MemoListResultShape,
    },
    SectionGetPage {
        name: "section.getPage", cli: "section-get-page", mcp: "section_get_page",
        summary: "Read explicit page, margin, numbering, visibility and grid values; absent attributes are null with unspecified source; no inherited defaults inferred", category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: SectionGetPageParams, output: GenericResultShape,
    },
    SectionSetPage {
        name: "section.setPage", cli: "section-set-page", mcp: "section_set_page",
        summary: "Edit only requested existing-section page/margin attributes in HWPUNIT; reject non-positive body area; never create nodes", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: SectionSetPageParams, output: MutationResultShape,
    },
    SectionSetPageNumbering {
        name: "section.setPageNumbering", cli: "section-set-page-numbering", mcp: "section_set_page_numbering",
        summary: "Edit only requested pageStartsOn BOTH/EVEN/ODD and page start number; preserve pic/tbl/equation counters", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: SectionSetPageNumberingParams, output: MutationResultShape,
    },
    SectionSetVisibility {
        name: "section.setVisibility", cli: "section-set-visibility", mcp: "section_set_visibility",
        summary: "Edit only requested first-page visibility flags and border/fill; preserve all other attributes and children", category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: SectionSetVisibilityParams, output: MutationResultShape,
    },
    StyleList {
        name: "style.list", cli: "style-list", mcp: "style_list",
        summary: "List header styles, character/paragraph properties and border fills with literal XML usage counts and unresolved-reference totals/examples; optional kind, limit and cursor",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: StyleListParams, output: StyleListResult,
    },
    TableGet {
        name: "table.get", cli: "table-get", mcp: "table_get",
        summary: "Read table geometry, explicit cells, merged coverage, paragraph previews and border/margin references; optional section/paragraph scope or one table nodeId, limit and cursor",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: TableGetParams, output: PagedQueryResult<hae_model::resource_query::TableView>,
    },
    PictureList {
        name: "picture.list", cli: "picture-list", mcp: "picture_list",
        summary: "List picture frames, original/current sizes, payload SHA-256, captions, crop/effect/flip flags, external status and document-wide duplicate media references; optional scope, limit and cursor",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: PictureListParams, output: PagedQueryResult<hae_model::resource_query::PictureView>,
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
    FieldList {
        name: "field.list", cli: "field-list", mcp: "field_list",
        summary: "List field markers, names/IDs, raw Command, current values and safe filling availability in a document or scope, paged with nextCursor",
        category: Query, mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None, params: FieldListParams, output: FieldListResultShape,
    },
    Hwp5Inspect {
        name: "hwp5.inspect", cli: "hwp5-inspect", mcp: "hwp5_inspect",
        summary: "Read a legacy HWP 5.0 (.hwp) file directly: FileHeader version/compression/encryption flags, section and paragraph counts, and paged paragraph text previews",
        category: Query, mutates: false, doc: false, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None, params: Hwp5InspectParams, output: Hwp5InspectResultShape,
    },
    FieldSetText {
        name: "field.setText", cli: "field-set-text", mcp: "field_set_text",
        summary: "Fill CLICK_HERE/CLICKHERE/HYPERLINK text in one paragraph, preserving markers/parameters/runs; rejects ambiguous or unsafe fields",
        category: Mutation, mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None, params: FieldSetTextParams, output: MutationResultShape,
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
    FormFill {
        name: "form.fill",
        cli: "form-fill",
        mcp: "form_fill",
        summary: "Fill empty value cells from exact label cells: same-row right cell, else the cell below; strict (default) rejects the whole request if any label fails, strict:false fills the resolvable labels and reports the rest",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: FormFillParams, output: FormFillResultShape,
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
    ParagraphSetStyle {
        name: "paragraph.setStyle", cli: "paragraph-set-style", mcp: "paragraph_set_style",
        summary: "Set styleIDRef on target or non-empty targets using an existing header styleId; no style creation or other paragraph attribute changes",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: ParagraphSetStyleParams, output: MutationResultShape,
    },
    ImageFit {
        name: "image.fit", cli: "image-fit", mcp: "image_fit",
        summary: "Fit existing image to its frame: contain preserves aspect inside, cover center-crops to fill, stretch fills; HWPUNIT source clipping, rendering matrices and embedded media preserved",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: ImageFitParams, output: MutationResultShape,
    },
    HeaderSetText {
        name: "header.setText", cli: "header-set-text", mcp: "header_set_text",
        summary: "Replace existing header/footer paragraph text by section scope (default first), kind and paragraphIndex (default 0); refuse controls, never create headers",
        category: Mutation,
        mutates: true, doc: true, ver: true, seq: true, dry: true, hist: true,
        stage1: true, reason: None,
        params: HeaderSetTextParams, output: MutationResultShape,
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
    DocumentExportText {
        name: "document.exportText",
        cli: "document-export-text",
        mcp: "document_export_text",
        summary: "Write the document as Markdown or HTML to a new file (tables, merged cells, pictures as media references, equations as raw script, notes, headers/footers); never changes the document",
        category: Query,
        mutates: false, doc: true, ver: false, seq: false, dry: false, hist: false,
        stage1: true, reason: None,
        params: DocumentExportTextParams, output: ExportTextResultShape,
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
