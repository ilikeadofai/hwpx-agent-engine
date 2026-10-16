//! Human-readable MCP tool descriptions, one function per command.
//!
//! This file is a plain table on purpose: a UX writer rewrites the wording
//! here without touching any logic. Keep one `fn` per command and one row in
//! [`TABLE`]. The text is what the agent reads in `tools/list`, so name the
//! inputs the agent must supply and say when a call changes the document.

/// A description function; the table maps a command name to one.
pub type Describe = fn() -> &'static str;

fn command_list() -> &'static str {
    "List all registered commands with their JSON Schemas, supported status, and history/mutation policies. Preconditions: none (no open document required). Read-only; never modifies any document."
}

fn command_describe() -> &'static str {
    "Return the input/output JSON Schema, precondition flags (`requiresDocument`, `requiresExpectedVersion`, `supportsDryRun`), and support status for one command. Pass the dotted registry name in `command` (e.g., `text.replace` or `cell.setText`). Fails with `UNKNOWN_COMMAND` if the name is not registered; read-only."
}

fn document_open() -> &'static str {
    "Open an unencrypted `.hwpx` document or `.hwpwork` project from `path` (inside `--input-root`, or `--output-root` for saved projects) and return `documentId` and starting `revision`. Signed or track-changes documents open in read-only mode (queries and unedited export only). Refuses encrypted or malformed archives (`PACKAGE_REJECTED`), locked projects (`PROJECT_LOCKED`), paths outside allowed roots (`PATH_NOT_ALLOWED`), or oversize files (`RESOURCE_LIMIT`)."
}

fn document_inspect() -> &'static str {
    "Return a paged structural outline of the open document (`documentId`), including each element's `nodeId`, kind, parent, text preview, table dimensions, and editability. Pass `cursor` from the previous reply's `nextCursor` and optional `limit` to page through large documents; cursors invalidate when `revision` changes. Read-only; never modifies the document."
}

fn document_close() -> &'static str {
    "Close the open document (`documentId`), discard unsaved in-memory edits and session history, and release any `.hwpwork` project lock. Save with `project_save` or export with `document_export` first if you need to keep changes."
}

fn element_query() -> &'static str {
    "Search elements in the open document (`documentId`) by `kind` (`section`, `paragraph`, `table`, `cell`, `picture`, `equation`), exact codepoint substring `textContains`, `ancestor` nodeId, or 0-based table cell coordinates (`table` nodeId, `row`, `col`). Returns all matching elements with their `nodeId`s (never auto-picks the first match) and reports matches split across grapheme boundaries in `unalignedMatches`. Read-only; use returned `nodeId`s as `target` in edit tools."
}

fn element_get() -> &'static str {
    "Return full properties, editability status, and complete text (with grapheme cluster count) for a single element identified by `target` (`nodeId` from `document_inspect` or `element_query`) in `documentId`. Fails with `TARGET_NOT_FOUND` if `target` does not exist. Read-only; never modifies the document."
}

fn text_replace() -> &'static str {
    "Replace a non-empty text `range` (`[start, end)` in extended grapheme clusters, `0 <= start < end`, within one paragraph) on paragraph `target` (`nodeId` from `document_inspect`/`element_query`) with `text` (no tabs or newlines; empty string deletes). Requires `expectedVersion` equal to the current revision; on `VERSION_CONFLICT`, re-query to get the latest revision and ranges. Refuses read-only documents (`UNSUPPORTED`), out-of-bounds or empty ranges (`INVALID_RANGE`), ranges crossing tabs/linebreaks/fields/controls (`RANGE_CROSSES_CONTROL`), or unknown inline elements (`UNSUPPORTED_CONTENT_IN_RANGE`); warns with `FORMAT_COLLAPSED` when spanning multiple char shapes. Set `dryRun: true` to preview outcome and diff without changing the document or revision."
}

fn paragraph_format() -> &'static str {
    "Update paragraph `target` (`nodeId` from `document_inspect`/`element_query`) with `alignment` (`left`, `center`, `right`, `justify`, `distribute`, `distributeSpace`) and/or `lineSpacing` (`{\"kind\": \"percent\", \"value\": 1..=1000}`), reusing or cloning the paragraph shape without altering global styles. Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query). Refuses read-only documents or uncloneable shapes (`UNSUPPORTED`), missing targets (`TARGET_NOT_FOUND`), or calls with neither option set (`INVALID_REQUEST`). Set `dryRun: true` to preview without changing the document."
}

fn cell_set_text() -> &'static str {
    "Replace the entire text of table cell `target` (`nodeId` from `document_inspect` or `element_query` with `table`/`row`/`col`) with `text` (no tabs or newlines). Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query), and an unmerged cell containing exactly one paragraph with at least one run. Refuses merged cells, multi-paragraph cells, empty-run cells, or read-only documents (`UNSUPPORTED`), and cells whose text crosses inline controls (`RANGE_CROSSES_CONTROL`). Set `dryRun: true` to preview without changing the document."
}

fn image_replace() -> &'static str {
    "Replace the media of picture `target` (`nodeId` from `document_inspect`/`element_query`) with a static PNG or JPEG file at `imagePath` (inside `--input-root`), preserving the picture's position, size, and caption while leaving other pictures sharing the old media untouched. Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query). Refuses read-only documents, cropped/effect pictures, or animated/non-PNG/JPEG images (`UNSUPPORTED`), disallowed paths (`PATH_NOT_ALLOWED`), or images over 40.96 megapixels (`RESOURCE_LIMIT`). Set `dryRun: true` to preview without changing the document."
}

fn command_batch() -> &'static str {
    "Execute multiple edit `steps` (`text.replace`, `paragraph.format`, `cell.setText`, `image.replace`) atomically as a single revision increment and single undo unit. Requires one top-level `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query); each step's ranges and preconditions are evaluated against the state produced by preceding steps. If any step fails, the entire batch rolls back with no state change and reports the failing step index. Set `dryRun: true` to validate and preview the combined diff without changing the document."
}

fn history_undo() -> &'static str {
    "Restore the document snapshot from before the last committed edit and increment `revision` by 1 (revisions always move forward). Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query). Fails with `HISTORY_EMPTY` if there is no undo unit available."
}

fn history_redo() -> &'static str {
    "Restore the next undone document snapshot (including exact media bytes) and increment `revision` by 1 without re-running commands. Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query). Fails with `HISTORY_EMPTY` if no undone state is available (any new edit clears the redo branch)."
}

fn history_clear() -> &'static str {
    "Discard all in-session undo and redo history units for `documentId` to free memory. Does not take `expectedVersion` and does not change document content or `revision` (reports `controlChanged`). Subsequent `history_undo` or `history_redo` calls will fail with `HISTORY_EMPTY`."
}

fn document_snapshot() -> &'static str {
    "Capture an explicit in-session snapshot of `documentId` at the current revision (with optional `label`) and return its `snapshotId` for later `document_diff` comparisons. Read-only; does not change document content or `revision`."
}

fn snapshot_release() -> &'static str {
    "Release a previously captured snapshot identified by `snapshotId` in `documentId`. Does not take `expectedVersion` and does not change document content or `revision`. Fails with `SNAPSHOT_UNAVAILABLE` if `snapshotId` is unknown or already released."
}

fn document_diff() -> &'static str {
    "Compare semantic content, formatting, table cells, media, and layout cache between `baseSnapshotId` (defaults to the original opened state) and `targetSnapshotId` (defaults to the current state) in `documentId`. Fails with `SNAPSHOT_UNAVAILABLE` if a referenced snapshot does not exist or was released. Read-only; never modifies the document."
}

fn document_preview() -> &'static str {
    "Render the open document (`documentId`) into isolated SVG/PNG page previews (optionally filtered by 1-based `pages`), returning `rendered`, `partial` (`PREVIEW_PAGE_LIMIT` over 100 pages), or `failed` status plus layout diagnostics. Previews are renderer estimates and do not guarantee Hancom Office layout fidelity. Fails with `RENDERER_UNAVAILABLE` when the sandboxed `rhwp` renderer is not configured or unverified; never modifies the document."
}

fn project_save() -> &'static str {
    "Atomically verify and save the open document, original baseline, and stable `nodeId` map to a `.hwpwork` project file under `--output-root` (`path` is required on first save and optional thereafter). Does not change content `revision` or history, and returns `savedVersion`. Refuses if a first-save target already exists (`TARGET_EXISTS`), if the bound file on disk was modified externally (`EXTERNAL_MODIFICATION`), or if the path is outside `--output-root` (`PATH_NOT_ALLOWED`)."
}

fn document_export() -> &'static str {
    "Independently verify preservation invariants and write the current document (`documentId`) to a new `.hwpx` file at `path` (inside `--output-root`). Original input files are never overwritten, and exporting to an existing file is refused with `TARGET_EXISTS` (`PATH_NOT_ALLOWED` if outside `--output-root`, `VERIFY_FAILED` if verification fails). Read-only with respect to session state and `revision`."
}

/// Command name (as in the registry) → description.
pub const TABLE: &[(&str, Describe)] = &[
    ("command.list", command_list),
    ("command.describe", command_describe),
    ("document.open", document_open),
    ("document.inspect", document_inspect),
    ("document.close", document_close),
    ("element.query", element_query),
    ("element.get", element_get),
    ("text.replace", text_replace),
    ("paragraph.format", paragraph_format),
    ("cell.setText", cell_set_text),
    ("image.replace", image_replace),
    ("command.batch", command_batch),
    ("history.undo", history_undo),
    ("history.redo", history_redo),
    ("history.clear", history_clear),
    ("document.snapshot", document_snapshot),
    ("snapshot.release", snapshot_release),
    ("document.diff", document_diff),
    ("document.preview", document_preview),
    ("project.save", project_save),
    ("document.export", document_export),
];

/// Description for a command, or `None` when the table has no row for it.
pub fn description(command: &str) -> Option<&'static str> {
    TABLE
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(_, describe)| describe())
}

// ---------------------------------------------------------------------------
// Adapter-level text (not tied to one command).
// ---------------------------------------------------------------------------

/// Shown on every `documentId` input property.
pub fn document_id_property() -> &'static str {
    "Optional: defaults to the currently open document; if given it must match the open document. Fails with TARGET_NOT_FOUND on mismatch or SESSION_REQUIRED if no document is open."
}

/// Shown on every `expectedVersion` input property.
pub fn expected_version_property() -> &'static str {
    "The current document revision (`revision` from `document_open`/queries, or `afterVersion` from the last edit/undo/redo). If stale, the call is refused with `VERSION_CONFLICT` and nothing changes; re-query the document to get the latest revision and ranges."
}

/// Shown on every `dryRun` input property.
pub fn dry_run_property() -> &'static str {
    "When true, validate and simulate the edit in isolation, returning the predicted outcome, diff, and diagnostics without modifying the document, its `revision`, or session history."
}

/// Returned as `instructions` from `initialize`.
pub fn server_instructions() -> &'static str {
    "Preservation-first HWPX document engine. Workflow: (1) call `document_open` to get `documentId` and `revision`; (2) call `document_inspect` or `element_query` to find target `nodeId`s and text; (3) call edit tools (`text_replace`, `paragraph_format`, `cell_set_text`, `image_replace`, `command_batch`) with optional `documentId` (defaults to the currently open document; if given it must match the open document) and `expectedVersion` = current revision (use `dryRun: true` to preview without changing anything); (4) on `VERSION_CONFLICT`, re-query before retrying; (5) export to a new `.hwpx` via `document_export` or save a `.hwpwork` project via `project_save`. Text ranges are half-open `[start, end)` extended grapheme cluster indices within a single paragraph. Request sequence numbering is handled automatically by the MCP adapter."
}
