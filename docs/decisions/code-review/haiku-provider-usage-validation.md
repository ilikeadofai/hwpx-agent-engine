# Independent mechanical validation: provider usage helper

Date: 2026-10-17. Validator: Haiku test sub-agent (read-only). Owned file: this record only.
No source, test, doc, git, formatting, workspace-gate, or renderer change was made.

Safety boundary: no real provider request, no `~/.claude/.credentials.json` read, no codex app-server
launch, no LLM inference probe, no reset-credit operation. Only the mocked unit suite and pure
parser calls on synthetic dicts were run. Synthetic canary strings are labelled `SYNTH-CANARY-XYZ`.

Targets:
- `scripts/check_provider_usage.py`
- `scripts/tests/test_provider_usage.py`
- `docs/decisions/resumption-review/provider-usage-fixes.md` (the fixes record; it lives under
  `resumption-review/`, not `code-review/`)

## 1. Test run

Command: `python3 -m unittest discover -s scripts/tests -v` (run from the repo root)

- Result: **Ran 26 tests — OK, exit 0**. 26 `... ok` lines, 0 failures, 0 errors.
- Re-run with `python3 -I` (isolated mode): same result, 26 tests OK.
- Mocked seams only: `subprocess.Popen`/`select`/`os.read` for codex; `Path.home`/`urlopen` for Claude.

Contract points the suite covers (all passed):

| Requirement | Test(s) |
|---|---|
| available/exit 0 = metadata only | `test_exit_zero_means_metadata_read_not_capacity` |
| at_limit via explicit blockers | `test_explicit_blocker_flag_is_at_limit`, `test_ordinary_usage_disallowed_is_at_limit` |
| at_limit via usedPercent >= 100 | `test_full_windows_at_limit_but_still_available`, `test_five_hour_full_is_at_limit` |
| partial invalid window keeps valid peer, capacity unknown | `test_partial_window_keeps_valid_peer`, `test_numeric_string_does_not_drop_valid_peer` |
| documented nested `rateLimits` fallback | `test_documented_single_bucket_fallback_and_null_secondary` |
| synthetic flat fallback | `test_flat_fallback_uses_top_level_used_percent_as_primary` |
| null secondary / null Claude model windows allowed | `test_documented_single_bucket_fallback_and_null_secondary`, `test_optional_model_windows_can_be_null_without_schema_issue` |
| numeric strings, NaN, inf, bool, negatives rejected | `test_percent_rejects_...`, `test_invalid_percent_types_rejected_with_reasons`, `test_numeric_string_utilization_rejected_with_explicit_issue` |
| non-JSON banner skipped; oversized frame capped | `test_skips_non_json_banner_line`, `test_caps_oversized_framed_line_before_parsing` |
| read-only RPC allowlist (no model/reset methods) | `test_sends_only_readonly_rpc_methods` |
| canary absence in output | `test_secret_canary_absent_from_parse_output`, `test_live_shaped_payload_headroom_and_no_canary`, `test_rpc_error_reported_as_reason_not_raw`, `test_http_error_surfaces_status_not_body` |

Note: `provider-usage-fixes.md` line 110 still says "(24 tests)"; the current count is 26, which the
record's later "Parent integration corrections" section already reflects. This is stale prose, not a
test failure.

## 2. Concrete issues found by mock-only parser probes

Probe script kept outside the repo: `/tmp/haiku-probe-20261017/probe.py`, run with `python3 -I`.
All inputs are synthetic dicts passed to `parse_codex` / `parse_claude`.

### I-1 (P2) — An absent required window is reported as `headroom`

- Codex: `{"rateLimitsByLimitId": {"codex": {"secondary": {"usedPercent": 30, ...}}}}`
  → `available: true`, `capacityState: "headroom"`, **no issues**.
- Claude: `{"seven_day": {"utilization": 20, ...}}` (no `five_hour` key)
  → `available: true`, `capacityState: "headroom"`, **no issues**.

By contrast, an explicit `"primary": null` yields `primary-window-not-object` and `unknown`.
So an omitted key is treated more leniently than a null one. The contract says unreadable required
utilization must not establish `headroom`. The Claude case is the more serious one, since `five_hour`
is the short window and is silently absent. The current tests do not cover an absent key.

Suggested fix: when a window key is absent for codex `primary` or Claude `five_hour` / `seven_day`,
add an issue such as `five_hour-missing` and let `incomplete` yield `unknown`. Keep optional model
windows exempt.

### I-2 (P2) — Allowlist breach: unvalidated passthrough values reach the output

The docstring and the record say output is an explicit allowlist and raw payloads are never emitted.
Several fields are copied without type checks. Confirmed by probe:

| Field | Passthrough | Probe result |
|---|---|---|
| codex `rateLimitReachedType` → `limitReached` | raw value | `{"token": "SYNTH-CANARY-XYZ"}` emitted |
| codex `windowDurationMins` | raw value | `"SYNTH-CANARY-XYZ"` emitted |
| codex top-level `ordinaryUsageAllowed` | raw value | `{"t": "SYNTH-CANARY-XYZ"}` emitted |
| Claude/codex `resetsAt` non-numeric | raw value | `{"t": "SYNTH-CANARY-XYZ"}` emitted; `true` emitted as-is |

The existing canary tests only plant canaries in fields the parser never copies (`accessToken`,
`accountId`, `email`), so they do not catch this. A real provider is unlikely to send these shapes,
but the stated safety guarantee is not enforced by code.

Suggested fix: type-check each copied field (`resetsAt`: finite number → ISO, or a string matching an
ISO pattern, else `None` plus an issue; `windowDurationMins`: int or `None`; flags: bool or `None`).
Add a canary test planted in each passthrough field.

### I-3 (P3) — Non-finite or out-of-range reset timestamps abort the whole provider

`window()` calls `dt.datetime.fromtimestamp` without a guard. Probe results:

- codex `primary.resetsAt = NaN` (accepted by `json.loads`) → `ValueError`
- Claude `five_hour.resets_at = 1e20` → `OverflowError`

Both propagate out of `parse_*`. `safely()` then returns `{"available": false, "errorType": ...}`
for the **entire provider**, discarding valid peer windows. That contradicts the H3 claim that one
malformed window should not discard the provider. Real API payloads with these values are unlikely.

Suggested fix: wrap the conversion in `try/except (OverflowError, OSError, ValueError)` and treat the
result as an invalid `resetsAt` (window-level issue, value `None`).

### I-4 (P3, minor) — EOF reported as a timeout

`rpc()` breaks on an empty read (process exited) and then raises `TimeoutError("quota_rpc_timeout")`.
The operator sees a timeout when the cause was an early exit. The reason name is also not in the
record's reason list. This is cosmetic: no secret is exposed and the result is still metadata-unavailable.

## 3. What was not verified

- Live provider behaviour, actual `resetsAt` format, and real window presence. No live read was done
  by design; those were covered only by the parent's live read, as recorded in the fixes doc.
- The `60/80/90%` scheduling policy in `model-roles.md`. This validation checked only the helper's
  stated semantics, not the caller.
- Full-process behaviour of `codex()` against a real app-server (only the mocked seam was exercised).
- Whether the parent's prior "actual percent not available" dispatch note was correct. Not assessed here,
  and no misassignment is claimed.

## 4. Verdict

- Test outcome: **26/26 pass, exit 0**. Reproducible with the exact command.
- Claimed contract behaviours (available/exit 0 metadata-only, blocker and 100% at_limit, partial-peer
  preservation, nested and flat fallbacks, null optional windows, numeric-string/NaN/bool rejection,
  read-only RPC set, canary absence) are supported by the suite and the parser code.
- Gaps: **I-1** (absent-key headroom) and **I-2** (passthrough allowlist breach) contradict the stated
  contract and need code plus tests. I-3 and I-4 are robustness and cosmetic items.
- This record is metadata-only validation. It is not an approval for dispatch, and it does not show that
  any provider will accept a request.
