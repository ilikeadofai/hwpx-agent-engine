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
    "Return a paged structural outline of the open document (`documentId`), including each element's `nodeId`, kind, parent, text preview, table dimensions, and per-command `edits` ({available, reason?, modes?, requires?}). A node is `editable` iff any edit is available; this does not imply text replacement. Paragraph `textSpans` lists up to 64 maximal replaceable [start,end) grapheme ranges; `textSpansTruncated` means later ranges were omitted. Cells report `childParagraphsEditable` for descendant text.replace access even when cell.setText is unavailable. Pass `cursor` from the previous reply's `nextCursor` and optional `limit` to page through large documents; cursors invalidate when `revision` changes. Read-only; never modifies the document."
}

fn document_close() -> &'static str {
    "Close the open document (`documentId`), discard unsaved in-memory edits and session history, and release any `.hwpwork` project lock. Save with `project_save` or export with `document_export` first if you need to keep changes."
}

fn element_query() -> &'static str {
    "Search elements in the open document (`documentId`) by `kind` (`section`, `paragraph`, `table`, `cell`, `picture`, `equation`), exact codepoint substring `textContains`, `ancestor` nodeId, or 0-based table cell coordinates (`table` nodeId, `row`, `col`). Returns all matching elements with their `nodeId`s (never auto-picks the first match) and reports matches split across grapheme boundaries in `unalignedMatches`. Each match includes per-command `edits`; `editable` means any command is available. For text.replace choose a non-empty subrange of paragraph `textSpans` ([start,end) grapheme indices; capped at 64, see `textSpansTruncated`). For blocked cell.setText, `childParagraphsEditable` means at least one descendant paragraph supports text.replace; query by `ancestor`. Apply edits.requires arguments when present. Advertising uses default arguments otherwise; preserve-only subranges are not exhaustively listed, including on no_text_span paragraphs. Read-only; use returned `nodeId`s as `target` in edit tools."
}

fn element_get() -> &'static str {
    "Return full properties, `outline.edits` command availability, paragraph `outline.textSpans`/`textSpansTruncated` (up to 64 maximal replaceable grapheme intervals), cell `outline.childParagraphsEditable`, and complete text (with grapheme cluster count) for a single element identified by `target` (`nodeId` from `document_inspect` or `element_query`) in `documentId`. Fails with `TARGET_NOT_FOUND` if `target` does not exist. Read-only; never modifies the document."
}

fn text_replace() -> &'static str {
    "Replace a paragraph grapheme range [start,end), or insert non-empty text at [p,p). No tabs/newlines in text. Check edits[\"text.replace\"].modes (replace/insert) and apply requires arguments (e.g. controls=preserve); default controls=reject requires a subrange/position in textSpans. Empty paragraphs advertise {start:0,end:0}; insertion uses the following character run, then preceding run, then the empty paragraph first run. controls=preserve changes only text fragments and keeps objects/tabs/line breaks/fields in original XML order; leading objects stay before the first text fragment. Existing empty hp:t may be reused; no text fragment means UNSUPPORTED_CONTENT_IN_RANGE. CONTROLS_PRESERVED reports count and new grapheme offsets. Unsupported markup/attributes still fail. Requires expectedVersion; VERSION_CONFLICT means re-query. FORMAT_COLLAPSED warns about character shapes. dryRun=true verifies without committing."
}

fn text_replace_all() -> &'static str {
    "Replace every exact match of `find` with `text` in one atomic step (one revision, one undo unit). Searches the whole document, or only `scope` (nodeId of a section, table, cell or paragraph) and the paragraphs below it, table cells included. Matching is exact Unicode codepoints, leftmost and non-overlapping per paragraph; a match is replaced only if it lies wholly inside the paragraph's editable `textSpans`. Matches that cross an object, tab, line break or field marker, split a grapheme cluster, touch unsupported markup, or sit in a locked paragraph are skipped and reported: `skippedCount`, `skipped[]` (per reason `crosses_control`, `grapheme_boundary`, `unsupported_markup`, `paragraph_not_editable` with `count` and up to 50 `locations` {nodeId, range}) and a `MATCHES_SKIPPED` warning; nothing is skipped silently. `find` must not be empty; `text` may be empty (deletes matches) but has no tabs/newlines. More than `maxMatches` (default 1000, max 10000) replaceable matches is refused with `RESOURCE_LIMIT` before anything is changed. Result: `replacementCount`, `paragraphCount`, and `matches[]` previews ({nodeId, range, before, after}; first 200). Run with `dryRun: true` first to review the list; the real call uses the same rules. Requires `expectedVersion`; `VERSION_CONFLICT` means re-query. Any failure leaves the document unchanged; `history_undo` restores the previous bytes. Character shapes inside a match collapse to the first one (`FORMAT_COLLAPSED`)."
}

fn paragraph_format() -> &'static str {
    "Update paragraph `target` (`nodeId` from `document_inspect`/`element_query`) with `alignment` (`left`, `center`, `right`, `justify`, `distribute`, `distributeSpace`) and/or `lineSpacing` (`{\"kind\": \"percent\", \"value\": 1..=1000}`), reusing or cloning the paragraph shape without altering global styles. Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query). Refuses read-only documents or uncloneable shapes (`UNSUPPORTED`), missing targets (`TARGET_NOT_FOUND`), or calls with neither option set (`INVALID_REQUEST`). Set `dryRun: true` to preview without changing the document."
}

fn cell_set_text() -> &'static str {
    "Replace text in existing cell target (nodeId); merged anchor cells are supported. Supply exactly one of text (one paragraph only) or paragraphs (string array matching the exact existing paragraph count). No new paragraphs are created. Wrong count returns INVALID_ARGUMENT with detail.paragraphCount; multi-paragraph cells with text return UNSUPPORTED and a paragraphs hint. controls=reject is default; controls=preserve keeps inline objects/tabs/line breaks/fields and reports CONTROLS_PRESERVED. Empty paragraphs use the first run; no run, nested tables/cells, unsupported markup/attributes or read-only documents still fail. Check edits[\"cell.setText\"].available and apply its requires guidance: requires.paragraphs=N means supply a string array of exactly N paragraphs, not the number N; requires.controls=preserve means also supply controls=preserve. Requires expectedVersion; on VERSION_CONFLICT re-query. dryRun=true verifies without changing document or history."
}

fn image_replace() -> &'static str {
    "Replace the media of picture `target` (`nodeId` from `document_inspect`/`element_query`) with a static PNG or JPEG file at `imagePath` (inside `--input-root`), preserving the picture's position, size, and caption while leaving other pictures sharing the old media untouched. Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query). Refuses read-only documents, cropped/effect pictures, or animated/non-PNG/JPEG images (`UNSUPPORTED`), disallowed paths (`PATH_NOT_ALLOWED`), or images over 40.96 megapixels (`RESOURCE_LIMIT`). Set `dryRun: true` to preview without changing the document."
}

fn command_batch() -> &'static str {
    "Execute multiple edit `steps` (`text.replace`, `text.replaceAll`, `paragraph.format`, `cell.setText`, `image.replace`) atomically as a single revision increment and single undo unit. Requires one top-level `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query); each step's ranges and preconditions are evaluated against the state produced by preceding steps. If any step fails, the entire batch rolls back with no state change and reports the failing step index. Set `dryRun: true` to validate and preview the combined diff without changing the document."
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
    "Capture an explicit in-session snapshot of `documentId` at the current revision (with optional `label`) and return its `snapshotId` for later `document_diff` comparisons. Creates session state without changing document content or `revision`."
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
    "Atomically verify and save the open document, original baseline, and stable `nodeId` map to a `.hwpwork` project file under `--output-root` (`path` is required on first save and optional thereafter). Overwrites the bound project on subsequent saves. Does not change content `revision` or history, and returns `savedVersion`. Refuses if a first-save target already exists (`TARGET_EXISTS`), if the bound file on disk was modified externally (`EXTERNAL_MODIFICATION`), or if the path is outside `--output-root` (`PATH_NOT_ALLOWED`)."
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
    ("text.replaceAll", text_replace_all),
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
pub fn document_id_property(modern: bool) -> &'static str {
    if modern {
        "Required documentId for stateless MCP 2026-07-28 calls: pass the handle returned by document_open in arguments.documentId on every document tool call. The handle lasts until document_close or server exit; reopen after restart."
    } else {
        "Optional documentId after legacy initialize: defaults to the currently open document; if given it must match the open document. Fails with TARGET_NOT_FOUND on mismatch or SESSION_REQUIRED if no document is open."
    }
}

/// Shown on every `expectedVersion` input property.
pub fn expected_version_property() -> &'static str {
    "The current document revision (`revision` from `document_open`/queries, or `afterVersion` from the last edit/undo/redo). If stale, the call is refused with `VERSION_CONFLICT` and nothing changes; re-query the document to get the latest revision and ranges."
}

/// Shown on every `dryRun` input property.
pub fn dry_run_property() -> &'static str {
    "When true, validate and simulate the edit in isolation, returning the predicted outcome, diff, and diagnostics without modifying the document, its `revision`, or session history."
}

/// Returned as `instructions` from `initialize` and `server/discover`.
pub fn server_instructions() -> &'static str {
    "Preservation-first HWPX document engine. Workflow: (1) call `document_open` to get `documentId` and `revision`; (2) call `document_inspect` or `element_query` to find target `nodeId`s and text; check per-command `edits` and choose text ranges within paragraph `textSpans` (`editable` means any available command); (3) call edit tools (`text_replace`, `text_replace_all` for document-wide exact find-and-replace, `paragraph_format`, `cell_set_text`, `image_replace`, `command_batch`) with `documentId` and `expectedVersion` = current revision (use `dryRun: true` to preview without changing anything); (4) on `VERSION_CONFLICT`, re-query before retrying; (5) export to a new `.hwpx` via `document_export` or save a `.hwpwork` project via `project_save`. For stateless MCP 2026-07-28 requests (server/discover then tools/call), documentId is required in arguments on every document tool call; use the handle from document_open. For legacy initialize-based sessions (2025-11-25 and earlier), documentId is optional and defaults to the currently open document. Handles last until document_close or server exit; reopen after restart. Text ranges are half-open `[start, end)` extended grapheme cluster indices within a single paragraph. Request sequence numbering is handled automatically by the MCP adapter."
}
