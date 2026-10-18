//! hae-exec: Command registry, session executor, revisions, request ordering, batch, dryRun and history.

mod preview;
mod queries;
pub mod registry;
pub mod session;

pub use registry::{
    command_describe, command_list, BatchStepParam, CellSetTextParams, CommandBatchParams,
    CommandCategory, CommandDescribeParams, CommandDescriptor, DocumentDiffParams,
    DocumentExportParams, DocumentInspectParams, DocumentOpenParams, DocumentPreviewParams,
    ElementGetParams, ElementQueryParams, EmptyParams, ImageReplaceParams, LineSpacingParam,
    ParagraphFormatParams, ParsedCommand, PictureListParams, ProjectSaveParams,
    SnapshotCreateParams, SnapshotReleaseParams, StyleListParams, TableGetParams, TextRangeParam,
    TextReplaceAllParams, TextReplaceParams, COMMAND_NAMES,
};
pub use session::{RequestEnvelope, ResponseEnvelope, SessionExecutor};

#[cfg(test)]
mod tests;
