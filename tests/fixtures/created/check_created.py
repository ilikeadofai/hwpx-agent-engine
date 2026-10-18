#!/usr/bin/env python3
"""Independent structural checker for a `document.create` package (0.4).

This is a *third* implementation of the package contract on purpose: it reads a
created `.hwpx` with the standard library only and checks the rules in
docs/decisions/document-paragraph-creation-design.md §7 without using the Rust
builder or verifier.

    python3 -I tests/fixtures/created/check_created.py FILE.hwpx \
        [--title TITLE] [--expect-texts "첫 문단,둘째 문단"]

Exit status 0 means every check passed; failures print `FAIL: ...`.
"""
import argparse
import sys
import zipfile
from xml.etree import ElementTree as ET

MIMETYPE = b"application/hwp+zip"
HP = "{http://www.hancom.co.kr/hwpml/2011/paragraph}"
HH = "{http://www.hancom.co.kr/hwpml/2011/head}"
HS = "{http://www.hancom.co.kr/hwpml/2011/section}"
HV = "{http://www.hancom.co.kr/hwpml/2011/version}"
OCF = "{urn:oasis:names:tc:opendocument:xmlns:container}"
OPF = "{http://www.idpf.org/2007/opf/}"

EXPECTED = [
    "mimetype",
    "version.xml",
    "META-INF/container.xml",
    "Contents/content.hpf",
    "Contents/header.xml",
    "Contents/section0.xml",
]


def fail(message):
    print("FAIL: " + message)
    return False


def paragraph_text(p):
    out = []
    for t in p.iter(HP + "t"):
        out.append(t.text or "")
    return "".join(out)


def check(path, title, expect_texts):
    ok = True
    with zipfile.ZipFile(path) as z:
        infos = z.infolist()
        names = [i.filename for i in infos]
        if names != EXPECTED:
            ok = fail("entry order %r" % names)
        if infos[0].compress_type != zipfile.ZIP_STORED:
            ok = fail("mimetype is not STORED")
        if z.read("mimetype") != MIMETYPE:
            ok = fail("mimetype value %r" % z.read("mimetype"))

        version = ET.fromstring(z.read("version.xml"))
        if version.tag != HV + "HCFVersion":
            ok = fail("version.xml root %r" % version.tag)
        if version.get("application") != "Hancom Office Hangul":
            ok = fail("version.xml application %r" % version.get("application"))
        if version.get("tagetApplication") != "WORDPROCESSOR":
            ok = fail("version.xml tagetApplication %r" % version.get("tagetApplication"))
        if int(version.get("major", "99")) > 5:
            ok = fail("version.xml major %r must be <= 5" % version.get("major"))

        container = ET.fromstring(z.read("META-INF/container.xml"))
        roots = [r for r in container.iter(OCF + "rootfile")]
        if len(roots) != 1 or roots[0].get("full-path") != "Contents/content.hpf":
            ok = fail("container rootfile %r" % [r.attrib for r in roots])
        if roots and roots[0].get("media-type") != "application/hwpml-package+xml":
            ok = fail("container media-type %r" % roots[0].get("media-type"))

        hpf = ET.fromstring(z.read("Contents/content.hpf"))
        items = {i.get("id"): i.get("href") for i in hpf.iter(OPF + "item")}
        spine = [r.get("idref") for r in hpf.iter(OPF + "itemref")]
        if items.get("header") != "Contents/header.xml" or items.get("section0") != "Contents/section0.xml":
            ok = fail("manifest items %r" % items)
        if spine != ["header", "section0"]:
            ok = fail("spine %r" % spine)
        got_title = hpf.find(OPF + "metadata/" + OPF + "title")
        got_title = "" if got_title is None else (got_title.text or "")
        if title is not None and got_title != title:
            ok = fail("title %r != %r" % (got_title, title))

        header = ET.fromstring(z.read("Contents/header.xml"))
        defined = {
            "paraPr": {e.get("id") for e in header.iter(HH + "paraPr")},
            "charPr": {e.get("id") for e in header.iter(HH + "charPr")},
            "style": {e.get("id") for e in header.iter(HH + "style")},
            "borderFill": {e.get("id") for e in header.iter(HH + "borderFill")},
        }
        faces = list(header.iter(HH + "fontface"))
        if len(faces) != 7:
            ok = fail("fontfaces %d != 7" % len(faces))
        for kind, ids in defined.items():
            if not ids:
                ok = fail("header defines no %s" % kind)
        for kind, ref in [
            ("paraPr", "paraPrIDRef"),
            ("charPr", "charPrIDRef"),
            ("style", "styleIDRef"),
            ("borderFill", "borderFillIDRef"),
        ]:
            for e in header.iter():
                for value in e.attrib.values():
                    pass  # attributes only, refs are checked in the section below
        section = ET.fromstring(z.read("Contents/section0.xml"))
        if section.tag != HS + "sec":
            ok = fail("section root %r" % section.tag)
        for e in section.iter():
            for kind, ref in [
                ("paraPr", "paraPrIDRef"),
                ("charPr", "charPrIDRef"),
                ("style", "styleIDRef"),
                ("borderFill", "borderFillIDRef"),
            ]:
                r = e.get(ref)
                if r is not None and r not in defined[kind]:
                    ok = fail("unresolved %s %r" % (ref, r))
        paras = [p for p in section if p.tag == HP + "p"]
        if not paras:
            ok = fail("section has no paragraphs")
        if not any(e.tag == HP + "secPr" for e in paras[0].iter()):
            ok = fail("first paragraph does not own hp:secPr")
        if any(e.tag == HP + "linesegarray" for e in section.iter()):
            ok = fail("new document must not carry layout caches")
        texts = [paragraph_text(p) for p in paras]
        print("paragraphs: %r" % texts)
        if expect_texts is not None and texts != expect_texts:
            ok = fail("paragraph texts %r != %r" % (texts, expect_texts))
    print("OK: " + path if ok else "FAILED: " + path)
    return ok


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--title")
    ap.add_argument("--expect-texts")
    args = ap.parse_args()
    expected = None
    if args.expect_texts is not None:
        expected = args.expect_texts.split(",")
    sys.exit(0 if check(args.file, args.title, expected) else 1)
