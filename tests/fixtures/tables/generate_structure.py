#!/usr/bin/env python3
"""Deterministic synthetic fixture for table row/column insertion and deletion.

Only the public MIT empty seed (`corpus/ref_empty.hwpx`) is read. Three tables:

  0 plain    4x3, row 0 is a header row (header="1", own borderFill), cell R2C1 has
             two paragraphs, every cell paragraph carries a layout cache
  1 merged   4x3: a colSpan=3 title, a rowSpan=2 cell at (2,0) and plain cells
  2 auto     3x2 "Hancom-style" table: rows 0 and 2 have height 0 (auto) and the
             table height (4000) does not equal the sum of the cell heights
  3 vertical 3x2: a rowSpan=2 cell at (0,0); deleting column 1 would empty row 1

`structure_object.hwpx` is the same document with a control element inside
cell R3C2 of table 0 (a deletion the engine must refuse).

Paragraphs before/after the tables carry caches too, so cache removal is
observable. `Preview/PrvText.txt` repeats some cell text.
"""
import hashlib
import importlib.util
import io
from pathlib import Path
import re
import zipfile

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('evaluation', ROOT / 'scripts/evaluate_mcp.py')
evaluation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluation)

LINESEG = '<hp:linesegarray><hp:lineseg textpos="0" vertpos="0" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="14000" flags="393216"/></hp:linesegarray>'


def para(pid, text, shape=0):
    run = f'<hp:run charPrIDRef="{shape}"><hp:t>{text}</hp:t></hp:run>' if text else f'<hp:run charPrIDRef="{shape}"/>'
    return f'<hp:p id="{pid}" paraPrIDRef="0" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0">{run}{LINESEG}</hp:p>'


def cell(row, col, text, *, w=14000, h=2400, cs=1, rs=1, header=0, border=3, extra=None, name='', ctrl=False):
    paras = para(10, text)
    if ctrl:
        paras = paras.replace(
            '</hp:p>',
            '<hp:run charPrIDRef="0"><hp:ctrl><hp:colPr id="" type="NEWSPAPER" layout="LEFT" colCount="1" sameSz="1" sameGap="0"/></hp:ctrl></hp:run></hp:p>',
            1,
        )
    if extra:
        paras += para(11, extra, shape=1)
    return (
        f'<hp:tc name="{name}" header="{header}" hasMargin="0" protect="0" editable="0" dirty="0" borderFillIDRef="{border}">'
        '<hp:subList id="" textDirection="HORIZONTAL" lineWrap="BREAK" vertAlign="CENTER" '
        f'linkListIDRef="0" linkListNextIDRef="0" textWidth="0" textHeight="0" hasTextRef="0" hasNumRef="0">{paras}</hp:subList>'
        f'<hp:cellAddr colAddr="{col}" rowAddr="{row}"/><hp:cellSpan colSpan="{cs}" rowSpan="{rs}"/>'
        f'<hp:cellSz width="{w}" height="{h}"/>'
        '<hp:cellMargin left="141" right="141" top="141" bottom="141"/></hp:tc>'
    )


def table(number, rows, cols, width, height, trs):
    head = (
        f'<hp:tbl id="{number}" zOrder="0" numberingType="TABLE" textWrap="TOP_AND_BOTTOM" '
        'textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" pageBreak="CELL" repeatHeader="1" '
        f'rowCnt="{rows}" colCnt="{cols}" cellSpacing="0" borderFillIDRef="3" noAdjust="0">'
        f'<hp:sz width="{width}" widthRelTo="ABSOLUTE" height="{height}" heightRelTo="ABSOLUTE" protect="0"/>'
        '<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" '
        'vertRelTo="PARA" horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>'
        '<hp:outMargin left="283" right="283" top="283" bottom="283"/>'
        '<hp:inMargin left="141" right="141" top="141" bottom="141"/>'
    )
    return head + ''.join(f'<hp:tr>{"".join(r)}</hp:tr>' for r in trs) + '</hp:tbl>'


def plain(ctrl=False):
    rows = [[cell(0, c, f'H{c}', header=1, border=4, name=f'head{c}') for c in range(3)]]
    for r in range(1, 4):
        rows.append([
            cell(r, c, f'R{r}C{c}', extra='R2C1-extra' if (r, c) == (2, 1) else None,
                 ctrl=ctrl and (r, c) == (3, 2))
            for c in range(3)
        ])
    return table(3000, 4, 3, 42000, 9600, rows)


def merged():
    rows = [
        [cell(0, 0, 'TITLE', w=42000, cs=3)],
        [cell(1, c, f'M1C{c}') for c in range(3)],
        [cell(2, 0, 'TALL', h=4800, rs=2), cell(2, 1, 'M2C1'), cell(2, 2, 'M2C2')],
        [cell(3, 1, 'M3C1'), cell(3, 2, 'M3C2')],
    ]
    return table(3001, 4, 3, 42000, 9600, rows)


def auto():
    rows = [
        [cell(0, c, f'A0C{c}', w=21000, h=0) for c in range(2)],
        [cell(1, c, f'A1C{c}', w=21000, h=3000) for c in range(2)],
        [cell(2, c, f'A2C{c}', w=21000, h=0) for c in range(2)],
    ]
    return table(3002, 3, 2, 42000, 4000, rows)


def vertical():
    rows = [
        [cell(0, 0, 'V', w=21000, h=4800, rs=2), cell(0, 1, 'X0', w=21000)],
        [cell(1, 1, 'X1', w=21000)],
        [cell(2, 0, 'Y0', w=21000), cell(2, 1, 'Y1', w=21000)],
    ]
    return table(3003, 3, 2, 42000, 7200, rows)


def carrier(pid, tbl):
    return (
        f'<hp:p {evaluation.P_ATTRS.format(id=pid)}><hp:run charPrIDRef="0">{tbl}<hp:t/></hp:run>'
        f'{LINESEG}</hp:p>'
    )


def generate(ctrl=False):
    with zipfile.ZipFile(evaluation.PUBLIC_SEED) as z:
        files = {i.filename: (z.read(i), i.compress_type) for i in z.infolist()}
    header = files['Contents/header.xml'][0].decode()
    colored = evaluation.BORDER_FILL.replace('id="3"', 'id="4"').replace(
        '</hh:borderFill>',
        '<hc:fillBrush><hc:winBrush faceColor="#AABBCC" hatchColor="#999999" alpha="0"/></hc:fillBrush></hh:borderFill>',
    )
    header = header.replace('<hh:borderFills itemCnt="2">', '<hh:borderFills itemCnt="4">').replace(
        '</hh:borderFills>', evaluation.BORDER_FILL + colored + '</hh:borderFills>'
    )
    files['Contents/header.xml'] = (header.encode(), 8)
    section = evaluation.section_xml(table=False, picture=False)
    body = (
        f'<hp:p {evaluation.P_ATTRS.format(id=200)}><hp:run charPrIDRef="0"><hp:t>BEFORE-TABLES</hp:t></hp:run>{LINESEG}</hp:p>'
        + carrier(201, plain(ctrl))
        + carrier(202, merged())
        + carrier(203, auto())
        + carrier(205, vertical())
        + f'<hp:p {evaluation.P_ATTRS.format(id=204)}><hp:run charPrIDRef="0"><hp:t>AFTER-TABLES</hp:t></hp:run>{LINESEG}</hp:p>'
    )
    section = section.replace('</hs:sec>', body + '</hs:sec>')
    files['Contents/section0.xml'] = (section.encode(), 8)
    hpf = files['Contents/content.hpf'][0].decode()
    hpf = re.sub(r'<opf:metadata>.*?</opf:metadata>', '<opf:metadata><opf:title>Synthetic table structure</opf:title></opf:metadata>', hpf, flags=re.S)
    files['Contents/content.hpf'] = (hpf.encode(), 8)
    files['Preview/PrvText.txt'] = (b'BEFORE-TABLES\r\nR1C0\r\nR3C2\r\nAFTER-TABLES', 8)
    out = io.BytesIO()
    with zipfile.ZipFile(out, 'w') as z:
        for name, (data, method) in files.items():
            i = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            i.create_system = 0
            i.external_attr = 0o100644 << 16
            z.writestr(i, data, compress_type=method, compresslevel=9 if method == 8 else None)
    return out.getvalue()


if __name__ == '__main__':
    for name, ctrl in (('structure.hwpx', False), ('structure_object.hwpx', True)):
        data = generate(ctrl)
        assert data == generate(ctrl), 'non-deterministic fixture'
        dest = Path(__file__).with_name(name)
        dest.write_bytes(data)
        print(f'{dest.name} sha256={hashlib.sha256(data).hexdigest()} deterministic=True')
