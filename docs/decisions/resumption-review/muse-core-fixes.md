# muse-core follow-up fixes — F1–F4 + C2 (hae-xml / hae-package)

- Parent review: `docs/decisions/resumption-review/muse-core.md` (scope `hae-zip` / `hae-xml` /
  `hae-package` vs spec §4/§11). This note records the authorized follow-up: fix F1–F4 with
  failing regression tests first, then green, plus the C2 manifest-href correction.
- Ownership respected: only `crates/hae-xml/`, `crates/hae-package/`, and this file touched.
  No changes to `hae-model` / `hae-edit` / `hae-verify` / `hae-project`, root manifests,
  ZIP gap policy, mimetype-descriptor policy, or `payload_set_sha256` form. No commits/pushes.
- Working tree contains unrelated uncommitted edits from concurrent agents (`Cargo.lock`,
  `crates/hae-edit/*`, `crates/hae-model/*`, …); they were not touched here. `git status` deltas
  attributable to this task are exactly the three files listed under "Changed files" below.

## Fixes

### F1 — duplicate expanded attribute names rejected (`crates/hae-xml/src/lib.rs`)
- Old: `seen` set keyed on qname string only → `<x xmlns:a="urn:U" xmlns:b="urn:U" a:k="1" b:k="2"/>`
  accepted.
- New: second key `seen_expanded: (ns_uri, local)` for all non-declaration attributes.
  Same URI + same local under different prefixes → `malformed_xml`. Namespace declarations
  (`xmlns`, `xmlns:*`) are tracked separately (qname-duplicate only) and never collide with
  normal attributes; declaration `Attr`s now store the full qname as `local` with `ns: None`
  so `Element::attr("foo")` can no longer match an `xmlns:foo` declaration.
- Kept accepting: same local with different URIs; `xmlns`/`xmlns:a` declarations alongside
  normal attributes; default-ns element with unprefixed `k` plus prefixed `a:k` (attributes
  never inherit the default namespace).

### F2 — QName colon rules (`parse_qname`, `crates/hae-xml/src/lib.rs`)
- Replaced `split_qname` (first-colon split, no validation) with `parse_qname`, applied to
  element names, attribute names, and declaration names: more than one `:` rejected, empty
  prefix/local rejected (`:x`, `x:`, `a:`, `:b`, `a::b`, `a:b:c`).
- `xmlns:a:b`-style names are rejected both in scope building and in attribute processing.

### F3 — XML 1.0 NCName validation (no ASCII-only restriction)
- Added `is_ncname_start` / `is_ncname_char` implementing XML 1.0 5th-edition ranges:
  `NameStartChar`/`NameChar` minus `:` (ASCII letters + `_`, `#xC0–#xD6`, `#xD8–#xF6`,
  `#xF8–#x2FF`, `#x370–#x37D`, `#x37F–#x1FFF`, `#x200C–#x200D`, `#x2070–#x218F`,
  `#x2C00–#x2FEF`, `#x3001–#xD7FF`, `#xF900–#xFDCF`, `#xFDF0–#xFFFD`, `#x10000–#xEFFFF`;
  plus `-`, `.`, digits, `#xB7`, `#x300–#x36F`, `#x203F–#x2040` as non-start chars).
- Each QName part (prefix, local, or whole unprefixed name) must be a valid NCName.
- Verified: `<1bad/>`, `<x 1a="2"/>`, `<-bad/>`, `<.bad/>` rejected; `_x`, `x-y.z`, Hangul
  (`한글`, U+AC00), CJK (`中`), middle-dot (`·`, U+00B7) in non-start position accepted;
  `·` in start position rejected.

### F4 — reserved `xml` / `xmlns` bindings (`crates/hae-xml/src/lib.rs`)
- `NS_XMLNS = "http://www.w3.org/2000/xmlns/"` added next to existing `NS_XML`.
- `xmlns:xml` must equal `NS_XML` (correct-URI rebinding still accepted; `xml:lang` then
  resolves to `NS_XML`); any other prefix bound to `NS_XML` rejected; `xmlns:xmlns`
  rejected; any prefix (or default) bound to `NS_XMLNS` rejected; empty-URI prefix
  rejection retained. `xmlns` as an element prefix stays rejected via unbound-prefix lookup.
- Seeded `xml → NS_XML` scope entry unchanged, so undeclared `xml:lang` keeps working.

### C2 — manifest `href` check is now path-segment based (`crates/hae-package/src/lib.rs:303`)
- Old: `href.contains("..")` (false-positive on legal segment `a..b`, and missed `.` segments).
- New, mirroring the container-rootfile check: reject iff `href.starts_with('/')`,
  contains `\`, or any `/`-separated segment is `..` or `.`.
- `BinData/a..b.bin` now resolves; `../outside.xml`, `a/../b.xml`, `a/../../b.xml`,
  `./x.xml`, `a/./b.xml` all reject with `path_escape`.

## Out of scope (left for parent, untouched)
- C1 (ZIP gaps/prefix), C3 (mimetype descriptor bit): no tightening, per instructions.
- C5 (`payload_set_sha256` flat form): unchanged; parent pins the golden vector.
- C4 (non-media IDREF ownership), C6–C8: no changes.

## Verification

RED (new tests fail on pre-fix code; tree restored byte-identical afterwards from `/tmp/opencode/fixbak/`):
- `cargo test -p hae-xml --lib` with HEAD `lib.rs` + new `tests.rs`:
  `8 passed; 4 failed` — exactly `f1…f4` FAILED, all 8 pre-existing tests ok.
- `cargo test -p hae-package --lib c2_` with old substring check + new tests:
  `0 passed; 2 failed` — positive `a..b` wrongly `path_escape`; `.`-segment hrefs wrongly
  accepted (`broken_manifest` instead of `path_escape`).

GREEN (final):
- `cargo test -p hae-zip -p hae-xml -p hae-package` → all ok:
  hae-package unit 5/5 (incl. 2 new C2), `adversarial` 1/1 (80+ cases, incl. reason codes),
  hae-xml 12/12 (8 old + 4 new), hae-zip 14/14, plus doctest binaries 0-tests ok.
- Corpus coverage included via hae-package unit tests (`bodojaryo`, `irb-sample`, 28-section
  plan file, `keep_all` byte-identical roundtrip) and hae-zip corpus tests.
- `cargo clippy -p hae-xml -p hae-package`: no warnings/errors.
- `cargo fmt --check -p hae-xml -p hae-package`: clean (after `cargo fmt`).

## Changed files
- `crates/hae-xml/src/lib.rs` — NCName tables + `parse_qname`, scope/declaration validation,
  expanded-duplicate detection, `NS_XMLNS`.
- `crates/hae-xml/src/tests.rs` — 4 new regression tests (`f1…f4`).
- `crates/hae-package/src/lib.rs` — segment-based href check + 2 new C2 tests (harness builds
  a minimal in-memory package: mimetype, container, content.hpf, section, `BinData/a..b.bin`).

## Unresolved findings
- None introduced by this task. Pre-existing parent-level decisions C1/C3/C4/C5 and notes
  C6–C8 remain open for the owning teams as recorded in `muse-core.md`.
