#!/usr/bin/env python3
"""Read provider quota metadata without inference, resets, or credential output.

Contract (see docs/decisions/resumption-review/provider-usage-fixes.md and the
"usage limit" paragraph of docs/decisions/model-roles.md):

* Only read-only operations are performed: the app-server RPCs ``initialize``,
  the ``initialized`` notification and ``account/rateLimits/read``, plus a
  read-only OAuth usage GET. No model request is issued and no reset credit is
  consumed.
* ``available`` and a zero exit status mean "quota metadata was read", NOT
  "the provider may be used". Scheduling must use the derived
  ``capacityState`` and the per-window ``usedPercent``/``resetsAt``.
* ``capacityState`` is ``headroom``, ``at_limit`` or ``unknown``. It is a
  metadata-derived signal only, not a guarantee that an inference request will
  be accepted.
* Output is an explicit allowlist: percentages, reset times, window minutes,
  blocker flags and reason names. Credentials, tokens, account identifiers and
  raw provider payloads are never emitted.

Exit status: ``0`` when every selected provider returned usable quota metadata
(at least one valid window); ``1`` otherwise. Exit ``0`` is metadata success,
not dispatch approval.
"""
import argparse
import concurrent.futures
import datetime as dt
import json
import math
import os
from pathlib import Path
import select
import subprocess
import time
import urllib.error
import urllib.request

MAX_LINE_BYTES = 64 * 1024
MAX_BUFFER_BYTES = 1024 * 1024
WINDOW_NAMES = ("primary", "secondary")
CLAUDE_WINDOWS = (("five_hour", 300), ("seven_day", 10080),
                  ("seven_day_opus", 10080), ("seven_day_sonnet", 10080))


def percent(value):
    """Return a validated finite non-negative percentage, or None.

    Bools, numeric strings, NaN/inf, negatives and non-numbers are rejected
    explicitly. Values above 100 are kept (they mean the window is exhausted).
    """
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    try:
        valid = math.isfinite(value) and value >= 0
    except OverflowError:
        valid = False
    if not valid:
        return None
    return value


def window(used, reset, minutes=None, issues=None, prefix="window"):
    issues = [] if issues is None else issues
    if reset is not None:
        try:
            if isinstance(reset, bool):
                raise ValueError("invalid timestamp")
            if isinstance(reset, (int, float)):
                parsed = dt.datetime.fromtimestamp(reset, dt.timezone.utc)
            elif isinstance(reset, str):
                parsed = dt.datetime.fromisoformat(reset.replace("Z", "+00:00"))
                if parsed.tzinfo is None:
                    raise ValueError("timezone required")
            else:
                raise ValueError("invalid timestamp")
            reset = parsed.astimezone(dt.timezone.utc).isoformat()
        except (ValueError, OverflowError, OSError):
            reset = None
            issues.append(prefix + "-reset-invalid")
    if minutes is not None and (type(minutes) is not int or minutes <= 0):
        minutes = None
        issues.append(prefix + "-minutes-invalid")
    return {"usedPercent": used, "remainingPercent": max(0, 100 - used),
            "resetsAt": reset, "windowDurationMins": minutes}


def boolean(value, issues, name):
    if value is None or type(value) is bool:
        return value
    issues.append(name + "-invalid")
    return None


def blocker_flags(limit, issues):
    reached = limit.get("rateLimitReachedType")
    # Preserve only known classifications, never arbitrary provider strings or objects.
    if reached is not None and not (type(reached) is bool or
            isinstance(reached, str) and reached in {
                "primary", "secondary", "credits", "individual", "spendControl",
                "usageLimit", "rateLimit", "spend_control", "usage_limit"}):
        issues.append("limitReached-invalid")
        reached = None
    return {"limitReached": reached,
            "spendControlReached": boolean(limit.get("spendControlReached"), issues, "spendControlReached")}


def _windows(buckets):
    """Collect every window mapping inside a provider's bucket structure."""
    found = []
    for value in buckets.values():
        if not isinstance(value, dict):
            continue
        if "usedPercent" in value:
            found.append(value)
        else:
            for name in WINDOW_NAMES:
                w = value.get(name)
                if isinstance(w, dict) and "usedPercent" in w:
                    found.append(w)
    return found


def capacity_state(buckets, blocked=False, incomplete=False):
    """Derive headroom/at_limit/unknown from valid windows and blocker flags."""
    if blocked:
        return "at_limit"
    windows = _windows(buckets)
    if not windows:
        return "unknown"
    if any(w["usedPercent"] >= 100 for w in windows):
        return "at_limit"
    if incomplete:
        return "unknown"
    return "headroom"


def _codex_windows(limit, issues):
    bucket = blocker_flags(limit, issues)
    seen = 0
    for name in WINDOW_NAMES:
        if name not in limit:
            if name == "primary":
                issues.append("primary-missing")
            continue
        w = limit.get(name)
        if w is None and name == "secondary":
            continue  # The documented protocol allows an absent secondary window.
        if not isinstance(w, dict):
            issues.append(name + "-window-not-object")
            continue
        used = percent(w.get("usedPercent"))
        if used is None:
            issues.append(name + "-usedPercent-invalid")
            continue
        bucket[name] = window(used, w.get("resetsAt"), w.get("windowDurationMins"), issues, name)
        seen += 1
    return bucket, seen


def parse_codex(value):
    """Parse an ``account/rateLimits/read`` result into an allowlisted summary."""
    issues = []
    buckets = {}
    limited = value.get("rateLimitsByLimitId")
    flat = value.get("rateLimits")
    if isinstance(limited, dict) and limited:
        items = [(key, limit, False) for key, limit in limited.items()]
    elif isinstance(flat, dict) and any(name in flat for name in WINDOW_NAMES):
        # Documented single-bucket fallback retains the nested window shape.
        items = [("codex", flat, False)]
    elif isinstance(flat, dict) and percent(flat.get("usedPercent")) is not None:
        items = [("codex", flat, True)]
    else:
        items = []
        if isinstance(flat, dict):
            issues.append("flat-rateLimits-without-valid-usedPercent")
    ordinary = boolean(value.get("ordinaryUsageAllowed"), issues, "ordinaryUsageAllowed")
    blocked = ordinary is False
    for key, limit, is_flat in items:
        if not isinstance(limit, dict):
            issues.append("bucket-not-object")
            continue
        if is_flat:
            used = percent(limit.get("usedPercent"))
            if used is None:
                issues.append("flat-rateLimits-without-valid-usedPercent")
                continue
            bucket = blocker_flags(limit, issues)
            bucket["primary"] = window(used, limit.get("resetsAt"),
                                       limit.get("windowDurationMins"), issues, "primary")
            seen = 1
        else:
            bucket, seen = _codex_windows(limit, issues)
        if bucket.get("limitReached") or bucket.get("spendControlReached"):
            blocked = True
        if not seen:
            issues.append("bucket-without-valid-windows")
            continue
        buckets[key] = bucket
    result = {"available": bool(_windows(buckets)),
              "capacityState": capacity_state(buckets, blocked, incomplete=bool(issues)),
              "ordinaryUsageAllowed": ordinary,
              "buckets": buckets}
    if issues:
        result["issues"] = sorted(set(issues))
    return result


def parse_claude(value):
    """Parse an OAuth usage response into an allowlisted summary."""
    issues = []
    buckets = {}
    for name, minutes in CLAUDE_WINDOWS:
        if name not in value:
            if name in ("five_hour", "seven_day"):
                issues.append(name + "-missing")
            continue
        w = value.get(name)
        if w is None and name in ("seven_day_opus", "seven_day_sonnet"):
            continue  # Optional model-specific windows are normally null.
        if not isinstance(w, dict):
            issues.append(name + "-window-not-object")
            continue
        used = percent(w.get("utilization"))
        if used is None:
            issues.append(name + "-utilization-invalid")
            continue
        buckets[name] = window(used, w.get("resets_at"), minutes, issues, name)
    result = {"available": bool(_windows(buckets)),
              "capacityState": capacity_state(buckets, incomplete=bool(issues)),
              "buckets": buckets}
    if issues:
        result["issues"] = sorted(set(issues))
    return result


def codex():
    proc = subprocess.Popen(["codex", "app-server", "--listen", "stdio://"],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, bufsize=0)
    buffer = b""

    def rpc(rid, method, params):
        nonlocal buffer
        proc.stdin.write(json.dumps({"id": rid, "method": method, "params": params}).encode()+b"\n")
        proc.stdin.flush()
        deadline = time.monotonic()+20
        while time.monotonic() < deadline:
            while b"\n" in buffer:
                line, buffer = buffer.split(b"\n", 1)
                # Cap the frame before parsing; skip it without retaining text.
                if len(line) > MAX_LINE_BYTES:
                    continue
                try:
                    value = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if not isinstance(value, dict) or value.get("id") != rid:
                    continue
                if "error" in value:
                    raise RuntimeError("quota_rpc_failed")
                return value.get("result", {})
            if len(buffer) > MAX_BUFFER_BYTES:
                raise RuntimeError("quota_response_limit")
            if select.select([proc.stdout], [], [], max(0, deadline-time.monotonic()))[0]:
                chunk = os.read(proc.stdout.fileno(), 65536)
                if not chunk:
                    raise EOFError("quota_rpc_eof")
                buffer += chunk
        raise TimeoutError("quota_rpc_timeout")

    try:
        rpc(1, "initialize", {"clientInfo": {"name": "hwpx-quota-check", "version": "0.1"},
                              "capabilities": {}})
        proc.stdin.write(b'{"method":"initialized"}\n')
        proc.stdin.flush()
        value = rpc(2, "account/rateLimits/read", {})
        if not isinstance(value, dict):
            return {"available": False, "capacityState": "unknown",
                    "buckets": {}, "issues": ["result-not-object"]}
        return parse_codex(value)
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=3)


def claude():
    # Read locally and send only to the provider; never serialize auth material.
    auth = json.loads((Path.home()/".claude"/".credentials.json").read_text())
    token = auth["claudeAiOauth"]["accessToken"]
    req = urllib.request.Request("https://api.anthropic.com/api/oauth/usage", headers={
        "Authorization": "Bearer "+token, "anthropic-beta": "oauth-2025-04-20",
        "User-Agent": "claude-code/usage-check"})
    with urllib.request.urlopen(req, timeout=15) as response:
        value = json.load(response)
    if not isinstance(value, dict):
        return {"available": False, "capacityState": "unknown",
                "buckets": {}, "issues": ["result-not-object"]}
    return parse_claude(value)


def safely(fn):
    try:
        return fn()
    except urllib.error.HTTPError as exc:
        return {"available": False, "capacityState": "unknown", "httpStatus": exc.code}
    except Exception as exc:
        return {"available": False, "capacityState": "unknown",
                "errorType": type(exc).__name__}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--provider", choices=("both", "codex", "claude"), default="both")
    parser.add_argument("--output", type=Path, help="Optional metadata-only JSON output")
    args = parser.parse_args()
    selected = {"codex": codex, "claude": claude}
    if args.provider != "both":
        selected = {args.provider: selected[args.provider]}
    with concurrent.futures.ThreadPoolExecutor(max_workers=len(selected)) as pool:
        pending = {name: pool.submit(safely, fn) for name, fn in selected.items()}
        result = {"checkedAt": dt.datetime.now(dt.timezone.utc).isoformat(),
                  "providers": {name: job.result() for name, job in pending.items()}}
    encoded = json.dumps(result, ensure_ascii=False, indent=2)+"\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded)
    print(encoded, end="")
    # Exit 0 means metadata was read, not that dispatch is approved.
    return 0 if all(v["available"] for v in result["providers"].values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
