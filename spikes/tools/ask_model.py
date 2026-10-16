#!/usr/bin/env python3
"""Send one bounded task to a model through the local OpenCodex proxy and record provenance."""
import json, sys, time, urllib.request, hashlib
from pathlib import Path
model, prompt_file, out_file = sys.argv[1:4]
prompt = Path(prompt_file).read_text()
body = {"model": model, "messages": [
    {"role": "system", "content": "You write small, correct, dependency-free code for a well-specified task. Output only the requested file content inside one fenced code block, then nothing else."},
    {"role": "user", "content": prompt}], "max_tokens": 16000}
req = urllib.request.Request("http://127.0.0.1:10100/v1/chat/completions", json.dumps(body, ensure_ascii=False).encode(), {"Content-Type": "application/json"})
t = time.monotonic()
with urllib.request.urlopen(req, timeout=300) as r:
    d = json.load(r)
msg = d["choices"][0]["message"].get("content") or ""
Path(out_file + ".raw.md").write_text(msg)
code = msg
if "```" in msg:
    part = msg.split("```", 2)[1]
    code = part.split("\n", 1)[1] if "\n" in part else part
Path(out_file).write_text(code)
meta = {"requested_model": model, "response_model": d.get("model"), "finish_reason": d["choices"][0].get("finish_reason"),
        "seconds": round(time.monotonic() - t, 1), "prompt_sha256": hashlib.sha256(prompt.encode()).hexdigest(),
        "output_sha256": hashlib.sha256(code.encode()).hexdigest()}
Path(out_file + ".provenance.json").write_text(json.dumps(meta, ensure_ascii=False, indent=2))
print(json.dumps(meta))

