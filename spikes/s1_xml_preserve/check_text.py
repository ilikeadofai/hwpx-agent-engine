#!/usr/bin/env python3
"""Independent S1 check with the Python standard library XML parser (different implementation from the Rust spike).
Confirms: edited entry is well-formed; exactly the target top-level paragraph's first non-empty pure text changed to NEW_TEXT;
all other hp:t texts are identical in order; linesegarray removed only from the target paragraph."""
import sys, zipfile, json
import xml.etree.ElementTree as ET
orig, edited, entry, para, new_text = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]
HP = "{http://www.hancom.co.kr/hwpml/2011/paragraph}"
def load(p):
    with zipfile.ZipFile(p) as z:
        return ET.fromstring(z.read(entry))
def paras(root):
    return [c for c in root if c.tag == HP + "p"]
def texts(p):
    return ["".join(t.itertext()) for t in p.iter(HP + "t")]
def lsa(p):
    return sum(1 for c in p if c.tag == HP + "linesegarray")
a, b = paras(load(orig)), paras(load(edited))
res = {"sameParagraphCount": len(a) == len(b)}
other_text_same, other_lsa_same, target = True, True, None
for i, (pa, pb) in enumerate(zip(a, b)):
    ta, tb = texts(pa), texts(pb)
    if i == para:
        changed = [k for k in range(min(len(ta), len(tb))) if ta[k] != tb[k]]
        target = {"changedIndices": changed, "newText": tb[changed[0]] if changed else None,
                  "lsaBefore": lsa(pa), "lsaAfter": lsa(pb), "sameTextCount": len(ta) == len(tb)}
    else:
        other_text_same &= ta == tb
        other_lsa_same &= lsa(pa) == lsa(pb)
res.update(otherParagraphTextsIdentical=other_text_same, otherLinesegIdentical=other_lsa_same, target=target)
res["ok"] = bool(res["sameParagraphCount"] and other_text_same and other_lsa_same and target
                 and len(target["changedIndices"]) == 1 and target["newText"] == new_text and target["sameTextCount"])
print(json.dumps(res, ensure_ascii=False))
sys.exit(0 if res["ok"] else 1)

