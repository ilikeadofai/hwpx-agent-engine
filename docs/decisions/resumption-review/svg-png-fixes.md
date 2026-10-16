# R1/R2 SVG safety and embedded raster limits

Date: 2026-10-16. Implementation scope: `crates/hae-render-rhwp/src/svg_safety.rs`, `src/png.rs`, new `tests/svg_png_limits.rs`, and this note. No git commands, commits, manifest edits, new dependencies, unsafe code, or workspace formatting. Integration in `lib.rs`, fonts/sandbox/pages, and executor preview belongs to the other writers.

## R1: static SVG boundary

`check_svg` now accepts an explicit static element/attribute vocabulary. SMIL (`set`, `animate`, `animateMotion`, `animateTransform`, `discard`), mutation attributes such as `attributeName`/`to`, event handlers, scripts, foreign namespaces/content, unsupported extension attributes, and `<use>` expansion are refused. Same-document fragments remain supported. `data:` hrefs are confined to `<image>` and base64 PNG/JPEG/GIF/WebP MIME types; links, nested SVG data images, and `data:text/html` are refused.

XML character references are resolved in attributes and appended to the collected `<style>` text, including references split across ordinary text and CDATA events. CSS backslashes (including XML-encoded backslashes), comments, and general at-rules are refused. Ordinary CSS declarations use an allowlist of presentation properties; CSS URLs may reference only same-document fragments.

One corpus-required exception is parsed separately: `@font-face` accepts one quoted family and one quoted base64 `data:font/ttf`, `font/otf`, `font/opentype`, `font/woff`, or `font/woff2` source with a matching `format()`, plus restricted static weight/style descriptors. External sources, `local()`, source fallback lists, non-font MIME types, extra properties, escapes, and other at-rules are refused. This preserves fresh output from `render-p35-font-native-bitmap.hwpx`. usvg does not load those CSS font sources; PNG text still uses the explicit `FontSet`. This check restricts font references; it does not validate the internals of embedded font binaries.

The checker also enforces a single SVG root, UTF-8/XML characters, 64 MiB input, 100,000 elements, depth 128, 128 attributes per element, and bounded diagnostics. Unknown vocabulary fails closed. These are syntax/complexity bounds, not a complete CPU or RSS bound for arbitrary paths, filters, patterns, or fonts.

## R2: headers and reservations before raster decoding

The data-image resolver validates the declared MIME against PNG/JPEG/GIF/WebP headers before returning any `ImageKind` to resvg. It reads dimensions and container framing without decoding pixels:

- PNG: IHDR dimensions and chunk bounds; duplicate IHDR and animation are refused. `iCCP`, `zTXt`, and `iTXt` are refused before the downstream decoder can inflate ancillary metadata.
- JPEG: SOF dimensions, supported precision/component/sampling factors, segment framing, scans, and EOI. Multiple frames, unsupported coding, MPO, and DNL dimension mutation are refused.
- GIF: logical-screen and image-descriptor dimensions, color-table/subblock framing, and one static frame. A small screen cannot hide an oversized frame. Multiple frames are refused.
- WebP: RIFF/chunk bounds, VP8/VP8L bitstream dimensions and VP8X canvas dimensions. Mismatched canvas/bitstream dimensions, duplicate bitstreams, animation and unknown chunk types are refused.

Individual dimensions, cumulative embedded pixels plus canvas pixels, and cumulative conservative byte reservations use checked arithmetic. Reservations include RGBA destination bytes, codec scratch/fixed tables, and compressed-image copies. JPEG scratch includes MCU padding. Lossless WebP/compressed ALPH also reserves by encoded length because meta-Huffman groups can be large independently of dimensions; this deliberately conservative reservation can reject otherwise valid lossless WebP under a small budget. Ordinary lossy VP8 does not pay that table reservation. Resolver errors survive usvg's normal silent-image-omission behavior and become a returned `PngError` before `resvg::render` or destination allocation.

Nonfinite or nonpositive scale becomes `1.0` for both sizing and rendering. Large positive scales still return a limit error. `PngOptions` keeps its existing two fields, so struct literals remain valid; `max_pixels` now bounds each raster/canvas and their cumulative total.

## Public API and errors

```rust
pub fn rasterize_svg_with_budget(
    svg: CheckedSvg<'_>,
    fonts: &FontSet,
    opts: &PngOptions,
    max_decoded_bytes: u64,
) -> Result<Vec<u8>, PngError>;
```

The existing `rasterize_svg(svg, fonts, opts)` delegates to this function with `Limits::default().max_session_memory` (currently 512 MiB). The adapter must call the bounded API with its actual remaining allowance; the compatibility wrapper cannot infer session usage.

`PngError::TooLarge { width, height, max_pixels }` identifies an individual pixel limit. `PixelLimit { required_pixels, max_pixels }` identifies the cumulative pixel limit. `MemoryLimit { required_bytes, max_decoded_bytes }` identifies an overflow or byte reservation limit. `InvalidImage(String)` and `UnsupportedImage(String)` distinguish malformed/mismatched headers from unsupported formats/features. Existing `Parse` and `Encode` remain.

The byte argument bounds decoded-buffer reservations, **not total process RSS**. SVG parsing/tree/path allocations, fonts, filter/mask/pattern intermediates, PNG encoding/output retention and other document/session state still need caller headroom and a separately supervised worker/process memory limit. Header preflight is not independent full validation of compressed pixel streams: downstream resvg decoders can still omit a corrupt stream that has structurally acceptable headers. This work does not claim complete R3/session-memory or worker containment.

## Verification and evidence

The initial RED run of nine regressions failed all nine against the old implementation: the three review attack classes, HTML/data links, unknown vocabulary/CSS, accepted compressed PNG, JPEG/GIF/WebP dimension guards, malformed resource omission, and invalid scale. The valid 7000x7000 1-bit PNG fixture has a 5,977-byte independently generated zlib stream and is constructed without allocating its pixels. Old code returned a 1x1 PNG with `max_pixels: 1`; fixed code returns `TooLarge` from header preflight. The review's 203,092 KiB RSS figure was not remeasured here.

The explicit byte-budget test was independently RED for a 10x10 RGBA canvas with a 399-byte budget. Codec-scratch and malformed data encoding regressions were also observed RED. An additional unrestricted-cap regression exposed a debug overflow in the new MCU-padding calculation; checked multiplication corrected it and the regression is GREEN.

Fresh sandboxed rhwp output from all nine exportable corpus documents is accepted, including the font/bitmap regression (57 SVG pages in the observed run). `irb-sample.hwpx` is excluded explicitly: rhwp rejects this non-Hancom ZIP in format detection before producing SVG. Existing page-one PNG tests exercise both `research-school-plan-reference.hwpx` and `gyehoek-reference.hwpx`; positive pixel tests cover PNG, JPEG, GIF, lossy WebP, and lossless WebP.

Commands run:

```text
cargo test -p hae-render-rhwp --test svg_png_limits --lib -- --test-threads=1
cargo test -p hae-render-rhwp --no-fail-fast
cargo clippy -p hae-render-rhwp --lib --test svg_png_limits -- -D warnings
cargo test --workspace --no-fail-fast
rustfmt --edition 2021 --config skip_children=true <the three owned Rust files>
```

The final full renderer-crate run passes all 90 tests: 38 unit, 11 adapter-resource, 6 page-join, 1 corpus-PNG, 12 render, and 22 SVG/PNG regression tests. Strict Clippy passes with warnings denied. This run includes the final checked-arithmetic correction.

The workspace run after concurrent artifact API changes compiled and ran all packages: 224 passed and 3 failed. Each failure was `preview_renders_page_one_when_hae_rhwp_is_set` in `hae-cli`, `hae-jsonl`, or `hae-mcp`'s `renderer_env` target. All three returned `RENDERER_UNAVAILABLE`, reason `memory_limit`, message `bounded font scan: font payload exceeds memory limit`, before SVG checking/rasterization. An earlier run stopped while the concurrent `IoRoots::write_preview_artifact` API was not yet implemented; the subsequent workspace run supersedes that compile failure. An earlier transient fake-renderer JSON failure in `unsafe_pages_are_withheld` was also superseded by the passing full renderer-suite run. No changes outside the assigned scope were made to address these failures.
