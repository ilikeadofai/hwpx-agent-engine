#!/usr/bin/env python3
"""Synthetic equation fixture only. Fixed ZIP metadata; no personal documents.
python3 -I tests/fixtures/editing/gen_equations.py --check-determinism
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("editing", HERE / "gen_editing.py")
editing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(editing)


def equation(i, script):
    return f'''<hp:equation id="{i}" zOrder="2" numberingType="EQUATION" textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" version="Equation Version 60" baseLine="66" textColor="#000000" baseUnit="1000" lineMode="CHAR" font="HancomEQN" xmlns:x="urn:equation:opaque" x:keep="&#65;">
<hp:sz width="5675" widthRelTo="ABSOLUTE" height="2250" heightRelTo="ABSOLUTE" protect="0"/>
<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>
<hp:outMargin left="56" right="56" top="0" bottom="0"/>
<hp:shapeComment>수식입니다. &amp; original</hp:shapeComment><hp:script>{script}</hp:script></hp:equation>'''


def generate():
    # A complete page definition makes this a useful native-open probe too.
    page = '''<hp:secPr id="0" textDirection="HORIZONTAL" spaceColumns="1134" tabStop="8000" tabStopVal="4000" tabStopUnit="HWPUNIT" outlineShapeIDRef="0" memoShapeIDRef="0" textVerticalWidthHead="0">
<hp:grid lineGrid="0" charGrid="0" wonggojiFormat="0"/>
<hp:startNum pageStartsOn="BOTH" page="0" pic="0" tbl="0" equation="0"/>
<hp:visibility hideFirstHeader="0" hideFirstFooter="0" hideFirstMasterPage="0" border="SHOW_ALL" fill="SHOW_ALL" hideFirstPageNum="0" hideFirstEmptyLine="0" showLineNumber="0"/>
<hp:lineNumberShape restartType="0" countBy="0" distance="0" startNumber="0"/>
<hp:pagePr landscape="WIDELY" width="59528" height="84188" gutterType="LEFT_ONLY"><hp:margin header="4252" footer="4252" gutter="0" left="8504" right="8504" top="5668" bottom="4252"/></hp:pagePr>
<hp:footNotePr><hp:autoNumFormat type="DIGIT" userChar="" prefixChar="" suffixChar=")" supscript="0"/><hp:noteLine length="-1" type="SOLID" width="0.12 mm" color="#000000"/><hp:noteSpacing betweenNotes="283" belowLine="567" aboveLine="850"/><hp:numbering type="CONTINUOUS" newNum="1"/><hp:placement place="EACH_COLUMN" beneathText="0"/></hp:footNotePr>
<hp:endNotePr><hp:autoNumFormat type="DIGIT" userChar="" prefixChar="" suffixChar=")" supscript="0"/><hp:noteLine length="14692344" type="SOLID" width="0.12 mm" color="#000000"/><hp:noteSpacing betweenNotes="0" belowLine="567" aboveLine="850"/><hp:numbering type="CONTINUOUS" newNum="1"/><hp:placement place="END_OF_DOCUMENT" beneathText="0"/></hp:endNotePr></hp:secPr>'''
    scripts = ['{a} over {b}  TIMES  {c} over {d} = {ac} over {bd}', '', 'a &lt; b &amp; c &gt; d + &#x3B1;']
    body = ''.join(editing.P(editing.R((page if i == 0 else '') + editing.T(f'Equation {i + 1}: ') + equation(100 + i, s)) + '<hp:linesegarray/>') for i, s in enumerate(scripts))
    # Empty scope for read queries.
    body += editing.P(editing.R(editing.T('No equations here.')) + '<hp:linesegarray/>')
    return editing.build(editing.SEC(body), extra=[('Preview/PrvText.txt', b'Equation 1: \nEquation 2: \nEquation 3: ', 8)])


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check-determinism', action='store_true')
    args = parser.parse_args()
    data = generate()
    if args.check_determinism:
        assert data == generate(), 'nondeterministic bytes'
        existing = HERE / 'equations.hwpx'
        if existing.exists():
            assert existing.read_bytes() == data, 'committed fixture drift'
    (HERE / 'equations.hwpx').write_bytes(data)
    print(f'equations.hwpx sha256={hashlib.sha256(data).hexdigest()} deterministic={args.check_determinism}')
