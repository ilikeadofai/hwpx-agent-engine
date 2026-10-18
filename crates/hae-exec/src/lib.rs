//! hae-exec: Command registry, session executor, revisions, request ordering, batch, dryRun and history.

mod preview;
pub mod registry;
pub mod session;

pub use registry::{
    command_describe, command_list, BatchStepParam, CellSetTextParams, CommandBatchParams,
    CommandCategory, CommandDescribeParams, CommandDescriptor, DocumentDiffParams,
    DocumentExportParams, DocumentInspectParams, DocumentOpenParams, DocumentPreviewParams,
    ElementGetParams, ElementQueryParams, EmptyParams, ImageReplaceParams, LineSpacingParam,
    LinkListParams, ParagraphFormatParams, ParsedCommand, ProjectSaveParams, SnapshotCreateParams,
    SnapshotReleaseParams, TextRangeParam, TextReplaceAllParams, TextReplaceParams,
    TrackChangeListParams, COMMAND_NAMES,
};
pub use session::{RequestEnvelope, ResponseEnvelope, SessionExecutor};

#[cfg(test)]
mod tests;
