"""Hand-authored synthetic fixtures. No oracle/engine imports or editing helpers.

Expected candidates are explicit template substitutions, not oracle predictions.
All dates/attributes/order and ZIP methods are fixed; entries use STORED.
"""
import base64
import io
import json
from pathlib import Path
import struct
import zipfile
import zlib

HP = 'http://www.hancom.co.kr/hwpml/2011/paragraph'
HH = 'http://www.hancom.co.kr/hwpml/2011/head'
HC = 'http://www.hancom.co.kr/hwpml/2011/core'
HS = 'http://www.hancom.co.kr/hwpml/2011/section'
OPF = 'http://www.idpf.org/2007/opf/'
SECTION = 'Contents/section0.xml'
HEADER = 'Contents/header.xml'
HPF = 'Contents/content.hpf'
CACHE = '<hp:linesegarray><hp:lineseg textpos="0" vertpos="100"/></hp:linesegarray>'
STYLE0 = '<hh:paraPr id="0" x:opaque="&#65;"><hh:align horizontal="LEFT" vertical="BASELINE"/><hh:lineSpacing type="PERCENT" value="160" unit="HWPUNIT"/><x:future flag=\'&#65;\'>opaque</x:future></hh:paraPr>'
STYLE1 = STYLE0.replace('id="0"', 'id="1"').replace('horizontal="LEFT"', 'horizontal="RIGHT"')
MEDIA_ITEM = '<opf:item id="new-image" href="BinData/new.png" media-type="image/png" isEmbeded="1"/>'


def png(rgb):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(b'\x00' + bytes(rgb))) + chunk(b'IEND', b''))


OLD_PNG = png((255, 0, 0))
NEW_PNG = png((0, 255, 0))


def paragraph(pid, text):
    return f'<hp:p id="{pid}" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run>{CACHE}</hp:p>'


def cell(pid, text):
    return ('<hp:tc borderFillIDRef="0"><hp:cellSpan colSpan="1" rowSpan="1"/>'
            '<hp:subList>' + paragraph(pid, text) + '</hp:subList></hp:tc>')


PIC = '<hp:pic id="P{n}" width="100" height="200" x:opaque="&#65;"><hc:img binaryItemIDRef="old-image" bright="0"/><hp:pos x="10" y="20"/></hp:pic>'
P0 = paragraph('p0', 'KEEP')
P1 = '<hp:p id="p1" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>Alpha </hp:t></hp:run><hp:run charPrIDRef="1"><hp:t>beta</hp:t></hp:run>' + CACHE + '</hp:p>'
P2 = '<hp:p id="p2" paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl rowCnt="1" colCnt="2"><hp:tr>' + cell('cell', 'cell') + cell('empty', '') + '</hp:tr></hp:tbl></hp:run>' + CACHE + '</hp:p>'
P3 = '<hp:p id="p3" paraPrIDRef="0"><hp:run charPrIDRef="0">' + PIC.format(n=0) + PIC.format(n=1) + '</hp:run>' + CACHE + '</hp:p>'
P4 = paragraph('p4', 'TAIL')
CELL_PATH = [2, 0, 0, 0, 0]
EMPTY_CELL_PATH = [2, 0, 0, 0, 1]
CELL_P_PATH = CELL_PATH + [1, 0]
PIC_PATH = [3, 0, 0]


def entries():
    section = (f'<?xml version="1.0" encoding="UTF-8"?>\n<hs:sec xmlns:hs="{HS}" xmlns:hp="{HP}" xmlns:hc="{HC}" xmlns:x="urn:oracle:opaque" x:keep="&#65;">'
               + P0 + P1 + P2 + P3 + P4 + '<x:opaque attr=\'&#65;\'>do not touch</x:opaque></hs:sec>')
    return {
        'mimetype': b'application/hwp+zip',
        'META-INF/container.xml': b'<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="Contents/content.hpf"/></rootfiles></container>',
        HPF: (f'<opf:package xmlns:opf="{OPF}"><opf:manifest><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/>'
              '<opf:item id="s0" href="Contents/section0.xml" media-type="application/xml"/><opf:item id="s1" href="Contents/section1.xml" media-type="application/xml"/>'
              '<opf:item id="old-image" href="BinData/old.png" media-type="image/png"/>'
              '</opf:manifest><opf:spine><opf:itemref idref="header"/><opf:itemref idref="s0"/><opf:itemref idref="s1"/></opf:spine></opf:package>').encode(),
        HEADER: (f'<hh:head xmlns:hh="{HH}" xmlns:x="urn:oracle:opaque"><hh:refList><hh:paraProperties itemCnt="2">'
                 + STYLE0 + STYLE1 + '</hh:paraProperties></hh:refList></hh:head>').encode(),
        SECTION: section.encode(),
        'Contents/section1.xml': (f'<hs:sec xmlns:hs="{HS}" xmlns:hp="{HP}">' + paragraph('other', 'OTHER') + '</hs:sec>').encode(),
        'BinData/old.png': OLD_PNG,
        'Preview/PrvText.txt': b'KEEP\nAlpha beta\n\ncell\n\n\nTAIL\nOTHER',
        'Preview/PrvImage.png': OLD_PNG,
        'Opaque/data.bin': b'\x00opaque payload\xff',
    }


def archive(payloads, method=zipfile.ZIP_STORED, level=None):
    out = io.BytesIO()
    with zipfile.ZipFile(out, 'w', compression=method, compresslevel=level) as z:
        for name, value in payloads.items():
            info = zipfile.ZipInfo(name, date_time=(2020, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED if name == 'mimetype' else method
            info.external_attr = 0o600 << 16
            z.writestr(info, value, compresslevel=level)
    return out.getvalue()


def unpack(data):
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        return {name: z.read(name) for name in z.namelist()}


def request(op, **kwargs):
    return {'schemaVersion': 1, 'operations': [{'op': op, 'entry': SECTION, **kwargs}]}


def case(name):
    original = entries()
    candidate = original.copy()
    # Explicit expected XML for every case, without reading the operation request.
    if name == 'noop':
        req = {'schemaVersion': 1, 'operations': []}
    elif name in ('text', 'cross_run', 'delete', 'escaped'):
        replacement = {'text': 'WORLD', 'cross_run': 'X', 'delete': '', 'escaped': '<&>'}[name]
        bounds = (3, 8) if name == 'cross_run' else (0, 10) if name == 'delete' else (6, 10)
        req = request('text.replace', path=[1], range={'start': bounds[0], 'end': bounds[1]}, text=replacement)
        new_p1 = {
            'text': P1.replace('beta', 'WORLD'),
            'cross_run': P1.replace('Alpha ', 'AlpX').replace('beta', 'ta'),
            'delete': P1.replace('Alpha ', '').replace('beta', ''),
            'escaped': P1.replace('beta', '&lt;&amp;&gt;'),
        }[name].replace(CACHE, '')
        tail = P2.replace('</hp:run>' + CACHE + '</hp:p>', '</hp:run></hp:p>')
        # Preserve nested cell caches; invalidate only the top-level table paragraph.
        tail = P2[:-len(CACHE + '</hp:p>')] + '</hp:p>'
        xml = candidate[SECTION].decode().replace(P1, new_p1).replace(P2, tail).replace(P3, P3.replace(CACHE, '')).replace(P4, P4.replace(CACHE, ''))
        candidate[SECTION] = xml.encode()
        expected_text = {'text': 'Alpha WORLD', 'cross_run': 'AlpXta', 'delete': '', 'escaped': 'Alpha <&>'}[name]
        candidate['Preview/PrvText.txt'] = candidate['Preview/PrvText.txt'].replace(b'Alpha beta', expected_text.encode())
    elif name in ('cell', 'empty_cell'):
        path = CELL_PATH if name == 'cell' else EMPTY_CELL_PATH
        req = request('cell.setText', path=path, text='NEW')
        old = paragraph('cell', 'cell') if name == 'cell' else paragraph('empty', '')
        new = paragraph('cell', 'NEW') if name == 'cell' else paragraph('empty', 'NEW')
        p2 = P2.replace(old, new.replace(CACHE, ''))
        p2 = p2[:-len(CACHE + '</hp:p>')] + '</hp:p>'
        candidate[SECTION] = candidate[SECTION].decode().replace(P2, p2).replace(P3, P3.replace(CACHE, '')).replace(P4, P4.replace(CACHE, '')).encode()
        candidate['Preview/PrvText.txt'] = b'KEEP\nAlpha beta\n\nNEW\n\n\nTAIL\nOTHER' if name == 'cell' else original['Preview/PrvText.txt']
    elif name in ('format_clone', 'format_reuse'):
        clone = name == 'format_clone'
        req = request('paragraph.format', path=[1], alignment='center' if clone else 'right', **({'lineSpacingPercent': 180} if clone else {}))
        p1 = P1.replace('paraPrIDRef="0"', 'paraPrIDRef="2"' if clone else 'paraPrIDRef="1"').replace(CACHE, '')
        p2 = P2[:-len(CACHE + '</hp:p>')] + '</hp:p>'
        candidate[SECTION] = candidate[SECTION].decode().replace(P1, p1).replace(P2, p2).replace(P3, P3.replace(CACHE, '')).replace(P4, P4.replace(CACHE, '')).encode()
        if clone:
            style = STYLE0.replace('id="0"', 'id="2"').replace('horizontal="LEFT"', 'horizontal="CENTER"').replace('value="160"', 'value="180"')
            candidate[HEADER] = candidate[HEADER].decode().replace('itemCnt="2"', 'itemCnt="3"').replace('</hh:paraProperties>', style + '</hh:paraProperties>').encode()
    elif name == 'image':
        req = request('image.replace', path=PIC_PATH, image={'mediaType': 'image/png', 'base64': base64.b64encode(NEW_PNG).decode()})
        candidate[SECTION] = candidate[SECTION].replace(b'binaryItemIDRef="old-image"', b'binaryItemIDRef="new-image"', 1)
        candidate[HPF] = candidate[HPF].replace(b'</opf:manifest>', MEDIA_ITEM.encode() + b'</opf:manifest>')
        candidate['BinData/new.png'] = NEW_PNG
    else:
        raise ValueError(name)
    return archive(original), archive(candidate), req


CASES = ('noop', 'text', 'cross_run', 'delete', 'escaped', 'cell', 'empty_cell', 'format_clone', 'format_reuse', 'image')
