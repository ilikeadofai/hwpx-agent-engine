# Independent Stage 3 renderer / preview review

Date: 2026-10-16. Base: `09af4b9` plus current environment wiring. Reviewer owns this file only. No source/test edits, commit, push, or workspace formatting were performed. Earlier combined GPT attempts exited before reviewing; **none of the findings below are inherited from those attempts**.

## Scope and verdict

Read design §§7/9/10/11/13, `2026-10-16-stage2-3-tasks.md`, `stage3-adapter-notes.md`, and `stage3-rhwp-rebuild.md`. Reviewed `crates/hae-render-rhwp/`, `hae-exec/src/preview.rs`, the preview handler/configuration in `session.rs`, and the current binary environment wiring at its call sites. Stage 2 transport ownership remains with the parent reviewer. The renderer crate and `hae-exec/src/preview.rs` match `09af4b9`. Concurrent executor changes add configuration-aware command discovery; that diff was inspected and does not change the reported rendering defects. Other agents' changes were not modified. Source line numbers below refer to `09af4b9` unless stated otherwise, so the concurrent insertion in `session.rs` does not invalidate references.

**Stage 3 completion contract is not met.** Real sandboxed SVG/PNG generation and corpus join counts pass, but the SVG boundary, resource enforcement, unavailable policy and preview result contracts have reproducible failures. Task 3-6's required visual evidence file is absent. This is not a claim that normal corpus rendering fails, nor a claim of a demonstrated escape from actual bubblewrap.

Severity: P1 = high, fix before claiming safe preview completion; P2 = medium, correctness/availability or required contract failure; P3 = low, malformed-output validation defect.

## Findings, in severity order

### R1 — P1: encoded CSS and SMIL bypass the SVG safety boundary

**Location:** `crates/hae-render-rhwp/src/svg_safety.rs:213`, `:247`, `:253`, `:264`, `:376`; exposure through `crates/hae-exec/src/preview.rs:116`.

`href`/`src` and literal `url(` scanning is insufficient to establish the promised script-free, external-reference-free SVG. CSS escape decoding is absent; XML character references occurring inside `<style>` are recognized but never appended to `style_text`. SVG animation can also set a dangerous URL through `attributeName` + `to`/`values`, outside the checked attribute names.

**Reproduction:** each of these is accepted by the public `check_svg` with `safe=true, issues=[]`:

```xml
<svg xmlns="http://www.w3.org/2000/svg"><style>@im\70ort 'https://example.invalid/x.css'; rect{fill:u\72l(https://example.invalid/x.svg#g)}</style><rect width="1" height="1"/></svg>
<svg xmlns="http://www.w3.org/2000/svg"><style>@im&#112;ort 'https://example.invalid/x.css'; rect{fill:u&#114;l(https://example.invalid/x.svg#g)}</style></svg>
<svg xmlns="http://www.w3.org/2000/svg"><a id="a"><text>click</text></a><set href="#a" attributeName="href" to="javascript:alert(1)"/></svg>
```

Independent Chromium parsing confirmed the escaped CSS becomes `fill: url("https://example.invalid/x.svg#g")`; XML parsing restores `@import` and `url`; attaching the third SVG changes the link's `href.animVal` to `javascript:alert(1)`. No link was clicked and no script execution or external-resource fetch was needed for this proof. `CheckedSvg` accepts the same bytes. PNG's external resolver is a useful separate defense, but returned opt-in SVG still violates §§9–10 / task 3-1. This finding does not assert that today's fixed rhwp emits every example, or that the engine itself contains a browser viewer.

**Impact:** consumers can receive active/external SVG labeled checked; embedding it in an active SVG context invalidates the safety guarantee.

**Fix:** use a strict SVG element/attribute allowlist, reject animation that can mutate URLs (or all SMIL), and parse/normalize CSS and XML references before checking. Restrict `data:` resources by element and MIME instead of allowing arbitrary data URLs. Add the above cases as independent negative fixtures.

### R2 — P1: embedded raster images bypass the pixel guard before decoding

**Location:** `crates/hae-render-rhwp/src/png.rs:83–88`, `:102`, `:111`, `:127`.

The guard checks only the destination SVG canvas. The data-image resolver accepts compressed raster bytes without checking their own dimensions or decoded allocation. resvg later decodes them in the engine process; it does not inherit bubblewrap's process boundary.

**Reproduction:** a valid 277,758-byte SVG with a **1×1 target canvas** and an embedded 7000×7000 RGBA PNG passes `CheckedSvg`. Calling `rasterize_svg(..., PngOptions { scale: 1.0, max_pixels: 1 })` succeeds with a 73-byte PNG. Peak child RSS measured with `getrusage` is **203,092 KiB**. The embedded image is 49,000,000 pixels, already above the default 40,960,000-pixel image limit. Generation and harness code are below.

**Impact:** a tiny requested preview can allocate hundreds of MiB; larger embedded rasters can exhaust or abort the host engine before a structured limit error. A target-canvas check is not an embedded-image limit.

**Fix:** inspect every data-image's dimensions with checked arithmetic before handing it to a decoder, enforce individual and cumulative decoded-byte/pixel budgets, cap parse/tree complexity before allocating, and contain rasterization in a supervised resource-limited worker. Propagate configured limits rather than substituting defaults.

### R3 — P1: preview buffers and renderer memory do not use the session budget

**Location:** `crates/hae-render-rhwp/src/lib.rs:455–506`; `crates/hae-render-rhwp/src/sandbox.rs:44–61`, `:247–254`; `crates/hae-exec/src/preview.rs:65–78`.

`Limits.max_session_memory` is never checked by the adapter or preview assembly, and the child supervisor installs no memory resource limit. All selected SVG/PNG pages are retained before response selection. Distinct manifest entries can repeat the same pathname, so the disk cap does not even bound retained SVG bytes. PNG options in the executor also come from `Default::default()` rather than the executor's `max_image_pixels`.

**Reproduction:** fake renderer writes one 1,048,650-byte safe SVG, lists the same file for eight page entries, and succeeds under real bwrap. With `cfg.limits.max_session_memory = 1_048_576`, preview returns `Rendered` and **8,389,200 retained SVG bytes**, without any limit diagnostic. The on-disk payload is only approximately 1 MiB. This is a reduced-budget reproduction, not a claim that an intentionally lowered budget was the default. At defaults, repeated paths can multiply a permitted 64 MiB file across 100 output pages, beyond 512 MiB.

**Impact:** per-file, pipe and `/out` caps do not provide the required actual session-memory bound. Even a request whose content will later be omitted from the 4 MiB response pays the full transient allocation cost.

**Fix:** reserve from the document/session's remaining budget before reads, JSON/SVG parsing, rasterization and retention; include current/history state, font data and transient buffers; stream/release pages or publish bounded artifacts rather than retaining all. Reject duplicate manifest paths if not explicitly supported. Apply OS-enforced child/worker memory limits and return explicit resource-limit failures.

### R4 — P2: page limiting happens after all SVG pages have been rendered

**Location:** `crates/hae-render-rhwp/src/lib.rs:376–396`, `:422–429`.

The adapter invokes unrestricted `export-svg`, waits for every page to finish, then applies `max_preview_pages` to the files it reads. Neither a requested subset nor the first-100 cap is passed to the renderer. The vendor command's range selection (`spikes/vendor/rhwp-src/src/cli/outputs/vector.rs:409` onward) selects all document pages when no target is provided.

**Reproduction:** the existing `page_limit_makes_a_partial_result` test renders a 28-page real document with a limit of 10, but only checks the 10 returned pages. The call at `lib.rs:379–385` still exports all 28. A reduced limit of one with the fake renderer likewise produces two output SVGs and then returns one with `Partial`; no pre-render page cap exists. The diagnostic claims only the first pages “were rendered” although later pages were generated too.

**Impact:** a long document may hit the time/disk caps on pages beyond 100 and return an unavailable error instead of the required usable first-100 partial preview (§10, task 3-2). Unrequested pages also consume resources.

**Fix:** determine page count with a bounded operation and render only the allowed range, using a verified CLI page/range mechanism or a pinned renderer change. Update manifest parsing to handle selected pages without assuming every page from zero was exported. Preserve total-page metadata separately.

### R5 — P2: failed PNG-only previews report rendered with no displayable page

**Location:** `crates/hae-render-rhwp/src/lib.rs:493–506`, `:533–539`; `crates/hae-exec/src/preview.rs:98–121`.

Rasterization failure adds a warning but leaves the page in `pages_out` and does not affect status. The executor's default request asks for PNG and suppresses SVG, so it can return no display content at all while claiming success.

**Reproduction:** fake renderer returns a safe 100000×100000 SVG. Adapter returns `status=Rendered`, zero PNGs, and `PNG_FAILED`. Default `document.preview {}` returns `ok=true`, `status="rendered"`, and a page containing only `page`, `svgBytes`, and `svgSha256`; neither `png` nor `svg` nor an `omitted` reason is present. This was reproduced through `SessionExecutor`, not just the renderer API.

**Impact:** an agent sees a completed preview with no usable image. Resource failure is reduced to an auxiliary warning. For mixed pages, status similarly fails to describe missing requested PNGs.

**Fix:** derive status from requested artifacts actually delivered: failed when none can be delivered, partial for missing artifacts on some pages. Include an explicit per-page failure reason. If SVG fallback is intended, return it under an explicit documented policy.

### R6 — P2: preview response budgeting counts raw SVG, not serialized JSON

**Location:** `crates/hae-exec/src/preview.rs:89–100`, `:116–117`, `:128–151`.

`svg_cost` is raw byte length; JSON escaping can approximately double it (or grow it further for controls). The fixed 768 KiB allowance also does not bound variable font metadata and diagnostic messages.

**Reproduction:** fake renderer supplies a safe SVG containing a comment of 2,300,000 quote characters. `document.preview {"includeSvg":true,"includePng":false}` accepts the SVG into its estimated budget, returns `status="rendered"` without `omitted`, and serializes to **4,601,188 bytes**, above the default 4,194,304-byte response limit. Exact harness below. This is the preview assembly defect; generic transport capping remains the parent reviewer's scope.

**Impact:** an otherwise successful preview is rejected by a correctly capped transport, or an uncapped caller receives an oversized response. Requesting fewer pages cannot repair an underestimate for a single page.

**Fix:** account for actual serialized/escaped content plus metadata and envelope before adding a page, and bound diagnostics/font metadata. Use an output-root artifact for content too large to inline (§11) so callers can still obtain the requested image.

### R7 — P2: missing or empty text previews silently pass the page-join precheck

**Location:** `crates/hae-render-rhwp/src/pages.rs:821–832`.

A missing/null `textPreview` is skipped. An empty preview compared with a nonempty paragraph produces `n=0` and an empty-slice equality, so the section is still considered checked and available.

**Reproduction:** make a `/tmp` copy of `ref_empty.hwpx` replacing `<hp:t/>` with `<hp:t>EXPECTED</hp:t>`. Feed `join_pages` one Page/Body/TextLine with `pi:0` and one dump page with section 0 and a `fullParagraph` at `paraIndex:0`. With `textPreview:""` or `null`, the result is `sections[0].available=true`, paragraph pages `[1]`. With `"definitely WRONG prefix"`, it correctly returns `text-prefix-mismatch` and makes the section unavailable. The first two inputs cannot establish the required correspondence.

**Impact:** malformed or incomplete renderer output yields apparently usable node/page correspondence instead of §7's fail-closed unavailable result. Such coordinates must not be used as evidence to identify an edit target.

**Fix:** require the text-preview field/type and handle empty-vs-nonempty as mismatch; compare a defined prefix/truncation format. Mark the entire affected section unavailable when required comparison evidence is absent.

### R8 — P2: the post-render bwrap version probe bypasses process supervision

**Location:** `crates/hae-render-rhwp/src/lib.rs:554`, `:646–653`.

Unlike every actual rhwp call, `bwrap --version` uses unbounded `Command::output()`: no timeout, cancellation, stdout/stderr caps or child cleanup. This runs on every successful preview after the renderer finishes.

**Reproduction:** configure a wrapper that forwards all ordinary arguments to `/usr/bin/bwrap` and, only for `--version`, sleeps three seconds then prints its version. The isolation self-test and renderer still use genuine bwrap. With `renderer_timeout_secs=1`, preview returns `Rendered` after **3.12 seconds** instead of timing out this probe. Replacing the sleep with an infinite wait would leave the preview stuck; flooding stdout would allocate outside the configured caps. Only the finite three-second case was executed.

**Impact:** preview can hang or consume unbounded memory after a successful bounded render; adapter cancellation does not interrupt the probe.

**Fix:** obtain/cache tool metadata through a bounded supervised process, or omit optional version metadata when it cannot be collected within the current deadline. Respect cancellation and output caps for this child too.

### R9 — P3: a manifest length of zero disables the exact-size validation

**Location:** `crates/hae-render-rhwp/src/lib.rs:460`, `:690–696`, `:742`.

`expect_len == 0` is a sentinel for render-tree outputs without manifest sizes, but the SVG manifest parser also accepts zero as a declared size.

**Reproduction:** return a manifest entry `{"page":0,"path":"/out/p.svg","bytes":0}` while writing a nonempty 62-byte safe SVG at that path. Preview returns `Rendered`, reads all 62 bytes, and reports no invalid-output diagnostic. The same lie with `bytes:1` is correctly rejected by an existing test.

**Impact:** the documented exact manifest/file-size boundary has an exception for malformed output; this does not itself bypass the separate 64 MiB file cap or demonstrate a filesystem escape.

**Fix:** use `Option<u64>` for an absent expected size, and compare `Some(0)` exactly for manifest entries.

## Verified protections and remaining policy decisions

- Actual rhwp executions: `preview` verifies the sandbox before staging/version/export; `page_info` also verifies before dump/tree export. `--version` of rhwp runs on the same copied executable **inside** bwrap. Runtime argument construction retains namespace, network, environment, capability and read-only bind restrictions on every rhwp invocation. No unsandboxed fallback was found. The self-test is cached once per adapter, not rerun before each command; the isolation arguments still apply each time.
- Timeout and cancellation: existing tests pass. An additional real-bwrap test forked a grandchild repeatedly appending `/out/ticks`; after timeout ticks stayed **38→38**, and after cancellation **54→54**, each observed over another 200 ms. This supports process-tree termination for the tested bwrap configuration. A bare `child.kill()` is not being reported as a tree-leak bug without evidence.
- Output caps: stdout/stderr and directory caps are enforced while running **and rechecked after exit** (`sandbox.rs:475–486`). The quick-exit bypass suspicion was rejected by code inspection. The sampled directory cap can overshoot within the 100 ms sampling interval; no hard filesystem quota was verified. This does not replace the memory findings above.
- Output paths: absolute sandbox `/out/` prefix, dot components, NUL, symlinks at every checked component, nonregular files, per-file size and ordinary manifest-size mismatches are checked. Existing escape/symlink fixtures pass. `read_out_file` uses pathname checks followed by open rather than descriptor-relative no-follow I/O. An exploit against a live hostile writer was **not** demonstrated: real-bwrap process-tree shutdown and random 0700 staging reduce that opportunity. Treat a TOCTOU claim as a hardening question, not a proven escape.
- **Manifest-only policy deviation:** `lib.rs:632–639` reads synthesized `render_tree_NNN.json` names, using dump-page count instead of a file manifest. Adapter notes disclose this; §9 does not state this exception. Paths still use the same checked reader, so no traversal bug follows merely from synthesized names. Either obtain a real manifest or explicitly approve/narrow the specification exception before claiming strict manifest-only compliance.
- Revision/state: `handle_preview` checks stale `expectedVersion` before rendering (`session.rs:834–852`). Rendering borrows a fixed document/revision synchronously, and failure paths do not install a candidate document, change stale-layout markers or advance content revision. Existing failure tests compare inspect results and exported bytes before/after missing sandbox, missing renderer and timeout; all pass. Existing post-edit/stale-version checks also pass. No stale-state mutation bug was found.
- Fonts: file-list/hash metadata and missing-family warnings exist, but full glyph coverage/substitution detection was not independently established. The configured font directory is live (not snapshotted); source reads/hash/database loading can race with external font changes, and rhwp also has read-only `/usr`. Exact font-environment reproducibility needs a separate targeted check; this review does not claim a demonstrated mismatch.
- Inline-only output is explicitly disclosed by adapter notes. A large page has no artifact retrieval path, and requesting fewer pages does not solve an individually oversized page. This remains a §11 delivery-policy gap even after serialized response accounting is fixed.

## Tests and completion accounting

Executed with Rust **1.93.1**. The real renderer file SHA-256 was independently read as `e91cd25fbf5bd8045371ba3e9bdaa1b05cf7df146d04773e65ec1e99e6cd05a7`. It was executed only through the adapter's real bwrap verification; no direct unsandboxed rhwp command was run by this review.

```sh
cargo test -p hae-render-rhwp -p hae-exec -- --nocapture
```

Initial run: **62 passed, 0 failed**. After concurrent configuration-discovery wiring appeared, reran the same two-package suite: **63 passed, 0 failed** (hae-exec unit 5 + preview integration 7; renderer unit 34 + page joins 4 + PNG integration 1 + render integration 12). The new command-discovery test passed as well. Real corpus evidence includes 28 pages; 637 paragraphs, 8 tables, 308 cells and 4 resolvable pictures; first-page PNG for both the 28-page school-plan corpus and the 19-page plan corpus. Additional `/tmp` probes reproduced R1–R3 and R5–R9, checked the reduced page cap, and checked descendant termination. Existing tests passing does not cover these negative cases.

| Task | Evidence / completion assessment |
|---|---|
| 3-0 rebuild | Existing rebuild record plus current 1.93.1 toolchain/hash confirmed; rebuild itself not repeated by this review. |
| 3-1 SVG safety | Existing examples pass; R1 prevents completion. |
| 3-2 adapter | Real SVG and isolation tests pass; R3/R4/R8/R9 and manifest-policy gap remain. |
| 3-3 page join | Corpus counts match; R7 violates unavailable policy for incomplete comparisons. |
| 3-4 PNG | Real first-page PNG generation passes; R2 prevents resource-safety completion. |
| 3-5 executor preview | Fixed revision/state preservation passes; R5/R6 prevent output-contract completion. |
| 3-6 visual check | `docs/decisions/stage3-visual-check.md` absent at review time. PNG bytes/tests are not visual acceptance evidence; this review did not substitute a visual verdict. |

`bash scripts/check.sh`, workspace clippy/fmt, complete transport suites, fresh rhwp rebuild, and Windows/Hancom comparisons were not run. The embedded Rust harness matches the tested `/tmp` source, its Python input-generation block was parsed and executed successfully from this report, and code fences were checked for balance. Transport wiring was inspected for the explicit renderer-configuration calls only; binary-interface validation belongs to the parent's review. `coverage=unknown` and `nativeComparison=not_checked` are appropriately retained.

## Reproduction appendix

Local working evidence is `/tmp/sol-stage3-review/{Cargo.toml,Cargo.lock,src/main.rs,bomb.svg,nonempty.hwpx,results.txt}`. The lockfile was copied from this checkout before the reported probes so dependency versions match the checkout. These `/tmp` files are scratch evidence, not additional repository-owned source/test changes. To rerun while they exist:

```sh
ROOT=/home/mattc/Documents/Codex/hwpx-agent-engine
cargo build --offline --manifest-path /tmp/sol-stage3-review/Cargo.toml --target-dir "$ROOT/target"
for mode in guard memory png-failure zero-bytes page-limit join tree png response bwrap-version; do
    "$ROOT/target/debug/sol-stage3-review" "$mode"
done
```

To reconstruct after `/tmp` cleanup, create `/tmp/sol-stage3-review/src`, save the following manifest and Rust program there, copy the checkout's `Cargo.lock`, and generate the two inputs with the Python block following it.

### Cargo.toml

```toml
[package]
name = "sol-stage3-review"
version = "0.0.0"
edition = "2024"
[dependencies]
hae-render-rhwp = { path = "/home/mattc/Documents/Codex/hwpx-agent-engine/crates/hae-render-rhwp" }
hae-exec = { path = "/home/mattc/Documents/Codex/hwpx-agent-engine/crates/hae-exec" }
hae-model = { path = "/home/mattc/Documents/Codex/hwpx-agent-engine/crates/hae-model" }
hae-diag = { path = "/home/mattc/Documents/Codex/hwpx-agent-engine/crates/hae-diag" }
serde_json = "=1.0.151"
```

### src/main.rs

```rust
use hae_render_rhwp::{AdapterConfig,RhwpAdapter,RenderRequest,fonts::FontSet,png::{CheckedSvg,PngOptions,rasterize_svg},sandbox::{Sandbox,TempDir,RunLimits,CancelToken},join_pages};
use hae_diag::Limits;
use serde_json::{json,Value};
use std::{fs,path::Path,os::unix::fs::PermissionsExt,sync::Arc};
const ROOT:&str="/home/mattc/Documents/Codex/hwpx-agent-engine";
fn main() {
 let td=TempDir::new_in(Path::new("/tmp"),"sol-stage3-").unwrap();
 let fonts=td.path().join("fonts");fs::create_dir(&fonts).unwrap();
 let arg=std::env::args().nth(1).unwrap_or_default();
 if arg=="guard" {
  for (name,s) in [
   ("css-escape",r#"<svg xmlns="http://www.w3.org/2000/svg"><style>@im\70ort 'https://example.invalid/x.css'; rect{fill:u\72l(https://example.invalid/x.svg#g)}</style><rect width="1" height="1"/></svg>"#),
   ("style-xml-reference",r#"<svg xmlns="http://www.w3.org/2000/svg"><style>@im&#112;ort 'https://example.invalid/x.css'; rect{fill:u&#114;l(https://example.invalid/x.svg#g)}</style><rect width="1" height="1"/></svg>"#),
   ("smil",r##"<svg xmlns="http://www.w3.org/2000/svg"><a id="a"><text>click</text></a><set href="#a" attributeName="href" to="javascript:alert(1)"/></svg>"##),
   ("data-html-link",r#"<svg xmlns="http://www.w3.org/2000/svg"><a href="data:text/html,%3Cscript%3Ealert(1)%3C/script%3E"><text>click</text></a></svg>"#),
  ] {let c=hae_render_rhwp::svg_safety::check_svg(s.as_bytes());println!("{name}: safe={} issues={:?}",c.is_safe(),c.issues);}
 } else if arg=="png" {
  let svg=fs::read("/tmp/sol-stage3-review/bomb.svg").unwrap();let fs=FontSet::scan(&fonts).unwrap();
  let out=rasterize_svg(CheckedSvg::new(&svg).unwrap(),&fs,&PngOptions{scale:1.,max_pixels:1});
  println!("1px target with 7000x7000 embedded raster: {:?}",out.as_ref().map(|x|x.len()));
 } else if arg=="join" {
  let doc=hae_model::Document::open(fs::read("/tmp/sol-stage3-review/nonempty.hwpx").unwrap(),&Limits::default()).unwrap();
  let tree=json!({"type":"Page","children":[{"type":"Body","children":[{"type":"TextLine","pi":0,"children":[]}]}]});
  for preview in [Value::String("".into()),Value::Null,Value::String("definitely WRONG prefix".into())] {
   let dump=json!({"pages":[{"section":0,"columns":[{"items":[{"kind":"fullParagraph","paraIndex":0,"textPreview":preview}]}]}]});
   let out=join_pages(&doc,&[tree.clone()],&dump); println!("preview={preview:?} sections={:?} counts={:?}",out.sections,out.counts);
  }
 } else if arg=="tree" {
  let sb=Sandbox::new("/usr/bin/bwrap",&fonts);sb.self_test(td.path()).unwrap();
  let ind=td.path().join("in");let out=td.path().join("out");fs::create_dir(&ind).unwrap();fs::create_dir(&out).unwrap();
  for do_cancel in [false,true] {
   let token=CancelToken::new();let tok=token.clone(); if do_cancel {std::thread::spawn(move||{std::thread::sleep(std::time::Duration::from_millis(150));tok.cancel();});}
   let mut limits=RunLimits::with_timeout_secs(1);limits.timeout=std::time::Duration::from_millis(400);
   let e=sb.run(&ind,&out,&["/usr/bin/sh","-c","(while :; do echo x >> /out/ticks; sleep 0.02; done) & sleep 30"],&limits,if do_cancel {Some(&token)} else {None}).unwrap_err();
   let n=fs::metadata(out.join("ticks")).unwrap().len();std::thread::sleep(std::time::Duration::from_millis(200));let after=fs::metadata(out.join("ticks")).unwrap().len();println!("tree cancel={do_cancel}: {e:?}, ticks={n}->{after}");assert_eq!(n,after);
  }
 } else {
  let body=match arg.as_str() {
   "memory" => r#"printf '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><!--' > /out/p.svg
head -c 1048576 /dev/zero | tr '\000' x >> /out/p.svg
printf -- '--></svg>' >> /out/p.svg
s=$(wc -c < /out/p.svg)
printf '{"format":"svg","pageCount":8,"pages":['
for i in 0 1 2 3 4 5 6 7; do [ "$i" -gt 0 ] && printf ','; printf '{"page":%s,"path":"/out/p.svg","bytes":%s}' "$i" "$s"; done
printf ']}'"#,
   "png-failure" => r#"printf '<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="100000"/>' > /out/p.svg
s=$(wc -c < /out/p.svg)
printf '{"format":"svg","pageCount":1,"pages":[{"page":0,"path":"/out/p.svg","bytes":%s}]}' "$s""#,
   "response" => r#"printf '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><!--' > /out/p.svg
head -c 2300000 /dev/zero | tr '\000' '"' >> /out/p.svg
printf -- '--></svg>' >> /out/p.svg
s=$(wc -c < /out/p.svg)
printf '{"format":"svg","pageCount":1,"pages":[{"page":0,"path":"/out/p.svg","bytes":%s}]}' "$s""#,
   "bwrap-version" => r#"printf '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>' > /out/p.svg
s=$(wc -c < /out/p.svg)
printf '{"format":"svg","pageCount":1,"pages":[{"page":0,"path":"/out/p.svg","bytes":%s}]}' "$s""#,
   "zero-bytes" => r#"printf '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>' > /out/p.svg
printf '{"format":"svg","pageCount":1,"pages":[{"page":0,"path":"/out/p.svg","bytes":0}]}'"#,
   "page-limit" => r#"printf '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>' > /out/p1.svg
cp /out/p1.svg /out/p2.svg
s=$(wc -c < /out/p1.svg)
printf '{"format":"svg","pageCount":2,"pages":[{"page":0,"path":"/out/p1.svg","bytes":%s},{"page":1,"path":"/out/p2.svg","bytes":%s}]}' "$s" "$s""#,
   _=>panic!("mode"),
  };
  let bin=td.path().join("rhwp");fs::write(&bin,format!("#!/usr/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'rhwp vfake'; exit 0; fi\n{body}\n")).unwrap();fs::set_permissions(&bin,fs::Permissions::from_mode(0o755)).unwrap();
  let mut cfg=AdapterConfig::new(bin,&fonts); cfg.limits.max_session_memory=1024*1024;if arg=="page-limit"{cfg.limits.max_preview_pages=1;}
  if arg=="bwrap-version" {let wrapper=td.path().join("bwrap");fs::write(&wrapper,"#!/usr/bin/sh\nif [ \"$1\" = \"--version\" ]; then sleep 3; echo slow-bwrap; exit 0; fi\nexec /usr/bin/bwrap \"$@\"\n").unwrap();fs::set_permissions(&wrapper,fs::Permissions::from_mode(0o755)).unwrap();cfg.bwrap_path=wrapper;cfg.limits.renderer_timeout_secs=1;}
  let ad=Arc::new(RhwpAdapter::new(cfg));
  let r=ad.preview(&RenderRequest{document_id:"doc-review",content_revision:7,hwpx:b"PK",pages:None,want_png:arg=="png-failure",png:Default::default()},None).unwrap();
  println!("{arg}: status={:?}, total={}, returned={}, svg_bytes={}, png_count={}, notes={:?}",r.status,r.total_pages,r.pages.len(),r.pages.iter().map(|p|p.svg.len()).sum::<usize>(),r.pages.iter().filter(|p|p.png.is_some()).count(),r.notes);
  if arg=="png-failure" || arg=="response" {let out=td.path().join("exec-out");fs::create_dir(&out).unwrap();let mut exec=hae_exec::SessionExecutor::new(format!("{ROOT}/tests/fixtures/corpus"),out,Limits::default()).unwrap();assert!(exec.call("document.open",json!({"path":format!("{ROOT}/tests/fixtures/corpus/ref_empty.hwpx")}),None,false).ok);exec.set_renderer(ad);let r=exec.call("document.preview",json!({"includeSvg":arg=="response","includePng":arg!="response"}),None,false);if arg=="response" {let v=r.result.as_ref().unwrap();println!("exec response: status={}, serialized_bytes={}, omitted={}",v["status"],serde_json::to_vec(&r).unwrap().len(),v["pages"][0].get("omitted").is_some());}else{println!("exec png-failure: {}",serde_json::to_string(&r).unwrap());}}
 }
}
```

### Generate bounded embedded-raster and page-join inputs

```python
from pathlib import Path
import base64, re, struct, zipfile, zlib

root = Path('/home/mattc/Documents/Codex/hwpx-agent-engine')
work = Path('/tmp/sol-stage3-review')
work.mkdir(exist_ok=True)
n = 7000
co = zlib.compressobj(9)
parts = []
row = b'\x00' + b'\x88\x33\x22\xff' * n
for _ in range(n):
    parts.append(co.compress(row))
parts.append(co.flush())
def chunk(kind, data):
    return (struct.pack('>I', len(data)) + kind + data
            + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff))
png = (b'\x89PNG\r\n\x1a\n'
       + chunk(b'IHDR', struct.pack('>IIBBBBB', n, n, 8, 6, 0, 0, 0))
       + chunk(b'IDAT', b''.join(parts)) + chunk(b'IEND', b''))
(work / 'bomb.svg').write_bytes(
    b'<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">'
    b'<image width="1" height="1" href="data:image/png;base64,'
    + base64.b64encode(png) + b'"/></svg>')
with zipfile.ZipFile(root / 'tests/fixtures/corpus/ref_empty.hwpx') as src:
    with zipfile.ZipFile(work / 'nonempty.hwpx', 'w') as dst:
        for info in src.infolist():
            data = src.read(info.filename)
            if info.filename == 'Contents/section0.xml':
                text, count = re.subn(r'<hp:t\s*/>', '<hp:t>EXPECTED</hp:t>',
                                      data.decode(), count=1)
                assert count == 1
                data = text.encode()
            dst.writestr(info, data)
```
