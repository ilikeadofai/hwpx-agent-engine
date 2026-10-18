//! hae-exec: Command registry, session executor, revisions, request ordering, batch, dryRun and history.

mod preview;
mod queries;
pub mod registry;
mod section;
pub mod session;

pub use registry::{
    command_describe, command_list, BatchStepParam, BookmarkListParams, CellSetTextParams,
    CommandBatchParams, CommandCategory, CommandDescribeParams, CommandDescriptor,
    DocumentDiffParams, DocumentExportParams, DocumentExportTextParams, DocumentInspectParams,
    DocumentOpenParams, DocumentPreviewParams, ElementGetParams, ElementQueryParams, EmptyParams,
    ExportTextFormat, ExportTextResultShape, FormFillParams, FormFillResultShape,
    Hwp5InspectParams, ImageReplaceParams, LineSpacingParam, LinkListParams, MemoListParams,
    ParagraphFormatParams, ParsedCommand, PictureListParams, ProjectSaveParams,
    SnapshotCreateParams, SnapshotReleaseParams, StyleListParams, TableGetParams, TextRangeParam,
    TextReplaceAllParams, TextReplaceParams, TrackChangeListParams, COMMAND_NAMES,
};
pub use session::{RequestEnvelope, ResponseEnvelope, SessionExecutor};

/// `--help` text describing how `document.preview` finds its renderer, shared
/// with the discovery rules in `hae-render-rhwp` so the two cannot drift.
pub use hae_render_rhwp::discovery::RENDERER_ENV_HELP;

#[cfg(test)]
mod tests;
