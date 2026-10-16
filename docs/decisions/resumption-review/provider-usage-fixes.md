# Provider usage helper fixes: H1 / H2 / H3 / H4 / H5

Date: 2026-10-17. Scope: `scripts/check_provider_usage.py`, its new unit tests,
this record, and the helper-semantics paragraph of `docs/decisions/model-roles.md`.
No other source, test, document, git, or formatting command was run against the
living tree. All unit tests are mocked: no real `codex` process, network call, or
real `~/.claude/.credentials.json` read occurs, and synthetic credential canaries
are asserted absent from every output. The live query itself is deferred to the
parent (see handoff).

## What changed, per finding

### H1 — `available`/exit 0 must not read as capacity

`available` is retained for compatibility but is now explicitly defined as
"usable quota metadata was read". A derived `capacityState` is emitted per
provider:

- `headroom` — at least one valid window, no window at or above 100%, and no
  explicit blocker flag.
- `at_limit` — an explicit blocker flag (`rateLimitReachedType` truthy,
  `spendControlReached` truthy, or `ordinaryUsageAllowed` explicitly `false`),
  or any valid window with `usedPercent >= 100`.
- `unknown` — no valid window could be parsed (empty, incomplete, or malformed
  response), or a transport/RPC/HTTP error.

`capacityState` is a metadata-derived signal only. It is **not** a guarantee that
an inference request will be accepted, and `available`/exit `0` remain metadata
success, not dispatch approval. The semantics are documented in the module
docstring and in `model-roles.md`.

### H2 — flat `rateLimits` fallback reported success with no usage data

When the response has no non-empty `rateLimitsByLimitId`, the flat `rateLimits`
object is now used only if it carries a valid top-level `usedPercent`, and that
value is mapped to a `primary` window (projected through `window(...)`). A flat
object without a valid `usedPercent` yields `available:false`,
`capacityState:"unknown"`, and the reason `flat-rateLimits-without-valid-usedPercent`.
Empty buckets are never emitted, so a "success bucket without valid windows" is
no longer possible: `available` is computed from actual windows.

### H3 — one malformed window discarded the whole provider

Each window is validated independently. A window missing or carrying an invalid
`usedPercent` is skipped with a reason name only (`primary-usedPercent-invalid`,
`secondary-window-not-object`, `bucket-without-valid-windows`,
`bucket-not-object`), while valid peer windows and other buckets are retained.
No raw payload value is copied into an issue name.

`percent(value)` accepts finite, non-negative numbers (including values above
100, which mean exhausted) and rejects booleans, numeric strings, `NaN`, `inf`,
negatives, and non-numbers.

### H4 — one non-JSON stdout line aborted the read

The RPC reader now skips any stdout line that is not decodable JSON (including a
startup banner) and continues within the original 20 s deadline. Each framed line
is also capped at `MAX_LINE_BYTES` (64 KiB) **before** parsing, so an oversized
frame is skipped without being parsed. The aggregate unframed buffer cap is
unchanged at `MAX_BUFFER_BYTES` (1 MiB) and still raises `quota_response_limit`.
Skipped line text is never retained, copied into an error, or emitted.

### H5 — non-numeric `utilization` was silently dropped

A Claude window whose `utilization` is not a validated number (for example a
numeric string such as `"12.5"`) is now rejected with the explicit reason
`<window>-utilization-invalid` instead of being silently dropped. Values are
**not** coerced; valid peer windows are kept and reported. No live-response type
assumption is made.

## Recorded semantics

| Item | Meaning |
|---|---|
| `available` (per provider) | usable quota metadata was read (>=1 valid window) |
| exit status `0` | every selected provider had `available:true` (metadata read only) |
| exit status `1` | at least one selected provider lacked usable metadata |
| `capacityState` | `headroom` / `at_limit` / `unknown` (see above) |
| window `usedPercent` | validated finite non-negative percent; `>100` treated as at limit |
| window `remainingPercent` | `max(0, 100 - usedPercent)` |
| window `resetsAt` | ISO-8601 UTC when numeric; timezone-bearing ISO strings are parsed and canonicalized; malformed values become null with an issue |
| `issues[]` | reason names only; present only when non-empty |
| error result | `{"available":false,"capacityState":"unknown",...}` with `httpStatus`/`errorType` only |

Retained safety contract: the only operations are the app-server RPCs
`initialize`, the `initialized` notification, and `account/rateLimits/read`, plus
one read-only OAuth usage GET. No model request, no reset-credit consumption, and
no credential, token, or account identifier is emitted. Provider output remains
an explicit allowlist.

Representative outputs (synthetic, from the parser):

```json
{"available": true, "capacityState": "headroom", "ordinaryUsageAllowed": null,
 "buckets": {"codex": {"limitReached": null, "spendControlReached": null,
   "primary": {"usedPercent": 55, "remainingPercent": 45,
               "resetsAt": "2026-09-21T14:13:20+00:00", "windowDurationMins": 300}}}}
```
```json
{"available": true, "capacityState": "headroom", "ordinaryUsageAllowed": null,
 "buckets": {"codex": {"limitReached": null, "spendControlReached": null,
   "secondary": {"usedPercent": 30, "remainingPercent": 70,
                 "resetsAt": "2026-09-21T14:13:20+00:00", "windowDurationMins": null}}},
 "issues": ["primary-usedPercent-invalid"]}
```

## Tests

New deterministic `unittest` suite: `scripts/tests/test_provider_usage.py`
(24 tests). It mocks the codex subprocess/select/`os.read` seam and the Claude
`Path.home`/`urlopen` seam; it never runs a real process, opens real credentials,
or contacts the network, and contains no 20 s delay. Covered modes: current
multi-bucket, flat fallback, full (100%), partial, empty, non-JSON banner,
oversized framed line, RPC error, non-numeric Claude `utilization`, HTTP error,
secret-canary absence, read-only RPC allowlist, and `main()` exit codes.

Run:

```
python3 -m unittest discover -s scripts/tests
python3 scripts/tests/test_provider_usage.py
```

TDD evidence: before the source change the suite ran RED — 24 tests, 23 errors
(missing `percent`/`parse_codex`/`parse_claude`/`MAX_LINE_BYTES`, plus the live
bugs), 1 pass. After the change:

```
Ran 24 tests in 0.009s

OK
```

`python3 -m py_compile scripts/check_provider_usage.py scripts/tests/test_provider_usage.py`
exits 0. A direct parser probe reproduced the corrected H2/H3 outcomes: legacy
flat → `headroom` with `primary.usedPercent: 55`; flat-without-usage → `unknown`
with the reason; partial → `headroom` with the valid `secondary` window kept and
`primary-usedPercent-invalid` recorded; empty → `unknown`; Claude `"12.5"` →
`unknown` with `five_hour-utilization-invalid`. No synthetic canary leaked.

## Limits / not claimed

- No live provider response was contacted in this fix round; the parent runs the
  live query. The flat-schema path remains supported but its live frequency is
  still unverified.
- The helper does not and cannot guarantee that an accepted dispatch will not
  hit a limit; `capacityState` is metadata-derived only.
- This fix does not add CLI features, an OS daemon, or background monitoring.

## Parent handoff

The parent will run the suite and the live query. Expected live-shaped output
adds one field per provider (`capacityState`) and only adds `issues` on partial
or malformed data; the existing `available`, `buckets`, `usedPercent`,
`remainingPercent`, `resetsAt`, and `windowDurationMins` fields are unchanged.
A provider captured at 100% now reports `capacityState:"at_limit"` while still
reporting `available:true` and exit `0`; callers must gate on `capacityState`
plus the per-window percentages.


## Parent integration corrections (2026-10-17)

The parent reran all 24 tests, then checked the documented single-bucket `rateLimits` shape and a live read. Three details required correction: the documented fallback retains `primary`/`secondary` (it is not the synthetic flat percentage shape); absent/null secondary windows and null optional Claude model windows are valid; and partially malformed required data must not establish `headroom` even when valid peer metadata is retained. Numeric flat fallback remains an additional compatibility case, with its live frequency unverified.

Added two regressions and strengthened three partial-response expectations. RED: 26 tests, 5 failures. GREEN after correction: 26 tests, no failures. Valid peer windows remain available, but issues now yield `capacityState: unknown` unless a known exhausted window or explicit blocker establishes `at_limit`. Null optional windows are skipped without issues. The parent live read succeeds and emits no optional-window warnings. `available` and exit 0 remain metadata-only; scheduling still checks all applicable windows and the project 80/90% policy.


## Independent Haiku follow-up: I-1 through I-4 (2026-10-17)

The 26-test review found absent required keys were still treated more leniently than null keys; allowed metadata fields could carry arbitrary unvalidated values; malformed reset timestamps could throw away peer usage; and early EOF was labelled timeout. The parent added four adversarial tests, observed RED (30 tests: 3 failures, 1 error), and corrected these paths. GREEN: all 30 tests pass.

Codex primary and Claude five-hour/seven-day absence now create reason-only issues. Valid peer windows remain, but capacity is unknown unless an observed exhausted window or explicit blocker establishes at-limit. Optional secondary/model windows may be null. Reset times accept only finite in-range timestamps or timezone-bearing ISO strings and are canonicalized to UTC; invalid reset values become null plus a reason. Minutes are positive integers or null, booleans are checked, and limit classifications are a fixed vocabulary (unknown values are withheld with an issue). Arbitrary strings/objects in these fields are never echoed. Invalid timestamps preserve numeric usage and other windows. Early stdout EOF is an EOFError, distinct from deadline expiry. Older 24/26 counts above describe their original rounds; current suite count is 30.
