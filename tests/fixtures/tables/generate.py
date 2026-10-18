#!/usr/bin/env python3
"""Deterministic synthetic table fixture. Only the public MIT empty seed is read."""
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


def cell(row, col, cs=1, rs=1):
    s = evaluation.table_cell_xml(col, f'TABLE-R{row}C{col}')
    s = s.replace('rowAddr="0"', f'rowAddr="{row}"')
    s = s.replace('colSpan="1" rowSpan="1"', f'colSpan="{cs}" rowSpan="{rs}"')
    s = s.replace('width="21000" height="2400"', f'width="{21000*cs}" height="{2400*rs}"')
    s = s.replace('</hp:p>', '<hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p>')
    return s


def table(number, shape):
    s = evaluation.table_xml().replace('id="1001"', f'id="{number}"')
    s = s.replace('rowCnt="1"', 'rowCnt="2"').replace('height="2400" heightRelTo', 'height="4800" heightRelTo')
    rows = {
        'merged': [[cell(0, 0, cs=2)], [cell(1, 0), cell(1, 1)]],
        'plain': [[cell(0, 0), cell(0, 1)], [cell(1, 0), cell(1, 1)]],
        'vertical': [[cell(0, 0, rs=2), cell(0, 1)], [cell(1, 1)]],
    }[shape]
    return re.sub(r'<hp:tr>.*?</hp:tr>', ''.join('<hp:tr>'+''.join(r)+'</hp:tr>' for r in rows), s)


def generate():
    with zipfile.ZipFile(evaluation.PUBLIC_SEED) as z:
        files = {i.filename: (z.read(i), i.compress_type) for i in z.infolist()}
    header = files['Contents/header.xml'][0].decode()
    colored = evaluation.BORDER_FILL.replace('id="3"', 'id="4"').replace('</hh:borderFill>', '<hc:fillBrush><hc:winBrush faceColor="#AABBCC" hatchColor="#999999" alpha="0"/></hc:fillBrush></hh:borderFill>')
    header = header.replace('<hh:borderFills itemCnt="2">', '<hh:borderFills itemCnt="4">').replace('</hh:borderFills>', evaluation.BORDER_FILL + colored + '</hh:borderFills>')
    files['Contents/header.xml'] = (header.encode(), 8)
    section = evaluation.section_xml(table=False, picture=False)
    extras = ''.join(f'<hp:p {evaluation.P_ATTRS.format(id=100+i)}><hp:run charPrIDRef="0">{table(2000+i, shape)}<hp:t/></hp:run><hp:linesegarray><hp:lineseg textpos="0"/></hp:linesegarray></hp:p>' for i, shape in enumerate(['merged', 'plain', 'vertical']))
    section = section.replace('</hs:sec>', extras + '<hp:p '+evaluation.P_ATTRS.format(id=110)+'><hp:run charPrIDRef="0"><hp:t>AFTER-TABLES</hp:t></hp:run><hp:linesegarray/></hp:p></hs:sec>')
    files['Contents/section0.xml'] = (section.encode(), 8)
    hpf = files['Contents/content.hpf'][0].decode()
    hpf = re.sub(r'<opf:metadata>.*?</opf:metadata>', '<opf:metadata><opf:title>Synthetic tables</opf:title></opf:metadata>', hpf, flags=re.S)
    files['Contents/content.hpf'] = (hpf.encode(), 8)
    files['Preview/PrvText.txt'] = (b'TABLE-R0C0\r\nAFTER-TABLES', 8)
    out = io.BytesIO()
    with zipfile.ZipFile(out, 'w') as z:
        for name, (data, method) in files.items():
            i = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            i.create_system = 0
            i.external_attr = 0o100644 << 16
            z.writestr(i, data, compress_type=method, compresslevel=9 if method == 8 else None)
    return out.getvalue()


if __name__ == '__main__':
    data = generate()
    assert data == generate(), 'non-deterministic fixture'
    dest = Path(__file__).with_name('dimensions.hwpx')
    dest.write_bytes(data)
    print(f'{dest.name} sha256={hashlib.sha256(data).hexdigest()} deterministic=True')
