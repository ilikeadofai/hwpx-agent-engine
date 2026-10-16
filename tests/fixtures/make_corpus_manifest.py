#!/usr/bin/env python3
"""corpus/ 의 HWPX 파일을 검사해 corpus/manifest.json 을 다시 만든다 (stdlib only).

사용: python3 -I tests/fixtures/make_corpus_manifest.py
출처 정보는 아래 SOURCES 표에 고정되어 있고, 구조 정보는 파일에서 직접 읽는다.
"""
import hashlib
import json
import os
import re
import sys
import zipfile
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus")

SKILL = ("jkf87/hwpx-skill", "ba90fc5ae15f4aead8dab4f9d69d4830b9569859")
KORDOC = ("chrisryugj/kordoc", "cd87f6e2ecc03d5ef773c8fc04be0b15d45da4a4")
EASY = ("nathankim0/easy-hwp", "039b7d0926b867b0a21ca7b3113666fef5c56897")
RHWP = ("edwardkim/rhwp", "1a76570e833917d15817415a53c09ad61ab3203f")

SOURCES = {
    "bodojaryo-reference.hwpx": (SKILL, "assets/bodojaryo-reference.hwpx", "보도자료 참조 서식"),
    "gyehoek-reference.hwpx": (SKILL, "assets/gyehoek-reference.hwpx", "계획서 참조 서식"),
    "problem-answer-reference.hwpx": (SKILL, "assets/problem-answer-reference.hwpx", "문제·정답 참조 서식"),
    "research-school-plan-reference.hwpx": (SKILL, "assets/research-school-plan-reference.hwpx", "연구학교 계획서 참조 서식, 다중 구역"),
    "ilban-gian.hwpx": (KORDOC, "templates/일반기안문_서식.hwpx", "일반기안문 서식"),
    "gani-gian.hwpx": (KORDOC, "templates/간이기안문_서식.hwpx", "간이기안문 서식"),
    "irb-sample.hwpx": (EASY, "IRB서식_샘플.hwpx", "IRB 서식 샘플, 비한컴 생성물로 보임"),
    "ref_empty.hwpx": (RHWP, "samples/hwpx/ref/ref_empty.hwpx", "빈 문서"),
    "anchor_char_height.hwpx": (RHWP, "samples/anchor_char_line_height/anchor_char_height.hwpx", "앵커 글자 줄 높이 재현 자료"),
    "render-p35-font-native-bitmap.hwpx": (RHWP, "samples/render-p35-font-native-bitmap.hwpx", "글꼴 렌더 재현 자료"),
}

SEC_RE = re.compile(r"^(?:.*/)?section\d+\.xml$")


def local(tag):
    return tag.rsplit("}", 1)[-1]


def inspect(path):
    z = zipfile.ZipFile(path)
    infos = z.infolist()
    names = [i.filename for i in infos]
    out = {"zip_entries": len(infos)}
    hpf_name = next((n for n in names if n.endswith("content.hpf")), None)
    manifest, spine = {}, []
    if hpf_name:
        root = ET.fromstring(z.read(hpf_name))
        for el in root.iter():
            if local(el.tag) == "item":
                manifest[el.get("id")] = el.get("href")
            elif local(el.tag) == "itemref":
                spine.append(el.get("idref"))
    base = os.path.dirname(hpf_name) if hpf_name else ""
    spine_hrefs = [manifest.get(i) for i in spine]
    out["content_hpf"] = hpf_name
    out["spine_idrefs"] = spine
    # href 해석: 패키지 루트 기준이 먼저, 없으면 content.hpf 폴더 기준 (irb-sample 은 후자)
    resolved, how = [], set()
    for h in spine_hrefs:
        if not h or not SEC_RE.match(h):
            continue
        if h in names:
            resolved.append(h)
            how.add("package-root")
        elif (base + "/" + h) in names:
            resolved.append(base + "/" + h)
            how.add("hpf-dir")
        else:
            resolved.append(None)
            how.add("unresolved")
    out["sections_in_spine_order"] = resolved
    out["section_href_base"] = sorted(how)
    secs = [n for n in names if SEC_RE.match(n)]
    out["section_entries_all"] = secs
    blob = {n: z.read(n).decode("utf-8") for n in secs}
    for key, tag in (("has_tbl", "hp:tbl"), ("has_pic", "hp:pic"), ("has_equation", "hp:equation")):
        out[key] = any(re.search("<" + tag + r"[\s/>]", t) for t in blob.values())
    out["signature_entries"] = [n for n in names if "signature" in n.lower()]
    out["encryption_entries"] = [n for n in names if "encrypt" in n.lower() or "encryption" in n.lower()]
    out["encrypted_flag_entries"] = [i.filename for i in infos if i.flag_bits & 1]
    out["compress_types"] = sorted({i.compress_type for i in infos})
    out["data_descriptor_entries"] = sum(1 for i in infos if i.flag_bits & 8)
    out["directory_entries"] = [n for n in names if n.endswith("/")]
    out["mimetype_first_stored"] = bool(infos) and infos[0].filename == "mimetype" and infos[0].compress_type == 0
    out["binary_entries"] = [n for n in names if n.startswith("BinData/")]
    return out


def main():
    items = []
    for fname in sorted(os.listdir(CORPUS)):
        if not fname.endswith(".hwpx"):
            continue
        path = os.path.join(CORPUS, fname)
        data = open(path, "rb").read()
        (repo, commit), spath, note = SOURCES[fname]
        items.append({
            "file": fname,
            "sha256": hashlib.sha256(data).hexdigest(),
            "bytes": len(data),
            "source_repo": repo,
            "source_commit": commit,
            "source_path": spath,
            "license": "MIT",
            "kind": "public-template",
            "notes": note,
            "observed": inspect(path),
        })
    with open(os.path.join(CORPUS, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(items, f, ensure_ascii=False, indent=2)
        f.write("\n")
    print("wrote", len(items), "entries")


if __name__ == "__main__":
    sys.exit(main())
