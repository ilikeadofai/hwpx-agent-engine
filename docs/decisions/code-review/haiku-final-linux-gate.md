# Final read-only mechanical Linux gate

Date: 2026-10-17. Validator: Haiku test sub-agent (read-only). Owned files: this record and the
evaluation outputs under `spikes/work/stage4-linux/2026-10-17-haiku-final/`.
No source, test, doc, git, formatting, or workspace change was made. No commit, no push.

Safety boundary: synthetic and public fixtures only. The renderer and raster worker ran locally;
network access was disabled inside their bubblewrap sandboxes. No
provider request, no `~/.claude/.credentials.json` read, no codex app-server launch, no reset
operation. `scripts/check_provider_usage.py` was not executed as a program; only its mocked tests
and pure parser calls on synthetic dicts ran.

Repository state at run time: `HEAD` = `09af4b9`. The working tree is dirty (29 modified tracked
files, 27 untracked entries, observed after the run), so every result below reflects the working
tree, not a clean commit.

## 1. Gate: `bash scripts/check.sh`

Log: `/tmp/hae-haiku-final-linux-gate.log`. Exit code: **0**.

| Stage | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (no diff output) |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass, no warnings |
| `cargo build -p hae-render-rhwp --bin hae-raster` | pass (`hae-raster` built) |
| `python3 -m unittest discover -s scripts/tests` | **Ran 30 tests — OK** |
| `cargo test --workspace` | **243 passed, 0 failed, 0 ignored** across 52 `test result:` suites |

The Rust total is counted from the log. It matches the expected 229 + 14 new tests exactly.
Run-level counts from the log: 52 suites, 243 passed, 0 failed, 0 ignored.

## 2. Oracle suite

Command: `python3 -m unittest discover -s tests/oracle`. Log: `/tmp/hae-haiku-final-oracle.log`.
Result: **Ran 88 tests — OK**, exit 0.

## 3. Determinism check

The tool named "fixturegen" in the brief is `tests/fixtures/editing/gen_editing.py`. No tool by
that name exists in the repo. `tests/fixtures/gen_adversarial.py` has no determinism flag.

Command: `python3 -I tests/fixtures/editing/gen_editing.py --check-determinism`
Result: **`determinism OK (27275 bytes)`**, exit 0. The check only generates in memory and writes
nothing. Log: `/tmp/hae-haiku-final-determinism.log`.

## 4. Fresh actual MCP evaluation

Command: `python3 scripts/evaluate_mcp.py --output-dir spikes/work/stage4-linux/2026-10-17-haiku-final`
The directory was absent before the run, so no existing evidence was overwritten.
Log: `/tmp/hae-haiku-final-mcp.log`. Summary: `…/2026-10-17-haiku-final/summary.json`.

- `status: passed`, `assertionsPassed: 86`, `assertionsFailed: 0`, exit 0, `failure: null`.
- `nativeComparison: not_checked` (the preview, after-edit preview, and summary all say so).
- Scenario: open, snapshot, preview before, query, dry run, original export byte-exact, apply
  (revision 0 to 1 exactly once), diff, preview after, undo, replacement corruption, redo,
  project save and reopen (ID and revision 3 stable), edited export, and reopening the exported
  HWPX as a new document.
- Engine `target/debug/hae-mcp`, sha256 `48fb1e84…c826d`.
- Renderer `rhwp v0.8.7` (commit `1a76570e`), sha256 `e91cd25f…05a7`. The recorded Stage 3 hash
  matches the observed hash.
- Raster worker `target/debug/hae-raster`: protocol `hae-raster-v1`, 51,521,288 bytes, sha256
  `3869a9a4…6fe9`, font snapshot hash `618f68d6…1d431` (1 file).
- Sandbox: bubblewrap 0.13.0, self-test passed. rhwp child `RLIMIT_AS` at most 503,309,894 bytes
  (this is not an address-space limit on the host engine process).
  Raster worker `RLIMIT_AS` at most 414,562,667 bytes via `/usr/bin/prlimit`, scoped to the worker
  process only.
- Font: `NanumGothic.ttf` copied into `inputs/fonts/` (4,691,820 bytes, sha256 `48a28e97…e733`).
  This is the ambient font from `/usr/local/share/fonts/n/`, not a bundled font.
- Synthetic document: 1 page, 10 nodes, 1,961 bytes, sha256 `b63ecac8…c599`. Its media is a
  67-byte synthetic PNG. The replacement images are 70 bytes and 21 bytes (after corruption).
- Preview before: `rendered`, PNG 23,871 bytes, SVG 1,795 bytes, `coverage: unknown`.
- Preview after: `rendered`, PNG 24,254 bytes, SVG 1,983 bytes, `coverage: unknown`.
- Applied export sha256 `48773c34…9c4c` (2,213 bytes). Redo export matches the applied bytes.
- Project file `artifacts/session.hwpwork`, sha256 `f856139c…d738`, 6,003 bytes.

Visual check: **UNVERIFIED**. The PNG and SVG files were written and measured, but no image was
viewed or described. Nothing here claims visual correctness.

## 5. Quota helper: I1–I4 probes (pure parser, mocked tests)

Mocked test run, verbose: `python3 -I -m unittest discover -s scripts/tests -p test_provider_usage.py -v`.
Result: **Ran 30 tests — OK**. Log: `/tmp/hae-haiku-final-quota-probes.log`.
The mock seams are `subprocess.Popen`, `select.select`, `os.read`, `Path.home`, and `urllib.request.urlopen`.
No real provider or network call is possible from these tests.

Pure-parser probe on synthetic dicts, `python3 -I` from `/tmp/haiku-final-probe/probe.py`.
The probe is outside the repo.

| ID | Case | Result | Covering test |
|---|---|---|---|
| I1 | Absent required window: codex `primary` missing; Claude `five_hour` missing | `capacityState: unknown`, issues `primary-missing` / `five_hour-missing`. Not `headroom`. Fixed. | `test_absent_required_window_retains_peer_but_is_unknown` |
| I2 | Bad typed passthrough: dict/canary in `ordinaryUsageAllowed`, `rateLimitReachedType`, `resetsAt`, `windowDurationMins` | Issues only, no canary in output, `available: true`, `unknown` | `test_passthrough_fields_cannot_emit_arbitrary_payloads` |
| I3 | `resets_at` = NaN, 1e20, `True`, dict-canary | Each → `five_hour-reset-invalid`; `unknown`; no canary. Valid `seven_day` peer retained by the test | `test_invalid_timestamps_cannot_discard_valid_peer_usage` |
| I4 | Early stdout EOF on codex app-server | Source contains `quota_rpc_eof`; mocked test asserts `errorType == EOFError`, not timeout | `test_early_stdout_eof_is_not_a_timeout` |

I4 was checked at the source-string level plus the mocked test. It was not run against a live
app-server.

## 6. Limits

- Scope is Linux, synthetic and public fixtures only, with local rendering and sandbox network isolation.
  No Hancom or native rendering claim. `nativeComparison` stays `not_checked`.
- Visual verification is UNVERIFIED (see section 4).
- Gate results reflect the dirty working tree, not commit `09af4b9` alone.
- The quota helper was checked by mocked tests and synthetic parser probes only. No real quota
  call was made, so live behavior is unverified.
- Font is an ambient system font copied into the run directory.
- Side effects of the gate run: `target/` build outputs, the `spikes/work/…/2026-10-17-haiku-final/`
  evaluation outputs (including a copied 4.6 MB font), and `__pycache__` directories from the
  Python test runs. The gate did not modify tracked source.

Parent clarification after reviewing the report and recorded MCP metadata: the renderer did run,
and `sandbox.memoryLimit` describes the rhwp child, not the host parent. The wording above was
corrected without changing source or rerunning the gate. Live quota reads by the parent are
recorded separately in `../stage234-linux-results.md`; this validator used mocked quota tests only.
