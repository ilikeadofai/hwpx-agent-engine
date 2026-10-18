#!/usr/bin/env python3
"""Two synthetic section bodies on the allowed ref_empty corpus package skeleton.

Preserves native secPr shape; fixed metadata and no personal document input.
Run: python3 -I tests/fixtures/editing/gen_section.py --check-determinism
"""
import argparse
import hashlib
import io
from pathlib import Path
import re
import zipfile

HERE = Path(__file__).resolve().parent
SEED = HERE.parent / 'corpus' / 'ref_empty.hwpx'


def generate():
    with zipfile.ZipFile(SEED) as z:
        files = {i.filename: z.read(i) for i in z.infolist()}
        methods = {i.filename: i.compress_type for i in z.infolist()}
    section = files['Contents/section0.xml'].decode()
    # Replace all body text with synthetic text while retaining native controls.
    section = section.replace('<hp:t/>', '<hp:t>Section zero synthetic text.</hp:t>')
    paragraph = '<hp:p id="900000001" paraPrIDRef="0" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0"><hp:run charPrIDRef="0"><hp:t>Following paragraph.</hp:t></hp:run><hp:linesegarray/></hp:p>'
    section = section.replace('</hs:sec>', paragraph + '</hs:sec>')
    files['Contents/section0.xml'] = section.encode()
    files['Contents/section1.xml'] = (section.replace('landscape="WIDELY"', 'landscape="NARROWLY"')
        .replace('Section zero', 'Section one')
        .replace('id="3121190098"', 'id="900000002"')
        .replace('id="900000001"', 'id="900000003"')).encode()
    files['Contents/header.xml'] = re.sub(rb'secCnt="[0-9]+"', b'secCnt="2"', files['Contents/header.xml'])
    hpf = files['Contents/content.hpf'].decode()
    hpf = hpf.replace('</opf:manifest>', '<opf:item id="section1" href="Contents/section1.xml" media-type="application/xml"/></opf:manifest>')
    hpf = hpf.replace('</opf:spine>', '<opf:itemref idref="section1"/></opf:spine>')
    files['Contents/content.hpf'] = hpf.encode()
    files['Preview/PrvText.txt'] = b'Section zero synthetic text.\nFollowing paragraph.\nSection one synthetic text.\nFollowing paragraph.'
    out = io.BytesIO()
    with zipfile.ZipFile(out, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, value in files.items():
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.compress_type = methods.get(name, zipfile.ZIP_DEFLATED)
            info.external_attr = 0o600 << 16
            z.writestr(info, value)
    return out.getvalue()


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check-determinism', action='store_true')
    args = p.parse_args()
    data = generate()
    if args.check_determinism:
        assert data == generate()
    (HERE / 'section.hwpx').write_bytes(data)
    print(f'section.hwpx sha256={hashlib.sha256(data).hexdigest()} deterministic={args.check_determinism}')
