#!/usr/bin/env python3
"""Deterministic synthetic paragraph-structure fixture. No personal data.

Produces ``tests/fixtures/paragraph/paragraphs.hwpx`` and
``tests/fixtures/paragraph/expected.json`` for the 0.4 paragraph.insert /
paragraph.delete commands (docs/decisions/document-paragraph-creation-design.md).

The document deliberately contains, in document order:
  0  첫 문단          section properties (hp:secPr) + layout cache
  1  둘째 문단        paraPrIDRef="1" (centred) — the paraPr clone test
  2  셋째 문단        no layout cache
  3  표를 담은 문단    contains hp:tbl (delete must be refused)
  4  개체를 담은 문단  contains hp:pic (delete must be refused)
  5  셀 문단 표       1x2 table: (0,0) one paragraph, (0,1) two paragraphs

Usage:
    python3 -I tests/fixtures/paragraph/gen_paragraph_structure.py
    python3 -I tests/fixtures/paragraph/gen_paragraph_structure.py --check-determinism
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("editing", HERE.parent / "editing" / "gen_editing.py")
e = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e)

SECPR = (
    '<hp:secPr id="" textDirection="HORIZONTAL" spaceColumns="1134" tabStop="8000"'
    ' outlineShapeIDRef="0" memoShapeIDRef="0" textVerticalWidthHead="0" masterPageCnt="0">'
    '<hp:grid lineGrid="0" charGrid="0" wonggojiFormat="0"/>'
    '<hp:startNum pageStartsOn="BOTH" page="0" pic="0" tbl="0" equation="0"/>'
    '<hp:visibility hideFirstHeader="0" hideFirstFooter="0" hideFirstMasterPage="0"'
    ' border="SHOW_ALL" fill="SHOW_ALL" hideFirstPageNum="0" hideFirstEmptyLine="0"'
    ' showLineNumber="0"/>'
    '<hp:lineNumberShape restartType="0" countBy="0" distance="0" startNumber="0"/>'
    '<hp:pagePr landscape="WIDELY" width="59528" height="84186" gutterType="LEFT_ONLY">'
    '<hp:margin header="4252" footer="4252" gutter="0" left="8504" right="8504"'
    ' top="5668" bottom="4252"/></hp:pagePr></hp:secPr>'
)

CACHE = (
    '<hp:linesegarray><hp:lineseg textpos="0" vertpos="0" vertsize="1000"'
    ' textheight="1000" baseline="850" spacing="600" horzsize="1000" flags="0"/>'
    '</hp:linesegarray>'
)


def para(pid, text, para_pr="0", char="0", cache=False, body=None):
    if body is None:
        inner = e.T(text) if text else ""
        body = (
            '<hp:run charPrIDRef="%s">%s</hp:run>' % (char, inner)
            if text
            else '<hp:run charPrIDRef="%s"/>' % char
        )
    return (
        '<hp:p id="%d" paraPrIDRef="%s" styleIDRef="0" pageBreak="0" columnBreak="0"'
        ' merged="0">%s%s</hp:p>' % (pid, para_pr, body, CACHE if cache else "")
    )


def cell(row, col, paras, width=10000, height=2000):
    body = "".join(paras)
    return (
        '<hp:tc><hp:cellAddr rowAddr="%d" colAddr="%d"/>'
        '<hp:cellSpan rowSpan="1" colSpan="1"/>'
        '<hp:cellSz width="%d" height="%d"/><hp:subList>%s</hp:subList></hp:tc>'
        % (row, col, width, height, body)
    )


def table(rows, cols, cells, width=20000, height=2000):
    return (
        '<hp:tbl rowCnt="%d" colCnt="%d" cellSpacing="0">'
        '<hp:sz width="%d" widthRelTo="ABSOLUTE" height="%d" heightRelTo="ABSOLUTE"'
        ' protect="0"/>%s</hp:tbl>'
        % (rows, cols, width, height, "".join(cells))
    )


def styles():
    return (
        '<hh:styles itemCnt="1"><hh:style id="0" type="PARA" name="바탕글"'
        ' engName="Normal" paraPrIDRef="0" charPrIDRef="0" nextStyleIDRef="0"'
        ' langID="1042" lockForm="0"/></hh:styles>'
    )


PARA_PR_1 = (
    e.DEFAULT_PARA_PR.replace('<hh:paraPr id="0"', '<hh:paraPr id="1"')
    .replace('horizontal="JUSTIFY"', 'horizontal="CENTER"')
)


def generate():
    header = e.header_xml([e.DEFAULT_PARA_PR, PARA_PR_1])
    header = header.replace(b"</hh:refList>", styles().encode("utf-8") + b"</hh:refList>")

    table_p = para(
        4,
        "",
        body='<hp:run charPrIDRef="0">%s</hp:run>'
        % table(
            1,
            1,
            [
                '<hp:tr>'
                + cell(0, 0, [para(41, "표 셀")])
                + "</hp:tr>"
            ],
        ),
    )
    picture_p = para(5, "", body='<hp:run charPrIDRef="0">%s</hp:run>' % e.pic("30", "image1"))
    cell_table_p = para(
        6,
        "",
        body='<hp:run charPrIDRef="0">%s</hp:run>'
        % table(
            1,
            2,
            [
                '<hp:tr>'
                + cell(0, 0, [para(61, "한 문단 셀")])
                + cell(0, 1, [para(62, "셀 문단 1", cache=True), para(63, "셀 문단 2")])
                + "</hp:tr>"
            ],
        ),
    )
    body = (
        para(1, "", body='<hp:run charPrIDRef="0">%s%s</hp:run>' % (SECPR, e.T("첫 문단")), cache=True)
        + para(2, "둘째 문단", para_pr="1", char="1", cache=True)
        + para(3, "셋째 문단")
        + table_p
        + picture_p
        + cell_table_p
    )
    return e.build(
        e.SEC(body),
        header=header,
        hpf_bytes=e.hpf_sec([("image1", "BinData/para.png", "image/png")]),
        extra=[
            ("BinData/para.png", e.png(2, 1), 8),
            ("Preview/PrvText.txt", "첫 문단\n둘째 문단".encode("utf-8"), 8),
        ],
    )


def describe(files):
    section = files["Contents/section0.xml"]
    return {
        "entry": "Contents/section0.xml",
        "topLevel": [p["text"] for p in e.para_descs(section)][:6],
        "paragraphs": [p["text"] for p in e.para_descs(section)],
        "roles": {
            "first": {"topLevel": 0, "text": "첫 문단", "ownsSectionProperties": True},
            "second": {"topLevel": 1, "text": "둘째 문단", "paraPrIDRef": "1", "charPrIDRef": "1"},
            "third": {"topLevel": 2, "text": "셋째 문단"},
            "tableCarrier": {"topLevel": 3, "text": "\ufffc"},
            "pictureCarrier": {"topLevel": 4, "text": "\ufffc"},
            "cellTableCarrier": {"topLevel": 5, "text": "\ufffc"},
            "singleParagraphCell": {"text": "한 문단 셀"},
            "multiParagraphCell": {"texts": ["셀 문단 1", "셀 문단 2"]},
        },
        "prvText": "첫 문단\n둘째 문단",
        "layoutCache": {"first": 1, "second": 1, "third": 0, "cellFirst": 1},
    }


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--check-determinism", action="store_true")
    args = ap.parse_args()
    data = generate()
    if args.check_determinism:
        assert data == generate()
    files = e.read_all(data)
    expected = describe(files)
    out = HERE / "paragraphs.hwpx"
    out.write_bytes(data)
    (HERE / "expected.json").write_text(
        json.dumps(expected, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print("paragraphs.hwpx sha256=" + hashlib.sha256(data).hexdigest())
