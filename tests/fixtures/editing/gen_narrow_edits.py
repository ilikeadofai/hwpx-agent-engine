#!/usr/bin/env python3
"""Deterministic synthetic style/header/footer/picture fixture. No personal data."""
import argparse
import hashlib
import importlib.util
from pathlib import Path
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('editing', HERE / 'gen_editing.py')
e = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e)

def generate():
    styles = '<hh:styles itemCnt="2">' + ''.join(
        f'<hh:style id="{i}" type="PARA" name="Style{i}" engName="Style{i}" paraPrIDRef="0" charPrIDRef="0" nextStyleIDRef="0" langID="1042" lockForm="0"/>'
        for i in range(2)) + '</hh:styles>'
    header = e.default_header().replace(b'</hh:refList>', styles.encode() + b'</hh:refList>')
    def p(text):
        return e.P(e.R(e.T(text)) + '<hp:linesegarray/>', ' styleIDRef="0"')
    def hf(kind, body):
        # 실제 HWPX 처럼 머리말/꼬리말은 run 안 hp:ctrl 아래에 둔다(코퍼스 관찰).
        return f'<hp:ctrl><hp:{kind} id="10" applyPageType="BOTH"><hp:subList id="" textDirection="HORIZONTAL" lineWrap="BREAK" vertAlign="TOP" linkListIDRef="0" linkListNextIDRef="0" textWidth="40000" textHeight="2000" hasTextRef="0" hasNumRef="0">{body}</hp:subList></hp:{kind}></hp:ctrl>'
    body = (p('Body A') + p('Body B') + e.P(e.R(
        hf('header', p('Header A') + p('Header B') + p('Number<hp:autoNum num="1"/>') + p('Margin<hp:tab width="4000"/>'))
        + hf('footer', p('Footer A'))), ' styleIDRef="0"')
        + e.P(e.R(e.pic('20', 'image1', w=1200, h=1200)), ' styleIDRef="0"'))
    return e.build(e.SEC(body), header=header,
        hpf_bytes=e.hpf_sec([('image1', 'BinData/narrow.png', 'image/png')]),
        extra=[('BinData/narrow.png', e.png(2, 1), 8), ('Preview/PrvText.txt', b'Header A\nFooter A\nBody A', 8)])

if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--check-determinism', action='store_true')
    args = ap.parse_args()
    data = generate()
    if args.check_determinism:
        assert data == generate()
    (HERE / 'narrow_edits.hwpx').write_bytes(data)
    print('narrow_edits.hwpx sha256=' + hashlib.sha256(data).hexdigest())
