#!/usr/bin/env python3
"""Synthetic charPr fixture; no personal documents. Fixed ZIP metadata.

python3 -I tests/fixtures/editing/gen_char_format.py --check-determinism
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("editing", HERE / "gen_editing.py")
editing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(editing)


def generate():
    header = editing.default_header()
    # Shape 1 is the bold equivalent of 0; 2/3 retain the original values.
    shape = editing._char_pr_xml(1)
    header = header.replace(shape.encode(), shape.replace("<hh:underline", "<hh:bold/><hh:underline").encode())
    body = (
        editing.P('<hp:run charPrIDRef="0" xmlns:x="urn:format" x:keep="&#65;"><hp:t>ABCDEF</hp:t></hp:run><hp:linesegarray/>')
        + editing.P(editing.R(editing.T('a<hp:tab width="4000"/>b')))
        + editing.P(editing.R(editing.T('e')) + editing.R(editing.T('\u0301&amp;👨‍👩Z')))
    )
    return editing.build(editing.SEC(body), header=header, extra=[("Preview/PrvText.txt", b"ABCDEF", 8)])


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--check-determinism", action="store_true")
    args = parser.parse_args()
    data = generate()
    if args.check_determinism:
        assert data == generate(), "generation is not deterministic"
    output = HERE / "char_format.hwpx"
    output.write_bytes(data)
    print(f"{output.name}: sha256={hashlib.sha256(data).hexdigest()} deterministic={args.check_determinism}")
