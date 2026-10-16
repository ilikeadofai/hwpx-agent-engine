# Independent review: `scripts/check_provider_usage.py` quota helper

Date: 2026-10-17. Reviewer owns this file only. No source, test, `model-roles.md`, git, or formatting changes were made. All provider interaction in this review was **mocked**; no real provider endpoint was contacted, no authentication file was read, and no real quota or account value appears in this report. Mock values (used percentages, timestamps, tokens, account IDs) are synthetic and clearly labelled.

## Scope and verdict

Read `scripts/check_provider_usage.py` (127 lines) and `docs/decisions/model-roles.md` (lines 20–28, 47–52). Narrow goals: correctness and security only — no LLM/inference call, no reset-credit consumption, no secret/token/account-ID output, bounded and terminating subprocess I/O, and quota-response parsing.

The helper meets the safety goals that were checked: it performs no model request, does not touch reset credits, emits no credential or account identifier in any tested path, and its subprocess I/O and termination are bounded and demonstrable. Two parsing defects and one contract-clarity defect are reproducible; the `available` flag is **not** a capacity signal, and one fallback path reports it with no quota data at all.

Severity: P1 = high; P2 = medium (correctness or misleading availability); P3 = low (robustness). No P1 was found. None of these findings enables credential disclosure, provider inference, or rate-limit consumption.

## Findings, in severity order

### H1 — P2: `available:true` and exit code `0` do not mean capacity remains

**Location:** `scripts/check_provider_usage.py:19` (`remainingPercent`), `:68` (`available: bool(buckets)`), `:79–94` (`claude`), `:123` (`return 0 if all(v["available"] ...)`).

`available` means "quota metadata was read", not "the provider can be used". A provider whose windows are fully consumed still returns `available: true` and makes `main()` exit `0`.

**Reproduction** (synthetic mock, `MOCK_MODE=full`): both codex windows at 100% used →

```json
{"available": true, "ordinaryUsageAllowed": true, "buckets": {"codex-SYNTHETIC": {
  "primary":   {"usedPercent": 100, "remainingPercent": 0, "resetsAt": "2026-09-21T14:13:20+00:00"},
  "secondary": {"usedPercent": 100, "remainingPercent": 0, "resetsAt": "2026-09-21T14:13:20+00:00"}}}}
```

`python3 scripts/check_provider_usage.py --provider codex` exits **0**. The same holds for the Claude path (a 100% `seven_day` bucket yields `available: true`), and `available` also ignores `rateLimitReachedType`/`ordinaryUsageAllowed`.

**Impact:** an orchestrator that gates scheduling on `available`, or on the process exit code, treats an exhausted provider as usable — the opposite of the intent in `model-roles.md:24–25` (80%/90% thresholds, hold new work at 100%). The parent's stated reading is correct: the caller **must** compute usability from `buckets[*].{primary,secondary}.usedPercent`/`resetsAt` and must not read `available`/exit code as capacity. This is not currently stated in the output or in `model-roles.md`.

**Fix (minimal):** document in `model-roles.md:28` that `available`/exit status mean "metadata read", and/or add one derived field (e.g. `anyWindowAtLimit`) computed from the buckets. No restructuring required.

### H2 — P2: the legacy flat `rateLimits` fallback returns `available:true` with no usage data

**Location:** `scripts/check_provider_usage.py:57`, `:59–67`.

```python
limits = value.get("rateLimitsByLimitId") or {"codex": value.get("rateLimits")}
```

When the response uses the flat shape (no `rateLimitsByLimitId`), the fallback wraps the whole flat object as bucket `"codex"`, but the loop only extracts `primary`/`secondary`. The flat `usedPercent`/`resetsAt`/`windowDurationMins` are never passed to `window()`, yet `buckets` is non-empty, so `available` is `true`.

**Reproduction** (`MOCK_MODE=legacy`): flat `{"usedPercent":55,"resetsAt":1790000000,"windowDurationMins":300}` →

```json
{"available": true, "ordinaryUsageAllowed": true,
 "buckets": {"codex": {"limitReached": null, "spendControlReached": null}}}
```

`main()` exits **0** with zero usage/reset fields. (An empty `rateLimitsByLimitId` with no `rateLimits` correctly gives `available: false` and `buckets: {}`.)

**Impact:** in the exact case this fallback exists to handle, the helper reports success while delivering no quota data — a silent false positive that directly undermines the meaning of `available` in H1.

**Fix (minimal):** in the fallback branch, run the flat object directly through `window(...)` when it carries `usedPercent`, instead of treating it as a bucket.

### H3 — P2: one malformed window discards the entire provider result

**Location:** `scripts/check_provider_usage.py:67` (`window(w["usedPercent"], ...)`), `:97–103`.

`w["usedPercent"]` is an unguarded subscript. Any window lacking a numeric `usedPercent` (schema drift, partial error payload) raises `KeyError`, which `safely()` converts to a whole-provider failure; all other windows and buckets from the same response are lost.

**Reproduction** (`MOCK_MODE=partial`, only `primary.usedPercent` removed) → `{"available": false, "errorType": "KeyError"}`.

**Impact:** a single drifted field turns a partially readable provider into "unavailable", which can push the orchestrator toward the wrong provider or toward treating both as down.

**Fix (minimal):** guard each window (`if isinstance(w.get("usedPercent"), (int, float))`) and skip only that window.

### H4 — P3: a single non-JSON stdout line aborts the whole read

**Location:** `scripts/check_provider_usage.py:37` (`json.loads(line)`), `:42–43` (cap), `:34` (deadline).

Unparsable stdout is not skipped. Any banner or non-JSONL diagnostic line makes the read fail entirely.

**Reproduction** (`MOCK_MODE=banner`, a `"codex app-server starting\n"` line before the first response) → `{"available": false, "errorType": "JSONDecodeError"}`.

**Impact:** low; the documented transport is JSONL, but a startup banner or stray diagnostic on stdout is enough to lose the reading.

**Fix (minimal):** wrap the parse in `try/except json.JSONDecodeError` and ignore non-JSON lines (the `id` match already discards unrelated frames).

### H5 — P3: non-numeric `utilization` silently drops a Claude window

**Location:** `scripts/check_provider_usage.py:89–93`.

If `utilization` is a numeric string (or absent), the window is skipped, so a provider with one such window returns `available:false, buckets:{}` with no error indicator — indistinguishable from "provider returned nothing".

**Reproduction** (synthetic Claude mock): `{"five_hour":{"utilization":"12.5"}}` → `{"available": false, "buckets": {}}`.

**Impact:** low; the real API's field type was not observed (see limits), so this is a robustness observation, not a confirmed live failure.

## Verified protections (tested)

Mock harness: a synthetic `codex` JSONRPC server (`fake_codex.py`, modes `ok/full/legacy/partial/empty/error/exit/flood/hang/banner`) invoked through the real `subprocess`/`select` path, and a Claude path exercised by replacing `urllib.request.urlopen` while pointing `HOME` at a synthetic credentials file. No real provider or real auth file was used. Scratch harness: `/tmp/opencode/pv-review/` (ephemeral).

- **No inference / no reset consumption:** code inspection shows the only RPCs sent are `initialize`, the `initialized` notification, and `account/rateLimits/read`. No `model`, no reset-credit, and no other mutating method is called.
- **No secret/token/account-ID output:** synthetic probes placed `accessToken`, `accountId`, and an email at the top level and inside buckets of the codex result, plus a synthetic OAuth token and account UUID in the Claude payload. None appeared in any output; codex output is an explicit allowlist (`usedPercent`, `resetsAt`, `windowDurationMins`, `rateLimitReachedType`, `spendControlReached`, `ordinaryUsageAllowed`). HTTP failure surfaces only the status code (`{"available":false,"httpStatus":401}`), not the body or header.
- **Bounded response I/O:** a 2 MiB unframed flood yields `RuntimeError` (`quota_response_limit`) quickly; the guard fires once the buffer exceeds 1 MiB, so it is bounded to roughly 1 MiB + one 64 KiB chunk.
- **Bounded time and termination:** an EOF-only server returns in ~0.0 s; a server that never replies returns `TimeoutError` at 20.0 s, and the child PID was confirmed gone afterward. `finally` (`:70–76`) does SIGTERM, waits 3 s, then SIGKILL, waits 3 s.
- **Design confirmation for the parent's question:** the bucket payload does carry `usedPercent`, `remainingPercent`, `resetsAt`, and `windowDurationMins`, which is sufficient for the orchestrator to apply the 80/90% and reset-time policy — provided it does not substitute `available`/exit code for that computation (H1).

## Limits of this review

- Every provider response was synthetic; no live schema, real quota value, or account data was observed. Whether the codex legacy flat path (H2) still occurs in production is **unverified**.
- `terminate()`/`kill()` target only the direct child. Descendant cleanup was not demonstrated and is not claimed.
- The unbounded `proc.stdin.write` (no timeout on a blocked pipe) was not exercised; only two small frames are sent, so the risk is low but unproven.
- Current validity of the Claude `seven_day_opus`/`seven_day_sonnet` keys and the `oauth-2025-04-20` beta header was not verified (no live call by design).
- `model-roles.md:28` names the tool but does not document `--provider`/`--output`, exit-code semantics, or the H1 distinction. Only this report was written; `model-roles.md` was not edited.

## Reproduction appendix

Scratch harness (ephemeral, synthetic only): `/tmp/opencode/pv-review/fake_codex.py` and `/tmp/opencode/pv-review/test_mocks.py`. `MOCK_MODE=full` reproduces H1 (run through `main --output` to see exit 0); `legacy` → H2; `partial` → H3; `banner` → H4. Only the `hang` mode is slow (~20 s); the rest return immediately. The Claude cases monkeypatch `urllib.request.urlopen` and point `HOME` at a synthetic credentials file — they never open `~/.claude/.credentials.json`.

Minimal self-contained codex repro (synthetic token/IDs; writes nothing outside `/tmp`): save as `fake_codex.py`, `chmod +x`, put a `codex` shim that `exec`s it on `PATH`, then run the snippet.

```python
import json, os, sys, time
MODE = os.environ.get("MOCK_MODE", "ok")
SECRET = {"accountId": "acct_SYNTHETIC_0001", "accessToken": "sk-SYNTHETIC-TOKEN"}
def emit(o): sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
def payload():
    if MODE == "legacy":
        return {"rateLimits": {"usedPercent": 55, "resetsAt": 1790000000}, **SECRET}
    b = {"rateLimitReachedType": None, "spendControlReached": False,
         "primary": {"usedPercent": 100 if MODE == "full" else 42, "resetsAt": 1790000000},
         "secondary": {"usedPercent": 100, "resetsAt": 1790000000}, **SECRET}
    if MODE == "partial": del b["primary"]["usedPercent"]
    return {"rateLimitsByLimitId": {"codex-SYNTHETIC": b}, "ordinaryUsageAllowed": True, **SECRET}
if MODE == "banner": sys.stdout.write("codex app-server starting\n"); sys.stdout.flush()
for raw in sys.stdin:
    m = json.loads(raw)
    if m.get("method") == "initialize": emit({"id": m["id"], "result": SECRET})
    elif m.get("method") == "account/rateLimits/read":
        if MODE == "hang": time.sleep(3600)
        else: emit({"id": m["id"], "result": payload()})
```

```python
import importlib.util, json, os, subprocess
spec = importlib.util.spec_from_file_location(
    "cpu", "scripts/check_provider_usage.py")
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
os.environ["PATH"] = "/tmp/opencode/pv-review/bin:" + os.environ["PATH"]
for mode in ("full", "legacy", "partial", "banner"):
    os.environ["MOCK_MODE"] = mode
    print(mode, json.dumps(m.safely(m.codex)))
```
