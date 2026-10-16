# HWPX core correctness review — hae-zip / hae-xml / hae-package (muse-core)

- Scope: committed core at `447d7c4` (`wip: preserve stage-1 document model…`), reviewed against
  `docs/superpowers/specs/2026-10-16-hwpx-engine-0.1-design.md` §4/§11 and plan tasks 1-3, 1-4, 1-6.
- Read-only review: no source or git state touched. Working tree had unrelated uncommitted edits
  (`Cargo.lock`, `crates/hae-edit/*`, `crates/hae-model/*`, `tests/oracle/`, …) from concurrent agents;
  `hae-zip`, `hae-xml`, `hae-package` on disk match HEAD (not in `git diff --name-only`).
- Tests run (all pass): `cargo test -p hae-zip -p hae-xml -p hae-package`
  (zip 14, xml 8, package unit, `adversarial` 80+ cases, `corpus` roundtrip), plus standalone
  reproductions under `/tmp/opencode/repro` (path deps to HEAD crates) and
  `/tmp/opencode/mk_mid2.py` → `/tmp/opencode/mimetype_desc.hwpx`.
- Convention below: **F = reproduced failure** (exact repro + expected vs current),
  **C = concern / coverage gap** (observed behavior or code reading, not claimed as failure).

## Reproduced failures

### F1 — XML accepts duplicate expanded attribute names (well-formedness violation)
- Location: `crates/hae-xml/src/lib.rs:389-394` (`seen` set keyed on qname string only).
- Repro (in `/tmp/opencode/repro`): `XmlDoc::parse(br#"<x xmlns:a="urn:U" xmlns:b="urn:U" a:k="1" b:k="2"/>"#.to_vec())`
  → `ACCEPT ["x|None"]`.
- Expected: reject `PACKAGE_REJECTED/malformed_xml` (XML 1.0: no two attributes with the same expanded
  name; same-URI `a:k`/`b:k` collide). Same-qname duplicates (`<x a="1" a="2"/>`) are correctly rejected.
- Severity: **Medium-High**. Divergent parse vs any conformant parser (Python ET, Hancom, rhwp);
  attribute value picked by later code is implementation-defined.

### F2 — XML accepts colons inside the local part of names
- Location: `crates/hae-xml/src/lib.rs:291-296` (`split_qname` splits on first `:` only, no check that
  `local` is colon-free); used at `:395-406` (attrs) and `:416-426` (elements).
- Repro: `XmlDoc::parse(b"<x xmlns:a=\"urn:U\" a:b:c=\"1\"/>")` → `ACCEPT`;
  `<a:b:c xmlns:a="urn:U"/>` → `ACCEPT` with ns `Some("urn:U")` (local recorded as `"b:c"`).
- Expected: reject `malformed_xml` (colon is the namespace separator; `NCName` cannot contain `:`).
- Severity: **Medium**. Same class as F1/F3: non-conformant input accepted into the preserved-byte pipeline.

### F3 — XML accepts invalid Names (digit-leading element/attribute names)
- Location: element name `crates/hae-xml/src/lib.rs:371-373` (UTF-8 check only);
  attribute name `crates/hae-xml/src/lib.rs:251-252` (`scan_attrs`, UTF-8 check only). No
  `NameStartChar`/`NameChar` validation anywhere; quick-xml does not enforce it either.
- Repro: `XmlDoc::parse(b"<1bad/>")` → `ACCEPT ["1bad|None"]`;
  `XmlDoc::parse(b"<x 1a=\"2\"/>")` → `ACCEPT`.
- Expected: reject `malformed_xml` (Python `ET.fromstring` rejects both).
- Severity: **Medium**. Malformed acceptance; downstream (rhwp/Hancom) will disagree on these files.

### F4 — Reserved `xml` prefix can be rebound to another namespace
- Location: `crates/hae-xml/src/lib.rs:322-324` (seeds `xml` → `NS_XML`) then `:379-388` unconditionally
  `insert`s `xmlns:xml="…"` over it; empty-URI rejection exists but wrong-URI does not.
- Repro: `XmlDoc::parse(b"<x xmlns:xml=\"urn:other\"/>")` → `ACCEPT`;
  `<x xmlns:xml="urn:other" xml:lang="en"/>` → `ACCEPT` with `xml:lang` resolved against `urn:other`.
- Expected: reject (Namespaces in XML: `xml` prefix must stay bound to
  `http://www.w3.org/XML/1998/namespace`). Rebinding to the correct URI is fine and stays accepted.
- Severity: **Low-Medium**. Real HWPX files never do this, but a hostile file can silently change
  `xml:lang`/`xml:space` semantics.

## Concerns and coverage gaps (not failures)

### C1 — ZIP inter-entry gaps and prepended prefix are accepted (overlap-only check)
- Location: `crates/hae-zip/src/lib.rs:470-480` (sorted-span overlap test only); no contiguity /
  first-offset-zero / gap check.
- Repro: `valid_base.hwpx` + 16 `0xAA` bytes spliced before CD (EOCD `cd_off` +16) → `ACCEPT`;
  16 `0xBB` bytes prepended with all local offsets and `cd_off` +16 → `ACCEPT`.
- Note: the `eocd_comment_spoof` fixture is still rejected, but via the `find_eocd` ambiguous-EOCD path
  (`lib.rs:139-163`), not via gap detection, although the fixture note proposes gap detection
  ("마지막 로컬 항목 끝과 cd_offset 사이에 설명 안 된 틈"). Pure gaps carry unreferenced bytes.
  Plan 1-3 requires only 비중첩, and spec §11 does not list gaps, so this is a **concern (Low-Medium)**,
  not a contract violation: tighten only if the project wants strict "no unexplained bytes" posture.

### C2 — Manifest `href` `".."` check is substring-based; false-positive on legal names
- Location: `crates/hae-package/src/lib.rs:301` (`href.contains("..")`) vs container rootfile check at
  `:271-280` (segment-based) and ZIP `check_name` (`hae-zip/src/lib.rs:102-115`, segment-based, allows
  segment `a..b`).
- Repro: entry `BinData/a..b.bin` is ZIP-legal (`OutEntry::Add` write OK), but adding manifest item
  `href="BinData/a..b.bin"` → `REJECT PACKAGE_REJECTED/path_escape` (`manifest href BinData/a..b.bin`).
- Severity: **Low** (nobody names files `a..b`, and fail-closed is safe), but the two layers disagree;
  suggest segment-based check to match `check_name`.

### C3 — `mimetype` with data-descriptor bit is accepted
- Location: `crates/hae-package/src/lib.rs:204-210` (`mimetype_stored` = method-only check) +
  `hae-zip` descriptor acceptance (`hae-zip/src/lib.rs:411-437`).
- Repro: `/tmp/opencode/mimetype_desc.hwpx` (built by `/tmp/opencode/mk_mid2.py`: STORED mimetype,
  flag bit 3, zeroed local sizes, appended descriptor, shifted offsets) → `ACCEPT` through
  `hae_package::open`.
- Spec §11 says only "첫 번째 무압축(STORED) 항목"; descriptor-bit STORED arguably passes that, and ODF
  magic-sniffing is the party that would object. **Concern (Low)** / spec ambiguity — either forbid the
  descriptor bit on `mimetype` explicitly or record acceptance as policy.

### C4 — Dangling-reference checks cover `binaryItemIDRef` only
- Location: `crates/hae-package/src/lib.rs:376-389`.
- Repro: `valid_base.hwpx` section with `paraPrIDRef="0"` → `"9999"` → `hae_package::analyze` → `ACCEPT`.
  (Manifest-missing, spine-ghost-idref, and `binaryItemIDRef="image9"` cases are all correctly rejected.)
- Spec §11 open checks list manifest/spine/META-INF relations; style IDREF validation may belong to
  `hae-model` edit gating rather than open. Record as **coverage gap (Medium for future edit safety)**:
  decide which layer owns non-media IDREF integrity before 1-7/1-10 land.

### C5 — `payload_set_sha256` canonical form is flat, spec text is ambiguous
- Location: `crates/hae-package/src/lib.rs:95-114` produces `["n1","h1","n2","h2"]` (flat), while spec §11
  ("(항목명, 해제한 payload의 SHA-256) 목록을 정규 JSON") also reads as `​[["n1","h1"],…]`.
  Deterministic and self-consistent (project manifest round-trips against itself), so no live bug —
  but any second implementation (oracle, other language) must copy the flat form byte-for-byte.
  **Concern**: pin the exact form with a golden vector in `diagnostic-codes` or the 1-15 tests.

### C6 — No XML element/attribute-count cap (depth-only)
- `ParseOptions` (`hae-xml/src/lib.rs:122-131`) caps depth at 128 (boundary verified: 128 accept / 129
  `RESOURCE_LIMIT`), but a wide-shallow document is bounded only by the ZIP cumulative inflate budget.
  Per plan 1-4 this is compliant; noted for the 512 MiB session-memory validation (task 1-16), which must
  measure a many-element file, not just deep/nested ones.

### C7 — Inflate-budget errors report `Stage::Validate` during open
- Location: `crates/hae-diag/src/lib.rs:276-291` (`Budget::try_reserve` hardcodes `Validate`).
  `deflate_bomb_many/single` correctly return `RESOURCE_LIMIT`, but with stage `validate` while every
  other open failure reports `open`. Diagnostic-shape nit (**Low**).

### C8 — ZIP64 locator check is position-exact
- Location: `crates/hae-zip/src/lib.rs:239-244` (looks only at `eocd-20`). Bytes wedged between a locator
  and EOCD would evade the sentinel test (subsequent parse would likely still fail on `cd_end != eocd`).
  **Concern (Low)**; current ZIP64 fixtures are correctly rejected.

## Verified working (no action)
- Namespace rebinding semantics: default-ns rebinding + restore (`default_namespace_rebinding`), foreign
  `hp` prefix treated as non-HWPX (`same_prefix_other_namespace`), unbound prefix rejected, `xmlns=""`
  un-declaration OK, `xmlns:p=""` rejected.
- Malformed XML: tag-order mismatch, unclosed, multi-root, bad declaration placement, raw `<` in attrs,
  `]]>` in text, unquoted/missing attr values, bad/undeclared entities, `&#0;`/`&#1;`/`&#X41;` rejected;
  `&lt;`-entity attrs, PIs, comments, CDATA accepted. Depth 128/129 boundary exact.
- ZIP: truncated inputs, EOCD spoof/benign-comment distinction, shared-header and nested-data overlaps,
  all four local/central mismatch kinds, CRC mismatch (matching headers, bad payload), declared-size
  smaller **and larger** than actual inflate (larger verified in repro: `size_mismatch`, inflated 343 vs
  declared 443), descriptor with/without signature accepted + descriptor-CRC mismatch rejected,
  encrypted flag / method-99 / `encryption.xml`, bzip2 / deflate64 (even unreferenced), all three
  multi-disk forms, both ZIP64 forms, duplicate / case-collision / 9 path-escape shapes / 3 non-UTF-8
  name shapes rejected, directory entries accepted, entry/inflate/total-size limits enforced,
  `keep_all` byte-identical (unit + all corpus files), 10k deterministic mutations panic-free.
- Package: mimetype value/position/compression/trailing-newline/missing rejected; container-missing,
  rootfile-missing, rootfile-`..` rejected; spine-missing-section, ghost idref, duplicate manifest id,
  dangling `binaryItemIDRef` rejected; unused media accepted; spine order (not filename order) used for
  section numbering; `zip_plan` keeps byte-identical payloads (pointer- or content-equal).

## Suggested next steps for owning teams (no code changed here)
1. `hae-xml`: validate `NCName` (at least: no second colon, valid start char) for element/attr names;
   compare expanded `(ns, local)` for duplicate attributes; forbid rebinding `xml` (and preferably
   `xmlns`) prefixes. Add the four F1–F4 inputs as regression tests.
2. Decide gap posture (C1) and record it in §11 or the 1-3 completion note; either is defensible, but the
   `eocd_comment_spoof` note currently promises more than the code implements.
3. `hae-package`: switch manifest href check to segment-based (C2); state mimetype-descriptor policy (C3);
   assign non-media IDREF ownership (C4); pin `payload_set_sha256` golden vector (C5).
