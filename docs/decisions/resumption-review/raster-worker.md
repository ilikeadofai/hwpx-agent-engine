# Supervised PNG raster worker (residual P1)

Date: 2026-10-17. Scope: `crates/hae-render-rhwp` (`src/lib.rs`, `src/fonts.rs`, new `src/worker.rs`, new `src/bin/hae-raster.rs`, `tests/common/mod.rs`, new `tests/raster_worker.rs`, new `tests/raster_env.rs`) and this note. No git commands, no new dependencies, no unsafe, no shell strings, no workspace-wide formatting (`rustfmt --config skip_children=true` on owned files only). `sandbox.rs`, `png.rs`, `svg_safety.rs`, executor/MCP/CLI/project were not modified.

## Policy

Product preview PNGs are made **only** by a separate `hae-raster` process. `RhwpAdapter::preview*` no longer calls `png::rasterize_svg*` and no longer builds a `fontdb` in the host process (the old `fonts.families()` call in the adapter is gone). There is no in-process fallback. The low-level `png::rasterize_svg[_with_budget]`, `CheckedSvg` and the two-field `PngOptions` are unchanged and remain for direct tests and for the worker itself.

Failure policy (never touches document state, content revision or history; the adapter stays usable afterwards):

| Condition | Result |
|---|---|
| worker path empty | `Err RENDERER_UNAVAILABLE`, reason `raster_worker_not_configured` (before any renderer work) |
| worker missing / not a regular file / over 256 MiB | `Err`, reason `raster_worker_missing` |
| cancellation | `Err`, reason `cancelled` (child process group killed) |
| overall preview deadline reached while the worker runs | `Err`, reason `timeout` (worker killed; deadline is the *remaining* preview deadline) |
| sandbox cannot start (`spawn_failed`) | `Err`, reason `spawn_failed` |
| less than 64 MiB left for the worker after parent reservations | `Err`, reason `memory_limit` |
| worker signal / abort (e.g. allocation past `RLIMIT_AS`) / non-zero exit / panic | per-page `PNG_FAILED` note (`raster_worker_killed` or `raster_worker_exit`, stderr tail ≤ 300 chars); the page keeps its SVG and has no PNG |
| worker-reported failure (exit 3, `result.json` `status:"error"`: `too_large`, `pixel_limit`, `memory_limit`, `parse`, `svg_unsafe`, `png_too_large`, …) | per-page `PNG_FAILED` (`raster_failed: <kind>: <message>`) |
| stdout/stderr/`/out` cap exceeded | per-page `PNG_FAILED` (`output_limit`) |
| output fails parent validation | per-page `PNG_FAILED` (`raster_output_invalid: <why>`) |

**R5 status rule (added on parent request):** when `want_png` is true and any returned page has no PNG, `RenderStatus` is `Partial` (previously it could stay `Rendered` next to `PNG_FAILED`). The SVG pages are retained. With `want_png == false` status is unchanged. All pages unsafe/absent is still `Failed`. The executor decides whether a PNG-only default response with no deliverable is a failure.

## Worker protocol (`hae-raster-v1`)

Run inside the same bwrap sandbox as rhwp (`--unshare-all`, no network, read-only `/usr`, cleared env, `--die-with-parent`) by `Sandbox::run_with_memory`, i.e. behind `/usr/bin/prlimit --as=N:N`, with the same supervisor: wall-clock = remaining preview deadline, cooperative cancel, stdout cap 4 KiB, stderr cap 16 KiB, `/out` cap `png_cap + 512 KiB` and ≤ 8 entries.

Per preview the parent stages once (`worker::RasterStage::prepare`, a fresh `hae-raster-*` temp dir, removed on drop):

- `/in/hae-raster`: a copy of the worker executable (0755), SHA-256 and size recorded while copying;
- `/in/fonts/NNNN.font` + `/in/fonts.json`: `FontSet::write_snapshot` (crate-private) writes the **already hashed** in-memory payloads under generated numeric names (`create_new`, never rereads the live directory); the manifest lists `file`, original relative `path`, `bytes`, `sha256`;
- the sandbox is built with an **empty** directory as its `/fonts` mount, so the live host font directory is not visible to the worker at all.

Per page the parent writes `/in/page.svg` and a fresh `/out` and runs

```
/in/hae-raster --svg /in/page.svg --svg-bytes N --svg-sha256 H --fonts-dir /in/fonts \
  --fonts-manifest /in/fonts.json --fonts-hash H --png /out/page.png --result /out/result.json \
  --scale S --max-pixels P --max-decoded-bytes B --max-png-bytes C
```

The worker (`worker::run_cli`, bin `hae-raster`) re-verifies SVG length/SHA-256, every font file (regular file, length, SHA-256, generated name) and the recomputed font-set hash (`FontSet::from_snapshot`), runs `CheckedSvg::new` and `rasterize_svg_with_budget`, refuses PNGs above `C`, writes `page.png` then `result.json` (`create_new`). Exit 0 ok, 2 usage, 3 reported failure. `result.json` carries protocol, worker version, echoed `svgSha256`/`fontsHash`, PNG file/bytes/sha256/width/height and the lower-cased font family names (≤ 1000, ≤ 200 bytes each) used for `FONT_UNAVAILABLE`.

The parent trusts none of it: `/out` must contain exactly `page.png` and `result.json`, both regular files (symlinks rejected via `read_out_file_bounded`); `result.json` ≤ 256 KiB, strict schema (`deny_unknown_fields`), protocol and echoed SVG hash / font hash must match; the PNG is read with the exact-size bounded reader, must end with the IEND chunk, have a valid signature and IHDR whose width/height equal the result and whose pixel count ≤ the configured cap, and its SHA-256 (computed by the parent) must equal the result's.

## Memory accounting (honest scope)

Before each page the parent reserves from the remaining session allowance: `png_cap = min(remaining/8, 64 MiB, out_cap_bytes)` for the PNG it will hold, plus 8 MiB for the bounded result read. **What remains is the worker's `RLIMIT_AS`** (and `max_decoded_bytes = RLIMIT_AS/4`, the existing decoded-buffer pre-check). So parent-held SVG/fonts/PNG headroom is subtracted first, and parser, tree, path, filter, font and encoder allocations of resvg happen only under the OS limit in the worker. `RLIMIT_AS` is **per-process virtual address space**, not RSS, not an aggregate with the parent and not a cgroup. It does not cover tmpfs (`/tmp` inside the sandbox; the worker writes nothing there), kernel memory or page cache. Workers run one at a time per preview. Floor `MIN_ADDRESS_SPACE = 64 MiB`: measured on a debug build an 800x1100 text page aborts at 32 MiB (`memory allocation of N bytes failed`, SIGABRT) and renders at 48 MiB. Host fonts are still charged 4× by the existing scan reservation (conservative; the host no longer builds a database).

## Public surface

- `AdapterConfig::raster_path: PathBuf` (explicit; PATH never searched). `AdapterConfig::new` defaults it to `worker::default_worker_path()`: `hae-raster` beside the running executable, with a trailing `deps` directory removed so test executables resolve to `target/<profile>/hae-raster`. `AdapterConfig::from_env` honours `HAE_RASTER` (empty ignored). Adding the field breaks struct-literal construction of `AdapterConfig`; none exists in the workspace.
- `pub mod worker` (`WORKER_NAME`, `PROTOCOL`, `default_worker_path`, `run_cli` for the binary). `FontSet::database_loaded()` is `#[doc(hidden)]`, an observation hook so tests can prove the host never built a font database.
- Metadata (no new `PreviewMetadata` field; the executor builds that struct in tests): `renderer.rasterizer = {name, protocol, sha256, bytes, fontSnapshot:{hash, files}}` (null when no PNG was requested) and `sandbox.rasterMemoryLimit = {tool, resource:"RLIMIT_AS", scope, bytesAtMost}` (null likewise; `bytesAtMost` is the largest per-page limit used).
- Behaviour change: `FONT_UNAVAILABLE` is now produced only when a worker rendered (the host has no font database); it says "raster output substitutes it", which was raster-specific already. With `want_png == false` the note is no longer emitted.

## Build prerequisites

`cargo build -p hae-render-rhwp --bin hae-raster` (or `cargo build --workspace`) must have run before any non-renderer crate's preview/PNG tests or the evaluation runner (the parent already added this to `scripts/check.sh`/evaluation and `HAE_RASTER` to the help strings). `cargo test -p hae-render-rhwp` builds the worker itself; its tests set `raster_path` from `CARGO_BIN_EXE_hae-raster`. Requests for the parent: none beyond what was already applied.

## Verification

TDD note: the implementation was written first and the tests second, so RED was obtained by **mutation** against the finished tests, each failing for the intended reason and restored afterwards:

| Mutation | Tests that failed |
|---|---|
| worker run without `RLIMIT_AS` (`None`) | `worker_runs_sandboxed_under_rlimit_as_…`, `worker_failures_are_contained_…` |
| parent skips PNG SHA-256 check | `invalid_worker_output_is_rejected_by_the_parent` |
| parent skips `/out` inventory check | `invalid_worker_output_is_rejected_by_the_parent` |
| host calls `fonts.families()` (builds fontdb) | `real_worker_png_equals_…`, `font_availability_notes_…` |
| `Partial` rule removed | `invalid_worker_output_…`, `worker_failures_…`, `symlinked_or_oversized_…`, `configured_pixel_and_png_caps_…` |

GREEN commands (all run, all pass; no full-workspace loop):

```text
cargo build -p hae-render-rhwp --bin hae-raster
cargo test -p hae-render-rhwp --no-fail-fast        # 39 lib + 11 adapter_budget + 6 pages_join + 1 png_page1 + 1 raster_env + 12 raster_worker + 12 render + 22 svg_png_limits, 0 failed
cargo clippy -p hae-render-rhwp --all-targets -- -D warnings
rustfmt --edition 2021 --config skip_children=true <owned .rs files>
```

New tests (`tests/raster_worker.rs`, `tests/raster_env.rs`, `fonts.rs` unit): real-worker PNG byte-identical to the in-process reference for a text page and for page 1 of both corpus documents (real rhwp, real fonts) with the font-snapshot hash equal to `metadata.fonts.hash`, worker SHA-256 equal to the file hash, host `database_loaded() == false`; a fake worker proving inside the sandbox that `RLIMIT_AS` is below the session allowance and equals 4× the decoded budget, `/in` and `/usr` are read-only, `/fonts` is empty, no host home, only loopback, cleared environment, exact stage listing, numeric font filenames with matching sha and recomputed set hash; over-allocation (64 × 64 MiB requests), SIGKILL, SIGABRT, silent exit, non-zero exit and structured exit 3 all contained with parent responsive, revision unchanged and no temp directories left; nine invalid-output forms rejected; symlinked `page.png` and over-cap output rejected; timeout `Err(timeout)` and cancellation `Err(cancelled)` while the worker runs; missing / directory / empty worker path explicit errors with no PNG and no host fontdb; configured request pixel cap, `max_image_pixels` and `out_cap_bytes` reach the real worker; `memory_limit` when under 64 MiB would remain; `HAE_RASTER` and sibling default; snapshot payload immutability against later live-file edits, tamper/rename/symlink/size/hash rejection.

## Residual scope

- `RLIMIT_AS` is virtual and per process; it is not a cgroup and says nothing about tmpfs/kernel memory. Accounting between parent and worker is by reservation, not measurement.
- A pathological SVG that hangs the worker consumes the remaining overall deadline and fails the whole preview with `timeout` (no per-page time slice).
- Each page costs one bwrap + worker start and a font parse; fonts are written to the stage once per preview.
- Worker integrity is by copied-executable SHA-256 recorded in metadata; there is no signature or pinned expected hash.
- `/in` stays visible to the worker including earlier staged material of the same preview (the same document).
