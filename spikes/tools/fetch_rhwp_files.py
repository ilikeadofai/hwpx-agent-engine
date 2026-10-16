#!/usr/bin/env python3
import sys, urllib.request, urllib.parse, concurrent.futures, time, os
from pathlib import Path
C = "1a76570e833917d15817415a53c09ad61ab3203f"
dest = Path(sys.argv[2]); paths = [l.rstrip("\n") for l in open(sys.argv[1]) if l.strip()]
def get(p):
    out = dest / p
    if out.exists() and out.stat().st_size > 0: return p, "skip", 0
    url = f"https://raw.githubusercontent.com/edwardkim/rhwp/{C}/" + urllib.parse.quote(p)
    for attempt in range(4):
        try:
            with urllib.request.urlopen(url, timeout=120) as r: data = r.read()
            out.parent.mkdir(parents=True, exist_ok=True); tmp = out.with_suffix(out.suffix + ".part")
            tmp.write_bytes(data); os.replace(tmp, out); return p, "ok", len(data)
        except Exception as e:
            err = str(e); time.sleep(2 * (attempt + 1))
    return p, "fail:" + err, 0
t = time.time(); total = 0; fails = []
with concurrent.futures.ThreadPoolExecutor(24) as ex:
    for i, (p, st, n) in enumerate(ex.map(get, paths), 1):
        total += n
        if st.startswith("fail"): fails.append((p, st))
        if i % 100 == 0: print(f"{i}/{len(paths)} {total/1e6:.1f}MB {time.time()-t:.0f}s", flush=True)
print("done", len(paths), "bytes", total, "fails", len(fails), flush=True)
for f in fails[:20]: print(f)

