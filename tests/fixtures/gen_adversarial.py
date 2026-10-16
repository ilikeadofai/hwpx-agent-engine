#!/usr/bin/env python3
"""악성/경계 HWPX 패키지 합성기 (Python 3, stdlib only, 결정적).

사용:
    python3 -I tests/fixtures/gen_adversarial.py OUTDIR [--expected PATH]

OUTDIR 에 사례별 .hwpx 파일을 쓰고, 기대 결과표를 PATH
(기본값 OUTDIR/adversarial_expected.json)에 쓴다.
모든 사례는 이 스크립트가 직접 만든 최소 유효 패키지(base)에서 파생되며,
각 사례가 의도한 결함(또는 수용 조건)을 실제로 갖는지 생성 시점에 assert 한다.
같은 zlib 구현이면 바이트까지 같다(zlib-ng 등 다른 구현이면 deflate 바이트는 달라질 수 있다).
"""
import bz2
import io
import json
import os
import struct
import sys
import warnings
import xml.etree.ElementTree as ET
import zipfile
import zlib

# ---------------------------------------------------------------- 상수

# 네임스페이스 URI 는 corpus/ref_empty.hwpx (rhwp, MIT) 의 section0.xml·content.hpf·container.xml 에서 옮겼다.
# corpus 가 옆에 있으면 아래 _selfcheck_namespaces() 가 다시 대조한다.
NS = {
    "ha": "http://www.hancom.co.kr/hwpml/2011/app",
    "hp": "http://www.hancom.co.kr/hwpml/2011/paragraph",
    "hs": "http://www.hancom.co.kr/hwpml/2011/section",
    "hc": "http://www.hancom.co.kr/hwpml/2011/core",
    "hh": "http://www.hancom.co.kr/hwpml/2011/head",
    "hpf": "http://www.hancom.co.kr/schema/2011/hpf",
    "opf": "http://www.idpf.org/2007/opf/",
    "ocf": "urn:oasis:names:tc:opendocument:xmlns:container",
}
HP = "{%s}" % NS["hp"]
OTHER_NS = "http://example.com/not-hwpx/paragraph"

MIB = 1 << 20
CAP = 128 * MIB          # 한 작업 누적 해제량 한도
MAX_ENTRIES = 2048
MAX_DEPTH = 128
MIMETYPE = b"application/hwp+zip"
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>'

SIG_LFH = b"PK\x03\x04"
SIG_CDH = b"PK\x01\x02"
SIG_EOCD = b"PK\x05\x06"
SIG_DD = b"PK\x07\x08"
LFH = struct.Struct("<4s5H3I2H")       # 30
CDH = struct.Struct("<4s6H3I5H2I")     # 46
EOCD = struct.Struct("<4s4H2IH")       # 22
DOS_TIME, DOS_DATE = 0, 0x21           # 1980-01-01 00:00:00


def _selfcheck_namespaces():
    here = os.path.dirname(os.path.abspath(__file__))
    ref = os.path.join(here, "corpus", "ref_empty.hwpx")
    if not os.path.exists(ref):
        return
    z = zipfile.ZipFile(ref)
    blob = b"".join(z.read(n) for n in ("Contents/section0.xml", "Contents/content.hpf", "META-INF/container.xml"))
    for pfx, uri in NS.items():
        assert ('xmlns:%s="%s"' % (pfx, uri)).encode() in blob, (pfx, uri)


# ---------------------------------------------------------------- 저수준 ZIP 작성기

def deflate(data):
    co = zlib.compressobj(9, zlib.DEFLATED, -15)
    return co.compress(data) + co.flush()


class E:
    """ZIP 항목 하나. 옵션(o)으로 로컬/중앙 헤더를 따로 망가뜨릴 수 있다."""

    def __init__(self, name, data=b"", method=8, **o):
        self.name = name.encode("utf-8") if isinstance(name, str) else name
        self.method = method
        self.o = o
        if "comp" in o:                       # 미리 압축한 데이터 (폭탄 등)
            self.comp, self.crc, self.usize = o["comp"], o["crc"], o["usize"]
        else:
            self.crc = zlib.crc32(data) & 0xFFFFFFFF
            self.usize = len(data)
            self.comp = deflate(data) if method == 8 else data   # 0/기타 방식은 data 를 그대로 둔다
        self.csize = len(self.comp)

    def flags(self):
        if "flags" in self.o:
            return self.o["flags"]
        try:
            self.name.decode("ascii")
            return 0
        except UnicodeDecodeError:
            try:
                self.name.decode("utf-8")
                return 0x800
            except UnicodeDecodeError:
                return 0


def build_zip(entries, comment=b"", eocd=None, cd_tail=b""):
    """entries -> (bytes, info). info 에 중앙 디렉터리 위치 등을 돌려준다."""
    eocd = eocd or {}
    out = bytearray()
    offs = []
    for e in entries:
        o = e.o
        if o.get("skip_local"):
            offs.append(None)
            continue
        offs.append(len(out))
        dd = o.get("dd", False)
        fl = o.get("lflags", e.flags()) | (8 if dd else 0)
        meth = o.get("lmethod", e.method)
        if dd:
            crc, cs, us = 0, 0, 0
        else:
            crc = o.get("lcrc", e.crc)
            cs = o.get("lcsize", e.csize)
            us = o.get("lusize", e.usize)
        name = o.get("lname", e.name)
        extra = o.get("lextra", b"")
        need = o.get("need", 10 if e.method == 0 else 20)
        out += LFH.pack(SIG_LFH, need, fl, meth, DOS_TIME, DOS_DATE, crc, cs, us, len(name), len(extra))
        out += name + extra + e.comp
        if dd:
            d = struct.pack("<3I", o.get("ddcrc", e.crc), e.csize, e.usize)
            out += (SIG_DD if o.get("ddsig", True) else b"") + d
    cd_off = len(out)
    n = 0
    for i, e in enumerate(entries):
        o = e.o
        if o.get("skip_central"):
            continue
        n += 1
        dd = o.get("dd", False)
        fl = o.get("cflags", e.flags()) | (8 if dd else 0)
        lho = o.get("lho")
        if lho is None:
            lho = offs[o.get("lho_from", i)]
        extra = o.get("cextra", b"")
        need = o.get("need", 10 if e.method == 0 else 20)
        out += CDH.pack(
            SIG_CDH, 20, need, fl, o.get("cmethod", e.method), DOS_TIME, DOS_DATE,
            o.get("ccrc", e.crc), o.get("ccsize", e.csize), o.get("cusize", e.usize),
            len(e.name), len(extra), 0, o.get("disk", 0), 0, 0, lho,
        )
        out += e.name + extra
    out += cd_tail
    cd_size = len(out) - cd_off
    eocd_off = len(out)
    out += EOCD.pack(
        SIG_EOCD, eocd.get("disk", 0), eocd.get("cd_disk", 0),
        eocd.get("n_disk", n), eocd.get("n_total", n),
        eocd.get("cd_size", cd_size), eocd.get("cd_off", cd_off), len(comment),
    )
    out += comment
    return bytes(out), {"cd_off": cd_off, "cd_size": cd_size, "eocd_off": eocd_off, "offs": offs, "n": n}


# ---------------------------------------------------------------- 최소 유효 패키지

def png_1x1():
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\x00\xff\x00\x00")) + chunk(b"IEND", b""))


def root_ns(prefixes):
    return " ".join('xmlns:%s="%s"' % (p, NS[p]) for p in prefixes)


def para(text, pid=1, pfx="hp"):
    return ('<%s:p id="%d" paraPrIDRef="0" styleIDRef="0"><%s:run charPrIDRef="0"><%s:t>%s</%s:t></%s:run></%s:p>'
            % (pfx, pid, pfx, pfx, text, pfx, pfx, pfx))


def section_xml(body=None, prefixes=("hs", "hp", "hc")):
    body = para("안녕 Hello") if body is None else body
    return ("%s<hs:sec %s>%s</hs:sec>" % (DECL, root_ns(prefixes), body)).encode("utf-8")


def header_xml():
    return ('%s<hh:head %s version="1.2" secCnt="1"><hh:refList/></hh:head>'
            % (DECL, root_ns(("hh", "hp", "hs")))).encode("utf-8")


def container_xml(full_path="Contents/content.hpf"):
    return ('%s<ocf:container xmlns:ocf="%s" xmlns:hpf="%s"><ocf:rootfiles>'
            '<ocf:rootfile full-path="%s" media-type="application/hwpml-package+xml"/>'
            '</ocf:rootfiles></ocf:container>' % (DECL, NS["ocf"], NS["hpf"], full_path)).encode("utf-8")


def hpf_xml(items=None, spine=None):
    items = items if items is not None else [
        ("header", "Contents/header.xml", "application/xml"),
        ("section0", "Contents/section0.xml", "application/xml"),
    ]
    spine = spine if spine is not None else ["header", "section0"]
    man = "".join('<opf:item id="%s" href="%s" media-type="%s"%s/>'
                  % (i, h, m, ' isEmbeded="1"' if h.startswith("BinData/") else "") for i, h, m in items)
    sp = "".join('<opf:itemref idref="%s"/>' % s for s in spine)
    return ('%s<opf:package xmlns:opf="%s" xmlns:hpf="%s" version="" unique-identifier="" id="">'
            '<opf:metadata><opf:title/><opf:language>ko</opf:language></opf:metadata>'
            '<opf:manifest>%s</opf:manifest><opf:spine>%s</opf:spine></opf:package>'
            % (DECL, NS["opf"], NS["hpf"], man, sp)).encode("utf-8")


def base_files(section=None, header=None, hpf=None, container=None, extra=()):
    """(name, bytes, method) 목록. mimetype 은 항상 STORED 로 맨 앞."""
    files = [
        ("mimetype", MIMETYPE, 0),
        ("META-INF/container.xml", container if container is not None else container_xml(), 8),
        ("Contents/content.hpf", hpf if hpf is not None else hpf_xml(), 8),
        ("Contents/header.xml", header if header is not None else header_xml(), 8),
        ("Contents/section0.xml", section if section is not None else section_xml(), 8),
    ]
    return files + list(extra)


def entries_of(files, **per_name):
    es = []
    for name, data, method in files:
        es.append(E(name, data, method, **per_name.get(name, {})))
    return es


def base_zip(**kw):
    return build_zip(entries_of(base_files()))[0]


# ---------------------------------------------------------------- 분석/검증 도우미

def parse(data, eocd_off=None):
    """EOCD 를 (기본: 마지막 서명 위치로) 읽어 중앙 항목과 로컬 헤더 요약을 돌려준다."""
    if eocd_off is None:
        eocd_off = data.rfind(SIG_EOCD)
    ec = EOCD.unpack_from(data, eocd_off)
    cd_size, cd_off = ec[5], ec[6]
    cds, p = [], cd_off
    for _ in range(ec[4]):
        f = CDH.unpack_from(data, p)
        nlen, elen, clen = f[10], f[11], f[12]
        name = data[p + 46:p + 46 + nlen]
        cds.append(dict(flags=f[3], method=f[4], crc=f[7], csize=f[8], usize=f[9], disk=f[13], lho=f[16],
                        name=name, extra=data[p + 46 + nlen:p + 46 + nlen + elen]))
        p += 46 + nlen + elen + clen
    return dict(eocd=ec, cds=cds, cd_off=cd_off, cd_size=cd_size, eocd_off=eocd_off)


def local_of(data, off):
    f = LFH.unpack_from(data, off)
    nlen, elen = f[9], f[10]
    return dict(flags=f[2], method=f[3], crc=f[6], csize=f[7], usize=f[8], name=data[off + 30:off + 30 + nlen],
                extra=data[off + 30 + nlen:off + 30 + nlen + elen], data_off=off + 30 + nlen + elen)


def py_open(data):
    warnings.simplefilter("ignore")
    return zipfile.ZipFile(io.BytesIO(data))


def py_fails(data):
    try:
        z = py_open(data)
        for i in z.infolist():
            z.read(i)
    except Exception:
        return True
    return False


def inflate_len(comp):
    d = zlib.decompressobj(-15)
    n, buf = 0, comp
    while buf:
        chunk = d.decompress(buf, 1 << 20)
        n += len(chunk)
        buf = d.unconsumed_tail
    n += len(d.flush())
    return n


def paragraphs(xml_bytes):
    """namespace 를 해석해 HWPX 문단(hp:p, 올바른 URI)의 텍스트(hp:t 연결)를 돌려준다."""
    root = ET.fromstring(xml_bytes)
    return ["".join(t.text or "" for t in p.iter(HP + "t")) for p in root.iter(HP + "p")]


def max_depth(xml_bytes):
    depth = best = 0
    for ev, _ in ET.iterparse(io.BytesIO(xml_bytes), events=("start", "end")):
        if ev == "start":
            depth += 1
            best = max(best, depth)
        else:
            depth -= 1
    return best


def read_entry(data, name):
    z = py_open(data)
    return z.read(name)


def spine_sections(data):
    z = py_open(data)
    root = ET.fromstring(z.read("Contents/content.hpf"))
    q = "{%s}" % NS["opf"]
    man = {i.get("id"): i.get("href") for i in root.iter(q + "item")}
    return [man[r.get("idref")] for r in root.iter(q + "itemref")
            if man.get(r.get("idref"), "").rsplit("/", 1)[-1].startswith("section")]


# ---------------------------------------------------------------- 사례 등록

CASES = []
NEW_REASONS = {}


def case(name, expect, stage, reason=None, code=None, notes="", check=None, detail=None, new=None):
    """데코레이터. 함수는 bytes 를 돌려준다. check(data) 는 의도한 결함/수용 조건을 assert 한다."""
    if expect == "reject" and code is None:
        code = "PACKAGE_REJECTED"

    def deco(fn):
        CASES.append(dict(name=name, fn=fn, expect=expect, stage=stage, reason=reason, code=code,
                          notes=notes, check=check, detail=detail))
        if new:
            NEW_REASONS[reason] = new
        return fn
    return deco


def add(name, data_fn, **kw):
    case(name, **kw)(data_fn)


def accepted_ok(data, xml_names=("Contents/section0.xml", "Contents/header.xml", "Contents/content.hpf",
                                 "META-INF/container.xml")):
    z = py_open(data)
    assert z.testzip() is None
    names = z.namelist()
    assert names[0] == "mimetype" and z.read("mimetype") == MIMETYPE
    for n in xml_names:
        if n in names:
            ET.fromstring(z.read(n))


def replace_section(sec_bytes, **kw):
    return build_zip(entries_of(base_files(section=sec_bytes), **kw.get("per", {})))[0]


# ================================================================ ZIP 수준

add("valid_base.hwpx", lambda: base_zip(), expect="accept", stage="zip",
    notes="이 합성기의 최소 유효 패키지. mimetype STORED 첫 항목, 나머지 DEFLATE.",
    check=accepted_ok, detail=lambda d: {"paragraph_texts": {"Contents/section0.xml": paragraphs(read_entry(d, "Contents/section0.xml"))}})


def _trunc_half():
    d = base_zip()
    return d[:len(d) // 2]


add("truncated_50pct.hwpx", _trunc_half, expect="reject", stage="zip", reason="truncated", new="파일이 중간에서 끊겨 EOCD 가 없거나 항목 범위가 파일 밖이다.",
    notes="유효 패키지의 앞 50%만 남김. EOCD 없음.",
    check=lambda d: _assert(py_fails(d) and SIG_EOCD not in d))

add("truncated_eocd_minus1.hwpx", lambda: base_zip()[:-1], expect="reject", stage="zip", reason="truncated",
    notes="마지막 1바이트 제거: EOCD 가 21바이트로 잘림.",
    check=lambda d: _assert(py_fails(d) and d[-21:-17] == SIG_EOCD))


def _assert(c, msg="assertion failed"):
    if not c:
        raise AssertionError(msg)
    return True


# ---- EOCD 위장
def _eocd_comment_spoof():
    es = entries_of(base_files())
    real, info = build_zip(es)
    # 가짜 중앙 디렉터리: header.xml 항목을 뺀 사본. 진짜 EOCD 의 주석 안에 넣는다.
    sans = [e for e in es if e.name != b"Contents/header.xml"]
    # 중앙 항목은 같은 로컬 헤더를 가리키므로 lho 를 원본에서 가져온다.
    offs = {e.name: o for e, o in zip(es, info["offs"])}
    fake_es = []
    for e in sans:
        e2 = E(e.name, b"", e.method, comp=e.comp, crc=e.crc, usize=e.usize, skip_local=True, lho=offs[e.name])
        fake_es.append(e2)
    # 가짜 CD 바이트를 직접 조립
    cd = bytearray()
    for e in fake_es:
        cd += CDH.pack(SIG_CDH, 20, 10 if e.method == 0 else 20, e.flags(), e.method, DOS_TIME, DOS_DATE,
                       e.crc, e.csize, e.usize, len(e.name), 0, 0, 0, 0, 0, e.o["lho"]) + e.name
    comment_len = len(cd) + 22
    real_eocd_off = info["eocd_off"]
    fake_cd_off = real_eocd_off + 22                       # 진짜 EOCD 고정부 바로 뒤 = 주석 시작
    fake_eocd = EOCD.pack(SIG_EOCD, 0, 0, len(fake_es), len(fake_es), len(cd), fake_cd_off, 0)
    out = bytearray(real[:real_eocd_off])
    out += EOCD.pack(SIG_EOCD, 0, 0, info["n"], info["n"], info["cd_size"], info["cd_off"], comment_len)
    out += bytes(cd) + fake_eocd
    return bytes(out)


def _chk_comment_spoof(d):
    zs = py_open(d).namelist()
    assert "Contents/header.xml" not in zs                     # 마지막 서명을 따르는 읽기는 가짜 CD 를 본다
    first = d.find(SIG_EOCD)
    real = parse(d, first)
    assert any(c["name"] == b"Contents/header.xml" for c in real["cds"])   # 진짜 CD 에는 있다
    assert d.count(SIG_EOCD) == 2 and real["eocd"][7] == len(d) - first - 22


add("eocd_comment_spoof.hwpx", _eocd_comment_spoof, expect="reject", stage="zip", reason="bad_zip_structure",
    new="ZIP 구조 불일치: 겹침·틈·EOCD 위장·범위 이탈 등 중앙/로컬 배치가 정합하지 않다.",
    notes="진짜 EOCD 의 주석 안에 가짜 CD(header.xml 누락)와 가짜 EOCD(주석 길이 0)를 심어, 끝에서부터 찾는 읽기와 앞에서부터 찾는 읽기가 다른 항목 목록을 보게 함. "
          "탐지: EOCD 서명이 끝 64KiB+22 창에 둘 이상이거나 마지막 로컬 항목 끝과 cd_offset 사이에 설명 안 된 틈이 있으면 거절.",
    check=_chk_comment_spoof)


def _eocd_data_spoof():
    fake = EOCD.pack(SIG_EOCD, 0, 0, 0, 0, 0, 0, 0)
    extra = [("Preview/PrvText.txt", b"prefix " + fake + b" suffix", 0)]
    return build_zip(entries_of(base_files(extra=extra)))[0]


add("eocd_signature_in_entry_data.hwpx", _eocd_data_spoof, expect="accept", stage="zip",
    notes="STORED 항목 본문에 EOCD 형식의 22바이트가 들어 있다. 실제 EOCD 는 파일 끝에 정확히 맞물리므로 수용해야 함(본문 안의 서명을 EOCD 로 보면 안 됨).",
    check=lambda d: (accepted_ok(d), _assert(d.count(SIG_EOCD) == 2 and d.find(SIG_EOCD) < d.rfind(SIG_EOCD))))

add("eocd_comment_benign.hwpx", lambda: build_zip(entries_of(base_files()), comment=b"generated by gen_adversarial")[0],
    expect="accept", stage="zip", notes="무해한 ZIP 주석(EOCD 서명 없음)은 수용.",
    check=lambda d: (accepted_ok(d), _assert(parse(d)["eocd"][7] > 0)))


# ---- 겹침
def _overlap_same_header():
    es = entries_of(base_files())
    names = [e.name for e in es]
    i = names.index(b"Contents/section0.xml")
    # header.xml 의 중앙 항목이 section0.xml 의 로컬 헤더를 가리킨다.
    j = names.index(b"Contents/header.xml")
    es[j].o["lho_from"] = i
    return build_zip(es)[0]


def _chk_overlap_same(d):
    p = parse(d)
    lhos = [c["lho"] for c in p["cds"]]
    assert len(lhos) != len(set(lhos))


add("overlapping_entries.hwpx", _overlap_same_header, expect="reject", stage="zip", reason="bad_zip_structure",
    notes="두 중앙 항목(header.xml, section0.xml)이 같은 로컬 헤더를 가리킨다. 로컬 이름 불일치도 겸함(순수 겹침은 overlapping_nested_entry).",
    check=_chk_overlap_same)


def _overlap_nested():
    # A(STORED) 의 본문 안에 B 의 완전한 로컬 헤더+데이터가 들어 있고, B 의 중앙 항목이 그 위치를 가리킨다. 헤더 정합은 모두 맞다.
    b_data = b"<b/>"
    b_name = b"Contents/nested.xml"
    b_blob = LFH.pack(SIG_LFH, 10, 0, 0, DOS_TIME, DOS_DATE, zlib.crc32(b_data) & 0xFFFFFFFF, len(b_data), len(b_data),
                      len(b_name), 0) + b_name + b_data
    a_data = b"A-prefix-" + b_blob + b"-A-suffix"
    es = entries_of(base_files(extra=[("Contents/outer.bin", a_data, 0)]))
    # outer 의 로컬 오프셋을 알려면 한 번 빌드해야 한다.
    _, info = build_zip(es)
    outer_idx = [e.name for e in es].index(b"Contents/outer.bin")
    outer_off = info["offs"][outer_idx]
    data_off = outer_off + 30 + len(b"Contents/outer.bin")
    b_off = data_off + len(b"A-prefix-")
    nested = E(b_name, b_data, 0, skip_local=True, lho=b_off)
    es.append(nested)
    return build_zip(es)[0]


def _chk_overlap_nested(d):
    p = parse(d)
    rng = []
    for c in p["cds"]:
        l = local_of(d, c["lho"])
        assert l["name"] == c["name"]            # 헤더는 정합
        rng.append((c["lho"], l["data_off"] + c["csize"]))
    rng.sort()
    assert any(rng[i][1] > rng[i + 1][0] for i in range(len(rng) - 1))


add("overlapping_nested_entry.hwpx", _overlap_nested, expect="reject", stage="zip", reason="bad_zip_structure",
    notes="한 항목의 STORED 본문 안에 다른 항목의 유효한 로컬 헤더+본문이 들어 있고 두 중앙 항목의 바이트 범위가 겹친다. 헤더 필드는 모두 정합.",
    check=_chk_overlap_nested)


# ---- 로컬/중앙 불일치
def _mod_section(**o):
    return build_zip(entries_of(base_files(), **{"Contents/section0.xml": o}))[0]


def _find(d, name):
    p = parse(d)
    c = next(c for c in p["cds"] if c["name"] == name)
    return c, local_of(d, c["lho"])


add("local_central_name_mismatch.hwpx", lambda: _mod_section(lname=b"Contents/evil0.xml"),
    expect="reject", stage="zip", reason="header_mismatch",
    new="같은 항목의 로컬 헤더와 중앙 항목의 이름·플래그·방식·크기가 서로 다르다.",
    notes="section0.xml 중앙 이름과 로컬 이름이 다르다(길이 다름).",
    check=lambda d: _assert(_find_by_cd_idx(d, 4)[1]["name"] != _find_by_cd_idx(d, 4)[0]["name"]))


def _find_by_cd_idx(d, i):
    p = parse(d)
    c = p["cds"][i]
    return c, local_of(d, c["lho"])


add("local_central_name_mismatch_samelen.hwpx", lambda: _mod_section(lname=b"Contents/sectionX.xml"),
    expect="reject", stage="zip", reason="header_mismatch",
    notes="이름 길이는 같고 내용만 다른 불일치(길이만 비교하면 놓침).",
    check=lambda d: _assert(len(_find_by_cd_idx(d, 4)[1]["name"]) == len(_find_by_cd_idx(d, 4)[0]["name"])
                            and _find_by_cd_idx(d, 4)[1]["name"] != _find_by_cd_idx(d, 4)[0]["name"]))

add("local_central_size_mismatch.hwpx", lambda: _mod_section(lusize=1, lcsize=1),
    expect="reject", stage="zip", reason="header_mismatch",
    notes="로컬 헤더의 압축/원본 크기(1,1)가 중앙 항목과 다르다(데이터 디스크립터 없음).",
    check=lambda d: (lambda c, l: _assert(l["usize"] != c["usize"] and l["csize"] != c["csize"] and not (l["flags"] & 8)))(*_find_by_cd_idx(d, 4)))

add("local_central_method_mismatch.hwpx", lambda: _mod_section(lmethod=0),
    expect="reject", stage="zip", reason="header_mismatch",
    notes="로컬은 STORED, 중앙은 DEFLATE.",
    check=lambda d: (lambda c, l: _assert(l["method"] != c["method"]))(*_find_by_cd_idx(d, 4)))

add("local_central_flags_mismatch.hwpx", lambda: _mod_section(lflags=0x800),
    expect="reject", stage="zip", reason="header_mismatch",
    notes="로컬에만 UTF-8 플래그(bit 11)가 켜져 있다.",
    check=lambda d: (lambda c, l: _assert(l["flags"] != c["flags"]))(*_find_by_cd_idx(d, 4)))

add("crc_mismatch.hwpx", lambda: _mod_section(lcrc=0xDEADBEEF, ccrc=0xDEADBEEF),
    expect="reject", stage="zip", reason="crc_mismatch",
    new="항목의 선언 CRC-32 와 해제한 데이터의 CRC-32 가 다르다.",
    notes="로컬·중앙 모두 같은(틀린) CRC 라 헤더 정합 검사는 통과하고 해제 후 CRC 대조에서만 걸린다.",
    check=lambda d: (lambda c, l: _assert(c["crc"] == l["crc"] and c["crc"] != zlib.crc32(zlib.decompress(d[l["data_off"]:l["data_off"] + c["csize"]], -15)) & 0xFFFFFFFF and py_fails(d)))(*_find_by_cd_idx(d, 4)))


def _size_smaller():
    full = section_xml()
    return build_zip(entries_of(base_files(), **{"Contents/section0.xml": dict(lusize=len(full) - 5, cusize=len(full) - 5)}))[0]


add("declared_size_smaller_than_actual_inflate.hwpx", _size_smaller, expect="reject", stage="zip", reason="size_mismatch",
    new="선언한 해제 크기와 실제 해제 길이가 다르다(스트리밍 해제 중 선언 초과 포함).",
    notes="로컬·중앙 모두 원본 크기를 실제보다 5바이트 작게 선언. CRC 는 전체 데이터 기준으로 정확. 선언 크기만 믿고 버퍼를 잡으면 안 됨.",
    check=lambda d: (lambda c, l: _assert(inflate_len(d[l["data_off"]:l["data_off"] + c["csize"]]) == c["usize"] + 5 and c["usize"] == l["usize"]))(*_find_by_cd_idx(d, 4)))


# ---- 데이터 디스크립터
add("data_descriptor_entry.hwpx", lambda: build_zip(entries_of(base_files(), **{"Contents/section0.xml": dict(dd=True), "Contents/header.xml": dict(dd=True)}))[0],
    expect="accept", stage="zip", notes="section0.xml·header.xml 에 bit 3 + 서명 있는 데이터 디스크립터. 로컬 헤더의 CRC·크기는 0. 실제 한컴 파일도 이 형태를 쓴다.",
    check=lambda d: (accepted_ok(d), (lambda c, l: _assert(l["flags"] & 8 and l["crc"] == 0 and l["csize"] == 0 and c["flags"] & 8))(*_find_by_cd_idx(d, 4))),
    detail=lambda d: {"paragraph_texts": {"Contents/section0.xml": paragraphs(read_entry(d, "Contents/section0.xml"))}})

add("data_descriptor_no_signature.hwpx", lambda: build_zip(entries_of(base_files(), **{"Contents/section0.xml": dict(dd=True, ddsig=False)}))[0],
    expect="accept", stage="zip", notes="서명(PK\\x07\\x08) 없는 데이터 디스크립터(12바이트). 선택 서명이므로 수용해야 함.",
    check=lambda d: (accepted_ok(d), _assert(parse(d)["cds"][4]["flags"] & 8)))

add("data_descriptor_crc_mismatch.hwpx", lambda: build_zip(entries_of(base_files(), **{"Contents/section0.xml": dict(dd=True, ddcrc=0x12345678)}))[0],
    expect="reject", stage="zip", reason="crc_mismatch",
    notes="디스크립터의 CRC 가 중앙 항목·실제 CRC 와 다르다.",
    check=lambda d: (lambda c, l: _assert(struct.unpack_from("<4sI", d, l["data_off"] + c["csize"])[1] != c["crc"]))(*_find_by_cd_idx(d, 4)))

# ---- 암호화·방식
add("encrypted_flag.hwpx", lambda: _mod_section(lflags=1, cflags=1), expect="reject", stage="zip", reason="encrypted",
    notes="section0.xml 의 일반 목적 비트 0(암호화) 설정.", check=lambda d: _assert(parse(d)["cds"][4]["flags"] & 1))

add("encrypted_method99_aes.hwpx", lambda: _mod_section(lmethod=99, cmethod=99), expect="reject", stage="zip", reason="encrypted",
    notes="WinZip AES(방식 99) 표기. 암호화로 거절(미지원 방식으로도 설명 가능하나 사유는 encrypted로 고정).",
    check=lambda d: _assert(parse(d)["cds"][4]["method"] == 99))

add("encryption_xml_entry.hwpx", lambda: build_zip(entries_of(base_files(extra=[("META-INF/encryption.xml", ('%s<encryption/>' % DECL).encode(), 8)])))[0],
    expect="reject", stage="package", reason="encrypted",
    notes="META-INF/encryption.xml 항목 존재. ZIP 플래그는 정상이지만 암호화 문서로 보고 거절.",
    check=lambda d: _assert("META-INF/encryption.xml" in py_open(d).namelist()))


def _bzip2():
    sec = section_xml()
    return build_zip(entries_of(base_files(), **{"Contents/section0.xml": dict(comp=bz2.compress(sec), crc=zlib.crc32(sec) & 0xFFFFFFFF, usize=len(sec), lmethod=12, cmethod=12)}))[0]


add("unsupported_method_bzip2.hwpx", _bzip2, expect="reject", stage="zip", reason="unsupported_method",
    new="STORED(0)·DEFLATE(8) 외의 압축 방식.", notes="section0.xml 방식 12(bzip2), 실제 bzip2 데이터.",
    check=lambda d: _assert(parse(d)["cds"][4]["method"] == 12 and bz2.decompress(d[local_of(d, parse(d)["cds"][4]["lho"])["data_off"]:][:parse(d)["cds"][4]["csize"]]) == section_xml()))

add("unsupported_method_deflate64.hwpx",
    lambda: build_zip(entries_of(base_files(extra=[("BinData/x.bin", b"\x00" * 8, 9)])))[0],
    expect="reject", stage="zip", reason="unsupported_method",
    notes="참조되지 않는 부가 항목의 방식 9(Deflate64). 쓰이지 않는 항목이라도 열 때 거절.",
    check=lambda d: _assert(parse(d)["cds"][-1]["method"] == 9))

# ---- 다중 디스크, ZIP64
add("multi_disk_eocd.hwpx", lambda: build_zip(entries_of(base_files()), eocd=dict(disk=1))[0], expect="reject", stage="zip", reason="multi_disk",
    new="ZIP 이 여러 디스크에 걸쳐 있음(디스크 번호 ≠ 0).", notes="EOCD 의 디스크 번호 = 1.",
    check=lambda d: _assert(parse(d)["eocd"][1] == 1))
add("multi_disk_cd_start.hwpx", lambda: build_zip(entries_of(base_files()), eocd=dict(cd_disk=1, n_disk=0))[0], expect="reject", stage="zip", reason="multi_disk",
    notes="EOCD 의 중앙 디렉터리 시작 디스크 = 1, 이 디스크의 항목 수 = 0.", check=lambda d: _assert(parse(d)["eocd"][2] == 1))
add("multi_disk_entry.hwpx", lambda: _mod_section(disk=1), expect="reject", stage="zip", reason="multi_disk",
    notes="section0.xml 중앙 항목의 시작 디스크 = 1.", check=lambda d: _assert(parse(d)["cds"][4]["disk"] == 1))


def _zip64_eocd():
    es = entries_of(base_files())
    d, info = build_zip(es)
    eocd_off = info["eocd_off"]
    z64 = struct.pack("<4sQ2H2I4Q", b"PK\x06\x06", 44, 45, 45, 0, 0, info["n"], info["n"], info["cd_size"], info["cd_off"])
    loc = struct.pack("<4sIQI", b"PK\x06\x07", 0, eocd_off, 1)
    ec = EOCD.pack(SIG_EOCD, 0, 0, 0xFFFF, 0xFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0)
    return d[:eocd_off] + z64 + loc + ec


add("zip64_eocd.hwpx", _zip64_eocd, expect="reject", stage="zip", reason="zip64_required",
    notes="ZIP64 EOCD 레코드·로케이터가 있고 EOCD 필드가 0xFFFF/0xFFFFFFFF. ZIP64 가 필요한 구조는 초기 한도 밖.",
    check=lambda d: _assert(d[-42:-38] == b"PK\x06\x07" and EOCD.unpack_from(d, len(d) - 22)[6] == 0xFFFFFFFF and py_open(d).namelist()[0] == "mimetype"))


def _zip64_entry():
    sec = section_xml()
    e = E("Contents/section0.xml", sec, 8)
    ex = struct.pack("<2HQQ", 1, 16, e.usize, e.csize)
    o = dict(lusize=0xFFFFFFFF, lcsize=0xFFFFFFFF, cusize=0xFFFFFFFF, ccsize=0xFFFFFFFF, lextra=ex, cextra=ex, need=45)
    return build_zip(entries_of(base_files(), **{"Contents/section0.xml": o}))[0]


add("zip64_entry_extra.hwpx", _zip64_entry, expect="reject", stage="zip", reason="zip64_required",
    notes="section0.xml 의 크기 필드가 0xFFFFFFFF 이고 ZIP64 extra(0x0001)에 실제 크기. 실제 값은 작지만 ZIP64 표기는 거절.",
    check=lambda d: _assert(parse(d)["cds"][4]["csize"] == 0xFFFFFFFF and parse(d)["cds"][4]["extra"][:2] == b"\x01\x00" and not py_fails(d)))

# ---- 이름 충돌·경로
add("duplicate_name.hwpx", lambda: build_zip(entries_of(base_files(extra=[("Contents/section0.xml", section_xml(para("두 번째")), 8)])))[0],
    expect="reject", stage="zip", reason="duplicate_name", notes="Contents/section0.xml 이 두 번 나온다(로컬·중앙 모두).",
    check=lambda d: (lambda ns: _assert(ns.count(b"Contents/section0.xml") == 2))([c["name"] for c in parse(d)["cds"]]))

add("case_collision.hwpx", lambda: build_zip(entries_of(base_files(extra=[("Contents/Section0.xml", section_xml(para("대문자")), 8)])))[0],
    expect="reject", stage="zip", reason="case_collision", notes="Contents/Section0.xml 과 Contents/section0.xml 이 대소문자만 다르다.",
    check=lambda d: (lambda ns: _assert(len({n.lower() for n in ns}) < len(ns) and len(set(ns)) == len(ns)))([c["name"] for c in parse(d)["cds"]]))


def _escape(name):
    return lambda: build_zip(entries_of(base_files(extra=[(name, b"x", 0)])))[0]


for fname, nm, note in [
    ("path_escape_dotdot", b"../evil.xml", "'..' 상위 경로 요소."),
    ("path_escape_dotdot_mid", b"Contents/../../evil.xml", "중간의 '..' 로 루트 밖으로."),
    ("path_escape_absolute", b"/abs/evil.xml", "선행 '/' 절대 경로."),
    ("path_escape_drive_backslash", b"C:\\x", "Windows 드라이브 문자 + 역슬래시."),
    ("path_escape_drive_slash", b"C:/x", "Windows 드라이브 문자 + 슬래시(콜론 포함 요소)."),
    ("path_escape_backslash", b"Contents\\section0.xml", "역슬래시 구분자(슬래시 정규화 시 section0.xml 과 충돌할 수 있음)."),
    ("path_escape_nul", b"Contents/a\x00b.xml", "이름 안의 NUL 바이트."),
    ("path_escape_dot_segment", b"Contents/./x.xml", "'.' 경로 요소(정규화되지 않은 이름)."),
    ("path_escape_empty_segment", b"Contents//x.xml", "빈 경로 요소('//')."),
]:
    add(fname + ".hwpx", _escape(nm), expect="reject", stage="zip", reason="path_escape",
        notes="참조되지 않는 부가 항목이라도 이름이 위험하면 거절. " + note,
        check=(lambda nm: lambda d: _assert(parse(d)["cds"][-1]["name"] == nm))(nm))

# ---- 이름 인코딩
EUC = "한글".encode("euc-kr")
add("non_utf8_name.hwpx", lambda: build_zip(entries_of(base_files(extra=[(b"BinData/" + EUC + b".png", b"x", 0)])))[0],
    expect="reject", stage="zip", reason="non_utf8", notes="부가 항목 이름이 EUC-KR 바이트이고 bit 11 이 꺼져 있다.",
    check=lambda d: (lambda c: _assert(not (c["flags"] & 0x800) and EUC in c["name"]))(parse(d)["cds"][-1]))
add("non_utf8_name_flag_set_invalid.hwpx",
    lambda: build_zip([*entries_of(base_files()), E(b"BinData/" + EUC + b".png", b"x", 0, flags=0x800)])[0],
    expect="reject", stage="zip", reason="non_utf8", notes="bit 11(UTF-8)을 켰지만 이름 바이트가 유효한 UTF-8 이 아니다.",
    check=lambda d: (lambda c: _assert(c["flags"] & 0x800 and not _try_utf8(c["name"])))(parse(d)["cds"][-1]))


def _try_utf8(b):
    try:
        b.decode("utf-8")
        return True
    except UnicodeDecodeError:
        return False


add("non_utf8_name_flag_clear_utf8_bytes.hwpx",
    lambda: build_zip([*entries_of(base_files()), E("BinData/한글.png".encode("utf-8"), b"x", 0, flags=0)])[0],
    expect="reject", stage="zip", reason="non_utf8",
    notes="이름 바이트는 유효한 UTF-8 이지만 bit 11 이 꺼져 있다(표준상 CP437). 정책: 보수적으로 거절. 총괄이 완화하면 이 항목을 accept 로 바꾼다.",
    check=lambda d: (lambda c: _assert(not (c["flags"] & 0x800) and _try_utf8(c["name"]) and max(c["name"]) > 0x7F))(parse(d)["cds"][-1]))

add("utf8_name_flag_set.hwpx", lambda: build_zip(entries_of(base_files(extra=[("BinData/한글.png", b"x", 0)])))[0],
    expect="accept", stage="zip", notes="부가 항목 이름이 UTF-8 이고 bit 11 이 켜져 있다.",
    check=lambda d: (accepted_ok(d), _assert("BinData/한글.png" in py_open(d).namelist() and parse(d)["cds"][-1]["flags"] & 0x800)))

add("directory_entries.hwpx", lambda: build_zip(_with_dirs())[0],
    expect="accept", stage="zip", notes="'META-INF/'·'Contents/' 디렉터리 항목 포함(corpus/irb-sample.hwpx 에서 실제 관찰). 디렉터리 항목은 수용하되 파일로 취급하지 않음.",
    check=lambda d: (accepted_ok(d), _assert(py_open(d).namelist()[1:3] == ["META-INF/", "Contents/"])))

def _with_dirs():
    es = entries_of(base_files())
    es[1:1] = [E("META-INF/", b"", 0), E("Contents/", b"", 0)]
    return es


# ---- mimetype
def _reorder(first_name):
    fs = base_files()
    m = next(f for f in fs if f[0] == "mimetype")
    fs.remove(m)
    fs.insert(1, m)
    return build_zip(entries_of(fs))[0]


add("mimetype_not_first.hwpx", lambda: _reorder("mimetype"), expect="reject", stage="zip", reason="bad_mimetype",
    notes="mimetype 이 두 번째 항목이다.", check=lambda d: _assert(parse(d)["cds"][0]["name"] != b"mimetype" and py_open(d).read("mimetype") == MIMETYPE))
add("mimetype_compressed.hwpx", lambda: build_zip([E("mimetype", MIMETYPE, 8), *entries_of(base_files())[1:]])[0], expect="reject", stage="zip", reason="bad_mimetype",
    notes="mimetype 이 DEFLATE 로 압축되어 있다(STORED 여야 함).", check=lambda d: _assert(parse(d)["cds"][0]["method"] == 8))
add("mimetype_wrong_value.hwpx", lambda: build_zip([E("mimetype", b"application/zip", 0), *entries_of(base_files())[1:]])[0], expect="reject", stage="zip", reason="bad_mimetype",
    notes="mimetype 값이 application/zip.", check=lambda d: _assert(py_open(d).read("mimetype") != MIMETYPE))
add("mimetype_trailing_newline.hwpx", lambda: build_zip([E("mimetype", MIMETYPE + b"\n", 0), *entries_of(base_files())[1:]])[0], expect="reject", stage="zip", reason="bad_mimetype",
    notes="값 끝에 개행이 붙음. 정확 일치만 수용(정책).", check=lambda d: _assert(py_open(d).read("mimetype") == MIMETYPE + b"\n"))
add("mimetype_missing.hwpx", lambda: build_zip(entries_of(base_files())[1:])[0], expect="reject", stage="zip", reason="bad_mimetype",
    notes="mimetype 항목 없음.", check=lambda d: _assert("mimetype" not in py_open(d).namelist()))

# ---- 한도
def _many(total):
    extra = [("Extra/e%04d.txt" % i, b"x", 0) for i in range(total - 5)]
    return build_zip(entries_of(base_files(extra=extra)))[0]


add("too_many_entries_2049.hwpx", lambda: _many(MAX_ENTRIES + 1), expect="reject", stage="zip", reason="entry_limit", code="RESOURCE_LIMIT",
    new="항목 수가 한도(2,048)를 넘었다.", notes="항목 2,049개(한도 2,048 초과). 각 1바이트 STORED.",
    check=lambda d: _assert(parse(d)["eocd"][4] == MAX_ENTRIES + 1 and py_open(d).testzip() is None))
add("entries_2048_boundary.hwpx", lambda: _many(MAX_ENTRIES), expect="accept", stage="zip",
    notes="항목 2,048개 정확히 한도. 수용(한도는 초과 시에만 거절).",
    check=lambda d: (accepted_ok(d), _assert(parse(d)["eocd"][4] == MAX_ENTRIES)))


def zero_stream(mib):
    """mib MiB 의 0 을 DEFLATE 한 (comp, crc, usize). 스트리밍으로 만든다."""
    co = zlib.compressobj(9, zlib.DEFLATED, -15)
    chunk = bytes(MIB)
    crc, parts = 0, []
    for _ in range(mib):
        parts.append(co.compress(chunk))
        crc = zlib.crc32(chunk, crc)
    parts.append(co.flush())
    return b"".join(parts), crc & 0xFFFFFFFF, mib * MIB


_ZERO8 = None


def bombs(count, mib=8):
    global _ZERO8
    if _ZERO8 is None or _ZERO8[0] != mib:
        _ZERO8 = (mib,) + zero_stream(mib)
    _, comp, crc, usize = _ZERO8
    extra_es = [E("BinData/bomb%02d.bin" % i, b"", 8, comp=comp, crc=crc, usize=usize) for i in range(count)]
    return build_zip(entries_of(base_files()) + extra_es)[0]


def _chk_bomb(count):
    def f(d):
        p = parse(d)
        total = sum(c["usize"] for c in p["cds"])
        assert total > CAP
        assert len(d) < 2_000_000
        for c in p["cds"][5:]:
            l = local_of(d, c["lho"])
            assert inflate_len(d[l["data_off"]:l["data_off"] + c["csize"]]) == c["usize"]
    return f


add("deflate_bomb_many.hwpx", lambda: bombs(17), expect="reject", stage="zip", reason="inflate_limit", code="RESOURCE_LIMIT",
    new="누적 해제량(또는 항목별 해제량)이 한도(128MiB)를 넘었다.",
    notes="8MiB 0 바이트 항목 17개(누적 136MiB > 128MiB), 항목당 한도 미만. 선언 크기는 정직. 스트리밍 누적으로 거절해야 함. 파일은 약 140KB.",
    check=_chk_bomb(17))
add("deflate_bomb_single.hwpx",
    lambda: build_zip(entries_of(base_files()) + [E("BinData/bomb.bin", b"", 8, **dict(zip(("comp", "crc", "usize"), zero_stream(130))))])[0],
    expect="reject", stage="zip", reason="inflate_limit", code="RESOURCE_LIMIT",
    notes="130MiB 0 바이트 항목 하나(단일 항목이 누적 한도 초과). 파일은 약 130KB.",
    check=lambda d: (lambda c: _assert(c["usize"] == 130 * MIB and len(d) < 2_000_000))(parse(d)["cds"][-1]))
add("inflate_just_under_cap.hwpx", lambda: bombs(15), expect="accept", stage="zip",
    notes="8MiB 0 바이트 항목 15개(누적 120MiB + 본문 수백 바이트 < 128MiB). 한도 미만이면 압축률이 높아도 수용(압축률만으로 거절하지 않음).",
    check=lambda d: (lambda t: _assert(t < CAP))(sum(c["usize"] for c in parse(d)["cds"])))


# ================================================================ 패키지 수준

def pkg(**kw):
    return build_zip(entries_of(base_files(**kw)))[0]


def _two_section_files(spine, texts=("첫째 구역", "둘째 구역")):
    items = [("header", "Contents/header.xml", "application/xml"),
             ("section0", "Contents/section0.xml", "application/xml"),
             ("section1", "Contents/section1.xml", "application/xml")]
    extra = [("Contents/section1.xml", section_xml(para(texts[1])), 8)]
    return dict(hpf=hpf_xml(items, spine), section=section_xml(para(texts[0])), extra=extra)


add("spine_missing_section.hwpx",
    lambda: pkg(hpf=hpf_xml([("header", "Contents/header.xml", "application/xml"), ("section0", "Contents/section0.xml", "application/xml"),
                             ("section1", "Contents/section1.xml", "application/xml")], ["header", "section0", "section1"])),
    expect="reject", stage="package", reason="broken_manifest", notes="spine/manifest 가 Contents/section1.xml 을 가리키지만 ZIP 에 항목이 없다.",
    check=lambda d: _assert("Contents/section1.xml" in read_entry(d, "Contents/content.hpf").decode() and "Contents/section1.xml" not in py_open(d).namelist()))
add("spine_idref_not_in_manifest.hwpx", lambda: pkg(hpf=hpf_xml(None, ["header", "section0", "ghost"])),
    expect="reject", stage="package", reason="broken_manifest", notes="spine 의 itemref idref=\"ghost\" 가 manifest 에 없다.",
    check=lambda d: _assert(b'idref="ghost"' in read_entry(d, "Contents/content.hpf") and b'id="ghost"' not in read_entry(d, "Contents/content.hpf")))
add("manifest_duplicate_id.hwpx",
    lambda: pkg(hpf=hpf_xml([("header", "Contents/header.xml", "application/xml"), ("section0", "Contents/section0.xml", "application/xml"),
                             ("section0", "Contents/section1.xml", "application/xml")], ["header", "section0"]),
                extra=[("Contents/section1.xml", section_xml(para("중복 id")), 8)]),
    expect="reject", stage="package", reason="broken_manifest", notes="manifest 에 id=\"section0\" 항목이 둘(서로 다른 href).",
    check=lambda d: _assert(read_entry(d, "Contents/content.hpf").count(b'id="section0"') == 2))


def _pic_section(ref):
    return section_xml('<hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0"><hp:pic id="2"><hc:img binaryItemIDRef="%s"/></hp:pic></hp:run></hp:p>' % ref)


add("dangling_media_ref.hwpx", lambda: pkg(section=_pic_section("image9")),
    expect="reject", stage="package", reason="broken_manifest",
    notes="구역이 binaryItemIDRef=\"image9\" 를 참조하지만 manifest 에 없다. 정책 선택(열 때 거절). 완화(경고만)하려면 accept 로 바꾼다.",
    check=lambda d: _assert(b'binaryItemIDRef="image9"' in read_entry(d, "Contents/section0.xml") and b'id="image9"' not in read_entry(d, "Contents/content.hpf")))

_MEDIA_ITEMS = [("header", "Contents/header.xml", "application/xml"), ("section0", "Contents/section0.xml", "application/xml"),
                ("image1", "BinData/image1.png", "image/png")]
add("unused_media_kept.hwpx", lambda: pkg(hpf=hpf_xml(_MEDIA_ITEMS, ["header", "section0"]), extra=[("BinData/image1.png", png_1x1(), 0)]),
    expect="accept", stage="package", notes="manifest 와 BinData 에 image1.png 가 있지만 구역에서 참조하지 않는다. 수용하고 다시 쓸 때 보존해야 함.",
    check=lambda d: (accepted_ok(d), _assert(b"binaryItemIDRef" not in read_entry(d, "Contents/section0.xml") and "BinData/image1.png" in py_open(d).namelist())),
    detail=lambda d: {"entries_preserved": ["BinData/image1.png"]})
add("used_media_ok.hwpx", lambda: pkg(hpf=hpf_xml(_MEDIA_ITEMS, ["header", "section0"]), section=_pic_section("image1"), extra=[("BinData/image1.png", png_1x1(), 0)]),
    expect="accept", stage="package", notes="참조되는 미디어(hp:pic > hc:img binaryItemIDRef)와 manifest 가 일치하는 정상 사례.",
    check=lambda d: (accepted_ok(d), _assert(b'binaryItemIDRef="image1"' in read_entry(d, "Contents/section0.xml"))),
    detail=lambda d: {"media_refs": ["image1"]})
add("spine_order_differs_from_filenames.hwpx",
    lambda: pkg(**_two_section_files(["header", "section1", "section0"])),
    expect="accept", stage="package", notes="spine 순서가 section1, section0 이다. 문서 순서는 spine 순서(파일명 정렬이 아님).",
    check=lambda d: (accepted_ok(d, xml_names=("Contents/section0.xml", "Contents/section1.xml", "Contents/content.hpf")),
                     _assert(spine_sections(d) == ["Contents/section1.xml", "Contents/section0.xml"])),
    detail=lambda d: {"sections_in_spine_order": spine_sections(d),
                      "paragraph_texts": {n: paragraphs(read_entry(d, n)) for n in ("Contents/section1.xml", "Contents/section0.xml")}})
add("missing_container_xml.hwpx", lambda: build_zip(entries_of([f for f in base_files() if f[0] != "META-INF/container.xml"]))[0],
    expect="reject", stage="package", reason="broken_manifest", notes="META-INF/container.xml 항목 없음.",
    check=lambda d: _assert("META-INF/container.xml" not in py_open(d).namelist()))
add("container_points_to_missing_hpf.hwpx", lambda: pkg(container=container_xml("Contents/nothere.hpf")),
    expect="reject", stage="package", reason="broken_manifest", notes="container.xml 의 rootfile full-path 가 존재하지 않는 항목.",
    check=lambda d: _assert("Contents/nothere.hpf" not in py_open(d).namelist() and b"nothere.hpf" in read_entry(d, "META-INF/container.xml")))
add("container_path_escape.hwpx", lambda: pkg(container=container_xml("../Contents/content.hpf")),
    expect="reject", stage="package", reason="path_escape", notes="container.xml 의 rootfile full-path 가 '..' 요소를 쓴다.",
    check=lambda d: _assert(b"../Contents/content.hpf" in read_entry(d, "META-INF/container.xml")))


# ================================================================ XML 수준

def xml_case(name, sec_bytes, expect, reason=None, code=None, notes="", check=None, detail=None, new=None, stage="xml"):
    case(name, expect, stage, reason, code, notes, check, detail, new)(lambda: pkg(section=sec_bytes))


def has(blob):
    return lambda d: _assert(blob in read_entry(d, "Contents/section0.xml"))


SEC_DT = lambda dt, body=None: ('%s%s<hs:sec %s>%s</hs:sec>' % (DECL, dt, root_ns(("hs", "hp", "hc")), body or para("본문"))).encode("utf-8")
xml_case("dtd_in_section.hwpx", SEC_DT("<!DOCTYPE hs:sec>"), "reject", "dtd_or_entity", new=None,
         notes="section0.xml 에 DOCTYPE 선언(실체 없음). 모든 DTD 를 거절.", check=has(b"<!DOCTYPE"))
xml_case("external_entity.hwpx", SEC_DT('<!DOCTYPE hs:sec [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>', para("&xxe;")), "reject", "dtd_or_entity",
         notes="외부 일반 실체(file:///etc/passwd)를 선언하고 본문에서 참조(XXE).", check=has(b"SYSTEM"))
xml_case("internal_entity_bomb.hwpx",
         SEC_DT('<!DOCTYPE hs:sec [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;"><!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">'
                '<!ENTITY d "&c;&c;&c;&c;&c;&c;&c;&c;&c;&c;"><!ENTITY e "&d;&d;&d;&d;&d;&d;&d;&d;&d;&d;">]>', para("&e;")), "reject", "dtd_or_entity",
         notes="내부 실체 중첩 확장(billion laughs 축소판, 확장 시 10^5배). DTD 자체를 거절하므로 확장하지 않음.", check=has(b"<!ENTITY e"))
case("dtd_in_header.hwpx", "reject", "xml", "dtd_or_entity", None, "header.xml 에 DOCTYPE. 구역 외 XML 도 같은 규칙.",
     lambda d: _assert(b"<!DOCTYPE" in read_entry(d, "Contents/header.xml")))(
    lambda: pkg(header=('%s<!DOCTYPE hh:head><hh:head %s version="1.2" secCnt="1"><hh:refList/></hh:head>' % (DECL, root_ns(("hh", "hp", "hs")))).encode()))

_EUC_SEC = ('<?xml version="1.0" encoding="EUC-KR"?><hs:sec %s>%s</hs:sec>' % (root_ns(("hs", "hp", "hc")), para("안녕 한글"))).encode("euc-kr")
xml_case("xml_non_utf8_encoding_decl.hwpx", _EUC_SEC, "reject", "non_utf8",
         notes="XML 선언이 encoding=\"EUC-KR\" 이고 본문 바이트도 EUC-KR. 편집 가능한 XML 은 UTF-8 로 한정(§11).",
         check=lambda d: _assert(b'encoding="EUC-KR"' in read_entry(d, "Contents/section0.xml") and read_entry(d, "Contents/section0.xml").decode("euc-kr").count("한글") == 1))
xml_case("xml_utf8_decl_invalid_bytes.hwpx",
         (DECL + '<hs:sec %s>' % root_ns(("hs", "hp", "hc"))).encode() + b'<hp:p id="1"><hp:run><hp:t>' + EUC + b'</hp:t></hp:run></hp:p></hs:sec>',
         "reject", "non_utf8", notes="선언은 UTF-8 인데 본문에 EUC-KR 바이트(유효하지 않은 UTF-8 시퀀스).",
         check=lambda d: _assert(not _try_utf8(read_entry(d, "Contents/section0.xml"))))
xml_case("xml_utf16_bom.hwpx", b"\xff\xfe" + section_xml().decode("utf-8").replace('encoding="UTF-8"', 'encoding="UTF-16"').encode("utf-16-le"),
         "reject", "non_utf8", notes="UTF-16LE BOM + encoding=\"UTF-16\". UTF-8 외 거절.",
         check=lambda d: _assert(read_entry(d, "Contents/section0.xml")[:2] == b"\xff\xfe"))
xml_case("bom_utf8.hwpx", b"\xef\xbb\xbf" + section_xml(para("BOM 문서")), "accept",
         notes="UTF-8 BOM(EF BB BF)으로 시작하는 구역. 수용하고, 다시 쓸 때 BOM·선언을 그대로 유지해야 함.",
         check=lambda d: (accepted_ok(d, xml_names=("Contents/header.xml", "Contents/content.hpf")), _assert(read_entry(d, "Contents/section0.xml")[:3] == b"\xef\xbb\xbf"),
                          ET.fromstring(read_entry(d, "Contents/section0.xml"))),
         detail=lambda d: {"paragraph_texts": {"Contents/section0.xml": paragraphs(read_entry(d, "Contents/section0.xml"))}, "bom_preserved": True})
xml_case("forbidden_control_char.hwpx", section_xml(para("앞\x01뒤")), "reject", "forbidden_char",
         new="XML 1.0 에서 금지된 문자(U+0000–U+0008, U+000B, U+000C, U+000E–U+001F, U+FFFE, U+FFFF)가 본문이나 문자 참조에 있다.",
         notes="본문에 U+0001 원문 바이트.", check=has(b"\x01"))
xml_case("forbidden_char_ref.hwpx", section_xml(para("앞&#1;뒤")), "reject", "forbidden_char",
         notes="문자 참조 &#1; (XML 1.0 에서 허용되지 않는 문자).", check=has(b"&#1;"))
xml_case("malformed_xml.hwpx", ('%s<hs:sec %s><hp:p id="1"><hp:run><hp:t>x</hp:run></hp:t></hp:p></hs:sec>' % (DECL, root_ns(("hs", "hp", "hc")))).encode(),
         "reject", "malformed_xml", new="XML 이 well-formed 가 아니다(태그 불일치, 닫히지 않은 요소 등).",
         notes="태그 닫는 순서가 어긋남.", check=lambda d: _assert(_parse_fails(read_entry(d, "Contents/section0.xml"))))


def _parse_fails(b):
    try:
        ET.fromstring(b)
    except ET.ParseError:
        return True
    return False


def nested(depth):
    """루트(hs:sec)=깊이 1 로 세어 최대 깊이가 depth 인 구역. p(2) run(3) 일반 요소(n) t(1)."""
    n = depth - 4
    inner = "<hp:subList>" * n + "<hp:t>깊다</hp:t>" + "</hp:subList>" * n
    return section_xml('<hp:p id="1" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0">%s</hp:run></hp:p>' % inner)


xml_case("depth_129.hwpx", nested(MAX_DEPTH + 1), "reject", "depth_limit", "RESOURCE_LIMIT",
         new="XML 요소 중첩 깊이가 한도(128)를 넘었다(루트=깊이 1).", notes="최대 깊이 129. 재귀 구현이면 스택을 쓰지 않고 거절해야 함.",
         check=lambda d: _assert(max_depth(read_entry(d, "Contents/section0.xml")) == MAX_DEPTH + 1))
xml_case("depth_128_boundary.hwpx", nested(MAX_DEPTH), "accept", notes="최대 깊이 정확히 128: 수용.",
         check=lambda d: (accepted_ok(d), _assert(max_depth(read_entry(d, "Contents/section0.xml")) == MAX_DEPTH)))

_FAKE_SEC = ('%s<hs:sec xmlns:hs="%s" xmlns:p0="%s" xmlns:hp="%s">%s%s</hs:sec>'
             % (DECL, NS["hs"], NS["hp"], OTHER_NS, para("REAL", 1, "p0"), para("FAKE", 2, "hp"))).encode()
xml_case("same_prefix_other_namespace.hwpx", _FAKE_SEC, "accept",
         notes="prefix hp 가 HWPX 문단 URI 가 아닌 http://example.com/not-hwpx/paragraph 에 묶임. 접두어가 아니라 URI 로 해석하므로 문단은 REAL(prefix p0) 하나뿐이고 FAKE 는 HWPX 문단이 아니다.",
         check=lambda d: (accepted_ok(d, xml_names=("Contents/header.xml", "Contents/content.hpf")), _assert(paragraphs(read_entry(d, "Contents/section0.xml")) == ["REAL"])),
         detail=lambda d: {"paragraph_texts": {"Contents/section0.xml": paragraphs(read_entry(d, "Contents/section0.xml"))}, "non_hwpx_prefix_elements": ["hp:p(FAKE)"]})

_DEF_SEC = ('%s<hs:sec xmlns:hs="%s" xmlns="%s">'
            '<p id="1"><run><t>DEFAULT-NS</t></run></p>'
            '<x xmlns="%s"><p id="2"><t>OTHER</t></p></x>'
            '<p xmlns="%s" id="3"><run><t>AGAIN</t></run></p>'
            '</hs:sec>' % (DECL, NS["hs"], NS["hp"], OTHER_NS, NS["hp"])).encode()
xml_case("default_namespace_rebinding.hwpx", _DEF_SEC, "accept",
         notes="기본 namespace 를 HWPX 문단 URI 로 선언하고, 중간에 다른 URI 로 재바인딩했다가 다시 원래 URI 로 복원. HWPX 문단은 DEFAULT-NS 와 AGAIN 둘(OTHER 는 제외).",
         check=lambda d: (accepted_ok(d, xml_names=("Contents/header.xml", "Contents/content.hpf")), _assert(paragraphs(read_entry(d, "Contents/section0.xml")) == ["DEFAULT-NS", "AGAIN"])),
         detail=lambda d: {"paragraph_texts": {"Contents/section0.xml": paragraphs(read_entry(d, "Contents/section0.xml"))}})

_TXT = "가&#xAC00;&amp;&lt;<![CDATA[ <raw> & ]]>끝"
xml_case("char_refs_and_cdata.hwpx", section_xml(para(_TXT)), "accept",
         notes="텍스트 런에 16진 문자 참조(&#xAC00;), &amp;, &lt;, CDATA 가 섞여 있다. 해석된 텍스트는 '가가&< <raw> & 끝', 원문 바이트는 그대로 보관해야 함.",
         check=lambda d: (accepted_ok(d, xml_names=("Contents/header.xml", "Contents/content.hpf")), _assert(paragraphs(read_entry(d, "Contents/section0.xml")) == ["가가&< <raw> & 끝"]),
                          has(b"&#xAC00;")(d), has(b"<![CDATA[ <raw> & ]]>")(d)),
         detail=lambda d: {"paragraph_texts": {"Contents/section0.xml": paragraphs(read_entry(d, "Contents/section0.xml"))}, "raw_notation_preserved": ["&#xAC00;", "&amp;", "<![CDATA[ <raw> & ]]>"]})


# ---------------------------------------------------------------- 실행

def main(argv):
    args = [a for a in argv[1:] if not a.startswith("--")]
    expected_path = None
    if "--expected" in argv:
        expected_path = argv[argv.index("--expected") + 1]
        args = [a for a in args if a != expected_path]
    if len(args) != 1:
        sys.stderr.write(__doc__)
        return 2
    outdir = args[0]
    os.makedirs(outdir, exist_ok=True)
    _selfcheck_namespaces()
    names = [c["name"] for c in CASES]
    assert len(names) == len(set(names)), "duplicate case names"
    cases_out, total = {}, 0
    for c in CASES:
        data = c["fn"]()
        if c["check"]:
            try:
                c["check"](data)
            except Exception as e:
                raise SystemExit("check failed for %s: %r" % (c["name"], e))
        if c["expect"] == "accept":
            accepted_ok(data, xml_names=tuple(n for n in ("Contents/section0.xml", "Contents/header.xml", "Contents/content.hpf", "META-INF/container.xml")
                                              if n in py_open(data).namelist()))
        with open(os.path.join(outdir, c["name"]), "wb") as f:
            f.write(data)
        total += len(data)
        ent = {"expect": c["expect"], "code": c["code"], "reason": c["reason"], "stage": c["stage"], "bytes": len(data), "notes": c["notes"]}
        if c["detail"]:
            ent["detail"] = c["detail"](data)
        cases_out[c["name"]] = ent
    known = {"encrypted", "duplicate_name", "case_collision", "path_escape", "zip64_required", "bad_mimetype", "broken_manifest",
             "dtd_or_entity", "non_utf8"}
    used = {c["reason"] for c in CASES if c["reason"]}
    new = [{"reason": r, "meaning": NEW_REASONS[r]} for r in sorted(used - known)]
    assert {n["reason"] for n in new} <= set(NEW_REASONS), "reason without meaning: %s" % (used - known - set(NEW_REASONS))
    doc = {
        "schema": 1,
        "notes": "code 는 열기 단계 오류 코드. reason 은 진단의 reason. stage 는 거절이 일어나는 층(zip|package|xml). "
                 "detail 은 accept 사례에서 파서가 돌려줘야 하는 관찰 값. bytes 는 생성 파일 크기(참고용).",
        "limits": {"max_entries": MAX_ENTRIES, "max_xml_depth": MAX_DEPTH, "max_cumulative_inflate_bytes": CAP},
        "new_reasons": new,
        "cases": cases_out,
    }
    path = expected_path or os.path.join(outdir, "adversarial_expected.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(doc, f, ensure_ascii=False, indent=2)
        f.write("\n")
    assert total < 5 * MIB, "total size %d" % total
    print("wrote %d cases, %d bytes total -> %s" % (len(cases_out), total, outdir))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
