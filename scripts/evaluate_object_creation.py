#!/usr/bin/env python3
"""Synthetic report object workflow through the real stdio MCP server.

No personal report is read. New output directories preserve prior evidence.
The test exercises native objects, independent commit/export gates, stable IDs,
six figure/caption associations, exact Undo/Redo, project reopening and preview.
Windows Hancom comparison is always not_checked.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import posixpath
import sys
import xml.etree.ElementTree as ET
import zipfile

from evaluate_mcp import McpClient, REPO_ROOT, synth_png

HP = "http://www.hancom.co.kr/hwpml/2011/paragraph"
HC = "http://www.hancom.co.kr/hwpml/2011/core"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=REPO_ROOT / "target/debug/hae-mcp")
    parser.add_argument("--rhwp", type=Path, default=REPO_ROOT / "spikes/vendor/rhwp-src/target/release/rhwp")
    parser.add_argument("--font-dir", type=Path, default=Path.home() / ".local/lib/hwpx-agent-engine/20261019-18a49567/bin/fonts")
    parser.add_argument("--bwrap", type=Path, default=Path("/usr/bin/bwrap"))
    parser.add_argument("--raster", type=Path)
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        parser.error("output directory must be empty")
    logs, checks = [], []
    summary = {"nativeComparison": "not_checked", "status": "failed", "assertions": checks}
    client = None

    def check(name, ok, detail=None):
        checks.append({"name": name, "ok": bool(ok), "detail": detail})
        if not ok:
            raise AssertionError(f"{name}: {detail}")

    def call(tool, params):
        response, error = client.call(tool, params)
        check(f"{tool}_ok", response.get("ok") and not error,
              None if response.get("ok") and not error else response)
        return response["result"]

    def query(kind):
        return call("element_query", {"kind": kind})["matches"]

    def para(text):
        hits = [m for m in query("paragraph") if m.get("textPreview") == text]
        check(f"unique_{text}", len(hits) == 1, len(hits))
        return hits[0]["nodeId"]

    def export(name):
        path = out / name
        call("document_export", {"path": str(path)})
        return path.read_bytes()

    try:
        client = McpClient(args.binary.resolve(), out, args.rhwp.resolve(), args.bwrap.resolve(),
                           args.font_dir.resolve(), 180, logs.append, args.raster)
        tools = {t["name"] for t in client.rpc("tools/list", {})["result"]["tools"]}
        check("four_new_tools", {"table_insert", "table_delete", "image_insert", "equation_insert"} <= tools)
        paragraphs = ["구역 설정 보존", "★삭제 후 작성할 것★", "결과표 자리", "수식 자리", "지도교사: "]
        paragraphs += [f"[그림{i}] 합성 실험 결과" for i in range(1, 7)]
        source = out / "created.hwpx"
        call("document_create", {"path": str(source), "paragraphs": paragraphs, "title": "객체 생성 회귀 보고서"})
        call("document_open", {"path": str(source)})
        for kind in ("picture", "equation", "table"):
            check(f"initial_no_{kind}", not query(kind))
        notice = para("★삭제 후 작성할 것★")
        call("table_insert", {"expectedVersion": 0, "target": notice, "position": "after",
             "rows": 1, "columns": 1, "width": 28000, "rowHeight": 2400, "cells": [["유의사항"]]})
        baseline_path = out / "baseline.hwpx"
        baseline = export(baseline_path.name)
        call("document_close", {})
        call("document_open", {"path": str(baseline_path)})
        notice_table = query("table")[0]["nodeId"]
        notice = para("★삭제 후 작성할 것★")
        result_anchor, equation_anchor = para("결과표 자리"), para("수식 자리")
        teacher = para("지도교사: ")
        figure_bytes = {}
        steps = [
            {"command": "table.delete", "params": {"target": notice_table}},
            {"command": "paragraph.delete", "params": {"target": notice}},
            {"command": "table.insert", "params": {"target": result_anchor, "position": "after", "rows": 3,
                "columns": 2, "width": 28000, "rowHeight": 2400,
                "cells": [["방법", "결과"], ["수치 적분", "0.3333"], ["정확값", "1/3"]]}},
            {"command": "equation.insert", "params": {"target": equation_anchor, "position": "after",
                "script": "int _ 0 ^ 1 x ^ 2 dx = 1 over 3", "width": 20000, "height": 2400}},
        ]
        for i in range(1, 7):
            path = out / f"figure-{i}.png"
            data = synth_png(24 + i, 16 + i, True)
            path.write_bytes(data)
            figure_bytes[i] = data
            steps.append({"command": "image.insert", "params": {
                "target": para(f"[그림{i}] 합성 실험 결과"), "position": "before", "path": str(path),
                "width": 12000, "height": 6000}})
        dry = call("command_batch", {"expectedVersion": 0, "dryRun": True, "steps": steps})
        check("dry_run_revision_unchanged", dry["dryRun"] and dry["beforeVersion"] == dry["afterVersion"] == 0)
        check("dry_run_created_ids_null", all(n["nodeId"] is None
              for step in dry.get("objects", []) for n in step.get("created", [])))
        check("dry_run_exact", export("dry-run.hwpx") == baseline)
        result = call("command_batch", {"expectedVersion": 0, "steps": steps})
        check("batch_single_revision", result["afterVersion"] == 1, result["afterVersion"])
        check("six_picture_nodes", len(query("picture")) == 6)
        check("one_native_equation", len(query("equation")) == 1)
        check("one_result_table", len(query("table")) == 1)
        objects = [m for k in ("picture", "equation", "table", "cell") for m in query(k)]
        ids = [m["nodeId"] for m in objects]
        identity_fields = ("nodeId", "kind", "parent", "section", "cell", "media", "size")
        identities = {m["nodeId"]: {key: m.get(key) for key in identity_fields} for m in objects}

        def check_identities(stage):
            current = [m["nodeId"] for k in ("picture", "equation", "table", "cell") for m in query(k)]
            check(f"{stage}_id_set_exact", len(current) == len(ids) and set(current) == set(ids))
            for node_id in ids:
                outline = call("element_get", {"target": node_id})["outline"]
                actual = {key: outline.get(key) for key in identity_fields}
                check(f"{stage}_identity_{node_id}", actual == identities[node_id],
                      None if actual == identities[node_id] else {"expected": identities[node_id], "actual": actual})
        check("teacher_still_blank_same_id", para("지도교사: ") == teacher)
        edited = export("edited.hwpx")
        with zipfile.ZipFile(out / "edited.hwpx") as package:
            xml = ET.fromstring(package.read("Contents/section0.xml"))
            ps = list(xml.findall(f"{{{HP}}}p"))
            manifest = ET.fromstring(package.read("Contents/content.hpf"))
            resources = {n.get("id"): n.get("href") for n in manifest.iter() if n.tag.split("}")[-1] == "item"}
            for i in range(1, 7):
                at = next(j for j, p in enumerate(ps)
                          if "".join(p.itertext()) == f"[그림{i}] 합성 실험 결과")
                imgs = list(ps[at - 1].iter(f"{{{HC}}}img")) if at else []
                check(f"figure_{i}_before_caption", len(imgs) == 1)
                href = resources[imgs[0].get("binaryItemIDRef")]
                member = href if href.startswith("BinData/") else posixpath.normpath("Contents/" + href)
                check(f"figure_{i}_payload", package.read(member) == figure_bytes[i])
            scripts = [n.text for n in xml.iter(f"{{{HP}}}script")]
            check("native_script_exact", scripts == ["int _ 0 ^ 1 x ^ 2 dx = 1 over 3"], scripts)
            check("notice_text_gone", "★삭제 후 작성할 것★" not in "".join(xml.itertext()))
            table = next(xml.iter(f"{{{HP}}}tbl"))
            texts = ["".join(cell.itertext()) for cell in table.iter(f"{{{HP}}}tc")]
            check("result_cells", texts == ["방법", "결과", "수치 적분", "0.3333", "정확값", "1/3"], texts)
        call("history_undo", {"expectedVersion": 1})
        check("undo_exact", export("undo.hwpx") == baseline)
        call("history_redo", {"expectedVersion": 2})
        check("redo_exact", export("redo.hwpx") == edited)
        check_identities("redo")
        preview = call("document_preview", {"pages": [1], "includePng": True, "includeSvg": True})
        check("preview_has_page", preview.get("status") in ("rendered", "partial") and bool(preview.get("pages")), preview.get("status"))
        check("preview_native_not_checked", preview.get("metadata", {}).get("nativeComparison") == "not_checked")
        page_count = preview["pageCount"]
        check("preview_page_budget", 1 <= page_count <= 8, page_count)
        if page_count > 1:
            preview = call("document_preview", {"pages": list(range(1, page_count + 1)),
                           "includePng": True, "includeSvg": True})
        check("preview_all_pages", len(preview["pages"]) == page_count)
        summary["previewPageCount"] = page_count
        for page in preview["pages"]:
            j = page["page"]
            if isinstance(page.get("svg"), str):
                (out / f"preview-{j}.svg").write_text(page["svg"], encoding="utf-8")
            if page.get("png", {}).get("base64"):
                (out / f"preview-{j}.png").write_bytes(base64.b64decode(page["png"]["base64"]))
        call("project_save", {"path": str(out / "saved.hwpwork")})
        call("document_close", {})
        call("document_open", {"path": str(out / "saved.hwpwork")})
        check_identities("project_reopen")
        check("project_reopen_exact", export("reopened.hwpx") == edited)
        summary["identityAudit"] = {"nodes": len(ids), "fields": list(identity_fields),
                                    "metadataComparisons": 2 * len(ids) * len(identity_fields)}
        summary["status"] = "passed"
    except Exception as exc:
        summary["error"] = f"{type(exc).__name__}: {exc}"
        print(summary["error"], file=sys.stderr)
    finally:
        if client:
            (out / "stderr.log").write_text(client.close(), encoding="utf-8")
        (out / "calls.jsonl").write_text("".join(json.dumps(x, ensure_ascii=False) + "\n" for x in logs), encoding="utf-8")
        summary["passed"] = sum(x["ok"] for x in checks)
        summary["failed"] = sum(not x["ok"] for x in checks)
        summary["binarySha256"] = hashlib.sha256(args.binary.read_bytes()).hexdigest()
        summary["files"] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob("*.hwpx")}
        (out / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({k: summary[k] for k in ("status", "passed", "failed", "nativeComparison")}))
    return 0 if summary["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
