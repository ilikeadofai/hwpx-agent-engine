#!/usr/bin/env python3
"""Local-only Windows round-2 artifacts. Never sends user fixtures to a model.

Uses the real stdio MCP engine. Private inputs/logs/outputs stay under spikes/work.
Existing outputs are refused. Originals are hashed before and after, never modified.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import shutil
import xml.etree.ElementTree as ET
import zipfile

from evaluate_mcp import McpClient, REPO_ROOT, build_hwpx, synth_png

HP = "{http://www.hancom.co.kr/hwpml/2011/paragraph}"
HC = "{http://www.hancom.co.kr/hwpml/2011/core}"
HH = "{http://www.hancom.co.kr/hwpml/2011/head}"
OPF = "{http://www.idpf.org/2007/opf/}"
SECTION = "Contents/section0.xml"
HEADER = "Contents/header.xml"
DEFAULT_RETURN = REPO_ROOT / "spikes/work/windows-return-20261017-2047/return"
DEFAULT_HANDOFF = REPO_ROOT / "spikes/work/windows-handoff-20261017-v2"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def payloads(path):
    with zipfile.ZipFile(path) as z:
        assert z.testzip() is None
        return {i.filename: z.read(i) for i in z.infolist()}


def rewrite(source, dest, changes):
    """Experimental/prep ZIP rewrite, explicitly NOT an engine operation."""
    with zipfile.ZipFile(source) as src, zipfile.ZipFile(dest, "w") as out:
        for info in src.infolist():
            out.writestr(info, changes.get(info.filename, src.read(info)))


def audit(path):
    """Observed structure, not an XSD validator or a native-open claim."""
    p = payloads(path)
    head, section, hpf = [ET.fromstring(p[n]) for n in [HEADER, SECTION, "Contents/content.hpf"]]
    local = lambda e: e.tag.split("}")[-1]
    refs = head.find(HH + "refList")
    lists = {local(x): {"attributes": x.attrib, "actualCount": len(x),
                        "childKinds": dict(Counter(local(c) for c in x))} for x in refs}
    count_errors = [{"tag": local(x), "attribute": k, "declared": x.attrib[k], "actual": len(x)}
                    for x in head.iter() for k in ["itemCnt", "fontCnt"]
                    if k in x.attrib and int(x.attrib[k]) != len(x)]
    maps = {attr: {x.attrib.get("id") for x in refs.find(HH + name)}
            if refs.find(HH + name) is not None else set() for attr, name in [
                ("charPrIDRef", "charProperties"), ("paraPrIDRef", "paraProperties"),
                ("styleIDRef", "styles"), ("borderFillIDRef", "borderFills"),
                ("tabPrIDRef", "tabProperties"), ("outlineShapeIDRef", "numberings")]}
    unresolved = []
    for x in list(head.iter()) + list(section.iter()):
        for attr, ids in maps.items():
            value = x.attrib.get(attr)
            if value is None or (local(x) == "paraHead" and attr == "charPrIDRef" and value == "4294967295"):
                continue
            if value not in ids:
                unresolved.append({"tag": local(x), "attribute": attr, "value": value})
    items = {x.attrib["id"]: x.attrib for x in hpf.findall(OPF + "manifest/" + OPF + "item")}
    media = []
    for image in section.iter(HC + "img"):
        item = items.get(image.attrib.get("binaryItemIDRef"), {})
        data = p.get(item.get("href"), b"")
        actual = ("image/png" if data.startswith(b"\x89PNG\r\n\x1a\n") else
                  "image/jpeg" if data.startswith(b"\xff\xd8") else
                  "image/bmp" if data.startswith(b"BM") else "unknown")
        media.append({"reference": image.attrib, "manifest": item, "actualType": actual,
                      "typeMatches": item.get("media-type") == actual})
    shapes = {}
    for kind in ["secPr", "tbl", "tc", "pic", "p"]:
        elems = list(section.iter(HP + kind))
        shapes[kind] = [{"attributes": x.attrib, "children": [local(c) for c in x]} for x in elems[:1]]
    with zipfile.ZipFile(path) as z:
        entries = [{"name": x.filename, "method": x.compress_type, "size": x.file_size} for x in z.infolist()]
    return {"path": str(path), "sha256": sha(path), "entries": entries,
            "mimetype": p.get("mimetype", b"").decode(),
            "rootNamespaces": {n: {k.decode(): v.decode() for k, v in
                                    re.findall(rb'xmlns:([^= ]+)="([^"]+)"', p[n].split(b">", 2)[1])}
                               for n in [HEADER, SECTION, "Contents/content.hpf"]},
            "headChildren": [local(x) for x in head], "referenceLists": lists,
            "countErrors": count_errors, "unresolvedReferences": unresolved,
            "manifest": items, "missingManifestPayloads": [i["href"] for i in items.values() if i["href"] not in p],
            "spine": [i.attrib for i in hpf.findall(OPF + "spine/" + OPF + "itemref")],
            "shapes": shapes, "images": media}


def variants(out, failed):
    out.mkdir()
    old = payloads(failed)
    paths = {}
    notes = {}

    def register(name, note):
        paths[name] = out / (name + ".hwpx")
        notes[name] = note
        return paths[name]

    shutil.copyfile(failed, register("00_failed_original", "Round-1 failed bytes, exact copy"))
    # Object deletion only; leave the broken old header/package/section defaults in place.
    text = old[SECTION].decode()
    text = re.sub(r'<hp:p\b[^>]*><hp:run charPrIDRef="0"><hp:tbl\b.*?</hp:tbl></hp:run></hp:p>', "", text)
    text = re.sub(r'<hp:p\b[^>]*><hp:run charPrIDRef="0"><hp:pic\b.*?</hp:pic></hp:run></hp:p>', "", text)
    assert "<hp:tbl" not in text and "<hp:pic" not in text
    rewrite(failed, register("v_0_old_text_only", "Only remove table/picture from failed section"), {SECTION: text.encode()})
    for name, table, picture in [("v_a_minimal_paragraph", False, False), ("v_b_plus_table", True, False),
                                  ("v_c_plus_picture", False, True), ("v_d_all_corrected", True, True)]:
        build_hwpx(register(name, "Complete public native package/header/section; synthetic content"),
                   old["BinData/image1.png"], table=table, picture=picture)
    fixed = payloads(paths["v_d_all_corrected"])
    rewrite(paths["v_d_all_corrected"], register("h_package_only", "Old header/section; only package infrastructure changed"),
            {HEADER: old[HEADER], SECTION: old[SECTION]})
    rewrite(failed, register("h_header_only", "Only replace header; old outlineShapeIDRef=0 remains a known mismatch"),
            {HEADER: fixed[HEADER]})
    old_sec = old[SECTION].decode()
    new_sec = fixed[SECTION].decode()
    sec_only = new_sec
    for kind in ["tbl", "pic"]:
        old_shape = re.search(rf'<hp:{kind}\b.*?</hp:{kind}>', old_sec).group()
        sec_only = re.sub(rf'<hp:{kind}\b.*?</hp:{kind}>', lambda _: old_shape, sec_only)
    # Section experiment includes reference header so secPr numbering references resolve.
    rewrite(paths["v_d_all_corrected"], register("h_section_defaults_old_shapes", "Fixed package/header/paragraph/secPr; both old shapes"),
            {SECTION: sec_only.encode()})
    for kind in ["tbl", "pic"]:
        old_shape = re.search(rf'<hp:{kind}\b.*?</hp:{kind}>', old_sec).group()
        changed = re.sub(rf'<hp:{kind}\b.*?</hp:{kind}>', lambda _: old_shape, new_sec)
        rewrite(paths["v_d_all_corrected"], register("h_old_" + kind, "Fixed baseline with only old " + kind + " substituted"),
                {SECTION: changed.encode()})
    dump(out / "manifest.json", [{"file": p.name, "sha256": sha(p), "experiment": notes[n],
                                  "nativeOpen": "not_checked"} for n, p in paths.items()])
    return paths


def run_engine(args, out):
    artifacts = out / "engine_on_user_fixtures"
    artifacts.mkdir()
    inputs = out / "inputs"
    inputs.mkdir()
    originals = sorted((args.return_root / "user_fixtures").glob("*.hwpx"))
    before_hashes = {str(p): sha(p) for p in originals}
    for source in originals:
        shutil.copyfile(source, inputs / source.name)
    replacement = inputs / "R2-replacement.png"
    replacement.write_bytes(synth_png(160, 80, True))
    logs = []
    client = McpClient(args.binary, out, args.rhwp, Path("/usr/bin/bwrap"),
                       inputs, 60, logs.append)
    reports = []

    def call(tool, **params):
        response, error = client.call(tool, params)
        assert not error and response.get("ok"), (tool, response)
        return response["result"]

    def query(kind, **params):
        return call("element_query", kind=kind, **params)["matches"]

    def export(name, source):
        dest = artifacts / name
        result = call("document_export", path=str(dest))
        assert dest.is_file()
        p, s = payloads(dest), payloads(source)
        changed = [n for n in s if p.get(n) != s[n]]
        assert set(changed) <= {SECTION, HEADER, "Contents/content.hpf", "Preview/PrvText.txt"}, changed
        return {"file": dest.name, "sha256": sha(dest), "export": result,
                "changedEntries": changed, "addedEntries": sorted(set(p) - set(s)),
                "nativeOpen": "not_checked"}

    def replace_and_format(para, label):
        detail = call("element_get", target=para["nodeId"])
        count = detail["graphemeCount"]
        assert count > 0
        return [{"command": "text.replace", "params": {"target": para["nodeId"],
                 "range": {"start": 0, "end": count}, "text": "R2 확인 " + label}},
                {"command": "paragraph.format", "params": {"target": para["nodeId"],
                 "alignment": "center", "lineSpacing": {"kind": "percent", "value": 180}}}]

    try:
        source = inputs / "01_text.hwpx"
        call("document_open", path=str(source))
        para = query("paragraph", textContains="본문 1")[0]
        call("command_batch", expectedVersion=0, steps=replace_and_format(para, "문단 텍스트·가운데 정렬·줄간격 180%"))
        reports.append(export("01_text_edited.hwpx", source))
        call("document_close")

        source = inputs / "02_table.hwpx"
        call("document_open", path=str(source))
        cell = query("cell")[0]
        refusal, error = client.call("cell_set_text", {"target": cell["nodeId"], "text": "R2 확인 셀", "expectedVersion": 0})
        assert error and not refusal["ok"] and cell["notEditableReason"] == "cell_paragraph_count"
        para = query("paragraph", ancestor=cell["nodeId"])[0]
        call("command_batch", expectedVersion=0, steps=replace_and_format(para, "셀 첫 문단 교체·나머지 문단 보존"))
        reports.append({**export("02_table_edited.hwpx", source), "cellSetTextRefusal": refusal,
                        "route": "text.replace on first cell paragraph + paragraph.format; 3 paragraphs preserved"})
        call("document_close")

        source = inputs / "03_picture_글자처럼취급.hwpx"
        call("document_open", path=str(source))
        picture = query("picture")[0]
        assert picture["editable"], picture
        call("image_replace", expectedVersion=0, target=picture["nodeId"], imagePath=str(replacement))
        reports.append({**export("03_picture_글자처럼취급_direct_edited.hwpx", source),
                        "marker": "none: source has no body text and engine 0.1 cannot insert text"})
        call("document_close")

        # Explicitly authorized non-engine preparation. Never label these bytes engine output.
        prep = inputs / "03_picture_글자처럼취급_prep.hwpx"
        original_section = payloads(source)[SECTION]
        needle = b"<hp:t/>"
        assert original_section.count(needle) == 1
        replacement_text = "<hp:t>R2 확인 그림 교체·가운데 정렬·줄간격 180%</hp:t>".encode()
        offset = original_section.index(needle)
        rewrite(source, prep, {SECTION: original_section.replace(needle, replacement_text, 1)})
        shutil.copyfile(prep, artifacts / prep.name)
        prep_record = {"file": prep.name, "kind": "NON_ENGINE_PREPARATION", "sourceSha256": sha(source),
                       "sha256": sha(prep), "entry": SECTION, "uncompressedByteOffset": offset,
                       "removedUtf8": needle.decode(), "insertedUtf8": replacement_text.decode(),
                       "entryBeforeSha256": hashlib.sha256(original_section).hexdigest(),
                       "entryAfterSha256": hashlib.sha256(payloads(prep)[SECTION]).hexdigest(),
                       "zipNote": "zipfile rewrites ZIP records/compression; all other uncompressed payloads identical"}
        assert {n for n, b in payloads(source).items() if payloads(prep)[n] != b} == {SECTION}
        reports.append(prep_record)
        call("document_open", path=str(prep))
        picture = query("picture")[0]
        para = query("paragraph", textContains="R2 확인")[0]
        call("command_batch", expectedVersion=0, steps=[
            {"command": "paragraph.format", "params": {"target": para["nodeId"], "alignment": "center",
             "lineSpacing": {"kind": "percent", "value": 180}}},
            {"command": "image.replace", "params": {"target": picture["nodeId"], "imagePath": str(replacement)}}])
        reports.append(export("03_picture_글자처럼취급_prep_edited.hwpx", prep))
        call("document_close")

        source = inputs / "04_complex.hwpx"
        call("document_open", path=str(source))
        untouched = artifacts / "04_complex_passthrough.hwpx"
        call("document_export", path=str(untouched))
        assert sha(untouched) == sha(source)
        equation = query("equation")[0]
        refusal, error = client.call("text_replace", {"target": equation["nodeId"], "range": {"start": 0, "end": 1},
                                                       "text": "R2 확인", "expectedVersion": 0})
        assert error and not refusal["ok"]
        merged = next(c for c in query("cell") if c.get("notEditableReason") == "merged_cell")
        merged_refusal, error = client.call("cell_set_text", {"target": merged["nodeId"], "text": "R2 확인", "expectedVersion": 0})
        assert error and not merged_refusal["ok"]
        after_refusal = artifacts / "04_complex_after_refusals.hwpx"
        call("document_export", path=str(after_refusal))
        assert sha(after_refusal) == sha(source)
        para = next(p for p in query("paragraph", textContains="모든 국민은 직업선택") if p["editable"])
        # This plain paragraph is outside controls. Never overwrite equation/chart/memo/merged cells.
        call("command_batch", expectedVersion=0, steps=replace_and_format(para, "복합 문서의 지원 문단만 편집"))
        record = export("04_complex_supported_paragraph_edited.hwpx", source)
        a, b = payloads(source), payloads(artifacts / record["file"])
        opaque_kinds = ["equation", "chart", "rect", "polygon", "container", "fieldBegin", "fieldEnd", "header", "footer"]
        opaque_checks = {}
        for kind in opaque_kinds:
            # Semantic subtrees; separate unchanged package entries are byte-checked below.
            original = [ET.tostring(x) for x in ET.fromstring(a[SECTION]).iter(HP + kind)]
            edited = [ET.tostring(x) for x in ET.fromstring(b[SECTION]).iter(HP + kind)]
            assert original == edited, kind
            opaque_checks[kind] = {"count": len(original), "subtreesIdentical": True}
        assert all(a[n] == b[n] for n in a if n.startswith(("Chart/", "BinData/")) or n == "Contents/memoExtended.xml")
        reports.append({**record, "equationRefusal": refusal, "mergedCellRefusal": merged_refusal,
                        "passthroughSha256": sha(untouched), "afterRefusalsSha256": sha(after_refusal),
                        "opaquePreservation": opaque_checks})
        call("document_close")
    finally:
        (out / "engine-stderr.log").write_text(client.close(), encoding="utf-8")
        (out / "engine-responses.jsonl").write_text("".join(json.dumps(x, ensure_ascii=False) + "\n" for x in logs), encoding="utf-8")
        dump(out / "engine-results.json", reports)
    assert before_hashes == {str(p): sha(p) for p in originals}
    dump(out / "originals-preserved.json", before_hashes)
    return reports


def verify_outputs(args, out):
    """Reopen the exported bytes in a fresh MCP process and inspect observable edits."""
    artifacts = out / "engine_on_user_fixtures"
    logs, checks = [], []
    client = McpClient(args.binary, out, args.rhwp, Path("/usr/bin/bwrap"),
                       out / "inputs", 60, logs.append)

    def call(tool, **params):
        response, error = client.call(tool, params)
        assert not error and response.get("ok"), (tool, response)
        return response["result"]

    try:
        for name in ["01_text_edited.hwpx", "02_table_edited.hwpx",
                     "03_picture_글자처럼취급_direct_edited.hwpx", "03_picture_글자처럼취급_prep_edited.hwpx",
                     "04_complex_supported_paragraph_edited.hwpx"]:
            path = artifacts / name
            call("document_open", path=str(path))
            record = {"file": name, "reopen": "passed", "sha256": sha(path)}
            if "direct" not in name:
                marked = call("element_query", kind="paragraph", textContains="R2 확인")["matches"]
                assert len(marked) == 1
                detail = call("element_get", target=marked[0]["nodeId"])
                assert detail["alignment"] == "CENTER" and detail["lineSpacingPercent"] == 180, detail
                record["markerAndFormat"] = {"text": detail["text"], "alignment": detail["alignment"],
                                              "lineSpacingPercent": detail["lineSpacingPercent"]}
            if name.startswith("03_picture"):
                picture = call("element_query", kind="picture")["matches"][0]
                assert picture["editable"]
                assert picture["media"]["sha256"] == sha(out / "inputs/R2-replacement.png")
                record["replacementMediaSha256"] = picture["media"]["sha256"]
            if name.startswith("02_table"):
                s = ET.fromstring(payloads(path)[SECTION])
                cells = list(s.iter(HP + "tc"))
                assert [len(c.find(HP + "subList")) for c in cells] == [3, 3, 3, 3]
                record["cellParagraphCounts"] = [3, 3, 3, 3]
            call("document_close")
            checks.append(record)
        # Independently compare opaque element byte spans using the stdlib-only oracle lexer.
        import sys
        sys.path.insert(0, str(REPO_ROOT / "tests/oracle"))
        from oracle import parse_xml
        a = payloads(out / "inputs/04_complex.hwpx")[SECTION]
        b = payloads(artifacts / "04_complex_supported_paragraph_edited.hwpx")[SECTION]
        kinds = {"equation", "chart", "rect", "polygon", "container", "fieldBegin", "fieldEnd", "header", "footer"}
        spans = lambda data: [(x.tag, data[x.start:x.end]) for x in parse_xml(data).root.walk()
                               if x.tag.split("}")[-1] in kinds]
        assert spans(a) == spans(b)
        checks.append({"file": "04_complex_supported_paragraph_edited.hwpx", "opaqueSubtreesByteExact": len(spans(a))})
    finally:
        client.close()
        dump(out / "reopen-verification.json", checks)
        (out / "reopen-responses.jsonl").write_text("".join(json.dumps(x, ensure_ascii=False) + "\n" for x in logs), encoding="utf-8")
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--return-root", type=Path, default=DEFAULT_RETURN)
    parser.add_argument("--handoff-root", type=Path, default=DEFAULT_HANDOFF)
    parser.add_argument("--output-dir", type=Path, default=REPO_ROOT / "spikes/work/windows-round2")
    parser.add_argument("--binary", type=Path, default=REPO_ROOT / "target/debug/hae-mcp")
    parser.add_argument("--rhwp", type=Path, default=REPO_ROOT / "spikes/vendor/rhwp-src/target/release/rhwp")
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(exist_ok=True, parents=True)
    for name in ["fixture_variants", "engine_on_user_fixtures", "inputs", "structure-audit.json", "engine-results.json"]:
        if (out / name).exists():
            parser.error(f"refusing to overwrite existing evidence: {out / name}")
    failed = args.handoff_root / "engine_inputs/01_original.hwpx"
    paths = variants(out / "fixture_variants", failed)
    compared = [failed, *sorted((args.return_root / "user_fixtures").glob("*.hwpx")),
                *sorted((args.handoff_root / "s4_inputs").glob("*_0_*.hwpx")), *paths.values()]
    dump(out / "structure-audit.json", [audit(p) for p in compared])
    run_engine(args, out)
    verify_outputs(args, out)
    sums = [f"{sha(p)}  {p.relative_to(out)}" for p in sorted(out.rglob("*.hwpx"))]
    (out / "SHA256SUMS.txt").write_text("\n".join(sums) + "\n", encoding="utf-8")
    print(f"Round-2 artifacts: {out}")


if __name__ == "__main__":
    main()
