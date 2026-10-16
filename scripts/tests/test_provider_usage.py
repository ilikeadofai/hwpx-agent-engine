"""Deterministic unit tests for scripts/check_provider_usage.py.

Safety: every provider interaction here is mocked. No test opens the real
``~/.claude/.credentials.json``, contacts a network endpoint, or runs a real
``codex`` process. Synthetic credential canaries are asserted absent from all
output. No test waits on the production 20 s deadline.

Run:
    python3 -m unittest discover -s scripts/tests
    python3 scripts/tests/test_provider_usage.py
"""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
import urllib.error
from unittest import mock


MODULE_PATH = Path(__file__).resolve().parents[1] / "check_provider_usage.py"
_spec = importlib.util.spec_from_file_location("check_provider_usage", MODULE_PATH)
cpu = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cpu)

CANARY_TOKEN = "sk-CANARY-TOKEN-0000"
CANARY_ACCOUNT = "acct_CANARY_0001"
CANARY_EMAIL = "canary@SYNTHETIC.invalid"
CANARIES = (CANARY_TOKEN, CANARY_ACCOUNT, CANARY_EMAIL)


def no_canary(testcase, value):
    serialized = json.dumps(value, ensure_ascii=False)
    for canary in CANARIES:
        testcase.assertNotIn(canary, serialized, "credential canary leaked")


class _FakeStream(io.BytesIO):
    def fileno(self):
        return -1


def codex_frames(result):
    """Raw stdout a well-behaved app-server would emit for the two RPCs."""
    return (json.dumps({"id": 1, "result": {}}) + "\n"
            + json.dumps({"id": 2, "result": result}) + "\n").encode()


@contextlib.contextmanager
def fake_codex(output):
    """Patch the subprocess/select/os.read seam; never spawn a real process."""
    stream = _FakeStream(output)
    proc = types.SimpleNamespace(
        stdin=io.BytesIO(),
        stdout=stream,
        terminate=lambda: None,
        wait=lambda timeout=None: 0,
        kill=lambda: None,
    )

    def fake_read(fd, count):
        return stream.read(count)

    def fake_select(rlist, wlist, xlist, timeout=None):
        return (list(rlist), [], [])

    with mock.patch.object(cpu.subprocess, "Popen", return_value=proc), \
         mock.patch.object(cpu.os, "read", side_effect=fake_read), \
         mock.patch.object(cpu.select, "select", side_effect=fake_select):
        yield proc


class _FakeHTTPResponse(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False


@contextlib.contextmanager
def claude_seam(payload=None, error=None):
    """Patch the credentials path and urlopen; never read real auth or network."""
    with tempfile.TemporaryDirectory() as tmp:
        home = Path(tmp)
        (home / ".claude").mkdir()
        (home / ".claude" / ".credentials.json").write_text(json.dumps(
            {"claudeAiOauth": {"accessToken": CANARY_TOKEN}}))
        response = _FakeHTTPResponse(json.dumps(payload or {}).encode())
        opener = (mock.Mock(side_effect=error) if error is not None
                  else mock.Mock(return_value=response))
        with mock.patch.object(cpu.Path, "home", return_value=home), \
             mock.patch.object(cpu.urllib.request, "urlopen", opener):
            yield


class PercentValidationTests(unittest.TestCase):
    def test_percent_accepts_valid_numbers_including_above_100(self):
        self.assertEqual(cpu.percent(0), 0)
        self.assertEqual(cpu.percent(42.5), 42.5)
        self.assertEqual(cpu.percent(150), 150)

    def test_percent_rejects_non_numeric_and_non_finite_and_boolean(self):
        self.assertIsNone(cpu.percent("50"))
        self.assertIsNone(cpu.percent(True))
        self.assertIsNone(cpu.percent(-1))
        self.assertIsNone(cpu.percent(float("nan")))
        self.assertIsNone(cpu.percent(float("inf")))
        self.assertIsNone(cpu.percent(None))


class CodexParseTests(unittest.TestCase):
    def parse(self, result):
        return cpu.parse_codex(result)

    def test_current_multi_shape_headroom(self):
        out = self.parse({
            "rateLimitsByLimitId": {"codex": {
                "rateLimitReachedType": None, "spendControlReached": False,
                "primary": {"usedPercent": 45, "resetsAt": 1790000000,
                            "windowDurationMins": 300},
                "secondary": {"usedPercent": 36, "resetsAt": 1790000000,
                              "windowDurationMins": 10080}}},
            "ordinaryUsageAllowed": True})
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "headroom")
        self.assertEqual(out["buckets"]["codex"]["primary"]["usedPercent"], 45)
        self.assertEqual(out["buckets"]["codex"]["primary"]["remainingPercent"], 55)
        self.assertNotIn("issues", out)

    def test_full_windows_at_limit_but_still_available(self):
        out = self.parse({"rateLimitsByLimitId": {"codex": {
            "primary": {"usedPercent": 100, "resetsAt": 1790000000},
            "secondary": {"usedPercent": 100, "resetsAt": 1790000000}}},
            "ordinaryUsageAllowed": True})
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "at_limit")
        self.assertEqual(out["buckets"]["codex"]["primary"]["remainingPercent"], 0)

    def test_flat_fallback_uses_top_level_used_percent_as_primary(self):
        out = self.parse({"rateLimits": {"usedPercent": 55, "resetsAt": 1790000000,
                                         "windowDurationMins": 300},
                          "ordinaryUsageAllowed": True})
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "headroom")
        self.assertEqual(out["buckets"]["codex"]["primary"]["usedPercent"], 55)

    def test_flat_without_valid_used_percent_is_unknown(self):
        out = self.parse({"rateLimits": {"resetsAt": 1790000000}})
        self.assertFalse(out["available"])
        self.assertEqual(out["capacityState"], "unknown")
        self.assertIn("flat-rateLimits-without-valid-usedPercent", out["issues"])

    def test_partial_window_keeps_valid_peer(self):
        out = self.parse({"rateLimitsByLimitId": {"codex": {
            "primary": {"resetsAt": 1790000000},
            "secondary": {"usedPercent": 30, "resetsAt": 1790000000}}}})
        self.assertTrue(out["available"])
        self.assertNotIn("primary", out["buckets"]["codex"])
        self.assertEqual(out["buckets"]["codex"]["secondary"]["usedPercent"], 30)
        self.assertIn("primary-usedPercent-invalid", out["issues"])
        self.assertEqual(out["capacityState"], "unknown")

    def test_documented_single_bucket_fallback_and_null_secondary(self):
        out = self.parse({"rateLimits": {"primary": {"usedPercent": 55},
                                          "secondary": None}})
        self.assertTrue(out["available"])
        self.assertEqual(out["buckets"]["codex"]["primary"]["usedPercent"], 55)
        self.assertEqual(out["capacityState"], "headroom")
        self.assertNotIn("issues", out)

    def test_empty_response_is_unknown(self):
        for value in ({}, {"rateLimitsByLimitId": {}}):
            out = self.parse(value)
            self.assertFalse(out["available"])
            self.assertEqual(out["capacityState"], "unknown")
            self.assertEqual(out["buckets"], {})

    def test_explicit_blocker_flag_is_at_limit(self):
        out = self.parse({"rateLimitsByLimitId": {"codex": {
            "rateLimitReachedType": "primary",
            "primary": {"usedPercent": 50, "resetsAt": 1790000000},
            "secondary": {"usedPercent": 10, "resetsAt": 1790000000}}}})
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "at_limit")

    def test_ordinary_usage_disallowed_is_at_limit(self):
        out = self.parse({"rateLimitsByLimitId": {"codex": {
            "primary": {"usedPercent": 10, "resetsAt": 1790000000}}},
            "ordinaryUsageAllowed": False})
        self.assertEqual(out["capacityState"], "at_limit")

    def test_invalid_percent_types_rejected_with_reasons(self):
        for bad in (-5, True, "50", float("nan")):
            out = self.parse({"rateLimitsByLimitId": {"codex": {
                "primary": {"usedPercent": bad, "resetsAt": 1790000000},
                "secondary": {"usedPercent": 20, "resetsAt": 1790000000}}}})
            self.assertTrue(out["available"])
            self.assertEqual(out["capacityState"], "unknown")
            self.assertIn("primary-usedPercent-invalid", out["issues"])
            self.assertEqual(out["buckets"]["codex"]["secondary"]["usedPercent"], 20)

    def test_secret_canary_absent_from_parse_output(self):
        out = self.parse({"rateLimitsByLimitId": {"codex": {
            "primary": {"usedPercent": 10, "resetsAt": 1790000000,
                        "accessToken": CANARY_TOKEN},
            "accountId": CANARY_ACCOUNT, "email": CANARY_EMAIL}},
            "accessToken": CANARY_TOKEN, "ordinaryUsageAllowed": True})
        no_canary(self, out)


class ClaudeParseTests(unittest.TestCase):
    def test_valid_windows_headroom(self):
        out = cpu.parse_claude({
            "five_hour": {"utilization": 42.5, "resets_at": 1790000000},
            "seven_day": {"utilization": 10, "resets_at": 1790000000}})
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "headroom")
        self.assertEqual(out["buckets"]["five_hour"]["windowDurationMins"], 300)
        self.assertEqual(out["buckets"]["seven_day"]["windowDurationMins"], 10080)

    def test_five_hour_full_is_at_limit(self):
        out = cpu.parse_claude({"five_hour": {"utilization": 100.0,
                                              "resets_at": 1790000000},
                                "seven_day": {"utilization": 16.0,
                                              "resets_at": 1790000000}})
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "at_limit")

    def test_numeric_string_utilization_rejected_with_explicit_issue(self):
        out = cpu.parse_claude({"five_hour": {"utilization": "12.5"}})
        self.assertFalse(out["available"])
        self.assertEqual(out["capacityState"], "unknown")
        self.assertIn("five_hour-utilization-invalid", out["issues"])

    def test_numeric_string_does_not_drop_valid_peer(self):
        out = cpu.parse_claude({"five_hour": {"utilization": "12.5"},
                                "seven_day": {"utilization": 20,
                                              "resets_at": 1790000000}})
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "unknown")
        self.assertIn("five_hour-utilization-invalid", out["issues"])
        self.assertEqual(out["buckets"]["seven_day"]["usedPercent"], 20)

    def test_optional_model_windows_can_be_null_without_schema_issue(self):
        out = cpu.parse_claude({"five_hour": {"utilization": 20},
                                "seven_day": {"utilization": 10},
                                "seven_day_opus": None, "seven_day_sonnet": None})
        self.assertEqual(out["capacityState"], "headroom")
        self.assertNotIn("issues", out)


class CodexSubprocessTests(unittest.TestCase):
    def test_sends_only_readonly_rpc_methods(self):
        with fake_codex(codex_frames({"rateLimitsByLimitId": {}})) as proc:
            cpu.codex()
        sent = proc.stdin.getvalue().decode()
        methods = [json.loads(line)["method"] for line in sent.splitlines()
                   if line.strip()]
        self.assertEqual(methods, ["initialize", "initialized",
                                   "account/rateLimits/read"])
        self.assertNotIn("model", sent)
        self.assertNotIn("reset", sent)

    def test_skips_non_json_banner_line(self):
        raw = (b"codex app-server starting\n"
               + codex_frames({"rateLimitsByLimitId": {"codex": {
                   "primary": {"usedPercent": 11, "resetsAt": 1790000000}}}}))
        with fake_codex(raw):
            out = cpu.codex()
        self.assertTrue(out["available"])
        self.assertEqual(out["buckets"]["codex"]["primary"]["usedPercent"], 11)

    def test_caps_oversized_framed_line_before_parsing(self):
        junk = b"x" * (cpu.MAX_LINE_BYTES + 4096) + b"\n"
        raw = junk + codex_frames({"rateLimitsByLimitId": {"codex": {
            "primary": {"usedPercent": 22, "resetsAt": 1790000000}}}})
        with fake_codex(raw):
            out = cpu.codex()
        self.assertTrue(out["available"])
        self.assertEqual(out["buckets"]["codex"]["primary"]["usedPercent"], 22)

    def test_rpc_error_reported_as_reason_not_raw(self):
        raw = (json.dumps({"id": 1, "result": {}}) + "\n"
               + json.dumps({"id": 2, "error": {"message": CANARY_TOKEN}}) + "\n")
        with fake_codex(raw.encode()):
            out = cpu.safely(cpu.codex)
        self.assertFalse(out["available"])
        self.assertEqual(out["capacityState"], "unknown")
        self.assertEqual(out["errorType"], "RuntimeError")
        no_canary(self, out)


class ClaudeSeamTests(unittest.TestCase):
    def test_live_shaped_payload_headroom_and_no_canary(self):
        payload = {"five_hour": {"utilization": 30.0, "resets_at": 1790000000},
                   "seven_day": {"utilization": 10.0, "resets_at": 1790000000},
                   "accountId": CANARY_ACCOUNT, "email": CANARY_EMAIL}
        with claude_seam(payload=payload):
            out = cpu.claude()
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "headroom")
        no_canary(self, out)

    def test_http_error_surfaces_status_not_body(self):
        error = urllib.error.HTTPError(
            "https://api.anthropic.com/api/oauth/usage", 401, "Unauthorized",
            {}, io.BytesIO(json.dumps({"detail": CANARY_TOKEN}).encode()))
        with claude_seam(error=error):
            out = cpu.safely(cpu.claude)
        self.assertFalse(out["available"])
        self.assertEqual(out["httpStatus"], 401)
        self.assertEqual(out["capacityState"], "unknown")
        no_canary(self, out)


class MalformedMetadataTests(unittest.TestCase):
    def test_absent_required_window_retains_peer_but_is_unknown(self):
        for out in [cpu.parse_codex({"rateLimitsByLimitId": {"codex": {
                "secondary": {"usedPercent": 20}}}}),
                cpu.parse_claude({"seven_day": {"utilization": 20}})]:
            self.assertTrue(out["available"])
            self.assertEqual(out["capacityState"], "unknown")
            self.assertTrue(out["issues"])

    def test_passthrough_fields_cannot_emit_arbitrary_payloads(self):
        value = {"ordinaryUsageAllowed": {"token": CANARY_TOKEN},
                 "rateLimitsByLimitId": {"codex": {
                     "rateLimitReachedType": {"token": CANARY_TOKEN},
                     "spendControlReached": CANARY_TOKEN,
                     "primary": {"usedPercent": 10, "resetsAt": {"token": CANARY_TOKEN},
                                 "windowDurationMins": CANARY_TOKEN}}}}
        out = cpu.parse_codex(value)
        no_canary(self, out)
        self.assertTrue(out["available"])
        self.assertEqual(out["capacityState"], "unknown")

    def test_invalid_timestamps_cannot_discard_valid_peer_usage(self):
        for stamp in [float("nan"), 1e20, True, {"token": CANARY_TOKEN}, CANARY_TOKEN]:
            out = cpu.parse_claude({"five_hour": {"utilization": 10, "resets_at": stamp},
                                    "seven_day": {"utilization": 20, "resets_at": 1790000000}})
            self.assertTrue(out["available"])
            self.assertEqual(out["buckets"]["seven_day"]["usedPercent"], 20)
            self.assertEqual(out["capacityState"], "unknown")
            no_canary(self, out)

    def test_early_stdout_eof_is_not_a_timeout(self):
        with fake_codex((json.dumps({"id": 1, "result": {}})+"\n").encode()):
            out = cpu.safely(cpu.codex)
        self.assertEqual(out["errorType"], "EOFError")


class MainExitCodeTests(unittest.TestCase):
    def run_main(self, provider_result):
        with mock.patch.object(cpu, "codex", return_value=provider_result), \
             mock.patch.object(sys, "argv",
                               ["check_provider_usage.py", "--provider", "codex"]), \
             mock.patch("sys.stdout", new_callable=io.StringIO) as captured:
            code = cpu.main()
        return code, captured.getvalue()

    def test_exit_zero_means_metadata_read_not_capacity(self):
        full = cpu.parse_codex({"rateLimitsByLimitId": {"codex": {
            "primary": {"usedPercent": 100, "resetsAt": 1790000000},
            "secondary": {"usedPercent": 100, "resetsAt": 1790000000}}},
            "ordinaryUsageAllowed": True})
        code, printed = self.run_main(full)
        self.assertEqual(code, 0)
        body = json.loads(printed)
        self.assertEqual(body["providers"]["codex"]["capacityState"], "at_limit")
        self.assertTrue(body["providers"]["codex"]["available"])

    def test_exit_one_when_metadata_unavailable(self):
        empty = cpu.parse_codex({"rateLimitsByLimitId": {}})
        code, printed = self.run_main(empty)
        self.assertEqual(code, 1)
        body = json.loads(printed)
        self.assertEqual(body["providers"]["codex"]["capacityState"], "unknown")


if __name__ == "__main__":
    unittest.main(verbosity=2)
