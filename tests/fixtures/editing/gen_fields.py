#!/usr/bin/env python3
"""Synthetic fields only. Fixed ZIP metadata; no personal input documents."""
import argparse
import hashlib
import importlib.util
from pathlib import Path
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('editing', HERE / 'gen_editing.py')
e = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e)

def begin(i, name='', kind='CLICKHERE'):
    return f'<hp:ctrl><hp:fieldBegin id="{i}" type="{kind}" name="{name}" fieldid="f{i}" editable="1" dirty="0" zorder="0" metaTag=""><hp:parameters cnt="1" name=""><hp:stringParam name="Command">guide&amp;raw</hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl>'
def end(i):
    return f'<hp:ctrl><hp:fieldEnd beginIDRef="{i}"/></hp:ctrl>'
def field(i, value='', name='', kind='CLICKHERE'):
    return begin(i, name, kind) + e.T(value) + end(i)
def generate():
    body = e.P(e.R(field(1, '', 'empty') + e.T(' outside ') + field(2, 'old&amp;value', 'filled')) + '<hp:linesegarray/>')
    body += e.P(e.R(field(3, 'link', 'link', 'HYPERLINK')))
    body += e.P(e.R(begin(4, 'multi') + e.T('first')))
    body += e.P(e.R(e.T('second') + end(4)))
    body += e.P(e.R(field(5, 'memo', '', 'MEMO')))
    body += e.P(e.R(begin(6, 'outer') + e.T('a') + field(7, 'inner', 'inner') + e.T('b') + end(6)))
    body += e.P(e.R(field(8, 'one', 'dup') + field(9, 'two', 'dup')))
    body += e.P(e.R(field(10, 'nameless')))
    body += e.P(e.R(field(11, 'a<hp:tab/>b', 'tab')))
    body += e.P(e.R(begin(12, 'object') + e.T('a') + '<hp:equation id="99"/>' + e.T('b') + end(12)))
    for i, content in [(13, 'a<!--keep-->b'), (14, 'a<![CDATA[b]]>'), (15, 'a<?keep yes?>b')]:
        body += e.P(e.R(field(i, content, 'lexical')))
    table = e._tbl(1, 1, [e._tr([e._tc(0, 0, 1, 1, [e.P(e.R(field(16, 'cell', 'cell')))])])])
    body += e.P(e.R(table))
    body += e.P(e.R(begin(17, 'split') + e.T('first')) + e.R(e.T('second') + end(17), '1'))
    return e.build(e.SEC(body), extra=[('Preview/PrvText.txt', b'not a matching paragraph', 8)])
def generate_gate():
    # Opening smoke test: no deliberately incomplete equation/table rejection probes.
    body = e.P(e.R(field(1, '', 'empty') + e.T(' outside ') + field(2, 'old&amp;value', 'filled')))
    return e.build(e.SEC(body))

if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check-determinism', action='store_true')
    p.add_argument('--gate-output', type=Path, help='Also generate a minimal CLICKHERE input for the native opening gate')
    args = p.parse_args()
    data = generate()
    if args.check_determinism:
        assert data == generate()
    (HERE / 'fields.hwpx').write_bytes(data)
    if args.gate_output:
        gate = generate_gate()
        if args.check_determinism:
            assert gate == generate_gate()
        args.gate_output.write_bytes(gate)
        print(f'{args.gate_output.name} sha256={hashlib.sha256(gate).hexdigest()} deterministic={args.check_determinism}')
    print(f'fields.hwpx sha256={hashlib.sha256(data).hexdigest()} deterministic={args.check_determinism}')
