# Adapter review fixes: R3 / R4 / R7 / R8 / R9

Date: 2026-10-16. Scope: `hae-render-rhwp` adapter, sandbox, font handling, page join, and owned regression tests. No git command, commit, or workspace formatting was performed. `svg_safety.rs`, `png.rs`, and `hae-exec` are owned by the other implementers and were not edited here. File-local rustfmt used `skip_children=true` so it did not recurse into those modules.

## Resolutions

### R3: remaining memory, duplicate paths, fonts, renderer OS limit

- Added `RhwpAdapter::preview_with_budget(&self, request, cancel, memory_budget)`. The existing `preview` delegates with `cfg.limits.max_session_memory`; the explicit allowance is also clamped to that configured ceiling. The executor must supply its remaining allowance after accounting for current/history state and response assembly. That executor accounting remains parent-owned.
- Reservations precede JSON parsing, SVG reads/scans, SVG retention, and rasterization. They include the snapshot, bounded metadata, retained pages, font payload/database workspace, and conservative parser workspace. Pipe capacities subdivide remaining memory before reader allocation. An oversized declared SVG reservation fails before the file is opened, including when the file is absent. Renderer staging now streams and hashes the executable in a 64 KiB buffer rather than reading the entire binary into a host Vec.
- Duplicate SVG paths are rejected both within one manifest and across individual page export manifests. Expected page number, total count and selected-page cardinality are verified before retention.
- Font payloads are limited to 32 MiB per file and 64 MiB in total, and further limited by the caller's available allowance. Directory traversal has a depth limit and an 8192-entry aggregate cap. Reads are capped and require the observed size to remain exact. Font database loading uses the hashed immutable payloads rather than reopening live files. Cached font sets receive the same per-call memory charge.
- The adapter calls the other agent's `rasterize_svg_with_budget(svg, fonts, opts, max_decoded_bytes)` and clamps `opts.max_pixels` to the configured image-pixel limit. It reserves raster/encoding headroom and retains only the actual PNG byte charge afterward. PNG implementation and worker isolation remain outside this ownership scope.
- Every rhwp command runs **inside verified bwrap**, through argv-only `/usr/bin/prlimit --as=N:N -- /in/rhwp ...`. `/usr/bin/prlimit` availability is checked. The hard address-space bound is no greater than the current remaining allowance, is inherited by descendants, and cannot be raised by the unprivileged renderer. No unsafe pre-exec hook is used. Metadata identifies the resource and scope.
- Negative tests first reproduced ignored session memory and an unlimited renderer address space. Green tests verify reservation before opening a file, cached-font accounting, duplicate-path rejection, the inherited finite hard limit, and actual denial of an oversized allocation in the sandbox. An oversized sparse font file and aggregate font-payload limits are covered.

**Limit scope:** the OS bound is per-process virtual address space (`RLIMIT_AS`), not aggregate descendant RSS or a cgroup limit. Host JSON/SVG/font/tree workspace uses conservative reservations, not allocator-level accounting. resvg still runs in the host process; tree/path/filter/font/encoder allocations do not acquire an OS-enforced host memory ceiling from this adapter. These changes close the demonstrated duplicate-buffer and unbounded rhwp cases; they do **not** establish a strict end-to-end RSS guarantee. A supervised raster worker or equivalent stronger containment is still needed for that claim. A fatal allocator/signal exit is reported as renderer failure; its cause is not inferred to be memory exhaustion without evidence.

### R4: limit export before SVG generation

Inspected pinned vendor `src/cli/outputs/vector.rs`: argument parsing accepts zero-based `--page` / `-p`; export selects `vec![p]`; the JSON envelope retains document-wide `pageCount` and lists only exported pages (`renderedCount`). This version has no general range option for SVG export.

Preview now runs bounded, supervised `dump-pages --json` first, reads the total page count, and invokes `export-svg --page INDEX --json` only for valid requested pages within `max_preview_pages`. It never invokes unrestricted SVG export. Page manifests need not begin at page zero; each single-page export must match the requested physical page and the independently obtained total. Total count remains in `PreviewOutput` and metadata. All renderer subprocesses share one deadline after the cached isolation self-test, so individual page exports do not multiply the configured timeout.

The RED test's renderer refuses to run unless the allowed page is selected explicitly. GREEN verifies physical page 2 is exported, page 3 is excluded by a cap of 2, and total count remains 3. Existing real 28-page corpus rendering, page selection, and the lowered page-cap fixture still run against the pinned binary. Page counting still lays out the document, and separate page exports reload it; this is bounded by process limits and can cost more CPU than a native range-capable renderer.

### R7: no empty comparison evidence for nonempty own text

Missing, null, non-string, empty, or whitespace-only `textPreview` now marks the section `text-prefix-mismatch` when the engine's normalized own paragraph text is nonempty. Empty-slice equality no longer establishes availability. Tests rebuild `ref_empty.hwpx` with `EXPECTED` and exercise each missing/empty/type case; their initial failure showed the section incorrectly available.

This intentionally changes the real school-plan corpus result: pinned rhwp provides empty preview evidence for nonempty text in section index 15. The section now fails closed. The explicit regression expects 27 available sections, 621 resolved paragraphs, 6 tables, 290 cells, and 4 pictures; section 15 withholds 16 paragraphs, 2 tables and 18 cells. Of the 14 already unavailable grouped pictures, one now has the section-level `text-prefix-mismatch` reason and 13 retain container reasons. The earlier 637 / 8 / 308 resolved counts cannot be retained as a success expectation under the corrected policy. The document is not edited by this change.

### R8: supervise optional bwrap metadata

Removed unbounded `Command::output()` from bwrap version lookup. The probe now uses the same supervised runner with cancellation, stdout/stderr caps (at most 4096 bytes each), a 250 ms maximum, and the remaining renderer deadline. Missing/slow/flooding/failed optional metadata is omitted. Cancellation still reaches the preview caller.

The supervisor uses safe `CommandExt::process_group(0)` and rustix group termination for ordinary host-wrapper descendants, in addition to killing/waiting for bwrap and preserving its existing namespace shutdown behavior. The RED case took over 3 seconds with a 1-second renderer timeout; GREEN finishes within the test deadline with version metadata omitted. Flood, descendant-tick shutdown, and cancellation-during-version tests also pass. This does not claim cleanup of arbitrary configured wrappers that intentionally escape their process group; actual rhwp remains inside bwrap's PID namespace.

### R9: zero is an exact expected size

Output readers distinguish `Some(expected_len)` from `None`; every SVG manifest size is passed as `Some`, including zero. Exact length is checked before and after reading. A declared zero for a nonempty file now fails. The RED test originally accepted `<svg/>` with an expected length of zero; it is green after the change.

## Render-tree manifest boundary

Pinned `export-render-tree` has no renderer-generated file manifest. The trusted adapter now inventories the finished `/out` directory before reads, accepts exactly `render_tree_001.json` through the bounded dump-page count (maximum 2000), rejects missing/extra/nonregular entries, records regular-file lengths, and uses that generated inventory for exact-size bounded reads. This narrowly scoped protocol adapter is documented in the crate. SVG export continues to require rhwp's manifest. The extra-file negative test failed before this change and passes afterward.

## Validation and integration notes

Toolchain: Rust 1.93.1. Real renderer SHA-256 independently checked: `e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7`. The default in-tree renderer fixture works without setting `HAE_RHWP`. Real renderer commands and tests retain the isolation self-test; no unsandboxed fallback was introduced. Existing executor tests compare inspect/export state before and after preview failures; executor state handling was not edited here.

Final command results are recorded below after completion. Earlier integration runs exposed two issues outside this ownership scope:

- Bare `cargo test --workspace` stops at `hae-cli::renderer_env::preview_renders_page_one_when_hae_rhwp_is_set`: no explicit font directory is supplied, so this machine's `/usr/share/fonts` contains 2655 matching files totaling 1,514,205,448 bytes. The new payload limit correctly refuses that directory. Use an explicit bounded font fixture / `HAE_FONT_DIR`; do not weaken the memory ceiling to make the ambient-font test pass. CLI/JSONL/MCP fixture ownership remains with the parent.
- During concurrent executor assembly changes, `preview::contract_tests::oversized_svg_is_delivered_as_an_exact_output_root_artifact` failed at `SVG artifact path`; this was not changed in the adapter. A subsequent whole-workspace run uses an explicit temporary NanumGothic directory and records the current result below.
- An initial clippy run found `manual_is_multiple_of` in the concurrently edited `svg_safety.rs`; that file was left to its owner. An owned test's `cloned_ref_to_slice_refs` was corrected locally.

### Completed validation

- `cargo test -p hae-render-rhwp`: **90 passed, 0 failed, 0 ignored** on the final adapter code. Breakdown: unit 38; adapter budget/process regressions 11; page joins 6; real first-page PNG 1; render integration 12; concurrently owned SVG/PNG limit integration 22. Log: `/tmp/hae-adapter-final-tests.log`.
- Whole workspace with an explicit temporary directory holding NanumGothic, using `HAE_FONT_DIR=<temporary font directory> cargo test --workspace`: **227 passed, 0 failed, 0 ignored**, exit 0. This snapshot included 21 SVG/PNG limit tests; the other agent subsequently added one more, included in the final 90-test renderer run. This full run also passed the parent-owned artifact test that failed during its earlier concurrent edit. Log: `/tmp/hae-adapter-workspace-bounded-font-tests.log`. The temporary font directory was removed after the run.
- `cargo clippy -p hae-render-rhwp --all-targets -- -D warnings`: exit 0 after final adapter changes and the other agent's SVG lint correction.
- File-local `rustfmt --check --edition 2021 --config skip_children=true` on the eight owned Rust files: exit 0. No workspace formatter was run.
- The earlier bare-workspace ambient-font failure was observed before the parent updated the CLI/JSONL/MCP fixtures. In the subsequent coordination update, the parent reports that all three `renderer_env` tests now explicitly select bounded font directories and bwrap. Those parent-owned edits were not changed here. The owned real-renderer fixtures select a bounded font set and the default in-tree rhwp binary successfully. The parent's final full gate will establish the final combined default-environment result.

The 16 MiB-per-MiB SVG workspace reservation and 17 MiB-per-MiB JSON reservation are conservative admission policies, **not** a measured or proven RSS expansion factor. Page metadata is reserved for the selected page count. Font payload/database/metadata reservation is checked before payload reads. The existing self-test remains cached separately from the shared renderer-command deadline. Native Hancom comparison and visual acceptance are parent-owned and were not performed by this implementation task.

### Final ownership handoff

The parent subsequently confirmed that executor calls now pass remaining session memory after current/history, snapshot, and an 8x response-workspace reservation. It also reports the artifact helper's seven tests complete, its exact SVG-artifact tests green (4/4), and one real MCP wire-budget artifact integration passing. These are coordination reports from the owner; the focused renderer results above are this implementer's direct validation. No additional full-workspace loop was started after this handoff instruction. Final workspace formatting, full-gate verification, visual acceptance, and the independent Luna review belong to the parent.
