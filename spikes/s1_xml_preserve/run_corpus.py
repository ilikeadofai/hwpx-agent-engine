#!/usr/bin/env python3
"""Run S1 over a corpus: for each document try top-level paragraphs until 3 edits succeed; run all three checkers."""
import json, subprocess, sys, zipfile, hashlib
from pathlib import Path
HERE = Path(__file__).resolve().parent
BIN = HERE / "target/release/s1-xml-preserve"
OUT = HERE.parent / "work/s1"; OUT.mkdir(parents=True, exist_ok=True)
NEW = "S1 시험 문장 & <검증> 😀 끝"
docs = [Path(p) for p in sys.argv[1:]]
summary = []
for doc in docs:
    z = zipfile.ZipFile(doc)
    sec = sorted(n for n in z.namelist() if n.startswith("Contents/section") and n.endswith(".xml"))[0]
    done = 0
    for para in range(0, 200):
        if done == 3: break
        tag = hashlib.sha1(str(doc).encode()).hexdigest()[:8]
        out = OUT / f"{tag}_p{para}.hwpx"
        r = subprocess.run([str(BIN), str(doc), str(out), sec, str(para), NEW], capture_output=True, text=True)
        rust = json.loads(r.stdout) if r.stdout.strip().startswith("{") else {"error": r.stderr}
        if "error" in rust:
            if "no pure-text" in rust["error"]: continue
            if "topLevelParagraphsScanned" not in rust and "paragraph" in rust["error"]: break
            summary.append({"doc": doc.name, "para": para, "rustError": rust["error"]}); break
        c = subprocess.run([sys.executable, str(HERE / "check_text.py"), str(doc), str(out), sec, str(para), NEW], capture_output=True, text=True)
        v = subprocess.run([sys.executable, str(HERE / "verify_zip.py"), str(doc), str(out), "--changed", sec], capture_output=True, text=True)
        cj, vj = json.loads(c.stdout), json.loads(v.stdout)
        row = {"doc": doc.name, "section": sec, "para": para, "out": out.name,
               "rustBytes": rust["prefixIdentical"] and rust["betweenCutsIdentical"] and rust["suffixIdentical"],
               "linesegRemoved": rust["linesegRemoved"], "lsa": [rust["linesegarraysBefore"], rust["linesegarraysAfter"]],
               "elements": [rust["elementsBefore"], rust["elementsAfter"]],
               "pythonSemantic": cj["ok"], "zip": vj["ok"], "zipDetail": {k: vj[k] for k in vj if k not in ("added_entries",)} if not vj["ok"] else None}
        row["ok"] = row["rustBytes"] and row["pythonSemantic"] and row["zip"]
        summary.append(row); done += 1
    if done == 0 and not any(s.get("doc") == doc.name for s in summary):
        summary.append({"doc": doc.name, "note": "no editable pure-text paragraph found in first 200"})
(OUT / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=1))
ok = [s for s in summary if s.get("ok")]
print(json.dumps({"runs": len([s for s in summary if "para" in s and "rustError" not in s]), "ok": len(ok), "problems": [s for s in summary if not s.get("ok")]}, ensure_ascii=False, indent=1))

