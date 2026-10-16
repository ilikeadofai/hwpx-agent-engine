#!/usr/bin/env python3
"""HWPX 편집 경계 시험 자료 합성기 (구현 계획 1-2b). Python 3 표준 라이브러리만 사용.

사용:
    python3 -I tests/fixtures/editing/gen_editing.py tests/fixtures/editing
    python3 -I tests/fixtures/editing/gen_editing.py tests/fixtures/editing --check-determinism

이 스크립트는 다음을 만든다.
  * tests/fixtures/editing/*.hwpx            편집 경계 합성 패키지 17개
  * tests/fixtures/editing/inputs/*         image.replace 검증용 교체 이미지 입력 8개
  * tests/fixtures/editing/editing_expected.json
        사례별 입력 구조·기대 변경·거절 코드·보존 조건 표

모든 자료는 합성이며 개인 문서를 쓰지 않는다. 패키지 골격(mimetype STORED 우선,
META-INF/container.xml·Contents/content.hpf·Contents/header.xml 배치)은
tests/fixtures/adversarial/valid_base.hwpx 에서 그대로 가져오고, 구역·manifest 만
사례에 맞게 바꾼다. 자료_원본 사본과 다른 시험 자료는 건드리지 않는다.

결정성: date_time 고정(1980-01-01), 항목 순서 고정, DEFLATE 는 zlib level 9.
같은 zlib 구현이면 같은 바이트가 나온다(--check-determinism 이 두 번 생성해 대조).
"""
import io
import json
import os
import struct
import sys
import unicodedata
import xml.etree.ElementTree as ET
import zipfile
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
SEED_PATH = os.path.join(HERE, "..", "adversarial", "valid_base.hwpx")

MIMETYPE = b"application/hwp+zip"
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>'

NS = {
    "hs": "http://www.hancom.co.kr/hwpml/2011/section",
    "hp": "http://www.hancom.co.kr/hwpml/2011/paragraph",
    "hc": "http://www.hancom.co.kr/hwpml/2011/core",
    "hh": "http://www.hancom.co.kr/hwpml/2011/head",
    "opf": "http://www.idpf.org/2007/opf/",
    "hpf": "http://www.hancom.co.kr/schema/2011/hpf",
    "ocf": "urn:oasis:names:tc:opendocument:xmlns:container",
}
HP = "{%s}" % NS["hp"]

OBJ = "\ufffc"  # 대체 문자: run 안 개체/컨트롤의 자리 표시자 (hae-model para.rs)
LIMITS = {
    "max_hwpx_bytes": 32 << 20,
    "max_entries": 2048,
    "max_xml_depth": 128,
    "max_image_pixels": 40_960_000,
    "max_session_memory": 512 << 20,
}


# ------------------------------------------------------------------ 자소/텍스트 oracle
# hae-model 은 unicode-segmentation 1.13.3(UAX#29 확장 자소 군집)을 쓴다. 여기서는
# 시험 문자열에 필요한 규칙만 구현해 독립 기대값(자소 경계·코드포인트 범위·검색)을
# 계산한다. 합성 문자열은 한글 자모·결합 문자·ZWJ 이모지·탭/줄바꿈만 쓴다.

def _cat(cp):
    return unicodedata.category(chr(cp))


def _is_control(cp):
    if cp in (0x0D, 0x0A, 0x200C, 0x200D):
        return False
    return _cat(cp) in ("Cc", "Cf", "Zl", "Zp")


def _is_extend(cp):
    return _cat(cp) in ("Mn", "Me") or cp in (0x200C,)


def _is_spacingmark(cp):
    return _cat(cp) == "Mc"


def _is_zwj(cp):
    return cp == 0x200D


def _is_extpict(cp):
    return (
        0x1F000 <= cp <= 0x1FAFF
        or 0x2600 <= cp <= 0x27BF
        or 0x2B00 <= cp <= 0x2BFF
        or cp in (0x00A9, 0x00AE, 0x203C, 0x2049, 0x2122, 0x2139, 0x3030, 0x303D, 0x3297, 0x3299)
    )


def _hangul_L(cp):
    return 0x1100 <= cp <= 0x115F or 0xA960 <= cp <= 0xA97C


def _hangul_V(cp):
    return 0x1160 <= cp <= 0x11A7 or 0xD7B0 <= cp <= 0xD7C6


def _hangul_T(cp):
    return 0x11A8 <= cp <= 0x11FF or 0xD7CB <= cp <= 0xD7FB


def _hangul_LV(cp):
    return 0xAC00 <= cp <= 0xD7A3 and (cp - 0xAC00) % 28 == 0


def _hangul_LVT(cp):
    return 0xAC00 <= cp <= 0xD7A3 and (cp - 0xAC00) % 28 != 0


def _no_break(cps, i):
    prev, cur = cps[i - 1], cps[i]
    if prev == 0x0D and cur == 0x0A:
        return True
    if _is_control(prev):
        return False
    if _is_control(cur):
        return False
    if _is_extend(cur) or _is_zwj(cur):
        return True
    if _is_spacingmark(cur):
        return True
    if _is_extpict(cur):
        j = i - 1
        if j >= 0 and cps[j] == 0x200D:
            k = j - 1
            while k >= 0 and _is_extend(cps[k]):
                k -= 1
            if k >= 0 and _is_extpict(cps[k]):
                return True
    if _hangul_L(prev) and (_hangul_L(cur) or _hangul_V(cur) or _hangul_LV(cur) or _hangul_LVT(cur)):
        return True
    if (_hangul_LV(prev) or _hangul_V(prev)) and (_hangul_V(cur) or _hangul_T(cur)):
        return True
    if (_hangul_LVT(prev) or _hangul_T(prev)) and _hangul_T(cur):
        return True
    return False


def graphemes(s):
    cps = [ord(c) for c in s]
    if not cps:
        return []
    clusters = [[cps[0]]]
    for i in range(1, len(cps)):
        if _no_break(cps, i):
            clusters[-1].append(cps[i])
        else:
            clusters.append([cps[i]])
    return clusters


def grapheme_bounds(s):
    b = {0}
    acc = 0
    for c in graphemes(s):
        acc += len(c)
        b.add(acc)
    return b


def cp_range_of(text, gstart, gend):
    cl = graphemes(text)
    if gstart < 0 or gend > len(cl) or gstart >= gend:
        return None
    return (sum(len(c) for c in cl[:gstart]), sum(len(c) for c in cl[:gend]))


def simulate_replace(text, grem, new):
    """문단 텍스트에서 자소 반개구간 [start,end) 를 new 로 바꾼 기대 결과."""
    cl = graphemes(text)
    return "".join(chr(cp) for c in cl[: grem[0]] for cp in c) + new + \
        "".join(chr(cp) for c in cl[grem[1]:] for cp in c)


def find_all(text, needle):
    """hae-model ParaText::find_all 과 같은 코드포인트 일치·자소 정렬 계산."""
    if not needle:
        return ([], 0)
    hay = [ord(c) for c in text]
    nd = [ord(c) for c in needle]
    if len(nd) > len(hay):
        return ([], 0)
    bounds = grapheme_bounds(text)
    out = []
    unaligned = 0
    for s in range(0, len(hay) - len(nd) + 1):
        if hay[s:s + len(nd)] == nd:
            if s in bounds and s + len(nd) in bounds:
                out.append((s, s + len(nd)))
            else:
                unaligned += 1
    return (out, unaligned)


def query_oracle(para_texts, needle):
    matched = []
    unaligned = 0
    for i, t in enumerate(para_texts):
        m, u = find_all(t, needle)
        unaligned += u
        if m:
            matched.append(i)
    return matched, unaligned


# ------------------------------------------------------------------ XML 조각

def root_ns(prefixes, extra=""):
    return " ".join('xmlns:%s="%s"' % (p, NS[p]) for p in prefixes) + extra


def R(inner, charpr="0"):
    return '<hp:run charPrIDRef="%s">%s</hp:run>' % (charpr, inner)


def T(s):
    return "<hp:t>%s</hp:t>" % s


def P(inner, attrs=""):
    return '<hp:p paraPrIDRef="0"%s>%s</hp:p>' % (attrs, inner)


def SEC(body, extra_ns=""):
    return (DECL + '<hs:sec xmlns:hs="%s" xmlns:hp="%s" xmlns:hc="%s"%s>%s</hs:sec>'
            % (NS["hs"], NS["hp"], NS["hc"], extra_ns, body)).encode("utf-8")


def pic(pid, ref, extra=""):
    return ('<hp:pic id="%s"><hp:sz width="1440" height="720"/><hp:pos/>'
            '<hp:imgDim dimwidth="1440" dimheight="720"/>%s'
            '<hc:img binaryItemIDRef="%s"/></hp:pic>' % (pid, extra, ref))


def merge_attrs(attrs):
    return "".join(' %s="%s"' % (k, v) for k, v in attrs)


def hpf(items, spine):
    man = "".join(
        '<opf:item id="%s" href="%s" media-type="%s"%s/>'
        % (i, h, m, ' isEmbeded="1"' if h.startswith("BinData/") else "")
        for i, h, m in items
    )
    sp = "".join('<opf:itemref idref="%s"/>' % s for s in spine)
    return ('%s<opf:package xmlns:opf="%s" xmlns:hpf="%s" version="" unique-identifier="" id="">'
            '<opf:metadata><opf:title/><opf:language>ko</opf:language></opf:metadata>'
            '<opf:manifest>%s</opf:manifest><opf:spine>%s</opf:spine></opf:package>'
            % (DECL, NS["opf"], NS["hpf"], man, sp)).encode("utf-8")


def hpf_sec(items_extra=None, spine_extra=None):
    items = [("header", "Contents/header.xml", "application/xml"),
             ("section0", "Contents/section0.xml", "application/xml")] + list(items_extra or [])
    spine = ["header", "section0"] + list(spine_extra or [])
    return hpf(items, spine)


# ------------------------------------------------------------------ PNG/JPEG 합성

def _zchunk(tag, data):
    c = struct.pack(">I", len(data)) + tag + data
    return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def png(w, h, rgb=True):
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2 if rgb else 0, 0, 0, 0)
    stride = b""
    for _ in range(h):
        stride += b"\x00" + (b"\xff\x00\x00" if rgb else b"\x7f") * w
    return (b"\x89PNG\r\n\x1a\n" + _zchunk(b"IHDR", ihdr)
            + _zchunk(b"IDAT", zlib.compress(stride, 9)) + _zchunk(b"IEND", b""))


# 자체 생성 1x1 grayscale baseline JPEG (합성, 제3자 자산 아님).
JPEG = bytes([
    255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0,
    255, 219, 0, 67, 0, 8, 6, 6, 7, 6, 5, 8, 7, 7, 7, 9, 9, 8, 10, 12, 20, 13,
    12, 11, 11, 12, 25, 18, 19, 15, 20, 29, 26, 31, 30, 29, 26, 28, 28, 32, 36,
    46, 39, 32, 34, 44, 35, 28, 28, 40, 55, 41, 44, 48, 49, 52, 52, 52, 31, 39,
    57, 61, 56, 50, 60, 46, 51, 52, 50, 255, 192, 0, 11, 8, 0, 1, 0, 1, 1, 1,
    17, 0, 255, 196, 0, 20, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 255, 196, 0, 20, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    255, 218, 0, 8, 1, 1, 0, 0, 63, 0, 63, 255, 217,
])


def huge_ihdr_png():
    p = bytearray(png(2, 1))
    p[16:20] = struct.pack(">I", 100_000)
    p[20:24] = struct.pack(">I", 100_000)
    return bytes(p)


def huge_sof_jpeg():
    j = bytearray(JPEG)
    i = j.find(b"\xff\xc0")
    assert i >= 0
    j[i + 5:i + 9] = b"\xff\xff\xff\xff"
    return bytes(j)


def apng():
    base = png(1, 1)
    actl = _zchunk(b"acTL", struct.pack(">II", 1, 0))
    return base[:33] + actl + base[33:]


# ------------------------------------------------------------------ 패키지 조립

def _seed():
    with zipfile.ZipFile(SEED_PATH) as z:
        order = [(i.filename, i.compress_type) for i in z.infolist()]
        return order, {i.filename: z.read(i.filename) for i in z.infolist()}


SEED_ORDER, SEED = _seed()


def zip_files(files):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for name, data, method in files:
            zi = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            zi.compress_type = method
            zi.create_system = 0
            zi.external_attr = 0o100644 << 16
            z.writestr(zi, data, compress_type=method, compresslevel=9 if method == 8 else None)
    return buf.getvalue()


def build(section, header=None, hpf_bytes=None, container=None, extra=()):
    """SEED 골격 그대로, 구역·manifest·추가 항목만 바꾼 패키지."""
    # SEED_ORDER 를 따르되 section0.xml 을 새 구역으로, 나머지는 SEED/override.
    payload = dict(SEED)
    payload["Contents/section0.xml"] = section
    if header is not None:
        payload["Contents/header.xml"] = header
    if hpf_bytes is not None:
        payload["Contents/content.hpf"] = hpf_bytes
    if container is not None:
        payload["META-INF/container.xml"] = container
    files = [(n, payload[n], m) for n, m in SEED_ORDER] + list(extra)
    return zip_files(files)


def build2(sec0, sec1, header=None):
    """구역 두 개(자연 순서: section0, section1)."""
    base = dict(SEED)
    base["Contents/section0.xml"] = sec0
    base["Contents/section1.xml"] = sec1
    if header is not None:
        base["Contents/header.xml"] = header
    base["Contents/content.hpf"] = hpf_sec([("section1", "Contents/section1.xml", "application/xml")],
                                           ["section1"])
    files = [(n, base[n], m) for n, m in SEED_ORDER]
    files.append(("Contents/section1.xml", sec1, 8))
    return zip_files(files)


# ------------------------------------------------------------------ 관찰/검증

def read_all(b):
    with zipfile.ZipFile(io.BytesIO(b)) as z:
        assert z.testzip() is None, "zip CRC 실패"
        infos = z.infolist()
        assert infos[0].filename == "mimetype", "mimetype 이 첫 항목이 아님"
        assert infos[0].compress_type == zipfile.ZIP_STORED, "mimetype 이 STORED 가 아님"
        files = {i.filename: z.read(i.filename) for i in infos}
    assert files["mimetype"] == MIMETYPE
    for n, d in files.items():
        if n.endswith(".xml") or n.endswith(".hpf"):
            ET.fromstring(d)  # well-formed
    return files


def _t_text(t):
    s = t.text or ""
    for c in t:
        if c.tag == HP + "tab":
            s += "\t"
        elif c.tag == HP + "lineBreak":
            s += "\n"
        elif c.tag == HP + "fwSpace":
            s += "\u3000"
        elif c.tag == HP + "nbSpace":
            s += "\u00a0"
        elif c.tag == HP + "hyphen":
            s += "-"
        else:
            s += OBJ
        s += c.tail or ""
    return s


def para_text(p):
    """hae-model ParaText 와 같은 규칙으로 문단 텍스트 계산."""
    out = []
    for run in p:
        if run.tag != HP + "run":
            continue
        for c in run:
            if c.tag == HP + "t":
                out.append(_t_text(c))
            elif c.tag == HP + "tab":
                out.append("\t")
            elif c.tag == HP + "lineBreak":
                out.append("\n")
            elif c.tag == HP + "secPr":
                pass
            else:
                out.append(OBJ)
    return "".join(out)


def para_descs(section_bytes):
    root = ET.fromstring(section_bytes)
    out = []
    for i, p in enumerate(root.iter(HP + "p")):
        t = para_text(p)
        out.append({"index": i, "text": t, "codepoints": len(t), "graphemes": len(graphemes(t))})
    return out


def top_paragraphs(section_bytes):
    root = ET.fromstring(section_bytes)
    return [p for p in root if p.tag == HP + "p"]


def tables_of(section_bytes):
    """가장 가까운 hp:tbl 에 속한 셀만 모은다(중첩 표는 자기 표에)."""
    root = ET.fromstring(section_bytes)
    out = []

    def walk(el, owner):
        for child in el:
            if child.tag == HP + "tbl":
                node = {"rowCnt": int(child.get("rowCnt")), "colCnt": int(child.get("colCnt")), "cells": []}
                out.append(node)
                walk(child, node)
            elif child.tag == HP + "tc" and owner is not None:
                addr = child.find(HP + "cellAddr")
                span = child.find(HP + "cellSpan")
                subs = child.find(HP + "subList")
                paras = []
                if subs is not None:
                    for p in subs.findall(HP + "p"):
                        paras.append(para_text(p))
                owner["cells"].append({
                    "row": int(addr.get("rowAddr")), "col": int(addr.get("colAddr")),
                    "rowSpan": int(span.get("rowSpan")), "colSpan": int(span.get("colSpan")),
                    "paragraphs": paras,
                })
                walk(child, owner)
            else:
                walk(child, owner)

    walk(root, None)
    return out


def pictures_of(section_bytes):
    root = ET.fromstring(section_bytes)
    out = []
    for picel in root.iter(HP + "pic"):
        img = None
        for d in picel.iter():
            if d.tag == "{http://www.hancom.co.kr/hwpml/2011/core}img":
                img = d
        out.append(img.get("binaryItemIDRef") if img is not None else None)
    return out


def check_package(files):
    assert "Contents/section0.xml" in files
    return files


# ------------------------------------------------------------------ 검증 헬퍼

def eq(actual, expected, what):
    assert actual == expected, "%s: expected %r, got %r" % (what, expected, actual)


def in_bytes(files, name, needle, what):
    assert needle in files[name], "%s: %r not found in %s" % (what, needle, name)


# ------------------------------------------------------------------ 사례 정의

CASES = []


def case(file, category, notes, build, check, operations, structure_builder=None):
    CASES.append(dict(file=file, category=category, notes=notes, build=build, check=check,
                      operations=operations, structure=structure_builder))


# ===== 1. 분할 run =====
def c_split_runs():
    body = P(R(T("abc"), "0") + R(T("def"), "1") + R(T("ghi"), "2"))
    return build(SEC(body))


def k_split_runs(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    texts = [d["text"] for d in para_descs(sec)]
    eq(texts, ["abcdefghi"], "split_runs 문단 텍스트")
    # 세 run 과 서로 다른 글자 모양이 실제로 있다.
    for frag in [b'<hp:run charPrIDRef="0"><hp:t>abc</hp:t>',
                 b'<hp:run charPrIDRef="1"><hp:t>def</hp:t>',
                 b'<hp:run charPrIDRef="2"><hp:t>ghi</hp:t>']:
        in_bytes(f, "Contents/section0.xml", frag, "run 구조")
    assert len(graphemes(texts[0])) == 9
    return {"entry": "Contents/section0.xml",
            "paragraphs": [{"index": 0, "text": texts[0], "graphemes": 9}],
            "runCharPrIdRef": ["0", "1", "2"]}


case(
    "split_runs.hwpx", "text",
    "한 문단이 서로 다른 글자 모양의 run 3개로 나뉜 경우. 범위가 run 경계를 지날 때 첫 글자 run 의 모양을 쓰고 나머지는 FORMAT_COLLAPSED 로 보고한다. 범위 밖 run 은 남기고 비게 된 hp:t 도 지우거나 합치지 않는다.",
    c_split_runs, k_split_runs,
    [
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [2, 8], "text": "X"}, "expect": "applied",
         "sourceText": "abcdefghi", "resultText": "abXi",
         "diagnostics": [{"code": "FORMAT_COLLAPSED"}],
         "preservedRaw": ['<hp:run charPrIDRef="1"><hp:t></hp:t></hp:run>'],
         "note": "run0 'c'|run1 'def'|run2 'gh' 를 지우고 첫 조각(run0)에 X 를 넣는다. 빈 hp:t 와 run 은 유지."},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [3, 4], "text": "Z"}, "expect": "applied",
         "sourceText": "abcdefghi", "resultText": "abcZefghi",
         "diagnostics": [], "note": "run1 안에서만 바꾸므로 FORMAT_COLLAPSED 없음."},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 9], "text": "SAME"}, "expect": "applied",
         "sourceText": "abcdefghi", "resultText": "SAME",
         "diagnostics": [{"code": "FORMAT_COLLAPSED"}], "note": "전체 삭제 후 대체. 빈 run 3개가 남는다."},
    ])


# ===== 2. NFC / NFD =====
def c_nfc_nfd():
    return build(SEC(P(R(T("가"))) + P(R(T("\u1100\u1161")))))


def k_nfc_nfd(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ds = para_descs(sec)
    eq([d["text"] for d in ds], ["가", "\u1100\u1161"], "nfc/nfd 텍스트")
    eq([d["codepoints"] for d in ds], [1, 2], "nfc/nfd 코드포인트")
    eq([d["graphemes"] for d in ds], [1, 1], "nfc/nfd 자소")
    # 가(U+AC00) 는 p0 만, NFD 자모는 p1 만.
    eq(query_oracle([d["text"] for d in ds], "가"), ([0], 0), "query 가")
    eq(query_oracle([d["text"] for d in ds], "\u1100"), ([], 1), "query ᄀ (정렬 밖)")
    eq(query_oracle([d["text"] for d in ds], "\u1100\u1161"), ([1], 0), "query 가")
    return {"entry": "Contents/section0.xml",
            "paragraphs": [{"index": i, "text": d["text"], "codepoints": d["codepoints"],
                            "graphemes": d["graphemes"]} for i, d in enumerate(ds)]}


case(
    "nfc_nfd.hwpx", "text",
    "NFC 한 글자(가 U+AC00) 문단과 NFD 자모 두 코드포인트(ᄀ U+1100 + ᅡ U+1161) 문단. 원문을 정규화하지 않고 코드포인트로 일치시킨다. NFD 자모 하나만 찾으면 자소 경계 밖이라 unalignedMatches 로만 센다.",
    c_nfc_nfd, k_nfc_nfd,
    [
        {"op": "element.query", "args": {"kind": "paragraph", "textContains": "가"},
         "expect": "ok", "matches": [{"paragraph": 0}], "unalignedMatches": 0,
         "note": "코드포인트 일치: U+AC00 은 NFC 문단만."},
        {"op": "element.query", "args": {"kind": "paragraph", "textContains": "\u1100"},
         "expect": "ok", "matches": [], "unalignedMatches": 1,
         "note": "ᄀ(U+1100) 하나는 자소 경계가 아니라 결과에서 제외."},
        {"op": "element.query", "args": {"kind": "paragraph", "textContains": "\u1100\u1161"},
         "expect": "ok", "matches": [{"paragraph": 1}], "unalignedMatches": 0},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 1},
         "args": {"range": [0, 1], "text": "각"}, "expect": "applied",
         "sourceText": "\u1100\u1161", "resultText": "각", "diagnostics": [],
         "note": "자소 한 개가 코드포인트 두 개를 덮는다."},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 1},
         "args": {"range": [0, 1], "text": "가"}, "expect": "applied",
         "sourceText": "\u1100\u1161", "resultText": "가",
         "diagnostics": [], "note": "NFC 로 '정규화'하지 않고 쓴 그대로 저장한다."},
    ])


# ===== 3. 결합 문자 run 경계 =====
def c_combining():
    p0 = P(R(T("e"), "0") + R(T("\u0301"), "1"))
    p1 = P(R(T("a\u0301")))
    return build(SEC(p0 + p1))


def k_combining(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ds = para_descs(sec)
    eq([d["text"] for d in ds], ["e\u0301", "a\u0301"], "결합 문자 텍스트")
    eq([d["graphemes"] for d in ds], [1, 1], "결합 문자 자소")
    eq(query_oracle(["e\u0301"], "e"), ([], 1), "e 단독 검색은 정렬 밖")
    return {"entry": "Contents/section0.xml",
            "paragraphs": [{"index": i, "text": d["text"], "codepoints": d["codepoints"],
                            "graphemes": d["graphemes"]} for i, d in enumerate(ds)],
            "runs": ["e", "\u0301"]}


case(
    "combining_across_runs.hwpx", "text",
    "e(run0) + U+0301 결합 악센트(run1) 로 자소 군집 하나가 run 경계를 지난다. 마찬가지로 한 run 안의 a + U+0301 도 자소 하나다. 자소 [0,1) 은 코드포인트 0..2 를 덮는다.",
    c_combining, k_combining,
    [
        {"op": "element.query", "args": {"kind": "paragraph", "textContains": "e"},
         "expect": "ok", "matches": [], "unalignedMatches": 1},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 1], "text": "é"}, "expect": "applied",
         "sourceText": "e\u0301", "resultText": "é", "diagnostics": [],
         "note": "run 을 넘는 자소 하나를 바꾼다."},
    ])


# ===== 4. ZWJ 이모지 run 경계 =====
def c_zwj():
    p0 = P(R(T("\U0001f468\u200d"), "0") + R(T("\U0001f469"), "1"))
    p1 = P(R(T("\U0001f468\u200d\U0001f469\u200d\U0001f466")))
    return build(SEC(p0 + p1))


def k_zwj(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ds = para_descs(sec)
    eq([d["text"] for d in ds], ["\U0001f468\u200d\U0001f469",
                                 "\U0001f468\u200d\U0001f469\u200d\U0001f466"], "zwj 텍스트")
    eq([d["graphemes"] for d in ds], [1, 1], "zwj 자소")
    eq([d["codepoints"] for d in ds], [3, 5], "zwj 코드포인트")
    eq(query_oracle([d["text"] for d in ds], "\u200d"), ([], 3), "ZWJ 단독 검색")
    # p1 은 '남 ZWJ 여 ZWJ 아이' 가 자소 하나라 3코드포인트(남ZWJ여)가 경계 밖이다.
    eq(query_oracle([d["text"] for d in ds], "\U0001f468\u200d\U0001f469"), ([0], 1), "이모지 쌍")
    return {"entry": "Contents/section0.xml",
            "paragraphs": [{"index": i, "text": d["text"], "codepoints": d["codepoints"],
                            "graphemes": d["graphemes"]} for i, d in enumerate(ds)]}


case(
    "zwj_emoji_across_runs.hwpx", "text",
    "ZWJ 이모지(👨‍👩)가 run0(👨+ZWJ) / run1(👩) 로 나뉜 문단과 한 run 의 가족 이모지. UAX#29 GB11 로 각각 자소 하나다. ZWJ(U+200D) 단독 검색은 자소 경계 밖이다.",
    c_zwj, k_zwj,
    [
        {"op": "element.query", "args": {"kind": "paragraph", "textContains": "\u200d"},
         "expect": "ok", "matches": [], "unalignedMatches": 3,
         "note": "p0 의 ZWJ 1개 + p1 의 ZWJ 2개가 모두 정렬 밖."},
        {"op": "element.query", "args": {"kind": "paragraph",
                                         "textContains": "\U0001f468\u200d\U0001f469"},
         "expect": "ok", "matches": [{"paragraph": 0}], "unalignedMatches": 1,
         "note": "p0 만 일치 구간 끝(3코드포인트)이 경계이고, p1 은 가족 이모지 하나라 경계 밖."},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 1], "text": "\U0001f642"}, "expect": "applied",
         "sourceText": "\U0001f468\u200d\U0001f469", "resultText": "\U0001f642",
         "diagnostics": [], "note": "자소 하나 = 코드포인트 3개, run 경계를 지난다."},
    ])


# ===== 5. 문자 참조 / CDATA =====
def c_char_refs():
    raw = "가&#xAC00;&amp;&lt;<![CDATA[ <raw> & ]]>끝"
    return build(SEC(P(R(T(raw)))))


def k_char_refs(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ds = para_descs(sec)
    text = "가가&< <raw> & 끝"
    eq([d["text"] for d in ds], [text], "문자 참조/CDATA 해석 텍스트")
    eq(ds[0]["codepoints"], 14, "코드포인트")
    for frag in [b"&#xAC00;", b"&amp;", b"&lt;", b"<![CDATA[ <raw> & ]]>"]:
        in_bytes(f, "Contents/section0.xml", frag, "원문 표기 보존")
    eq(query_oracle([text], "가"), ([0], 0), "query 가")
    return {"entry": "Contents/section0.xml",
            "paragraphs": [{"index": 0, "text": text, "codepoints": 14, "graphemes": 14}],
            "rawNotation": ["&#xAC00;", "&amp;", "&lt;", "<![CDATA[ <raw> & ]]>"]}


case(
    "char_refs_cdata.hwpx", "text",
    "한 run 에 16진 문자 참조(&#xAC00;), &amp;, &lt;, CDATA(<![CDATA[ <raw> & ]]>), 일반 텍스트가 섞여 있다. 텍스트는 해석해 보여 주되 원문 표기는 보관한다. 참조 한 글자를 범위로 덮으면 참조 전체를, CDATA 안을 덮으면 CDATA 경계를 쪼개고 새 텍스트만 이스케이프한다.",
    c_char_refs, k_char_refs,
    [
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [1, 2], "text": "X"}, "expect": "applied",
         "sourceText": "가가&< <raw> & 끝", "resultText": "가X&< <raw> & 끝",
         "diagnostics": [],
         "preservedRaw": ['&#xAC00;|replaced'], "note": "&#xAC00; 참조 전체가 X 로 바뀐다."},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [6, 9], "text": "Y"}, "expect": "applied",
         "sourceText": "가가&< <raw> & 끝", "resultText": "가가&< <Y> & 끝",
         "diagnostics": [],
         "note": "CDATA 안 'raw' 만 바꾼다. 앞뒤 CDATA 조각은 남고 새 텍스트는 &gt; 로 이스케이프된다."},
        {"op": "element.query", "args": {"kind": "paragraph", "textContains": "가"},
         "expect": "ok", "matches": [{"paragraph": 0}], "unalignedMatches": 0},
    ])


# ===== 6. 탭/컨트롤 =====
def c_tabs_control():
    # 실제 HWPX 처럼 hp:tab·hp:lineBreak·hp:fwSpace 를 hp:t 안에 둔다(코퍼스 관찰).
    p0 = P(R(T("a<hp:tab/>b<hp:lineBreak/>c")))
    p1 = P(R(T("z") + '<hp:fieldBegin id="1"/>' + T("w")))
    p2 = P(R(T("m<hp:fwSpace/>n")))
    return build(SEC(p0 + p1 + p2))


def k_tabs_control(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ds = para_descs(sec)
    eq([d["text"] for d in ds], ["a\tb\nc", "z" + OBJ + "w", "m\u3000n"], "탭/컨트롤 텍스트")
    eq([d["graphemes"] for d in ds], [5, 3, 3], "탭/컨트롤 자소")
    for frag in [b"<hp:tab/>", b"<hp:lineBreak/>", b"<hp:fieldBegin", b"<hp:fwSpace/>"]:
        in_bytes(f, "Contents/section0.xml", frag, "컨트롤 요소")
    return {"entry": "Contents/section0.xml",
            "paragraphs": [{"index": i, "text": d["text"], "graphemes": d["graphemes"]}
                           for i, d in enumerate(ds)],
            "controls": {"p0": ["tab", "lineBreak"], "p1": ["fieldBegin"], "p2": ["fwSpace"]}}


case(
    "tabs_control.hwpx", "text",
    "탭·줄바꿈(hp:tab·hp:lineBreak), 필드(hp:fieldBegin), 공백(hp:fwSpace)이 텍스트 사이에 있다. 범위가 탭·줄바꿈·필드를 지나면 RANGE_CROSSES_CONTROL, hp:t 안의 fwSpace 같은 다른 자식을 지나면 UNSUPPORTED_CONTENT_IN_RANGE, 새 텍스트에 탭·줄바꿈이 있으면 INVALID_REQUEST.",
    c_tabs_control, k_tabs_control,
    [
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 1], "text": "X"}, "expect": "applied",
         "sourceText": "a\tb\nc", "resultText": "X\tb\nc", "diagnostics": []},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 3], "text": "X"}, "expect": "reject",
         "code": "RANGE_CROSSES_CONTROL", "reason": None, "note": "범위가 탭을 지난다."},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 1], "text": "X\ty"}, "expect": "reject",
         "code": "INVALID_REQUEST", "reason": None, "note": "새 텍스트에 탭."},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 1},
         "args": {"range": [0, 3], "text": "Q"}, "expect": "reject",
         "code": "RANGE_CROSSES_CONTROL", "reason": None, "note": "hp:fieldBegin 컨트롤을 지난다."},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 1},
         "args": {"range": [0, 1], "text": "Q"}, "expect": "applied",
         "sourceText": "z" + OBJ + "w", "resultText": "Q" + OBJ + "w", "diagnostics": []},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 2},
         "args": {"range": [0, 1], "text": "Q"}, "expect": "applied",
         "sourceText": "m\u3000n", "resultText": "Q\u3000n", "diagnostics": []},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 2},
         "args": {"range": [0, 3], "text": "Q"}, "expect": "reject",
         "code": "UNSUPPORTED_CONTENT_IN_RANGE", "reason": None, "note": "hp:fwSpace 자식을 포함."},
    ])


# ===== 7. 다중 구역 =====
def c_multi_section():
    s0 = P(R(T("구역0 문단0"))) + P(R(T("구역0 문단1")))
    s1 = P(R(T("구역1 문단0")))
    return build2(SEC(s0), SEC(s1))


def k_multi_section(b):
    f = read_all(b)
    eq(sorted(n for n in f if n.endswith("section1.xml")), ["Contents/section1.xml"], "section1 존재")
    root = ET.fromstring(f["Contents/content.hpf"])
    opf = "{%s}" % NS["opf"]
    man = {i.get("id"): i.get("href") for i in root.iter(opf + "item")}
    spine = [man[r.get("idref")] for r in root.iter(opf + "itemref")]
    eq(spine, ["Contents/header.xml", "Contents/section0.xml", "Contents/section1.xml"], "spine 순서")
    d0 = para_descs(f["Contents/section0.xml"])
    d1 = para_descs(f["Contents/section1.xml"])
    eq([d["text"] for d in d0], ["구역0 문단0", "구역0 문단1"], "section0 텍스트")
    eq([d["text"] for d in d1], ["구역1 문단0"], "section1 텍스트")
    eq(query_oracle([d["text"] for d in d0], "구역0"), ([0, 1], 0), "section0 query")
    return {"spine": spine,
            "sections": [
                {"section": 0, "entry": "Contents/section0.xml",
                 "paragraphs": [d["text"] for d in d0]},
                {"section": 1, "entry": "Contents/section1.xml",
                 "paragraphs": [d["text"] for d in d1]},
            ]}


case(
    "multi_section.hwpx", "structure",
    "구역 두 개(Contents/section0.xml, Contents/section1.xml). 구역 번호는 content.hpf spine 순서를 따른다. 한 구역을 고쳐도 다른 구역 항목은 바뀌지 않아야 한다.",
    c_multi_section, k_multi_section,
    [
        {"op": "element.query", "args": {"kind": "paragraph", "textContains": "구역0"},
         "expect": "ok", "matches": [{"entry": "Contents/section0.xml", "paragraph": 0},
                                     {"entry": "Contents/section0.xml", "paragraph": 1}],
         "unalignedMatches": 0},
        {"op": "text.replace",
         "target": {"entry": "Contents/section1.xml", "paragraph": 0},
         "args": {"range": [0, 4], "text": "바뀐"}, "expect": "applied",
         "sourceText": "구역1 문단0", "resultText": "바뀐문단0", "diagnostics": [],
         "preservedEntries": ["Contents/section0.xml"],
         "note": "section0 은 바이트 그대로 남는다."},
    ])


# ===== 8. 단순 셀 =====
def _tc(row, col, rs, cs, paras, addr_attrs=""):
    ps = "".join(paras)
    return ('<hp:tc><hp:cellAddr rowAddr="%d" colAddr="%d"%s/>'
            '<hp:cellSpan rowSpan="%d" colSpan="%d"/>'
            '<hp:subList>%s</hp:subList></hp:tc>' % (row, col, addr_attrs, rs, cs, ps))


def _tbl(rows, cols, trs):
    return ('<hp:tbl rowCnt="%d" colCnt="%d">%s</hp:tbl>' % (rows, cols, "".join(trs)))


def _tr(cells):
    return "<hp:tr>%s</hp:tr>" % "".join(cells)


def c_simple_cell():
    t = _tbl(1, 2, [_tr([
        _tc(0, 0, 1, 1, [P(R(T("셀A")))]),
        _tc(0, 1, 1, 1, [P(R("<hp:t/>", "3"))]),
    ])])
    return build(SEC(P(R(t))))


def k_simple_cell(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ts = tables_of(sec)
    eq(len(ts), 1, "표 개수")
    eq((ts[0]["rowCnt"], ts[0]["colCnt"]), (1, 2), "표 크기")
    eq([c["paragraphs"] for c in ts[0]["cells"]], [["셀A"], [""]], "셀 문단 텍스트")
    return {"entry": "Contents/section0.xml", "tables": [
        {"table": 0, "rowCnt": 1, "colCnt": 2, "cells": [
            {"row": 0, "col": 0, "rowSpan": 1, "colSpan": 1, "paragraphs": ["셀A"]},
            {"row": 0, "col": 1, "rowSpan": 1, "colSpan": 1, "paragraphs": [""], "emptyCell": True},
        ]}]}


case(
    "simple_cell.hwpx", "table",
    "1행 2열 표. (0,0) 셀은 문단 하나에 텍스트, (0,1) 셀은 빈 문단(run 안 자기닫힘 hp:t). cell.setText 는 문단 하나인 셀만 바꾸며, 빈 셀은 첫 run 에 텍스트를 넣는다.",
    c_simple_cell, k_simple_cell,
    [
        {"op": "cell.setText", "target": {"table": 0, "row": 0, "col": 0},
         "args": {"text": "바뀐 셀"}, "expect": "applied",
         "sourceText": "셀A", "resultText": "바뀐 셀", "diagnostics": [],
         "note": "표를 담은 최상위 문단부터 linesegarray 캐시를 지운다."},
        {"op": "cell.setText", "target": {"table": 0, "row": 0, "col": 1},
         "args": {"text": "빈 칸"}, "expect": "applied",
         "sourceText": "", "resultText": "빈 칸", "diagnostics": [],
         "note": "빈 셀: 첫 run 의 hp:t 에 텍스트를 넣는다."},
        {"op": "cell.setText", "target": {"table": 0, "row": 0, "col": 0},
         "args": {"text": "X\tY"}, "expect": "reject",
         "code": "INVALID_REQUEST", "reason": None},
        {"op": "element.query", "args": {"table": 0, "row": 0, "col": 0}, "expect": "ok",
         "matches": [{"table": 0, "row": 0, "col": 0, "rowSpan": 1, "colSpan": 1}],
         "unalignedMatches": 0},
    ])


# ===== 9. 병합 셀 =====
def c_merged_cells():
    t = _tbl(2, 2, [
        _tr([_tc(0, 0, 2, 1, [P(R(T("병합")))]), _tc(0, 1, 1, 1, [P(R(T("우상")))])]),
        _tr([_tc(1, 1, 1, 1, [P(R(T("우하")))])]),
    ])
    return build(SEC(P(R(t))))


def k_merged_cells(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ts = tables_of(sec)
    eq((ts[0]["rowCnt"], ts[0]["colCnt"]), (2, 2), "표 크기")
    addrs = [(c["row"], c["col"], c["rowSpan"], c["colSpan"]) for c in ts[0]["cells"]]
    eq(addrs, [(0, 0, 2, 1), (0, 1, 1, 1), (1, 1, 1, 1)], "셀 주소/병합")
    return {"entry": "Contents/section0.xml", "tables": [
        {"table": 0, "rowCnt": 2, "colCnt": 2, "cells": [
            {"row": 0, "col": 0, "rowSpan": 2, "colSpan": 1, "paragraphs": ["병합"], "merged": True},
            {"row": 0, "col": 1, "rowSpan": 1, "colSpan": 1, "paragraphs": ["우상"]},
            {"row": 1, "col": 1, "rowSpan": 1, "colSpan": 1, "paragraphs": ["우하"]},
        ]}]}


case(
    "merged_cells.hwpx", "table",
    "2행 2열 표에서 (0,0) 이 rowSpan=2 로 병합되어 (1,0) 주소가 비어 있다. 병합 셀은 조회·보존 대상이며 cell.setText 는 UNSUPPORTED(merged_cell). (row,col) 조회는 병합 영역 전체를 덮는 셀을 돌려준다.",
    c_merged_cells, k_merged_cells,
    [
        {"op": "cell.setText", "target": {"table": 0, "row": 0, "col": 0},
         "args": {"text": "바꿈"}, "expect": "reject",
         "code": "UNSUPPORTED", "reason": "merged_cell"},
        {"op": "cell.setText", "target": {"table": 0, "row": 0, "col": 1},
         "args": {"text": "바꿈"}, "expect": "applied",
         "sourceText": "우상", "resultText": "바꿈", "diagnostics": []},
        {"op": "element.query", "args": {"table": 0, "row": 1, "col": 0}, "expect": "ok",
         "matches": [{"table": 0, "row": 0, "col": 0, "rowSpan": 2, "colSpan": 1}],
         "unalignedMatches": 0, "note": "(1,0) 은 병합 영역이라 (0,0) 셀이 나온다."},
    ])


# ===== 10. 다문단 셀 =====
def c_multiparagraph_cell():
    t = _tbl(1, 2, [_tr([
        _tc(0, 0, 1, 1, [P(R(T("한 문단")))]),
        _tc(0, 1, 1, 1, [P(R(T("문단1"))), P(R(T("문단2")))]),
    ])])
    return build(SEC(P(R(t))))


def k_multiparagraph_cell(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ts = tables_of(sec)
    eq(ts[0]["cells"][1]["paragraphs"], ["문단1", "문단2"], "다문단 셀")
    eq(ts[0]["cells"][0]["paragraphs"], ["한 문단"], "단문단 셀")
    node_text = "\n".join(ts[0]["cells"][1]["paragraphs"])
    eq(node_text, "문단1\n문단2", "셀 node_text 결합")
    return {"entry": "Contents/section0.xml", "tables": [
        {"table": 0, "rowCnt": 1, "colCnt": 2, "cells": [
            {"row": 0, "col": 0, "paragraphs": ["한 문단"]},
            {"row": 0, "col": 1, "paragraphs": ["문단1", "문단2"], "multiParagraph": True},
        ]}]}


case(
    "multiparagraph_cell.hwpx", "table",
    "1행 2열 표. (0,0) 은 문단 하나, (0,1) 은 문단 둘. cell.setText 는 문단이 둘 이상인 셀을 UNSUPPORTED(cell_paragraph_count) 로 거절한다. 조회 텍스트는 셀 문단들을 '\\n' 로 잇는다.",
    c_multiparagraph_cell, k_multiparagraph_cell,
    [
        {"op": "cell.setText", "target": {"table": 0, "row": 0, "col": 0},
         "args": {"text": "바꿈"}, "expect": "applied",
         "sourceText": "한 문단", "resultText": "바꿈", "diagnostics": []},
        {"op": "cell.setText", "target": {"table": 0, "row": 0, "col": 1},
         "args": {"text": "바꿈"}, "expect": "reject",
         "code": "UNSUPPORTED", "reason": "cell_paragraph_count"},
        {"op": "element.query", "args": {"table": 0, "row": 0, "col": 1}, "expect": "ok",
         "matches": [{"table": 0, "row": 0, "col": 1}], "unalignedMatches": 0},
    ])


# ===== 11. 중첩 표 =====
def c_nested_table():
    inner = _tbl(1, 1, [_tr([_tc(0, 0, 1, 1, [P(R(T("안쪽")))])])])
    # 바깥 셀은 문단 하나이고 그 안에 중첩 표가 있다. 문단이 하나여야
    # cell_block_reason 이 cell_paragraph_count 보다 nested_table 을 먼저 낸다.
    outer = _tbl(1, 1, [_tr([_tc(0, 0, 1, 1, [P(R(T("바깥") + inner))])])])
    return build(SEC(P(R(outer))))


def k_nested_table(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ts = tables_of(sec)
    eq(len(ts), 2, "표 개수(바깥+안쪽)")
    eq(ts[0]["cells"][0]["paragraphs"], ["바깥" + OBJ], "바깥 셀 문단(중첩 표는 객체 자리)")
    eq(ts[1]["cells"][0]["paragraphs"], ["안쪽"], "안쪽 셀 문단")
    return {"entry": "Contents/section0.xml", "tables": [
        {"table": 0, "cells": [{"row": 0, "col": 0, "paragraphs": ["바깥" + OBJ], "containsNestedTable": True}]},
        {"table": 1, "cells": [{"row": 0, "col": 0, "paragraphs": ["안쪽"]}]},
    ]}


case(
    "nested_table.hwpx", "table",
    "바깥 표의 한 셀 안에 문단과 중첩 표가 있다. 중첩 표를 담은 셀은 cell.setText 를 UNSUPPORTED(nested_table) 로 거절한다. (table,row,col) 조회는 가장 가까운 표 조상을 기준으로 하므로 바깥 표 조회에 안쪽 셀이 섞이지 않는다.",
    c_nested_table, k_nested_table,
    [
        {"op": "cell.setText", "target": {"table": 0, "row": 0, "col": 0},
         "args": {"text": "바꿈"}, "expect": "reject",
         "code": "UNSUPPORTED", "reason": "nested_table"},
        {"op": "element.query", "args": {"table": 0, "row": 0, "col": 0}, "expect": "ok",
         "matches": [{"table": 0, "row": 0, "col": 0}], "unalignedMatches": 0},
        {"op": "element.query", "args": {"table": 1, "row": 0, "col": 0}, "expect": "ok",
         "matches": [{"table": 1, "row": 0, "col": 0}], "unalignedMatches": 0},
        {"op": "element.query", "args": {"kind": "table"}, "expect": "ok",
         "matches": [{"table": 0}, {"table": 1}], "unalignedMatches": 0},
    ])


# ===== 12. 해석 불가 요소/속성 =====
def c_opaque():
    body = (
        P(R(T("locked")), ' id="7"')
        + P(R(T("free")), ' id="8"')
        + P(R('<hp:t x:keep="preserve">attr</hp:t>'), ' id="9"')
        + P(R(T("has") + "<x:inline/>"), ' id="10"')
        + '<x:reference target="7"/>'
        + "<x:box><x:inner/></x:box>"
    )
    return build(SEC(body, extra_ns=' xmlns:x="urn:example:opaque"'))


def k_opaque(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ds = para_descs(sec)
    eq([d["text"] for d in ds], ["locked", "free", "attr", "has" + OBJ], "opaque 문단 텍스트")
    for frag in [b'<x:reference target="7"/>', b"<x:box>", b"<x:inner/>", b"<x:inline/>",
                 b'x:keep="preserve"']:
        in_bytes(f, "Contents/section0.xml", frag, "opaque 요소/속성")
    return {"entry": "Contents/section0.xml",
            "paragraphs": [{"index": i, "text": d["text"]} for i, d in enumerate(ds)],
            "expectedLocks": {
                "Contents/section0.xml#id7": "opaque_reference:<nodeId>",
                "Contents/section0.xml#id10": "contains_opaque",
            },
            "opaqueElements": ["x:reference", "x:inline", "x:box"],
            "documentEditable": True}


case(
    "opaque_attrs.hwpx", "opaque",
    "외부 namespace(urn:example:opaque) 요소 x:reference 가 hp:p id=7 을, x:inline 이 문단 id=10 을 참조/포함한다. 해석 불가 요소는 수용하되 그 대상을 잠근다(각각 opaque_reference:<nodeId>, contains_opaque). hp:t 의 x:keep 같은 미해석 속성은 그 hp:t 를 덮는 text.replace 를 UNSUPPORTED_CONTENT_IN_RANGE 로 거절한다. 해석 불가 요소가 있다고 문서 전체가 읽기 전용이 되지는 않는다.",
    c_opaque, k_opaque,
    [
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 1], "text": "Q"}, "expect": "reject",
         "code": "UNSUPPORTED", "reasonPrefix": "opaque_reference:"},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 3},
         "args": {"range": [0, 1], "text": "Q"}, "expect": "reject",
         "code": "UNSUPPORTED", "reason": "contains_opaque"},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 2},
         "args": {"range": [0, 1], "text": "Q"}, "expect": "reject",
         "code": "UNSUPPORTED_CONTENT_IN_RANGE", "reason": None},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 1},
         "args": {"range": [0, 1], "text": "Q"}, "expect": "applied",
         "sourceText": "free", "resultText": "Qree", "diagnostics": []},
        {"op": "element.query", "args": {"kind": "opaque"}, "expect": "ok",
         "matches": [{"opaque": 0}, {"opaque": 1}, {"opaque": 2}], "unalignedMatches": 0},
    ])


# ===== 13. 공유 미디어 =====
def c_shared_media():
    body = (P(R(pic(1, "image1"))) + P(R(pic(2, "image1"))) + P(R(pic(3, "image2"))))
    extra = [("BinData/image1.png", png(2, 1), 0), ("BinData/image2.png", png(2, 1), 0)]
    return build(SEC(body), hpf_bytes=hpf_sec(
        [("image1", "BinData/image1.png", "image/png"),
         ("image2", "BinData/image2.png", "image/png")]), extra=extra)


def k_shared_media(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    refs = pictures_of(sec)
    eq(refs, ["image1", "image1", "image2"], "그림 참조")
    assert "BinData/image1.png" in f and "BinData/image2.png" in f
    return {"entry": "Contents/section0.xml",
            "pictures": [{"index": 0, "media": "image1"}, {"index": 1, "media": "image1"},
                         {"index": 2, "media": "image2"}],
            "sharedMedia": "image1",
            "mediaEntries": ["BinData/image1.png", "BinData/image2.png"]}


case(
    "shared_media.hwpx", "media",
    "그림 3개가 미디어 2개를 공유한다(그림0·그림1 -> image1, 그림2 -> image2). image.replace 는 대상 그림의 참조만 새 미디어 항목으로 바꾸므로 공유하던 다른 그림과 원래 BinData 는 그대로 남는다.",
    c_shared_media, k_shared_media,
    [
        {"op": "image.replace", "target": {"entry": "Contents/section0.xml", "picture": 0},
         "args": {"image": "inputs/ok.jpg"}, "expect": "applied",
         "resourceChanges": [{"kind": "media", "action": "added"},
                             {"kind": "media", "action": "referenced"}],
         "preservedEntries": ["BinData/image1.png", "BinData/image2.png"],
         "preservedRefs": [{"picture": 1, "media": "image1"}, {"picture": 2, "media": "image2"}],
         "note": "새 미디어 항목을 끝에 추가하고 manifest 참조를 갱신. 위치·크기·캡션·캐시 불변."},
        {"op": "image.replace", "target": {"entry": "Contents/section0.xml", "picture": 2},
         "args": {"image": "inputs/ok.png"}, "expect": "applied",
         "resourceChanges": [{"kind": "media", "action": "added"},
                             {"kind": "media", "action": "referenced"}],
         "preservedRefs": [{"picture": 0, "media": "image1"}, {"picture": 1, "media": "image1"}]},
    ])


# ===== 14. 자른 그림 =====
def c_cropped():
    cropped = pic(1, "image1", '<hp:imgClip left="10" top="0" right="1440" bottom="720"/>')
    full = pic(2, "image1", '<hp:imgClip left="0" top="0" right="1440" bottom="720"/>')
    extra = [("BinData/image1.png", png(2, 1), 0)]
    return build(SEC(P(R(cropped)) + P(R(full))),
                 hpf_bytes=hpf_sec([("image1", "BinData/image1.png", "image/png")]), extra=extra)


def k_cropped(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    eq(pictures_of(sec), ["image1", "image1"], "그림 참조")
    in_bytes(f, "Contents/section0.xml", b'<hp:imgClip left="10"', "자른 그림 표시")
    in_bytes(f, "Contents/section0.xml", b'<hp:imgClip left="0" top="0" right="1440" bottom="720"/>',
             "클립이 그림 전체와 같은 경우")
    return {"entry": "Contents/section0.xml",
            "pictures": [{"index": 0, "media": "image1", "cropped": True},
                         {"index": 1, "media": "image1", "cropped": False}],
            "imgDim": [1440, 720]}


case(
    "cropped_picture.hwpx", "media",
    "그림0 은 imgClip left=10 으로 잘려 있고, 그림1 은 클립(0,0,1440,720)이 그림 크기와 같아 자르기로 보지 않는다. 자르기가 있는 그림은 image.replace 를 UNSUPPORTED(cropped) 로 거절한다.",
    c_cropped, k_cropped,
    [
        {"op": "image.replace", "target": {"entry": "Contents/section0.xml", "picture": 0},
         "args": {"image": "inputs/ok.png"}, "expect": "reject",
         "code": "UNSUPPORTED", "reason": "cropped"},
        {"op": "image.replace", "target": {"entry": "Contents/section0.xml", "picture": 1},
         "args": {"image": "inputs/ok.png"}, "expect": "applied",
         "resourceChanges": [{"kind": "media", "action": "added"},
                             {"kind": "media", "action": "referenced"}]},
    ])


# ===== 15. 효과 그림 =====
def c_effects():
    glow = pic(1, "image1", "<hp:effects><hp:glow/></hp:effects>")
    gray = ('<hp:pic id="2"><hp:sz width="1440" height="720"/><hp:pos/>'
            '<hp:imgDim dimwidth="1440" dimheight="720"/>'
            '<hc:img binaryItemIDRef="image1" effect="GRAY_SCALE"/></hp:pic>')
    bright = ('<hp:pic id="3"><hp:sz width="1440" height="720"/><hp:pos/>'
              '<hp:imgDim dimwidth="1440" dimheight="720"/>'
              '<hc:img binaryItemIDRef="image1" bright="5"/></hp:pic>')
    clean = pic(4, "image1")
    extra = [("BinData/image1.png", png(2, 1), 0)]
    return build(SEC(P(R(glow)) + P(R(gray)) + P(R(bright)) + P(R(clean))),
                 hpf_bytes=hpf_sec([("image1", "BinData/image1.png", "image/png")]), extra=extra)


def k_effects(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    eq(pictures_of(sec), ["image1"] * 4, "그림 참조")
    for frag in [b"<hp:effects>", b'effect="GRAY_SCALE"', b'bright="5"']:
        in_bytes(f, "Contents/section0.xml", frag, "효과 표시")
    return {"entry": "Contents/section0.xml",
            "pictures": [{"index": 0, "effects": ["effects"]},
                         {"index": 1, "effects": ["effect!=REAL_PIC"]},
                         {"index": 2, "effects": ["bright!=0"]},
                         {"index": 3, "effects": []}]}


case(
    "effects_picture.hwpx", "media",
    "그림0 은 hp:effects(glow), 그림1 은 hc:img effect=GRAY_SCALE, 그림2 는 bright=5 로 효과가 있다. 효과가 있는 그림은 image.replace 를 UNSUPPORTED(effects) 로 거절한다. 그림3 은 효과가 없어 바꿀 수 있다.",
    c_effects, k_effects,
    [
        {"op": "image.replace", "target": {"entry": "Contents/section0.xml", "picture": i},
         "args": {"image": "inputs/ok.png"}, "expect": "reject",
         "code": "UNSUPPORTED", "reason": "effects"} for i in range(3)
    ] + [
        {"op": "image.replace", "target": {"entry": "Contents/section0.xml", "picture": 3},
         "args": {"image": "inputs/ok.png"}, "expect": "applied",
         "resourceChanges": [{"kind": "media", "action": "added"},
                             {"kind": "media", "action": "referenced"}]},
    ])


# ===== 16. 수식 =====
def c_equation():
    body = P(R(T("앞 ") + '<hp:equation version="1.0"><hp:script>a^2+b^2=c^2</hp:script></hp:equation>' + T(" 뒤")))
    return build(SEC(body))


def k_equation(b):
    f = read_all(b)
    sec = f["Contents/section0.xml"]
    ds = para_descs(sec)
    eq([d["text"] for d in ds], ["앞 " + OBJ + " 뒤"], "수식 자리 표시자 텍스트")
    eq(len(list(ET.fromstring(sec).iter(HP + "equation"))), 1, "수식 개수")
    in_bytes(f, "Contents/section0.xml", b"a^2+b^2=c^2", "수식 스크립트 보존")
    return {"entry": "Contents/section0.xml",
            "paragraphs": [{"index": 0, "text": "앞 " + OBJ + " 뒤", "graphemes": 5}],
            "equations": 1, "equationEditable": False,
            "notEditableReason": "equation_edit_unsupported"}


case(
    "equation.hwpx", "object",
    "문단 안에 hp:equation(수식)이 있다. 0.1 은 수식을 조회·보존만 하고 편집하지 않는다(조회 결과 editable=false, equation_edit_unsupported). 수식을 지나는 text.replace 는 RANGE_CROSSES_CONTROL, 그 앞 텍스트만 덮으면 적용된다.",
    c_equation, k_equation,
    [
        {"op": "element.query", "args": {"kind": "equation"}, "expect": "ok",
         "matches": [{"equation": 0, "editable": False,
                      "notEditableReason": "equation_edit_unsupported"}], "unalignedMatches": 0},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 5], "text": "Q"}, "expect": "reject",
         "code": "RANGE_CROSSES_CONTROL", "reason": None},
        {"op": "text.replace", "target": {"entry": "Contents/section0.xml", "paragraph": 0},
         "args": {"range": [0, 2], "text": "Q"}, "expect": "applied",
         "sourceText": "앞 " + OBJ + " 뒤", "resultText": "Q" + OBJ + " 뒤", "diagnostics": []},
    ])


# ===== 17. 문단 서식 =====
HDR_PARA = ('%s<hh:head %s version="1.2" secCnt="1"><hh:refList><hh:paraProperties itemCnt="1">'
            '<hh:paraPr id="0" x:keep="preserve"><hh:align horizontal="LEFT" vertical="BASELINE"/>'
            '<hh:lineSpacing type="PERCENT" value="160"/><x:untouched>opaque</x:untouched>'
            '</hh:paraPr></hh:paraProperties></hh:refList></hh:head>'
            % (DECL, root_ns(("hh", "hp", "hs"), extra=' xmlns:x="urn:opaque"'))).encode("utf-8")


def c_para_format():
    return build(SEC(P(R(T("첫째"))) + P(R(T("둘째"))) + P(R(T("셋째")))), header=HDR_PARA)


def k_para_format(b):
    f = read_all(b)
    h = f["Contents/header.xml"]
    eq(h.count(b"<hh:paraPr "), 1, "paraPr 개수")
    in_bytes(f, "Contents/header.xml", b'itemCnt="1"', "itemCnt")
    in_bytes(f, "Contents/header.xml", b'x:keep="preserve"', "해석 불가 속성 보존")
    in_bytes(f, "Contents/header.xml", b"<x:untouched>opaque</x:untouched>", "해석 불가 자식 보존")
    sec = f["Contents/section0.xml"]
    eq([d["text"] for d in para_descs(sec)], ["첫째", "둘째", "셋째"], "문단 텍스트")
    return {"entry": "Contents/section0.xml", "header": "Contents/header.xml",
            "headerParaPr": 1, "headerItemCnt": 1,
            "paragraphs": ["첫째", "둘째", "셋째"],
            "paraPrIDRef": ["0", "0", "0"]}


case(
    "para_format.hwpx", "format",
    "문단 3개가 모두 paraPrIDRef=0 을 쓴다. 헤더 paraPr 에는 해석 불가 속성(x:keep)과 자식(x:untouched)이 있다. paragraph.format 은 문단 모양을 복제할 때 이런 원문을 그대로 옮기고, 같은 값의 모양이 이미 있으면 새로 만들지 않고 재사용한다(resourceChanges action=reused).",
    c_para_format, k_para_format,
    [
        {"op": "paragraph.format", "target": {"entry": "Contents/section0.xml", "paragraph": 1},
         "args": {"alignment": "CENTER"}, "expect": "applied",
         "resourceChanges": [{"kind": "paraPr", "resourceId": "1", "action": "added"}],
         "note": "새 paraPr id=1 을 헤더에 추가(itemCnt 1->2). 다른 문단의 paraPrIDRef 는 그대로."},
        {"op": "command.batch",
         "steps": [
             {"op": "paragraph.format", "target": {"entry": "Contents/section0.xml", "paragraph": 1},
              "args": {"alignment": "CENTER"}},
             {"op": "paragraph.format", "target": {"entry": "Contents/section0.xml", "paragraph": 2},
              "args": {"alignment": "CENTER"}},
         ], "expect": "applied",
         "resourceChanges": [{"kind": "paraPr", "resourceId": "1", "action": "added"},
                             {"kind": "paraPr", "resourceId": "1", "action": "reused"}],
         "changed": [{"paragraph": 1}, {"paragraph": 2}],
         "note": "두 번째 문단은 이미 만들어 둔 같은 모양을 재사용한다."},
        {"op": "paragraph.format", "target": {"entry": "Contents/section0.xml", "paragraph": 1},
         "args": {"alignment": "CENTER", "lineSpacing": {"kind": "percent", "value": 200}},
         "expect": "applied",
         "resourceChanges": [{"kind": "paraPr", "resourceId": "1", "action": "added"}],
         "note": "정렬과 퍼센트 줄간격을 함께 바꾼다."},
    ])


# ------------------------------------------------------------------ 교체 이미지 입력

def image_inputs():
    out = {}
    out["ok.png"] = {"bytes": png(2, 1), "expect": "accept", "format": "png", "dimensions": [2, 1],
                     "notes": "합성 정적 PNG(2x1 RGB)."}
    out["ok.jpg"] = {"bytes": JPEG, "expect": "accept", "format": "jpeg", "dimensions": [1, 1],
                     "notes": "자체 생성 1x1 grayscale baseline JPEG."}
    out["huge_ihdr.png"] = {"bytes": huge_ihdr_png(), "expect": "reject", "code": "RESOURCE_LIMIT",
                            "reason": None, "notes": "IHDR 100000x100000 = 10^10 화소 > 40,960,000. 디코딩 전에 거절."}
    out["huge_sof.jpg"] = {"bytes": huge_sof_jpeg(), "expect": "reject", "code": "RESOURCE_LIMIT",
                           "reason": None, "notes": "SOF0 65535x65535 화소가 한도를 넘는다."}
    out["apng.png"] = {"bytes": apng(), "expect": "reject", "code": "UNSUPPORTED",
                       "reason": "animated_image", "notes": "acTL 청크가 있는 APNG(프레임 1개라도 거절)."}
    out["truncated.png"] = {"bytes": png(2, 1)[:-4], "expect": "reject", "code": "UNSUPPORTED",
                            "reason": "invalid_image", "notes": "마지막 4바이트를 잘라 IEND/CRC 가 깨진 PNG."}
    out["truncated.jpg"] = {"bytes": JPEG[:-2], "expect": "reject", "code": "UNSUPPORTED",
                            "reason": "invalid_image", "notes": "EOI(FF D9) 를 지운 JPEG."}
    out["not_image.gif"] = {"bytes": b"GIF89a\x01\x00\x01\x00\x00\x00", "expect": "reject",
                            "code": "UNSUPPORTED", "reason": "unsupported_image_format",
                            "notes": "PNG/JPEG 서명이 아닌 바이트."}
    return out


def check_image_input(name, meta):
    b = meta["bytes"]
    if name.startswith("ok"):
        assert meta["expect"] == "accept"
    if name == "huge_ihdr.png":
        w, h = struct.unpack(">II", b[16:24])
        assert w * h > LIMITS["max_image_pixels"], "거대 PNG 화소 assert"
    if name == "huge_sof.jpg":
        i = b.find(b"\xff\xc0")
        h, w = struct.unpack(">HH", b[i + 5:i + 9])
        assert h * w > LIMITS["max_image_pixels"], "거대 JPEG 화소 assert"
    if name == "apng.png":
        assert b.find(b"acTL") > 0, "acTL 없음"
    if name == "truncated.png":
        assert png(2, 1).startswith(b) and b != png(2, 1), "truncated.png 가 원본 접두어가 아님"
    if name == "truncated.jpg":
        assert JPEG.startswith(b) and not b.endswith(b"\xff\xd9"), "truncated.jpg 가 EOI 없음이 아님"
    if name == "not_image.gif":
        assert not (b.startswith(b"\x89PNG") or b.startswith(b"\xff\xd8")), "이미지 서명 assert"


# ------------------------------------------------------------------ 실행

def run(outdir, expected_path):
    os.makedirs(outdir, exist_ok=True)
    inputs_dir = os.path.join(outdir, "inputs")
    os.makedirs(inputs_dir, exist_ok=True)

    cases_out = {}
    total = 0
    for c in CASES:
        data = c["build"]()
        files = read_all(data)  # zip 무결성·mimetype·XML well-formed assert
        check_package(files)
        structure = c["check"](data)
        if c["structure"] is not None:
            structure = c["structure"](data)
        for op in c["operations"]:
            validate_op(op)
        out = os.path.join(outdir, c["file"])
        with open(out, "wb") as fh:
            fh.write(data)
        total += len(data)
        cases_out[c["file"]] = {
            "category": c["category"], "expect": "accept", "notes": c["notes"],
            "bytes": len(data), "structure": structure, "operations": c["operations"],
        }

    inputs_out = {}
    for name, meta in image_inputs().items():
        check_image_input(name, meta)
        with open(os.path.join(inputs_dir, name), "wb") as fh:
            fh.write(meta["bytes"])
        total += len(meta["bytes"])
        entry = {k: v for k, v in meta.items() if k != "bytes"}
        entry["bytes"] = len(meta["bytes"])
        inputs_out["inputs/" + name] = entry

    doc = {
        "schema": 1,
        "plan": "1-2b",
        "base": "tests/fixtures/adversarial/valid_base.hwpx (패키지 골격만 차용, payload 는 전부 합성)",
        "provenance": "전부 합성. 개인 문서·제3자 자산 없음. 자료_원본 사본과 다른 시험 자료는 건드리지 않음.",
        "notes": "structure 는 생성 시점에 파서로 계산한 관찰 값. operations 의 applied 기대 resultText 는 "
                 "자소 군집 oracle(simulate_replace)로 검산한다. reject 는 code(와 reason/reasonPrefix)를 고정한다. "
                 "target 은 무작위 nodeId 가 아니라 구조 선택자(entry/paragraph/table·row·col/picture/equation)다.",
        "targetSelectors": {
            "paragraph": "entry 안 최상위 hp:p 의 0부터 세는 순번",
            "table+row+col": "표 안 셀 주소(hp:cellAddr rowAddr·colAddr, 0부터)",
            "picture": "entry 안 hp:pic 의 0부터 세는 순번",
            "equation": "entry 안 hp:equation 의 0부터 세는 순번",
        },
        "limits": LIMITS,
        "codes": {
            "applied": "변경 적용(revision +1)",
            "UNSUPPORTED": "지원하지 않는 변경·문서 조건. reason 참조",
            "INVALID_REQUEST": "요청 형식 오류(새 텍스트의 탭·줄바꿈 등)",
            "INVALID_RANGE": "길이 0 이거나 문단 밖 범위",
            "RANGE_CROSSES_CONTROL": "범위가 탭·줄바꿈·필드·개체 경계를 지남",
            "UNSUPPORTED_CONTENT_IN_RANGE": "범위 안 텍스트 외 자식·해석 불가 속성",
            "RESOURCE_LIMIT": "한도 초과(화소 등)",
        },
        "imageInputReasons": {
            "unsupported_image_format": "PNG/JPEG 서명이 아님",
            "invalid_image": "잘리거나 CRC·구조가 깨진 PNG/JPEG",
            "animated_image": "APNG(acTL/fcTL/fdAT). 프레임 1개라도 거절",
            "multiframe_image": "MPO 등 다중 프레임 JPEG",
            None: "화소 한도 초과는 RESOURCE_LIMIT(별도 reason 없음)",
        },
        "cases": cases_out,
        "inputs": inputs_out,
    }
    path = expected_path or os.path.join(outdir, "editing_expected.json")
    payload = json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=False) + "\n"
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(payload)
    assert total < 2 << 20, "합성 자료 총 크기 초과: %d" % total
    print("wrote %d cases + %d image inputs, %d bytes total -> %s"
          % (len(cases_out), len(inputs_out), total, outdir))
    return payload


def validate_op(op):
    """applied text/cell op 의 resultText 를 자소 oracle 로 검산한다."""
    if op.get("op") in ("text.replace", "cell.setText") and op.get("expect") == "applied":
        src = op["sourceText"]
        if op["op"] == "cell.setText":
            result = op["args"]["text"]
        else:
            result = simulate_replace(src, op["args"]["range"], op["args"]["text"])
        eq(result, op["resultText"], "oracle %s %r" % (op.get("op"), op.get("args")))
        # sourceText 가 실제 관찰 텍스트와 같은지는 case check 가 확인한다.
        cp = None if op["op"] == "cell.setText" else cp_range_of(src, *op["args"]["range"])
        if cp is not None:
            assert cp is not None and cp[0] < cp[1] or op["args"]["range"][0] < op["args"]["range"][1]


def check_determinism():
    a = run_to_bytes()
    b = run_to_bytes()
    eq(a, b, "두 번 생성 결과")
    print("determinism OK (%d bytes)" % len(a))


def run_to_bytes():
    global CASES
    blobs = []
    for c in CASES:
        blobs.append(c["build"]())
        for op in c["operations"]:
            validate_op(op)
    for name, meta in sorted(image_inputs().items()):
        blobs.append(meta["bytes"])
    return b"".join(blobs)


def main(argv):
    args = [a for a in argv[1:] if not a.startswith("--")]
    expected = None
    if "--expected" in argv:
        expected = argv[argv.index("--expected") + 1]
        args = [a for a in args if a != expected]
    if "--check-determinism" in argv:
        check_determinism()
        return 0
    if len(args) != 1:
        sys.stderr.write(__doc__)
        return 2
    run(args[0], expected)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
