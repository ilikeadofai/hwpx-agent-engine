#!/usr/bin/env python3
"""S2 스파이크: HWPX XML 구조 경로 <-> rhwp 레이아웃 주소 대조.

실행:  python3 -I spikes/s2_mapping/map_paths.py <file.hwpx>... [--json out.json] [--rhwp <bin>] [-v]

표준 라이브러리만 사용한다. rhwp 는 서브프로세스로 호출한다.
  - rhwp export-render-tree <file> -o <tmp>   (페이지별 render_tree_NNN.json)
  - rhwp dump-pages <file> --json            (페이지 -> section 번호, 최상위 문단/표/개체 항목)

우리 쪽 키  : (ZIP 항목, 구조 경로 = 부모의 요소 자식 중 서수들의 열, 요소 종류)
rhwp 쪽 주소: ((S,si),(p,pi),(c,ci),(cell,row,col),(p,pi),(c,ci) ...)
  S    = 구역 번호 (content.hpf spine 순서)
  p    = 문단 번호 (최상위: 구역 XML 루트의 hp:p 서수, 셀 안: 셀 hp:subList 의 hp:p 서수)
  c    = 문단 안 컨트롤 번호 (rhwp 파서가 Paragraph.controls 에 push 하는 순서를 재현)
  cell = 표 셀 (hp:cellAddr 의 rowAddr, colAddr)
두 쪽에서 같은 규칙으로 주소를 만들어 조인한다.
"""
import argparse
import collections
import glob
import json
import os
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_RHWP = os.path.normpath(
    os.path.join(HERE, "..", "vendor", "rhwp-src", "target", "release", "rhwp"))
SENTINEL = 18446744073709551615  # usize::MAX, 머리말/꼬리말 빈 줄 등


def local(tag):
    return tag.rsplit("}", 1)[-1] if isinstance(tag, str) else ""


# ---------------------------------------------------------------- 우리 쪽 (XML)

# rhwp 파서(parse_paragraph_body)가 문단 controls 에 push 하는 문단-직속 개체
OBJ_KINDS = {
    "tbl", "pic", "rect", "ellipse", "line", "connectLine", "arc", "polygon",
    "curve", "container", "compose", "dutmal", "equation", "btn", "checkBtn",
    "radioBtn", "comboBox", "edit", "ole", "chart",
}
# <hp:ctrl> 자식 중 controls 에 push 되는 것 (fieldEnd 는 push 하지 않는다)
CTRL_CHILDREN = {
    "colPr", "header", "footer", "footNote", "endNote", "autoNum",
    "hiddenComment", "indexmark", "fieldBegin", "pageHiding", "pageNumCtrl",
    "pageNum", "bookmark", "newNum",
}
SHAPE_KINDS = {"rect", "ellipse", "line", "connectLine", "arc", "polygon",
               "curve", "container", "ole", "chart"}
TRACKED = {"p", "tbl", "tc", "pic", "equation"} | SHAPE_KINDS


def emulate_controls(p):
    """hp:p 의 Paragraph.controls 순서를 rhwp 파서 규칙대로 재현. [(elem, kind)]"""
    out = []

    def rec(el):
        for ch in el:
            n = local(ch.tag)
            if n == "secPr":
                out.append((ch, "secPr"))            # SectionDef
                if any(local(x.tag) == "colPr" for x in ch.iter()):
                    out.append((ch, "colPr@secPr"))  # secPr 안 colPr -> ColumnDef 1개 추가
                continue
            if n == "ctrl":
                for cc in ch:
                    cn = local(cc.tag)
                    if cn in CTRL_CHILDREN:
                        out.append((cc, cn))
                continue
            if n == "switch":
                inner = [x for x in ch.iter() if local(x.tag) in ("chart", "ole")]
                if inner:
                    out.append((inner[0], local(inner[0].tag)))
                continue
            if n in OBJ_KINDS:
                out.append((ch, n))
                continue
            if n in ("t", "linesegarray"):
                continue
            rec(ch)

    rec(p)
    return out


def para_text(p):
    """문단 직속 텍스트 (개체 안 문단 제외)."""
    parts = []

    def rec(el):
        for ch in el:
            n = local(ch.tag)
            if n == "t":
                parts.append("".join(ch.itertext()))
            elif n in ("linesegarray", "ctrl", "switch", "secPr") or n in OBJ_KINDS:
                continue
            else:
                rec(ch)

    rec(p)
    return "".join(parts)


def read_package(z):
    """content.hpf 에서 rhwp 규칙(spine 순서, media-type=application/xml, href 에 'section')으로
    구역 목록을 만든다. 반환: (rhwp 순서, 파일명 사전순, spine 에 없는 section 항목)"""
    hpf = ET.fromstring(z.read("Contents/content.hpf"))
    items, spine = {}, []
    for el in hpf.iter():
        n = local(el.tag)
        if n == "item":
            items[el.get("id")] = (el.get("href"), el.get("media-type"))
        elif n == "itemref":
            spine.append(el.get("idref"))
    spine_secs = [items[i][0] for i in spine
                  if i in items and items[i][1] == "application/xml" and "section" in items[i][0]]
    manifest_secs = [h for (h, m) in items.values() if m == "application/xml" and "section" in h]
    rhwp_order = spine_secs if spine_secs else sorted(manifest_secs)
    return rhwp_order, sorted(manifest_secs), sorted(set(manifest_secs) - set(spine_secs))


class Rec:
    __slots__ = ("entry", "path", "kind", "addr", "reason", "attrs")

    def __init__(self, entry, path, kind, addr, reason, attrs):
        self.entry, self.path, self.kind = entry, path, kind
        self.addr, self.reason, self.attrs = addr, reason, attrs

    @property
    def key(self):
        return "%s#/%s:%s" % (self.entry, "/".join(map(str, self.path)), self.kind)


def build_our_side(z, rhwp_order):
    recs = []
    notes = collections.Counter()
    for si, entry in enumerate(rhwp_order):
        raw = z.read(entry)
        notes["selfclosing_hp_p"] += len(re.findall(rb"<hp:p\b[^>]*/>", raw))
        root = ET.fromstring(raw)
        paths = {}

        def assign(el, path):
            paths[el] = path
            for i, ch in enumerate(el):
                assign(ch, path + (i,))

        assign(root, ())

        def add(el, kind, addr, reason=None, **attrs):
            r = Rec(entry, paths[el], kind, addr, reason, attrs)
            recs.append(r)
            return r

        def unaddressed(el, reason, anchor):
            """주소를 얻을 수 없는 컨테이너 내부: 추적 대상 요소만 기록. anchor = 가장 가까운 주소 있는 조상 컨트롤."""
            for ch in el:
                n = local(ch.tag)
                if n == "p":
                    do_para(ch, None, reason, anchor)
                elif n in TRACKED:
                    add(ch, n, None, reason, anchor=anchor)
                    unaddressed(ch, reason, anchor)
                else:
                    unaddressed(ch, reason, anchor)

        def do_para(p, addr, reason, anchor=None):
            add(p, "p", addr, reason, text=para_text(p), anchor=anchor)
            for ci, (el, kind) in enumerate(emulate_controls(p)):
                caddr = addr + (("c", ci),) if addr is not None else None
                if kind in TRACKED:
                    attrs = {}
                    if kind == "tbl":
                        attrs = {"rows": int(el.get("rowCnt", -1)), "cols": int(el.get("colCnt", -1))}
                    add(el, kind, caddr, reason, anchor=anchor, **attrs)
                else:
                    add(el, "ctl:" + kind, caddr, reason, anchor=anchor)
                if kind == "tbl":
                    for tr in el:
                        if local(tr.tag) != "tr":
                            unaddressed(tr, "caption/other", caddr if caddr is not None else anchor)
                            continue
                        for tc in tr:
                            if local(tc.tag) != "tc":
                                continue
                            ca = next((x for x in tc if local(x.tag) == "cellAddr"), None)
                            cell_addr = None
                            if caddr is not None and ca is not None:
                                cell_addr = caddr + (("cell", int(ca.get("rowAddr")), int(ca.get("colAddr"))),)
                            rs = reason if cell_addr is None else None
                            add(tc, "tc", cell_addr, rs, anchor=anchor)
                            sub = next((x for x in tc if local(x.tag) == "subList"), None)
                            if sub is not None:
                                k = 0
                                for pp in sub:
                                    if local(pp.tag) == "p":
                                        do_para(pp, cell_addr + (("p", k),) if cell_addr is not None else None, rs, anchor)
                                        k += 1
                elif kind in ("header", "footer", "footNote", "endNote", "hiddenComment"):
                    unaddressed(el, "in-" + kind, caddr if caddr is not None else anchor)
                elif kind in SHAPE_KINDS or kind in ("compose", "dutmal", "equation", "pic"):
                    unaddressed(el, "in-" + kind, caddr if caddr is not None else anchor)

        pi = 0
        for ch in root:
            if local(ch.tag) == "p":
                do_para(ch, (("S", si), ("p", pi)), None)
                pi += 1
            else:
                notes["non_p_child_of_sec"] += 1
    return recs, notes


# ---------------------------------------------------------------- rhwp 쪽

def run_rhwp(rhwp, path, workdir):
    rt_dir = os.path.join(workdir, "rt")
    os.makedirs(rt_dir, exist_ok=True)
    r1 = subprocess.run([rhwp, "export-render-tree", path, "-o", rt_dir],
                        capture_output=True, text=True, timeout=600)
    r2 = subprocess.run([rhwp, "dump-pages", path, "--json"],
                        capture_output=True, text=True, timeout=600)
    if r1.returncode != 0 or r2.returncode != 0:
        raise RuntimeError("rhwp 실패: %s / %s" % (r1.stderr[-300:], r2.stderr[-300:]))
    pages_doc = json.loads(r2.stdout)
    files = sorted(glob.glob(os.path.join(rt_dir, "render_tree_*.json")),
                   key=lambda f: int(re.search(r"(\d+)\.json$", f).group(1)))
    trees = []
    for f in files:
        with open(f, encoding="utf-8") as fh:
            trees.append(json.load(fh))
    return trees, pages_doc


class Item:
    __slots__ = ("page", "kind", "addr", "zone", "extra")

    def __init__(self, page, kind, addr, zone, **extra):
        self.page, self.kind, self.addr, self.zone, self.extra = page, kind, addr, zone, extra


def walk_render(tree, page, sec, items):
    """render tree 를 내려가며 (kind, addr) 항목을 수집.
    base 가 None 이면 주소를 만들 수 없는 문맥(머리말/꼬리말/각주/도형 내부 등)."""

    def valid(v):
        return isinstance(v, int) and v != SENTINEL

    def rec(n, base, zone):
        t = n.get("type")
        kids = n.get("children", [])
        if t == "Page":
            for c in kids:
                rec(c, (("S", sec),), "page-layer")
            return
        if t in ("Header", "Footer", "MasterPage", "FootnoteArea"):
            for c in kids:
                rec(c, None, t.lower())
            return
        if t == "Body":
            for c in kids:
                rec(c, (("S", sec),), "body")
            return
        if t == "Column":
            for c in kids:
                rec(c, base, zone)
            return
        if t == "Table":
            pi, ci = n.get("pi"), n.get("ci")
            addr = base + (("p", pi), ("c", ci)) if base is not None and valid(pi) and valid(ci) else None
            items.append(Item(page, "tbl", addr, zone, rows=n.get("rows"), cols=n.get("cols")))
            for c in kids:
                if c.get("type") == "Cell":
                    cb = addr + (("cell", c.get("row"), c.get("col")),) if addr is not None else None
                    items.append(Item(page, "tc", cb, zone))
                    for cc in c.get("children", []):
                        rec(cc, cb, zone)
                else:
                    rec(c, None, zone + "/tbl-extra")
            return
        if t == "TextLine":
            pi = n.get("pi")
            if base is not None and valid(pi):
                items.append(Item(page, "p", base + (("p", pi),), zone))
            for c in kids:
                rec(c, base, zone)
            return
        if t == "TextRun":
            return
        if t == "Image":
            pi, ci = n.get("pi"), n.get("ci")
            addr = base + (("p", pi), ("c", ci)) if base is not None and valid(pi) and valid(ci) else None
            if addr is not None and any(seg[0] == "cell" for seg in base):
                # 실측: 셀 안 그림의 pi 는 셀 문단 번호가 아니라 최외곽 본문 문단 번호이므로 버린다.
                addr = base + (("p", "*"), ("c", ci))
            items.append(Item(page, "pic", addr, zone, wrap=n.get("textWrap")))
            return
        if t == "Equation":
            items.append(Item(page, "equation", None, zone))
            return
        if t in ("Placeholder", "RawSvg"):
            addr = None
            if all(valid(n.get(k)) for k in ("si", "pi", "ci")):
                addr = (("S", n["si"]), ("p", n["pi"]), ("c", n["ci"]))
            items.append(Item(page, "ole/chart", addr, zone))
            return
        if t in ("Rect", "Ellipse", "Path", "Line", "Group", "TextBox", "Form", "FnMarker"):
            items.append(Item(page, "shape:" + t, None, zone))
            for c in kids:
                rec(c, None, zone + "/" + t.lower())
            return
        for c in kids:
            rec(c, base, zone)

    rec(tree, None, "root")


# ---------------------------------------------------------------- 조인

REND_KIND = {"p": "p", "tbl": "tbl", "tc": "tc", "pic": "pic"}


def join_addr(rec):
    """조인용 주소. 셀 안 그림은 셀 문단 번호를 '*' 로 지운다 (rhwp JSON 이 그 값을 내지 않음)."""
    a = rec.addr
    if a is not None and rec.kind == "pic" and any(seg[0] == "cell" for seg in a):
        return a[:-2] + (("p", "*"), a[-1])
    return a


def analyze(path, rhwp, workdir, order="spine"):
    z = zipfile.ZipFile(path)
    rhwp_order, lexical, not_in_spine = read_package(z)
    if order == "lexical":   # A/B 실험: 파일명 사전순을 구역 번호로 가정하면 어떻게 되는가
        rhwp_order = lexical
    elif order == "numeric":  # A/B 실험: 파일명의 숫자 순
        rhwp_order = sorted(lexical, key=lambda h: int(re.search(r"(\d+)\.xml$", h).group(1)))
    recs, notes = build_our_side(z, rhwp_order)
    trees, pages_doc = run_rhwp(rhwp, path, workdir)
    pages = pages_doc["pages"]
    result = {"file": path, "sections": len(rhwp_order), "pages": len(pages),
              "render_tree_files": len(trees),
              "section_order_spine_vs_lexical_equal": rhwp_order == lexical,
              "sections_not_in_spine": not_in_spine, "notes": dict(notes)}
    if len(trees) != len(pages):
        result["warning"] = "render tree 파일 수 != dump-pages 쪽 수"

    items = []
    for i, tree in enumerate(trees):
        walk_render(tree, i, pages[i]["section"] if i < len(pages) else 0, items)

    # dump-pages: 쪽별 최상위 항목 (문단은 paraIndex, 개체는 paraIndex+controlIndex)
    dp_para, dp_obj = [], []
    for pg in pages:
        sec = pg["section"]
        for col in pg["columns"]:
            for it in col["items"]:
                pi, ci, k = it.get("paraIndex"), it.get("controlIndex"), it["kind"]
                if k in ("fullParagraph", "partialParagraph"):
                    dp_para.append((pg["pageIndex"], (("S", sec), ("p", pi)), it.get("textPreview")))
                else:
                    dp_obj.append((pg["pageIndex"], k, (("S", sec), ("p", pi), ("c", ci)),
                                   (it.get("control") or {}).get("type")))

    ours_by_addr = collections.defaultdict(list)
    for r in recs:
        if join_addr(r) is not None:
            ours_by_addr[join_addr(r)].append(r)

    rend_pages = collections.defaultdict(lambda: collections.defaultdict(set))   # kind -> addr -> pages
    for it in items:
        if it.addr is not None:
            rend_pages[it.kind][it.addr].add(it.page)
    for (pg, a, _tp) in dp_para:
        rend_pages["p"][a].add(pg)
    dp_pages = collections.defaultdict(set)                                       # addr -> pages
    for (pg, k, a, ct) in dp_obj:
        dp_pages[a].add(pg)

    host_pages = collections.defaultdict(set)    # 최상위 문단 주소 -> 그 문단이 품은 최상위 개체가 나온 쪽
    for it in items:
        if it.addr is not None and len(it.addr) >= 3 and it.kind in ("tbl", "pic"):
            host_pages[it.addr[:2]].add(it.page)
    for a, pgs_ in dp_pages.items():
        host_pages[a[:2]] |= pgs_

    tbl_dims = collections.defaultdict(set)
    for it in items:
        if it.kind == "tbl" and it.addr is not None:
            tbl_dims[it.addr].add((it.extra["rows"], it.extra["cols"]))

    # --- 우리 요소별 판정
    stat = collections.defaultdict(collections.Counter)
    reasons = collections.defaultdict(collections.Counter)
    resolved = {}
    for r in recs:
        kind = r.kind
        if kind.startswith("ctl:"):
            continue
        label = kind
        if kind == "p":
            label = "p(top)" if (r.addr is not None and len(r.addr) == 2) else "p(nested)"
        elif kind in SHAPE_KINDS:
            label = "shape:" + kind
        ja = join_addr(r)
        if ja is None:
            stat[label]["unmatched"] += 1
            reasons[label]["no-address:" + (r.reason or "?")] += 1
            anc = r.attrs.get("anchor")
            if anc is not None:
                ap = dp_pages.get(anc, set()) | rend_pages["tbl"].get(anc, set()) | rend_pages["pic"].get(anc, set())
                reasons[label]["  (page via ancestor %s: %s)" % ("ok" if ap else "none", r.reason)] += 1
            continue
        if len(ours_by_addr[ja]) > 1:
            stat[label]["ambiguous"] += 1
            reasons[label]["duplicate-address"] += 1
            continue
        if kind == "tbl" and tbl_dims.get(ja) and tbl_dims[ja] != {(r.attrs["rows"], r.attrs["cols"])}:
            stat[label]["ambiguous"] += 1
            reasons[label]["rows/cols-conflict"] += 1
            continue
        via = []
        pgs = set()
        if kind in REND_KIND:
            pgs |= rend_pages[REND_KIND[kind]].get(ja, set())
            if pgs:
                via.append("render-tree")
        if kind == "p" and len(ja) == 2 and not pgs:
            # 표/그림만 품은 문단은 문단 항목이 없고 개체 항목만 나온다 -> 소유 개체의 쪽을 쓴다
            host = host_pages.get(ja, set())
            if host:
                pgs |= host
                via.append("host-of-control")
        if kind != "p" and kind != "tc":
            d = dp_pages.get(ja, set())
            if d:
                via.append("dump-pages")
            pgs |= d
        if pgs:
            stat[label]["unique"] += 1
            reasons[label]["via " + "+".join(via)] += 1
            if len(pgs) > 1:
                reasons[label]["spans>1 page"] += 1
            resolved[r.key] = sorted(pgs)
        else:
            stat[label]["unmatched"] += 1
            reasons[label]["addressable-but-on-no-page"] += 1

    # --- rhwp 쪽 항목 판정 (고아/주소 없음)
    rhwp_side = collections.defaultdict(collections.Counter)
    orphans = []
    for it in items:
        if it.kind.startswith("shape:") or it.kind == "ole/chart":
            continue
        if it.addr is None:
            rhwp_side[it.kind]["no-address(%s)" % it.zone] += 1
            continue
        c2 = [c for c in ours_by_addr.get(it.addr, []) if c.kind == it.kind]
        if len(c2) == 1:
            rhwp_side[it.kind]["resolved"] += 1
        elif not c2:
            rhwp_side[it.kind]["orphan"] += 1
            orphans.append((it.kind, str(it.addr), [c.kind for c in ours_by_addr.get(it.addr, [])]))
        else:
            rhwp_side[it.kind]["ambiguous"] += 1

    # --- 쪽 단위
    per_page = []
    for i in range(len(trees)):
        row = {"page": i + 1, "section": pages[i]["section"] if i < len(pages) else None}
        for kind in ("pic", "tbl", "equation"):
            its = [it for it in items if it.page == i and it.kind == kind]
            if kind == "equation":   # render tree 의 Equation 노드는 주소가 없다 -> dump-pages 항목 사용
                its = [Item(i, "equation", a, "dump-pages") for (pg, k, a, ct) in dp_obj
                       if pg == i and ct == "equation"]
            ok, bad = [], collections.Counter()
            for it in its:
                if it.addr is None:
                    bad["no-address(%s)" % it.zone] += 1
                    continue
                c2 = [c for c in ours_by_addr.get(it.addr, []) if c.kind == kind]
                if len(c2) == 1:
                    ok.append(c2[0].key)
                else:
                    bad["orphan/ambiguous"] += 1
            row[kind] = {"resolved": ok, "unresolved": dict(bad), "rhwp_items": len(its)}
        per_page.append(row)

    # --- 교차검증
    v2 = collections.Counter()   # 최상위 문단 textPreview 앞부분
    for (pg, a, tp) in dp_para:
        if not tp:
            continue
        c2 = [c for c in ours_by_addr.get(a, []) if c.kind == "p"]
        if len(c2) != 1:
            v2["no-our-paragraph"] += 1
            continue
        ours = re.sub(r"\s+", "", c2[0].attrs["text"])
        theirs = re.sub(r"\s+", "", tp)
        n = min(len(ours), len(theirs), 8)
        v2["match" if ours[:n] == theirs[:n] else "MISMATCH"] += 1
    v3 = collections.Counter()   # dump-pages 가 말하는 개체 종류 vs 우리 요소 종류
    for (pg, k, a, ct) in dp_obj:
        c2 = [c for c in ours_by_addr.get(a, []) if not c.kind.startswith("ctl:")]
        v3["%s/%s -> %s" % (k, ct, c2[0].kind if c2 else "NONE")] += 1
    v4 = collections.Counter()   # render Cell (row,col) 이 우리 tc 와 대응하는가
    for it in items:
        if it.kind == "tc" and it.addr is not None:
            v4["found" if any(c.kind == "tc" for c in ours_by_addr.get(it.addr, [])) else "orphan"] += 1

    result.update({
        "our_counts": {k: dict(v) for k, v in sorted(stat.items())},
        "our_reasons": {k: dict(v) for k, v in sorted(reasons.items())},
        "rhwp_side": {k: dict(v) for k, v in sorted(rhwp_side.items())},
        "orphans_sample": orphans[:10],
        "verify_text_prefix": dict(v2),
        "verify_dump_pages_kind_vs_our_kind": dict(v3),
        "verify_cells": dict(v4),
        "per_page": per_page,
        "resolved_keys": resolved,
    })
    return result


# ---------------------------------------------------------------- 경계 사례 합성 픽스처

def _rewrite(src, dst, edits):
    """src 의 ZIP 항목 일부를 문자열 변환 후 dst 로 저장. edits = {entry: fn(str)->str}"""
    with zipfile.ZipFile(src) as zin, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename in edits:
                data = edits[info.filename](data.decode("utf-8")).encode("utf-8")
            zout.writestr(info, data)


def make_fixtures(e_path, rsp_path, outdir):
    """공개 템플릿(E_from_edit, rsp_sec17_p0)을 문자열 수준으로 변형해 경계 사례를 만든다.
    - edge_E.hwpx      : 셀 안 2문단 각각 그림 / run 중간 인라인 그림 2개 / 수식 / 표 안 표(그림 포함)
    - selfclosing_p.hwpx: 최상위 자기닫힘 <hp:p/> 삽입 (rhwp 가 건너뛰는지)
    - spine_swap.hwpx  : content.hpf spine 에서 section1 <-> section2 순서 교환
    """
    os.makedirs(outdir, exist_ok=True)
    sec = "Contents/section0.xml"

    def edge(x):
        pics = list(re.finditer(r"<hp:pic .*?</hp:pic>", x, re.S))
        pic_inline = pics[-1].group(0)                       # 마지막 셀의 treatAsChar=1 그림
        tbl_start = x.rfind("<hp:tbl")
        tbl_end = x.find("</hp:tbl>", tbl_start) + len("</hp:tbl>")
        tbl = x[tbl_start:tbl_end]
        # (a) 마지막 표의 첫 셀: 문단 복제 -> 셀 안 2문단, 각각 그림 ci 0
        sl = tbl.find("<hp:subList")
        pa = tbl.find("<hp:p ", sl)
        pb = tbl.find("</hp:p>", pa) + len("</hp:p>")
        tbl2 = tbl[:pb] + tbl[pa:pb] + tbl[pb:]
        x = x[:tbl_start] + tbl2 + x[tbl_end:]
        # (b) 새 최상위 문단들 (pageBreak=1 로 새 쪽에서 시작)
        pf = ('<hp:p id="9%03d" paraPrIDRef="0" styleIDRef="0" pageBreak="1" columnBreak="0" merged="0">'
              '<hp:run charPrIDRef="0">%s</hp:run></hp:p>')
        inline = "<hp:t>앞</hp:t>" + pic_inline + "<hp:t>중간</hp:t>" + pic_inline + "<hp:t>뒤</hp:t>"
        eq = ('<hp:equation id="9100" zOrder="0" numberingType="EQUATION" textWrap="TOP_AND_BOTTOM" '
              'textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" version="Equation Version 60" '
              'baseLine="85" textColor="#000000" baseUnit="1000" lineMode="CHAR" font="HYhwpEQ">'
              '<hp:sz width="3000" widthRelTo="ABSOLUTE" height="1500" heightRelTo="ABSOLUTE" protect="0"/>'
              '<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" '
              'vertRelTo="PARA" horzRelTo="COLUMN" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>'
              '<hp:outMargin left="0" right="0" top="0" bottom="0"/>'
              '<hp:script>x^2 + 1 over 2</hp:script></hp:equation>')
        # 표 안 표: 마지막 표 복사본의 첫 셀 문단을 "표 복사본을 품은 문단" 으로 교체
        sl = tbl.find("<hp:subList")
        pa = tbl.find("<hp:p ", sl)
        pb = tbl.find("</hp:p>", pa) + len("</hp:p>")
        inner_p = ('<hp:p id="9200" paraPrIDRef="34" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0">'
                   '<hp:run charPrIDRef="37">' + tbl + "</hp:run></hp:p>")
        nested = tbl[:pa] + inner_p + tbl[pb:]
        add = (pf % (1, inline)) + (pf % (2, "<hp:t>수식:</hp:t>" + eq)) + (pf % (3, nested))
        return x.replace("</hs:sec>", add + "</hs:sec>")

    _rewrite(e_path, os.path.join(outdir, "edge_E.hwpx"), {sec: edge})

    def selfclosing(x):
        i = x.rfind("<hp:p ", 0, x.rfind("<hp:tbl"))
        return x[:i] + ('<hp:p id="9300" paraPrIDRef="0" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0"/>'
                        ) + x[i:]

    _rewrite(e_path, os.path.join(outdir, "selfclosing_p.hwpx"), {sec: selfclosing})

    def swap(x):
        a = '<opf:itemref idref="section1"'
        b = '<opf:itemref idref="section2"'
        return x.replace(a, "@@A@@").replace(b, a).replace("@@A@@", b)

    _rewrite(rsp_path, os.path.join(outdir, "spine_swap.hwpx"), {"Contents/content.hpf": swap})
    return [os.path.join(outdir, n) for n in ("edge_E.hwpx", "selfclosing_p.hwpx", "spine_swap.hwpx")]


def print_report(res, verbose=False):
    print("== %s" % res["file"])
    print("  sections=%d pages=%d render_tree_files=%d spine==lexical:%s not_in_spine=%s notes=%s" % (
        res["sections"], res["pages"], res["render_tree_files"],
        res["section_order_spine_vs_lexical_equal"], res["sections_not_in_spine"], res["notes"]))
    print("  [우리 요소 판정]  종류 | unique | ambiguous | unmatched")
    for k, v in res["our_counts"].items():
        print("   %-18s %6d %9d %9d" % (k, v.get("unique", 0), v.get("ambiguous", 0), v.get("unmatched", 0)))
    for k, v in res["our_reasons"].items():
        print("   reasons %-14s %s" % (k, v))
    print("  [rhwp 항목 판정]", json.dumps(res["rhwp_side"], ensure_ascii=False))
    print("  [검증] text-prefix:", res["verify_text_prefix"], " dump-pages kind:",
          res["verify_dump_pages_kind_vs_our_kind"], " cells:", res["verify_cells"])
    if res["orphans_sample"]:
        print("  orphans:", res["orphans_sample"][:3])
    if verbose:
        for row in res["per_page"]:
            print("   page %2d sec %s  pic %d/%d  tbl %d/%d  eq %d/%d  unresolved=%s" % (
                row["page"], row["section"],
                len(row["pic"]["resolved"]), row["pic"]["rhwp_items"],
                len(row["tbl"]["resolved"]), row["tbl"]["rhwp_items"],
                len(row["equation"]["resolved"]), row["equation"]["rhwp_items"],
                {k: v["unresolved"] for k, v in row.items() if isinstance(v, dict) and v["unresolved"]}))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("hwpx", nargs="*")
    ap.add_argument("--rhwp", default=DEFAULT_RHWP)
    ap.add_argument("--json", help="상세 결과 JSON 저장 경로")
    ap.add_argument("-v", "--verbose", action="store_true")
    ap.add_argument("--section-order", choices=("spine", "lexical", "numeric"), default="spine")
    ap.add_argument("--make-fixtures", nargs=3, metavar=("E_FROM_EDIT", "RSP_SEC17", "OUTDIR"),
                    help="경계 사례 합성 HWPX 3개를 OUTDIR 에 만들고 종료")
    ap.add_argument("--summary", action="store_true", help="여러 파일의 우리 요소 판정 합계표")
    a = ap.parse_args()
    if a.make_fixtures:
        for f in make_fixtures(*a.make_fixtures):
            print(f)
        return
    allres = []
    for f in a.hwpx:
        with tempfile.TemporaryDirectory(prefix="s2map_") as wd:
            res = analyze(f, a.rhwp, wd, a.section_order)
        print_report(res, a.verbose)
        allres.append(res)
    if a.summary:
        tot = collections.defaultdict(collections.Counter)
        for res in allres:
            for k, v in res["our_counts"].items():
                for st, n in v.items():
                    tot[k][st] += n
        print("== 합계 (%d 파일)" % len(allres))
        print("   %-18s %7s %9s %9s" % ("종류", "unique", "ambiguous", "unmatched"))
        for k, v in sorted(tot.items()):
            print("   %-18s %7d %9d %9d" % (k, v["unique"], v["ambiguous"], v["unmatched"]))
    if a.json:
        with open(a.json, "w", encoding="utf-8") as fh:
            json.dump(allres, fh, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
