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

fn link_list() -> &'static str {
    "List the HYPERLINK fields of the open document (`documentId`) in document order: containing paragraph `nodeId`, fieldBegin `id`/`fieldid`, every `hp:parameters` entry's raw kind/name/value (`Command`, `Path`, `Category`, `TargetType`, `DocOpenType`, ...), display text, and whether the target is inside this document or external. `displayText` is the field's `hp:script` text when present, otherwise the character data between the fieldBegin and its matching fieldEnd (`displayTextSource`: script/between/none); Hancom can write both markers next to each other, leaving `displayText` empty. `reference.scope` is `internal` (Category `HWPHYPERLINK_TYPE_HWP`, a `#`-Path, or a `?`-Command bookmark), `external` (Category `URL`/`EMAIL`, or an addressed Path/Command head), or `unknown` when nothing identifies the target; `reference.target` repeats `Path`, else the `Command` head with `\\` escapes resolved. Optional `scope` limits the list to one nodeId and its descendants. Pass `cursor` from `nextCursor` and optional `limit` (default 100, max 500) to page; `total` counts filtered links and cursors invalidate when `revision` changes. Read-only; never modifies the document."
}

fn trackchange_list() -> &'static str {
    "List the revision marks of the open document (`documentId`): for each `hp:insertBegin`/`insertEnd`/`deleteBegin`/`deleteEnd` mark the containing paragraph `nodeId`, `section`, mark `id`, `TcId`, `paraEnd`, and the codepoint `textOffset` inside its `hp:t`, resolved against the header records as `changeType` (Insert/Delete/CharShape/ParaShape), `date`, `authorId`, and `authorName`. The reply also carries every header `changes`/`authors` record (from `hh:trackChanges`/`hh:trackChangeAuthors`), `readonlyReasons`, and `hasTrackChanges`. Documents carrying track changes stay read-only (edits are refused with `UNSUPPORTED`/`document_readonly`); this tool only reports them and never lifts that lock. Optional `scope` limits marks to one nodeId and its descendants. Pass `cursor` from `nextCursor` and optional `limit` (default 100, max 500) to page; `markCount` counts filtered marks and cursors invalidate when `revision` changes. Read-only; never modifies the document."
}

fn section_get_page() -> &'static str {
    "Read the page setup of one existing section, chosen by `scope` (nodeId of the top-level paragraph that contains `hp:secPr`) or `sectionIndex` (0-based, default 0); supplying both is refused. Returns `landscape`, `width`, `height`, `gutterType`, `margins`, `textDirection`, `spaceColumns`, `startNum` (`pageStartsOn`, `page`, `pic`, `tbl`, `equation`), `visibility`, and `grid`, each as `{\"value\":…,\"source\":\"explicit\"|\"unspecified\"}`. Values come from XML attributes only: absent attributes are `null` with `unspecified`, and HWPUNIT measurements are never substituted from inherited or application defaults. Flags keep their stored numeric form. Column count/width (`colPr`) and page-number display controls are read-only here. Read-only; never modifies the document."
}

fn section_set_page() -> &'static str {
    "Change only the page properties you name in existing section target (`scope` nodeId of the `secPr` paragraph, or `sectionIndex` 0-based default 0): `landscape` (`NARROWLY`/`WIDELY`), `width`/`height` (HWPUNIT, >= 1), `gutterType` (`LEFT_ONLY`/`LEFT_RIGHT`/`TOP_BOTTOM`), and `margins` (`left`, `right`, `top`, `bottom`, `header`, `footer`, `gutter`). Unnamed attributes, children, and other sections keep their original bytes. `landscape` does not swap the stored `width`/`height`; the swap applies only to the body-area check. The call is refused with `INVALID_REQUEST` (`non_positive_body_area`) when the resulting body width or height (width minus left/right and the horizontal gutter; height minus top/bottom/header/footer and the vertical gutter) is not positive, and with `UNSUPPORTED` (`structure_change_unsupported`) when a required original measurement is missing or the section/`secPr`/`pagePr`/`margin` shape is not supported — missing elements are never created, though named missing attributes are added. Requires `expectedVersion` (`VERSION_CONFLICT` if stale -> re-query). Because the paper area changes line breaking, every `linesegarray` cache in that section (table cells included) is removed while other sections are preserved. Supported in `command_batch` as one undo unit; `dryRun: true` validates without changing the document, revision, or history."
}

fn section_set_page_numbering() -> &'static str {
    "Change only the page numbering you name in existing section target (`scope` nodeId of the `secPr` paragraph, or `sectionIndex` 0-based default 0): `pageStartsOn` (`BOTH`/`EVEN`/`ODD`) and `page` (start number, >= 0). The `pic`, `tbl`, and `equation` counters, all other attributes, and other sections are preserved byte-for-byte; no page-number display control is created. A request naming neither field is refused, as is a value outside the listed enums. Requires `expectedVersion` (`VERSION_CONFLICT` if stale -> re-query). Like the other section edits, the changed section's `linesegarray` caches are removed because layout may change; body text and `PrvText` are untouched. Supported in `command_batch`; `dryRun: true` previews without committing."
}

fn section_set_visibility() -> &'static str {
    "Change only the first-page visibility flags you name in existing section target (`scope` nodeId of the `secPr` paragraph, or `sectionIndex` 0-based default 0): `hideFirstHeader`, `hideFirstFooter`, `hideFirstPageNum`, `hideFirstEmptyLine`, `showLineNumber` (booleans) and `border`/`fill` (`SHOW_ALL`/`HIDE_ALL`/`SHOW_FIRST`/`HIDE_FIRST`). Attributes outside this list (for example `hideFirstMasterPage`) and other sections keep their original bytes. A request naming no flag is refused, as is an unlisted enum value. Requires `expectedVersion` (`VERSION_CONFLICT` if stale -> re-query). Removes the changed section's `linesegarray` caches only; body text and `PrvText` are untouched. Supported in `command_batch`; `dryRun: true` previews without committing."
}

fn style_list() -> &'static str {
    "Read header resources with kind=style/charPr/paraPr/borderFill (optional), limit=1..500 and cursor from nextCursor. Returns items with ID, XML properties, resolved font faces and literal reference counts (body versus other XML), plus document-wide unresolvedReferenceCount/unresolvedBodyReferenceCount and up to 20 unresolvedExamples; counts are not character counts. Read-only; requires an open document, no expectedVersion; cursors bind to document/revision/filter."
}
fn table_get() -> &'static str {
    "Read tables in the document or scope (section/paragraph nodeId), or one table nodeId in table; optional limit=1..500/cursor. Returns items with rows/cols, explicit cell addresses/spans, HWPUNIT sizes/margins, paragraph counts/previews, border references, spacing and outer margins; coveredCells comes only from complete explicit spans and identifies overlapping actual cells without guessing. Missing XML values are null; read-only, no expectedVersion; oversized single tables fail RESOURCE_LIMIT."
}
fn picture_list() -> &'static str {
    "Read picture items, optionally within scope (section/paragraph nodeId), with limit=1..500/cursor. Reports containing paragraph, frame position/size, orgSz/curSz, media reference/payload SHA-256, caption, crop comparison, effects/shadow/flip/reverse, literal isEmbeded and external status; otherPictureNodeIds lists document-wide references to the same media. Missing values stay null and external resources are never fetched; read-only, no expectedVersion."
}

fn field_list() -> &'static str {
    "List fields in documentId, optionally restricted to scope (paragraph, cell, table or section nodeId) and paged with limit (default 100, max 500) plus the revision-bound nextCursor. Returns containing paragraph nodeId, type, name, fieldid/id, decoded Command and commandRaw (original parameter inner XML), current text, empty/multiParagraph/nested/inCell flags, paragraphIds, setTextSupported and reason. `CLICK_HERE` (Hancom's real 누름틀 spelling), `CLICKHERE` (synthetic fixture spelling) and `HYPERLINK` are fill candidates; MEMO/proofreading fields are query-only. Read-only; no expectedVersion required. Multi-paragraph fields intersecting scope are listed with their starting nodeId."
}
fn field_set_text() -> &'static str {
    "Replace only field value text in target (containing paragraph nodeId), with text (no tabs/newlines). Select by optional name and/or fieldid (matches fieldid or id); occurrence is a 0-based index among matches. Default index is 0 only for an unambiguous match; multiple matches without occurrence return AMBIGUOUS_TARGET. Missing/out-of-range selectors return TARGET_NOT_FOUND. Supports `CLICK_HERE`/`CLICKHERE`/`HYPERLINK`; rejects other types, multi-paragraph/nested fields, objects/tabs, CDATA/comments/PI or opaque markup with a reason. Preserves fieldBegin/fieldEnd and every attribute/parameter byte, existing runs and shapes; text goes into the first carrier, others become empty. An empty field may use a text carrier in the existing run; no model nodes are created. Requires documentId/current expectedVersion, supports dryRun and command.batch, and one undo unit with exact byte restoration. VERSION_CONFLICT means re-query. HYPERLINK Command/URL is never changed. Verified against synthetic fixtures and the public 간이기안문/일반기안문 templates that carry real `CLICK_HERE` fields; private 공문서 and Windows Hancom rendering remain unverified."
}

fn hwp5_inspect() -> &'static str {
    "Read a legacy HWP 5.0 binary document (`path`, ending in `.hwp`, inside `--input-root`) directly; no open document is required and the file is never modified. Returns the FileHeader `version` (`5.1.0.1` form), `compressed`, `encrypted`, `distribution`, `drm` flags and raw `properties`; container `sectionCount` (from `BodyText/SectionN` streams), `paragraphCount`, `charCount`; per-section counts; and a page of paragraphs (global `paragraphIndex`, `sectionIndex`, record `level`, `charCount`, `preview`, `previewTruncated`). `previewChars` caps each preview (default 120, 1..=1000) and `limit` the page (default 100, 1..=500); pass `cursor` from `nextCursor` to continue (the cursor binds to the file SHA-256, so different file bytes give VERSION_CONFLICT). Paragraph text is decoded from UTF-16LE with control characters removed (line break -> newline, tab kept, non-breaking/fixed spaces mapped; auto-number text is not invented), so `charCount` counts Unicode scalar values of the text this reader can see, and nested table-cell paragraphs are included. Encrypted (`encrypted`), distribution (`distribution_unsupported`) and DRM documents, missing parts (`missing_component`) and damaged containers or record streams (`malformed_compound_file`, `malformed_record_stream`) are refused with PACKAGE_REJECTED; size/paragraph ceilings are RESOURCE_LIMIT. Read-only; this does not read or change the HWPX session document."
}

fn text_replace() -> &'static str {
    "Replace a paragraph grapheme range [start,end), or insert non-empty text at [p,p). No tabs/newlines in text. Check edits[\"text.replace\"].modes (replace/insert) and apply requires arguments (e.g. controls=preserve); default controls=reject requires a subrange/position in textSpans. Empty paragraphs advertise {start:0,end:0}; insertion uses the following character run, then preceding run, then the empty paragraph first run. controls=preserve changes only text fragments and keeps objects/tabs/line breaks in original XML order; crossing field markers is refused; leading objects stay before the first text fragment. Existing empty hp:t may be reused; no text fragment means UNSUPPORTED_CONTENT_IN_RANGE. CONTROLS_PRESERVED reports count and new grapheme offsets. Unsupported markup/attributes still fail. Requires expectedVersion; VERSION_CONFLICT means re-query. FORMAT_COLLAPSED warns about character shapes. dryRun=true verifies without committing."
}

fn text_replace_all() -> &'static str {
    "Replace every exact match of `find` with `text` in one atomic step (one revision, one undo unit). Searches the whole document, or only `scope` (nodeId of a section, table, cell or paragraph) and the paragraphs below it, table cells included. Matching is exact Unicode codepoints, leftmost and non-overlapping per paragraph; a match is replaced only if it lies wholly inside the paragraph's editable `textSpans`. Matches that cross an object, tab, line break or field marker, split a grapheme cluster, touch unsupported markup, or sit in a locked paragraph are skipped and reported: `skippedCount`, `skipped[]` (per reason `crosses_control`, `grapheme_boundary`, `unsupported_markup`, `paragraph_not_editable` with `count` and up to 50 `locations` {nodeId, range}) and a `MATCHES_SKIPPED` warning; nothing is skipped silently. `find` must not be empty; `text` may be empty (deletes matches) but has no tabs/newlines. More than `maxMatches` (default 1000, max 10000) replaceable matches is refused with `RESOURCE_LIMIT` before anything is changed. Result: `replacementCount`, `paragraphCount`, and `matches[]` previews ({nodeId, range, before, after}; first 200). Run with `dryRun: true` first to review the list; the real call uses the same rules. Requires `expectedVersion`; `VERSION_CONFLICT` means re-query. Any failure leaves the document unchanged; `history_undo` restores the previous bytes. Character shapes inside a match collapse to the first one (`FORMAT_COLLAPSED`)."
}

fn equation_list() -> &'static str {
    "List equations in documentId or scope nodeId. Returns equation nodeId, paragraphNodeId, id, decoded script and scriptRaw (exact inner XML), shapeComment, size and all equation properties. Optional limit 1..500 (default 100) and nextCursor; cursor binds to document, scope and revision. Empty/absent/opaque scripts remain visible. Read-only."
}
fn equation_get() -> &'static str {
    "Read equation target (existing equation nodeId), with all equation_list fields and paragraphTextPreview (first 60 graphemes). scriptRaw preserves XML entity spellings, script contains decoded characters when readable. Missing/wrong-kind targets fail. Read-only."
}
fn equation_set_script() -> &'static str {
    "Replace only hp:script text of equation target with nonempty script. XML text escaping is automatic: pass literal script characters, not XML-encoded text. Attributes, size, position, margins and comment are preserved. Refuses opaque script markup, XML-forbidden characters, unbalanced groups and unverified backslash/quoted/control notation. No LaTeX/MathML conversion or full equation grammar validation. Requires current expectedVersion; supports dryRun, command_batch and byte-exact history_undo."
}
fn equation_set_properties() -> &'static str {
    "Set only supplied props on equation target: baseLine (uint32), textColor (#RRGGBB), font, baseUnit (positive uint32), lineMode (CHAR or LINE), version. At least one property; strings must be nonempty XML text without controls. Script and size remain byte-identical; no size recalculation or equation creation. Requires current expectedVersion; supports dryRun, command_batch and byte-exact history_undo."
}

fn text_format() -> &'static str {
    "Apply character properties only to a non-empty half-open grapheme range in paragraph target (nodeId). props requires one or more of bold, italic, underline, strikeout (booleans), heightHwpunit (integer 100..100000), textColor (#RRGGBB). fontRef is unsupported. Split run/text carriers while preserving other attributes, original text/entity bytes and unselected shapes. Reuse an identical charPr or append at most one new charPr; requests needing multiple new definitions fail with UNSUPPORTED (multiple_charPr_required). controls=reject by default; preserve leaves tabs/objects in their original shape and position, but field boundary crossings and opaque markup fail. Requires documentId and current expectedVersion; on VERSION_CONFLICT re-query. dryRun validates without changing document/history. Supported in command_batch; one call/batch is one undo unit."
}

fn paragraph_format() -> &'static str {
    "Update paragraph `target` (`nodeId` from `document_inspect`/`element_query`) with `alignment` (`left`, `center`, `right`, `justify`, `distribute`, `distributeSpace`) and/or `lineSpacing` (`{\"kind\": \"percent\", \"value\": 1..=1000}`), reusing or cloning the paragraph shape without altering global styles. Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query). Refuses read-only documents or uncloneable shapes (`UNSUPPORTED`), missing targets (`TARGET_NOT_FOUND`), or calls with neither option set (`INVALID_REQUEST`). Set `dryRun: true` to preview without changing the document."
}

fn cell_set_text() -> &'static str {
    "Replace text in existing cell target (nodeId); merged anchor cells are supported. Supply exactly one of text (one paragraph only) or paragraphs (string array matching the exact existing paragraph count). No new paragraphs are created. Wrong count returns INVALID_ARGUMENT with detail.paragraphCount; multi-paragraph cells with text return UNSUPPORTED and a paragraphs hint. controls=reject is default; controls=preserve keeps inline objects/tabs/line breaks (field marker crossings are refused) and reports CONTROLS_PRESERVED. Empty paragraphs use the first run; no run, nested tables/cells, unsupported markup/attributes or read-only documents still fail. Check edits[\"cell.setText\"].available and apply its requires guidance: requires.paragraphs=N means supply a string array of exactly N paragraphs, not the number N; requires.controls=preserve means also supply controls=preserve. Requires expectedVersion; on VERSION_CONFLICT re-query. dryRun=true verifies without changing document or history."
}

fn table_set_column_widths() -> &'static str {
    "Set table target's positive HWPUNIT widths array (exact colCnt) or uniform width. Updates merged cellSz by column sums and the absolute table width; reports tableSize and changedCells in intents. Zero-spacing, complete, determinable grids only; effective margins require content space. Requires expectedVersion; supports dryRun, atomic batch and exact Undo. To add or remove rows/columns use `table_insert_row`/`table_delete_row`/`table_insert_column`/`table_delete_column`."
}
fn table_set_row_heights() -> &'static str {
    "Set table target's positive HWPUNIT heights array (exact rowCnt) or uniform height. Updates each cellSz by row-span sums and the absolute table height. Stored heights are minimum extents; Hancom may grow rows for text. Refuses indeterminate merged grids or margin violations. Requires expectedVersion; supports dryRun, batch and Undo."
}
fn cell_set_format() -> &'static str {
    "Set existing cell target's borderFillId (string or integer), background (#RRGGBB or none), and/or partial cellMargin {left,right,top,bottom}. Unknown IDs return TARGET_NOT_FOUND. Background selects an existing resource with identical borders; no match returns UNSUPPORTED/background_resource_unavailable. Never adds nodes or modifies shared header resources. Margins enable hasMargin and must leave content space. Requires expectedVersion; supports dryRun, batch and Undo."
}
fn table_insert_row() -> &'static str {
    "Insert `count` (default 1, max 1024) empty rows into table target so the first new row gets the 0-based address `index` (0..=rowCnt; `index` = number of rows kept above it, so \"after the 3rd row\" is `index: 3`). Each new cell clones the attributes, borders and margins of a neighbouring row (the row above `index`, or the row at `index` when the one above is a title row or has vertical merges) with one empty paragraph that keeps the first paragraph's shape; the table height grows by the template row height. CREATES NEW NODES: the new cells and paragraphs get new nodeIds listed in `structure[].created` (null in `dryRun`), existing cells keep their nodeIds but cells at or below `index` move down by `count` rows (`structure[].shifted`, `changedIds`) — re-query addresses before targeting by row/col. Refused: `index` out of range (`INVALID_ARGUMENT`/`index_out_of_range`), more than 1024 rows (`table_size_limit`), a boundary that splits a merged cell (`UNSUPPORTED`/`insert_inside_merged_cell`), no plain neighbouring row (`no_simple_template_line`), nested tables, cell zones, relative or non-determinable geometry. Cached layout of the table and everything after it in the section is dropped. Requires expectedVersion (`VERSION_CONFLICT` if stale); supports dryRun, atomic `command_batch` and exact Undo/Redo (Redo keeps the same new nodeIds)."
}
fn table_delete_row() -> &'static str {
    "Delete `count` (default 1) rows of table target starting at 0-based address `index`; the table shrinks by their heights and rows below move up. REMOVES NODES: the deleted cells and paragraphs are listed in `structure[].removed`; Undo restores them with the same nodeIds. Refused with `UNSUPPORTED`/`merged_cell_blocks_delete` (detail.cells) when any merged cell overlaps the range, `delete_content_unsupported` when a cell holds pictures, equations, fields, shapes or notes, and `INVALID_ARGUMENT` for a range outside the table (`index_out_of_range`) or deleting every row (`would_empty_table`). Deleted text is not removed from the preview text file (`stale.prvText`). Requires expectedVersion; supports dryRun, atomic `command_batch` and exact Undo/Redo."
}
fn table_insert_column() -> &'static str {
    "Insert `count` (default 1, max 1024) empty columns into table target so the first new column gets the 0-based address `index` (0..=colCnt). Each row gets new cells cloned from the neighbouring column (the one left of `index`, else the one at `index`) with empty text; the table widens by the cloned column's width times `count` (existing columns keep their width — follow with `table_set_column_widths` to fit a page). CREATES NEW NODES (`structure[].created`; null ids in `dryRun`); cells at or right of `index` move right and appear in `structure[].shifted`. Refused: out-of-range `index`, more than 1024 columns, a boundary that splits a merged cell (`insert_inside_merged_cell`), no plain neighbouring column, nested tables, cell zones, non-determinable column widths. Requires expectedVersion; supports dryRun, atomic `command_batch` and exact Undo/Redo."
}
fn table_delete_column() -> &'static str {
    "Delete `count` (default 1) columns of table target starting at 0-based address `index`; the table narrows by their widths and cells to the right move left. REMOVES NODES (`structure[].removed`; Undo restores the same nodeIds). Refused: merged cells overlapping the range (`merged_cell_blocks_delete`), cells holding objects or fields (`delete_content_unsupported`), a row that would be left without cells (`row_would_become_empty`), a range outside the table, deleting every column. Requires expectedVersion; supports dryRun, atomic `command_batch` and exact Undo/Redo."
}
fn cell_set_size() -> &'static str {
    "Set nonmerged cell target's positive HWPUNIT width and/or height. Changes that entire column/row track, propagates dimensions to intersecting cells and the table total, and reports changedCells/tableSize in intents. Direct merged-cell requests return UNSUPPORTED/merged_cell_size_unsupported; use table track commands instead. Requires expectedVersion; supports dryRun, batch and exact Undo."
}

fn form_fill() -> &'static str {
    "Fill empty form value cells from exact label cells in tables. `values` maps each label (exact cell text, single line) to the text to write; `scope` limits the search to one table or section nodeId. For each label the tool finds the one cell whose text equals the label, then writes the value into the same-row right cell, or, when there is no right cell, the cell directly below it — only when that cell is empty and editable (merged anchor cells and multi-paragraph cells are supported through the cell.setText rules; only the first paragraph receives the value). Nothing is guessed: a missing label, more than one matching label cell, no right/below cell, an ambiguous or non-empty target, two labels resolving to the same cell, or a blocked cell is reported per label in `skipped` with its `reason`. `strict: true` (default) rejects the whole request before changing anything and returns UNSUPPORTED/TARGET_NOT_FOUND/AMBIGUOUS_TARGET with `detail.failures`; `strict: false` fills the resolvable labels, reports the rest in `skipped`, and emits a FORM_FILL_SKIPPED warning. The whole request is one revision and one undo unit. Requires expectedVersion; VERSION_CONFLICT means re-query. dryRun=true validates and previews without changing document or history."
}

fn image_replace() -> &'static str {
    "Replace the media of picture `target` (`nodeId` from `document_inspect`/`element_query`) with a static PNG or JPEG file at `imagePath` (inside `--input-root`), preserving the picture's position, size, and caption while leaving other pictures sharing the old media untouched. Requires `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query). Refuses read-only documents, cropped/effect pictures, or animated/non-PNG/JPEG images (`UNSUPPORTED`), disallowed paths (`PATH_NOT_ALLOWED`), or images over 40.96 megapixels (`RESOURCE_LIMIT`). Set `dryRun: true` to preview without changing the document."
}

fn paragraph_set_style() -> &'static str {
    "Set existing header styleId (integer) on exactly one of target (paragraph nodeId) or non-empty targets (array). Changes only styleIDRef; missing style returns TARGET_NOT_FOUND. No new styles. Requires expectedVersion; supports dryRun, command_batch and exact Undo."
}
fn image_fit() -> &'static str {
    "Fit picture target to its current frame: mode contain (aspect preserved inside), cover (center crop to fill), stretch (fill with distortion). Embedded PNG/JPEG pixels stay unchanged; source imgDim/imgClip use 75 HWPUNIT/pixel. Preserve renderingInfo matrices and manifest embedding. Requires expectedVersion; supports dryRun, command_batch and exact Undo."
}
fn header_set_text() -> &'static str {
    "Replace text of an existing header/footer paragraph. scope is a section nodeId (default first section); kind header/footer; paragraphIndex defaults to 0, counting direct header/footer paragraphs in document order. Missing paragraph returns TARGET_NOT_FOUND; margin/numbering/field or other controls are refused with reason header_control. Never creates header/footer/text carriers. Requires expectedVersion; supports dryRun, command_batch and exact Undo."
}

fn command_batch() -> &'static str {
    "Execute multiple edit `steps` (`field.setText`, `text.format`, `text.replace`, `text.replaceAll`, `paragraph.format`, `cell.setText`, `form.fill`, `image.replace`, `paragraph.setStyle`, `image.fit`, `header.setText`, `table.setColumnWidths`, `table.setRowHeights`, `table.insertRow`, `table.deleteRow`, `table.insertColumn`, `table.deleteColumn`, `cell.setFormat`, `cell.setSize`, `equation.setScript`, `equation.setProperties`) atomically as a single revision increment and single undo unit. Requires one top-level `expectedVersion` equal to the current revision (`VERSION_CONFLICT` if stale → re-query); each step's ranges and preconditions are evaluated against the state produced by preceding steps. If any step fails, the entire batch rolls back with no state change and reports the failing step index. Set `dryRun: true` to validate and preview the combined diff without changing the document."
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

fn document_stats() -> &'static str {
    "Return document-wide or scoped summary metrics across structural elements (sections, paragraphs, tables, cells, pictures, equations), controls (fields, footnotes, endnotes, memos), and total extended grapheme cluster character count. When `scope` is supplied (nodeId of section, table, cell, or paragraph), counts only elements within that subtree. Read-only; does not modify the document or increment revision."
}

fn section_list() -> &'static str {
    "List document sections in spine order with paragraph counts and paper/column settings (orientation, width, height, margins, column count, and page start number). Reports exact XML attributes and marks missing/inherited settings as unspecified without guessing. Supports cursor pagination (`limit`, `cursor`). Read-only; does not modify the document or increment revision."
}

fn note_list() -> &'static str {
    "List footnotes and endnotes across the document with note kind, numbering, body paragraph nodeId, combined text, and referring paragraph nodeId. Reports null when note number is not found in XML. Supports kind filter (`footnote` or `endnote`) and cursor pagination (`limit`, `cursor`). Read-only; does not modify the document or increment revision."
}

fn document_export() -> &'static str {
    "Independently verify preservation invariants and write the current document (`documentId`) to a new `.hwpx` file at `path` (inside `--output-root`). Original input files are never overwritten, and exporting to an existing file is refused with `TARGET_EXISTS` (`PATH_NOT_ALLOWED` if outside `--output-root`, `VERIFY_FAILED` if verification fails). Read-only with respect to session state and `revision`."
}

fn document_export_text() -> &'static str {
    "Convert the open document (`documentId`) to Markdown or HTML and write it to a new file at `path` (absolute, inside `--output-root`, ending in `.md`/`.markdown` for `format` \"markdown\" or `.html`/`.htm` for \"html\"). Optional `scope` is a section `nodeId` (default: every section); `includeNotes` (default true) controls footnotes/endnotes. Converts paragraphs, tables (merged cells: HTML `colspan`/`rowspan`; Markdown leaves the covered slots empty and says so at the top), pictures as media references (file name + SHA-256, no image file is created) with captions, equation `hp:script` text verbatim as code, footnotes/endnotes, and headers/footers in their own section. Centre/right paragraph alignment is a style in HTML and ignored (with a comment) in Markdown. Text is escaped; objects it cannot interpret are written as `⟦미지원: name⟧` and counted. The response carries only a summary (`path`, `bytes`, `sha256`, `counts`); the text is in the file. Never changes the document: `revision` is unchanged and `expectedVersion` is not needed. Refused with `TARGET_EXISTS` (file exists), `PATH_NOT_ALLOWED` (outside `--output-root`), `INVALID_ARGUMENT` (extension does not match `format`), or `RESOURCE_LIMIT` (output larger than `max_text_export_bytes`, 16 MiB; the limit is in `detail.limitBytes`)."
}

/// Command name (as in the registry) → description.
pub const TABLE: &[(&str, Describe)] = &[
    ("command.list", command_list),
    ("command.describe", command_describe),
    ("document.open", document_open),
    ("document.inspect", document_inspect),
    ("document.stats", document_stats),
    ("section.list", section_list),
    ("note.list", note_list),
    ("document.close", document_close),
    ("element.query", element_query),
    ("element.get", element_get),
    ("link.list", link_list),
    ("trackchange.list", trackchange_list),
    ("section.getPage", section_get_page),
    ("section.setPage", section_set_page),
    ("section.setPageNumbering", section_set_page_numbering),
    ("section.setVisibility", section_set_visibility),
    ("style.list", style_list),
    ("table.get", table_get),
    ("picture.list", picture_list),
    ("field.list", field_list),
    ("field.setText", field_set_text),
    ("hwp5.inspect", hwp5_inspect),
    ("text.format", text_format),
    ("equation.list", equation_list),
    ("equation.get", equation_get),
    ("equation.setScript", equation_set_script),
    ("equation.setProperties", equation_set_properties),
    ("text.replace", text_replace),
    ("text.replaceAll", text_replace_all),
    ("paragraph.format", paragraph_format),
    ("cell.setText", cell_set_text),
    ("form.fill", form_fill),
    ("image.replace", image_replace),
    ("table.setColumnWidths", table_set_column_widths),
    ("table.setRowHeights", table_set_row_heights),
    ("cell.setFormat", cell_set_format),
    ("cell.setSize", cell_set_size),
    ("table.insertRow", table_insert_row),
    ("table.deleteRow", table_delete_row),
    ("table.insertColumn", table_insert_column),
    ("table.deleteColumn", table_delete_column),
    ("paragraph.setStyle", paragraph_set_style),
    ("image.fit", image_fit),
    ("header.setText", header_set_text),
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
    ("document.exportText", document_export_text),
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
    "Preservation-first HWPX document engine. Workflow: (1) call `document_open` to get `documentId` and `revision`; (2) call `document_inspect` or `element_query` to find target `nodeId`s and text; check per-command `edits` and choose text ranges within paragraph `textSpans` (`editable` means any available command); (3) call edit tools (`text_format`, `text_replace`, `text_replace_all` for document-wide exact find-and-replace, `paragraph_format`, `cell_set_text`, `form_fill` for label-based form filling, `image_replace`, `paragraph_set_style`, `image_fit`, `header_set_text`, `equation_set_script`, `equation_set_properties`, `command_batch`) with `documentId` and `expectedVersion` = current revision (use `dryRun: true` to preview without changing anything); (4) on `VERSION_CONFLICT`, re-query before retrying; (5) export to a new `.hwpx` via `document_export` or save a `.hwpwork` project via `project_save`; to get the content as Markdown or HTML use `document_export_text` (writes a new text file, never changes the document). For stateless MCP 2026-07-28 requests (server/discover then tools/call), documentId is required in arguments on every document tool call; use the handle from document_open. For legacy initialize-based sessions (2025-11-25 and earlier), documentId is optional and defaults to the currently open document. Handles last until document_close or server exit; reopen after restart. Text ranges are half-open `[start, end)` extended grapheme cluster indices within a single paragraph. Request sequence numbering is handled automatically by the MCP adapter."
}
