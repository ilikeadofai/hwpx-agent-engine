#!/usr/bin/env python3
"""Independent, fail-closed HWPX preservation oracle. Python standard library only."""
from __future__ import annotations

import argparse
import base64
import copy
from dataclasses import dataclass, field
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import sys
import unicodedata
import xml.parsers.expat as expat
import zipfile
import zlib

HP = 'http://www.hancom.co.kr/hwpml/2011/paragraph'
HP16 = 'http://www.hancom.co.kr/hwpml/2016/paragraph'
HS = 'http://www.hancom.co.kr/hwpml/2011/section'
HH = 'http://www.hancom.co.kr/hwpml/2011/head'
HC = 'http://www.hancom.co.kr/hwpml/2011/core'
OPF = 'http://www.idpf.org/2007/opf/'
OCF = 'urn:oasis:names:tc:opendocument:xmlns:container'
MAX_FILE = 32 * 1024 * 1024
MAX_EXPANDED = 128 * 1024 * 1024


class OracleError(Exception):
    def __init__(self, code, message):
        self.code, self.message = code, message
        super().__init__(message)


def need(condition, code, message):
    if not condition:
        raise OracleError(code, message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def q(ns, local):
    return ns + '}' + local


def hp(node, local):
    return node.tag in (q(HP, local), q(HP16, local))


@dataclass
class Node:
    tag: str
    attrs: dict
    children: list = field(default_factory=list)
    gaps: list = field(default_factory=list)
    text: str = ''
    opening: bytes = b''
    closing: bytes = b''
    allowed: set = field(default_factory=set)
    text_changed: bool = False
    generated: bool = False
    changed_gaps: bool = False
    expanded: bool = False
    start: int = 0
    open_end: int = 0
    close_start: int = 0
    end: int = 0

    def walk(self):
        yield self
        for child in self.children:
            yield from child.walk()

    def at(self, path):
        node = self
        for index in path:
            need(type(index) is int and index >= 0 and index < len(node.children),
                 'INVALID_TARGET', 'structural path does not resolve')
            node = node.children[index]
        return node

    def set_attr(self, name, value):
        need(name in self.attrs, 'UNSUPPORTED', 'cannot synthesize missing attribute ' + name)
        if self.attrs[name] != value:
            self.allowed.add(name)
            self.attrs[name] = value

    def remove(self, index):
        del self.children[index]
        self.gaps[index:index + 2] = [self.gaps[index] + self.gaps[index + 1]]

    def append(self, node):
        self.children.append(node)
        self.gaps.append(b'')


@dataclass
class Xml:
    root: Node
    before: bytes
    after: bytes


def parse_xml(data):
    try:
        decoded = data.decode('utf-8-sig')
    except UnicodeDecodeError as exc:
        raise OracleError('INVALID_XML', 'non UTF-8 XML') from exc
    need(not re.search(r'<!\s*(DOCTYPE|ENTITY)', decoded, re.I), 'INVALID_XML', 'DTD/entity forbidden')
    declaration = re.match(r'\s*<\?xml\b[^?]*\?>', decoded)
    if declaration:
        encoding = re.search(r'encoding\s*=\s*[\'"]([^\'"]+)', declaration[0], re.I)
        need(not encoding or encoding[1].lower() == 'utf-8', 'INVALID_XML', 'non UTF-8 declaration')
    parser = expat.ParserCreate(namespace_separator='}')
    stack, roots = [], []

    def tag_end(start):
        quote = None
        for i in range(start, len(data)):
            char = data[i]
            if quote:
                if char == quote:
                    quote = None
            elif char in (34, 39):
                quote = char
            elif char == 62:
                return i + 1
        raise OracleError('INVALID_XML', 'unclosed opening tag')

    def start(tag, attrs):
        need(len(stack) < 128, 'RESOURCE_LIMIT', 'XML depth exceeds 128')
        i = parser.CurrentByteIndex
        end = tag_end(i)
        node = Node(tag, attrs, start=i, open_end=end, opening=data[i:end])
        (stack[-1].children if stack else roots).append(node)
        stack.append(node)

    def end(tag):
        node = stack.pop()
        if node.opening.rstrip().endswith(b'/>'):
            node.close_start = node.end = node.open_end
        else:
            node.close_start = parser.CurrentByteIndex
            node.end = tag_end(node.close_start)
            node.closing = data[node.close_start:node.end]
        bounds = [node.open_end] + [child.end for child in node.children]
        ends = [child.start for child in node.children] + [node.close_start]
        node.gaps = [data[a:b] for a, b in zip(bounds, ends)]

    def chars(value):
        if stack:
            stack[-1].text += value

    parser.StartElementHandler = start
    parser.EndElementHandler = end
    parser.CharacterDataHandler = chars
    try:
        parser.Parse(data, True)
    except expat.ExpatError as exc:
        raise OracleError('INVALID_XML', 'XML parse failed') from exc
    need(len(roots) == 1, 'INVALID_XML', 'expected one root')
    root = roots[0]
    return Xml(root, data[:root.start], data[root.end:])


def safe_name(name):
    return (isinstance(name, str) and bool(name) and not name.startswith('/')
            and not any(x in name for x in ('\\', ':', '\x00', '%', '?', '#'))
            and all(p not in ('', '.', '..') for p in name.rstrip('/').split('/')))


class Package:
    def __init__(self, source):
        if isinstance(source, (str, Path)):
            need(Path(source).stat().st_size <= MAX_FILE, 'RESOURCE_LIMIT', 'archive exceeds 32 MiB')
            source = Path(source).read_bytes()
        need(isinstance(source, bytes) and len(source) <= MAX_FILE, 'RESOURCE_LIMIT', 'archive exceeds 32 MiB')
        self.raw = source
        self.payload, self.compressed, self.methods, self.xml = {}, {}, {}, {}
        try:
            with zipfile.ZipFile(io.BytesIO(source)) as archive:
                infos = archive.infolist()
                need(0 < len(infos) <= 2048, 'RESOURCE_LIMIT', 'ZIP entry count outside limits')
                need(sum(i.file_size for i in infos) <= MAX_EXPANDED, 'RESOURCE_LIMIT', 'ZIP expansion exceeds limit')
                names = [i.filename for i in infos]
                need(len(set(names)) == len(names), 'INVALID_PACKAGE', 'duplicate ZIP entry')
                need(len({n.casefold() for n in names}) == len(names), 'INVALID_PACKAGE', 'case collision')
                self.names = names
                for info in infos:
                    name = info.filename
                    need(safe_name(name), 'INVALID_PACKAGE', 'unsafe ZIP entry name')
                    need(not info.flag_bits & 1, 'INVALID_PACKAGE', 'encrypted ZIP entry')
                    need(info.compress_type in (0, 8), 'UNSUPPORTED', 'ZIP method unsupported')
                    need(info.flag_bits & 0x800 or name.isascii(), 'UNSUPPORTED', 'non UTF-8 ZIP name')
                    need(info.extract_version < 45, 'UNSUPPORTED', 'ZIP64 unsupported')
                    offset = info.header_offset
                    need(offset >= 0 and source[offset:offset + 4] == b'PK\x03\x04', 'INVALID_PACKAGE', 'bad local header')
                    header = struct.unpack_from('<4s5H3I2H', source, offset)
                    need(header[2] == info.flag_bits and header[3] == info.compress_type,
                         'INVALID_PACKAGE', 'local header method/flags disagree')
                    local_name = source[offset + 30:offset + 30 + header[-2]]
                    need(local_name == name.encode('utf-8'), 'INVALID_PACKAGE', 'local header name disagrees')
                    begin = offset + 30 + header[-2] + header[-1]
                    finish = begin + info.compress_size
                    need(finish <= len(source), 'INVALID_PACKAGE', 'truncated ZIP data')
                    self.compressed[name] = source[begin:finish]
                    with archive.open(info) as member:
                        value = member.read(min(info.file_size, MAX_EXPANDED) + 1)
                    need(len(value) == info.file_size, 'INVALID_PACKAGE', 'ZIP size disagrees')
                    self.payload[name] = value
                    self.methods[name] = info.compress_type
                    if name.lower().endswith(('.xml', '.hpf', '.rdf')):
                        self.xml[name] = parse_xml(value)
        except (zipfile.BadZipFile, struct.error, RuntimeError, NotImplementedError, zlib.error) as exc:
            raise OracleError('INVALID_PACKAGE', 'ZIP decoding failed') from exc
        need(self.names[0] == 'mimetype' and self.methods['mimetype'] == 0
             and self.payload['mimetype'] == b'application/hwp+zip', 'INVALID_PACKAGE', 'bad mimetype')
        need(not any('encrypt' in n.lower() for n in names), 'UNSUPPORTED', 'encryption metadata')
        self.relations()

    def resolve(self, href, base):
        need(safe_name(href), 'INVALID_REFERENCE', 'unsafe manifest href')
        candidates = {x for x in (href, base + href) if x in self.payload}
        need(len(candidates) == 1, 'INVALID_REFERENCE', 'missing or ambiguous manifest href')
        return next(iter(candidates))

    def relations(self):
        need('META-INF/container.xml' in self.xml, 'INVALID_PACKAGE', 'missing container')
        refs = [n.attrs.get('full-path') for n in self.xml['META-INF/container.xml'].root.walk()
                if n.tag == q(OCF, 'rootfile') and n.attrs.get('full-path', '').endswith('.hpf')]
        need(len(refs) == 1 and refs[0] in self.xml, 'INVALID_REFERENCE', 'ambiguous/missing rootfile')
        self.hpf = refs[0]
        base = self.hpf.rsplit('/', 1)[0] + '/' if '/' in self.hpf else ''
        tree = self.xml[self.hpf].root
        manifests = [n for n in tree.walk() if n.tag in (q(OPF, 'manifest'), q(OPF.rstrip('/'), 'manifest'))]
        need(len(manifests) == 1, 'INVALID_REFERENCE', 'expected one manifest')
        self.manifest = manifests[0]
        self.items = {}
        for node in self.manifest.children:
            need(node.tag in (q(OPF, 'item'), q(OPF.rstrip('/'), 'item')), 'UNSUPPORTED', 'opaque manifest child')
            key, href = node.attrs.get('id'), node.attrs.get('href')
            need(key and key not in self.items and href, 'INVALID_REFERENCE', 'duplicate/missing manifest id')
            self.items[key] = (self.resolve(href, base), node)
        self.sections = []
        for n in tree.walk():
            if n.tag in (q(OPF, 'itemref'), q(OPF.rstrip('/'), 'itemref')):
                ref = n.attrs.get('idref')
                need(ref in self.items, 'INVALID_REFERENCE', 'dangling spine idref')
                entry = self.items[ref][0]
                need(entry in self.xml, 'INVALID_REFERENCE', 'spine entry is not XML')
                if self.xml[entry].root.tag == q(HS, 'sec'):
                    need(entry not in self.sections, 'INVALID_REFERENCE', 'duplicate spine section')
                    self.sections.append(entry)
        need(self.sections, 'INVALID_REFERENCE', 'no spine sections')
        need({n for n, x in self.xml.items() if x.root.tag == q(HS, 'sec')} == set(self.sections),
             'INVALID_REFERENCE', 'section outside spine')
        heads = [n for n, x in self.xml.items() if x.root.tag == q(HH, 'head')]
        need(len(heads) <= 1, 'UNSUPPORTED', 'multiple headers')
        self.header = heads[0] if heads else None
        for entry, tree in self.xml.items():
            for node in tree.root.walk():
                for key, ref in node.attrs.items():
                    if key.rsplit('}', 1)[-1] == 'binaryItemIDRef':
                        need(ref in self.items, 'INVALID_MEDIA_REFERENCE', 'unknown binaryItemIDRef')
                        media_entry, item = self.items[ref]
                        need(media_entry.startswith('BinData/') and item.attrs.get('media-type', '').startswith('image/'),
                             'INVALID_MEDIA_REFERENCE', 'binary reference is not embedded image media')
        if self.header:
            styles = [n for n in self.xml[self.header].root.walk() if n.tag == q(HH, 'paraPr')]
            ids = [n.attrs.get('id') for n in styles]
            need(None not in ids and len(set(ids)) == len(ids), 'INVALID_REFERENCE', 'duplicate paragraph style ids')
            for entry in self.sections:
                for node in self.xml[entry].root.walk():
                    if hp(node, 'p'):
                        need(node.attrs.get('paraPrIDRef') in ids, 'INVALID_REFERENCE', 'dangling paragraph style')


def masked_open(node, attributes):
    value = node.opening
    for name in sorted(attributes):
        pattern = rb'(\s' + re.escape(name.encode()) + rb'\s*=\s*)([\'"])(.*?)\2'
        value, count = re.subn(pattern, lambda m: m[1] + m[2] + b'__ORACLE_VALUE__' + m[2], value, flags=re.S)
        need(count == 1, 'UNSUPPORTED', 'attribute has no unique lexical token')
    return value


def compare_node(expected, actual, location):
    need(expected.tag == actual.tag, 'STRUCTURE_CHANGED', location)
    need(expected.attrs == actual.attrs, 'ATTRIBUTE_CHANGED', location)
    need(len(expected.children) == len(actual.children), 'STRUCTURE_CHANGED', location)
    need(expected.text == actual.text, 'TEXT_CHANGED', location)
    if not expected.generated:
        left, right = masked_open(expected, expected.allowed), masked_open(actual, expected.allowed)
        if expected.changed_gaps or expected.expanded:
            if expected.expanded:
                left = re.sub(rb'\s*/>$', b'>', left)
            need(left == right and expected.gaps == actual.gaps, 'LEXICAL_CHANGED', location)
            if not expected.expanded:
                need(expected.closing == actual.closing, 'LEXICAL_CHANGED', location)
        elif expected.text_changed:
            left, right = re.sub(rb'/\s*>$', b'>', left), re.sub(rb'/\s*>$', b'>', right)
            need(not actual.children and not re.search(rb'<', b''.join(actual.gaps)),
                 'LEXICAL_CHANGED', 'new text must be escaped character data: ' + location)
        else:
            need(expected.closing == actual.closing, 'LEXICAL_CHANGED', location)
            need(expected.gaps == actual.gaps, 'LEXICAL_CHANGED', location)
        need(left == right, 'LEXICAL_CHANGED', location)
    for i, (a, b) in enumerate(zip(expected.children, actual.children)):
        compare_node(a, b, location + '/' + str(i))


def schema(obj, required, optional=()):
    need(isinstance(obj, dict) and set(required) <= obj.keys()
         and obj.keys() <= set(required) | set(optional), 'INVALID_REQUEST', 'missing or unknown JSON fields')


def simple_graphemes(text):
    # Conservative proven subset: one scalar = one grapheme. No approximation of UAX #29.
    for ch in text:
        n, cat = ord(ch), unicodedata.category(ch)
        need(cat[0] not in ('M', 'C') and n <= 0xffff
             and not (0x1100 <= n <= 0x11ff or 0xa960 <= n <= 0xa97f
                      or 0xd7b0 <= n <= 0xd7ff or 0xfe00 <= n <= 0xfe0f),
             'UNSUPPORTED_GRAPHEME', 'requires full pinned UAX #29 segmentation')
    return list(text)


def plain_text(value):
    need(isinstance(value, str), 'INVALID_REQUEST', 'text must be string')
    need(all(c not in '\t\r\n' and (ord(c) >= 32) and not 0xd800 <= ord(c) <= 0xdfff
             and ord(c) not in (0xfffe, 0xffff) for c in value), 'INVALID_REQUEST', 'invalid replacement text')


def pieces(paragraph):
    need(hp(paragraph, 'p'), 'INVALID_TARGET', 'target must be paragraph')
    result = []
    for child in paragraph.children:
        if hp(child, 'linesegarray'):
            continue
        need(hp(child, 'run') and set(child.attrs) <= {'charPrIDRef'},
             'UNSUPPORTED', 'paragraph has controls or opaque run attributes')
        need(child.children, 'UNSUPPORTED', 'run without text element')
        for node in child.children:
            need(hp(node, 't') and not node.children and not node.attrs,
                 'UNSUPPORTED', 'non-plain text or opaque text attributes')
            result.append(node)
    need(result, 'UNSUPPORTED', 'paragraph has no runs')
    return result


def text_of(paragraph):
    return ''.join(t.text for t in pieces(paragraph))


def replace_range(paragraph, start, end, replacement, empty_cell=False):
    nodes = pieces(paragraph)
    original = ''.join(t.text for t in nodes)
    simple_graphemes(original)
    need(type(start) is int and type(end) is int and 0 <= start < end <= len(original)
         or empty_cell and start == end == 0 and original == '', 'INVALID_RANGE', 'range must be nonempty and within paragraph')
    new_text = original[:start] + replacement + original[end:]
    if new_text == original:
        return False
    position, inserted = 0, False
    for node in nodes:
        a, b = position, position + len(node.text)
        position = b
        if (max(a, start) < min(b, end)) or (empty_cell and not inserted):
            first, last = max(start - a, 0), min(end - a, len(node.text))
            value = node.text[:first] + (replacement if not inserted else '') + node.text[last:]
            inserted = True
            if node.text != value:
                node.text, node.text_changed = value, True
    need(inserted, 'INVALID_RANGE', 'no text carrier')
    return True


def decode_gap(raw):
    need(b'<!' not in raw and b'<?' not in raw and not any(c in raw for c in (b'\t', b'\n', b'\r')),
         'UNSUPPORTED', 'CDATA/comments/PI/literal controls are outside oracle text subset')
    result=[]
    for m in re.finditer(rb'&(?:#[0-9]+|#x[0-9a-fA-F]+|amp|lt|gt|quot|apos);|[^&]',raw):
        token=m[0]
        if token.startswith(b'&'):
            value=token[1:-1].decode()
            ch=chr(int(value[2:],16)) if value.startswith('#x') else chr(int(value[1:])) if value.startswith('#') else {'amp':'&','lt':'<','gt':'>','quot':'"','apos':"'"}[value]
            result.append((m.start(),m.end(),ch))
        elif token[0]<128:
            result.append((m.start(),m.end(),token.decode()))
    # Non-ASCII is decoded a scalar at a time, with byte ranges retained.
    if any(c>=128 for c in raw):
        result=[];offset=0
        for token in re.findall(r'&(?:#[0-9]+|#x[0-9a-fA-F]+|amp|lt|gt|quot|apos);|[^&]',raw.decode()):
            data=token.encode();ch=decode_gap(data)[0][2] if token.startswith('&') else token
            result.append((offset,offset+len(data),ch));offset+=len(data)
    return result


def v02_view(paragraph):
    need(hp(paragraph,'p'),'INVALID_TARGET','target must be paragraph')
    scalars=[];empty=[];runs=[]
    for run in paragraph.children:
        if hp(run,'linesegarray'):continue
        need(hp(run,'run'),'UNSUPPORTED','non-run paragraph child')
        runs.append(run)
        need(not run.text.strip(),'UNSUPPORTED','direct run text')
        need(not any(b'<!' in gap or b'<?' in gap for gap in run.gaps),'UNSUPPORTED','run lexical barriers')
        for t in run.children:
            if hp(t,'secPr'):continue
            if not hp(t,'t'):
                scalars.append((None,0,0,0,'\ufffc'));continue
            need(set(t.attrs)<={'charPrIDRef'},'UNSUPPORTED','opaque hp:t attributes')
            if not t.text and not t.children and all(not g for g in t.gaps):empty.append((t,len(scalars)))
            for i,gap in enumerate(t.gaps):
                for a,b,ch in decode_gap(gap):scalars.append((t,i,a,b,ch))
                if i<len(t.children):
                    child=t.children[i]
                    need(hp(child,'tab') or hp(child,'lineBreak'),'UNSUPPORTED','unknown text child')
                    scalars.append((None,0,0,0,'\t' if hp(child,'tab') else '\n'))
    return scalars,empty,runs


def v02_controls(paragraph):
    result=[];cp=0
    for run in paragraph.children:
        if not hp(run,'run'):continue
        for unit in run.children:
            if hp(unit,'secPr'):continue
            if hp(unit,'t'):
                for i,gap in enumerate(unit.gaps):
                    cp+=len(decode_gap(gap))
                    if i<len(unit.children):result.append((cp,unit.children[i],run,unit));cp+=1
            else:result.append((cp,unit,run,unit));cp+=1
    return result


def v02_text(paragraph):
    return ''.join(s[4] for s in v02_view(paragraph)[0])


def replace_v02(paragraph,start,end,replacement,preserve=False,empty_cell=False):
    scalars,empty,runs=v02_view(paragraph)
    original=''.join(s[4] for s in scalars)
    simple_graphemes(original.replace('\t','').replace('\n',''))
    need(type(start) is int and type(end) is int and 0<=start<=end<=len(scalars),'INVALID_RANGE','invalid original grapheme range')
    need(start!=end or replacement or empty_cell,'INVALID_RANGE','empty insertion')
    if start==end:
        if original and all(t is None for t,*_ in scalars):
            controls=v02_controls(paragraph)
            def site(row,before):
                cp,obj,run,unit=row
                related=[c for c in controls if c[3] is unit]
                if (before and cp!=related[0][0]) or (not before and cp!=related[-1][0]):return None
                return run,unit,before
            right=next((r for r in controls if r[0]==start),None)
            left=next((r for r in reversed(controls) if r[0]+1==start),None)
            picked=(site(right,True) if right else None) or (site(left,False) if left else None)
            need(picked is not None,'INVALID_RANGE','object boundary has no same-run insertion site')
            run,unit,before=picked;index=run.children.index(unit)+(0 if before else 1)
            node=Node(run.tag.rsplit('}',1)[0]+'}t',{},text=replacement,gaps=[xml_text(replacement)],generated=True)
            run.children.insert(index,node);run.gaps.insert(index+1 if before else index,b'')
            return bool(replacement)
        if original:
            chosen=scalars[start] if start<len(scalars) and scalars[start][0] is not None else scalars[start-1] if start>0 and scalars[start-1][0] is not None else None
            need(chosen is not None,'INVALID_RANGE','outside insertion spans')
            node,g,a,b,_=chosen;at=a if start<len(scalars) and chosen is scalars[start] else b
            node.gaps[g]=node.gaps[g][:at]+xml_text(replacement)+node.gaps[g][at:]
            node.text=node.text[:sum(1 for t,*_ in scalars[:start] if t is node)]+replacement+node.text[sum(1 for t,*_ in scalars[:start] if t is node):]
            node.changed_gaps=True
            return bool(replacement)
        need(runs,'UNSUPPORTED','no first run')
        if not replacement:return False
        if empty:
            node=next((t for t,_ in empty if t in runs[0].children),None)
            need(node is not None,'UNSUPPORTED','first run has no reusable text')
            node.text=replacement;node.gaps=[xml_text(replacement)];node.changed_gaps=True
            node.expanded=not node.closing
        else:
            run=runs[0]
            tag=paragraph.tag.rsplit('}',1)[0]+'}t'
            node=Node(tag,{},text=replacement,gaps=[xml_text(replacement)],generated=True)
            run.append(node);run.expanded=not run.closing
        return True
    covered=scalars[start:end]
    if preserve:
        need(not any(start<=cp<end and any(hp(n,'fieldBegin') or hp(n,'fieldEnd') for n in obj.walk()) for cp,obj,_,_ in v02_controls(paragraph)), 'UNSUPPORTED', 'field_boundary_in_range')
    need(preserve or all(t is not None for t,*_ in covered),'UNSUPPORTED','range crosses controls')
    mapped=[x for x in covered if x[0] is not None]
    need(all(ch not in '\t\r\n' for _,_,_,_,ch in mapped),'UNSUPPORTED','literal/reference control character')
    if not mapped:
        node=next((t for t,cp in empty if start<=cp<=end),None)
        need(node is not None,'UNSUPPORTED','no existing text fragment')
        if not replacement:return False
        node.text=replacement;node.gaps=[xml_text(replacement)];node.changed_gaps=True;node.expanded=not node.closing
        return True
    wanted=original[:start];put=False
    for node,_,_,_,ch in covered:
        if node is None:wanted+=ch
        elif not put:wanted+=replacement;put=True
    wanted+=original[end:]
    if wanted==original:return False
    edits={};inserted=False
    for node,g,a,b,ch in mapped:
        edits.setdefault((id(node),g),(node,[]))[1].append((a,b,xml_text(replacement) if not inserted else b''));inserted=True
    for (_,g),(node,cuts) in edits.items():
        raw=node.gaps[g]
        for a,b,value in reversed(cuts):raw=raw[:a]+value+raw[b:]
        node.gaps[g]=raw;node.changed_gaps=True
    for node in {id(x[0]):x[0] for x in mapped}.values():
        node.text=''.join(ch for gap in node.gaps for _,_,ch in decode_gap(gap))
    return v02_text(paragraph)!=original


def xml_text(value):
    return value.replace('&','&amp;').replace('<','&lt;').replace('>','&gt;').encode()


def cell_paragraphs(cell):
    need(hp(cell,'tc'),'INVALID_TARGET','target must be tc')
    need(not any(hp(n,'tbl') or hp(n,'tc') and n is not cell for n in cell.walk()),'UNSUPPORTED','nested table/cell')
    subs=[n for n in cell.children if hp(n,'subList')]
    need(len(subs)==1 and subs[0].children and all(hp(n,'p') for n in subs[0].children),'UNSUPPORTED','cell paragraph structure')
    return subs[0].children


def semantic(node, ignore_id=False):
    return (node.tag, {k: v for k, v in node.attrs.items() if not (ignore_id and k == 'id')},
            node.text, [semantic(c) for c in node.children])


def png_bytes(image):
    schema(image, ('base64', 'mediaType'))
    need(image['mediaType'] == 'image/png', 'UNSUPPORTED_IMAGE', 'oracle supports static 8-bit noninterlaced PNG only')
    need(isinstance(image['base64'], str) and len(image['base64']) <= MAX_FILE * 4 // 3 + 4,
         'INVALID_REQUEST', 'invalid image base64')
    try:
        data = base64.b64decode(image['base64'], validate=True)
    except ValueError as exc:
        raise OracleError('INVALID_REQUEST', 'invalid image base64') from exc
    need(data.startswith(b'\x89PNG\r\n\x1a\n'), 'INVALID_IMAGE', 'PNG signature')
    pos, chunks = 8, []
    while pos < len(data):
        need(pos + 12 <= len(data), 'INVALID_IMAGE', 'truncated PNG')
        size = struct.unpack_from('>I', data, pos)[0]
        kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + size]
        need(pos + 12 + size <= len(data), 'INVALID_IMAGE', 'truncated PNG chunk')
        crc = struct.unpack_from('>I', data, pos + 8 + size)[0]
        need(zlib.crc32(kind + body) & 0xffffffff == crc, 'INVALID_IMAGE', 'PNG CRC')
        need(kind in (b'IHDR', b'IDAT', b'IEND'), 'UNSUPPORTED_IMAGE', 'PNG ancillary/animation chunks unsupported')
        chunks.append((kind, body))
        pos += size + 12
    need(len(chunks) >= 3 and chunks[0][0] == b'IHDR' and len(chunks[0][1]) == 13
         and chunks[-1] == (b'IEND', b'') and all(k == b'IDAT' for k, _ in chunks[1:-1]),
         'INVALID_IMAGE', 'PNG chunk sequence')
    width, height, depth, color, compression, filtering, interlace = struct.unpack('>IIBBBBB', chunks[0][1])
    need(0 < width * height <= 40960000 and width > 0 and height > 0, 'RESOURCE_LIMIT', 'PNG pixel limit')
    need(depth == 8 and color in (0, 2, 4, 6) and compression == filtering == interlace == 0,
         'UNSUPPORTED_IMAGE', 'PNG mode unsupported')
    stride = width * {0: 1, 2: 3, 4: 2, 6: 4}[color] + 1
    need(stride * height <= MAX_EXPANDED, 'RESOURCE_LIMIT', 'PNG expansion limit')
    try:
        decoder = zlib.decompressobj()
        raw = decoder.decompress(b''.join(b for _, b in chunks[1:-1]), stride * height + 1)
    except zlib.error as exc:
        raise OracleError('INVALID_IMAGE', 'PNG zlib failure') from exc
    need(len(raw) == stride * height and decoder.eof and not decoder.unused_data
         and all(raw[i] <= 4 for i in range(0, len(raw), stride)), 'INVALID_IMAGE', 'PNG scanline data')
    return data


def one(nodes, message):
    need(len(nodes) == 1, 'UNSUPPORTED', message)
    return nodes[0]


def preview_text(package):
    lines = []
    for entry in package.sections:
        for paragraph in package.xml[entry].root.walk():
            if hp(paragraph, 'p'):
                # Flatten only direct run text, never include descendant cell text twice.
                lines.append(''.join(t.text for r in paragraph.children if hp(r, 'run')
                                     for t in r.children if hp(t, 't') and not t.children))
    return '\n'.join(lines)


def update_preview(original, expected, changed):
    name = 'Preview/PrvText.txt'
    if not changed or name not in original.payload:
        return
    before, after = preview_text(original), preview_text(expected)
    if before == after:
        return
    payload = original.payload[name]
    formats = []
    for encoding, bom in (('utf-8', b''), ('utf-8', b'\xef\xbb\xbf'), ('utf-16-le', b'\xff\xfe')):
        for newline in ('\n', '\r\n'):
            for trailing in ('', newline):
                if bom + (before.replace('\n', newline) + trailing).encode(encoding) == payload:
                    formats.append(bom + (after.replace('\n', newline) + trailing).encode(encoding))
    need(formats and len(set(formats)) == 1, 'UNSUPPORTED_PREVIEW',
         'PrvText is not an unambiguous full paragraph transcript')
    expected.payload[name] = formats[0]


def expected_state(original, candidate, request):
    schema(request, ('schemaVersion', 'operations'), ('intents',))
    need(type(request['schemaVersion']) is int and request['schemaVersion'] == 1,
         'INVALID_REQUEST', 'schemaVersion must be 1')
    ops = request['operations']
    need(isinstance(ops, list) and len(ops) <= 256, 'INVALID_REQUEST', 'operations must be array (max 256)')
    if ops:
        need(not any('signature' in name.lower() for name in original.names), 'UNSUPPORTED', 'signed document')
        need(not any('track' in n.tag.rsplit('}', 1)[-1].lower() for x in original.xml.values() for n in x.root.walk()),
             'UNSUPPORTED', 'tracked document')
    expected = copy.deepcopy(original)
    intents, changes, anchors, touched = [], [], {}, set()
    new_images, new_styles = set(), set()
    for op in ops:
        need(isinstance(op, dict), 'INVALID_REQUEST', 'operation must be object')
        kind = op.get('op')
        requirements = {
            'text.replace': (('range', 'text'), ('controls',)),
            'cell.setText': ((), ('text','paragraphs','controls')),
            'paragraph.format': ((), ('alignment', 'lineSpacingPercent')),
            'image.replace': (('image',), ()),
        }
        need(kind in requirements, 'INVALID_REQUEST', 'unknown operation')
        required, optional = requirements[kind]
        schema(op, ('op', 'entry', 'path') + required, optional)
        entry, path = op['entry'], op['path']
        need(isinstance(entry, str) and entry in original.sections and isinstance(path, list)
             and path and all(type(i) is int and i >= 0 for i in path), 'INVALID_TARGET', 'entry/path must identify section element')
        root = expected.xml[entry].root
        target = root.at(path)
        # Each operation owns a distinct target paragraph or picture in this initial oracle.
        paras = cell_paragraphs(target) if kind == 'cell.setText' else [target]
        para = paras[0]
        key = (entry, id(para))
        need(key not in touched, 'UNSUPPORTED', 'multiple operations on same paragraph/picture')
        touched.add(key)
        intent = {'op': kind, 'entry': entry, 'path': path}
        changed = False
        if kind in ('text.replace', 'cell.setText'):
            controls=op.get('controls','reject')
            need(controls in ('reject','preserve'),'INVALID_REQUEST','unknown controls mode')
            preserve=controls=='preserve'
            if kind=='text.replace':
                plain_text(op['text']);bounds=op['range'];schema(bounds,('start','end'))
                if preserve or bounds['start']==bounds['end']:
                    changed=replace_v02(para,bounds['start'],bounds['end'],op['text'],preserve)
                    intent['expectedText']=v02_text(para)
                else:
                    changed=replace_range(para,bounds['start'],bounds['end'],op['text'])
                    intent['expectedText']=text_of(para)
            else:
                need(('text' in op)!=('paragraphs' in op),'INVALID_ARGUMENT','text/paragraphs are mutually exclusive')
                values=op.get('paragraphs',[op.get('text')])
                need('paragraphs' in op or len(paras)==1,'UNSUPPORTED','use paragraphs')
                need(isinstance(values,list) and len(values)==len(paras),'INVALID_ARGUMENT','exact paragraph count required')
                for para,value in zip(paras,values):
                    plain_text(value)
                    text=v02_text(para)
                    changed=replace_v02(para,0,len(text),value,preserve,empty_cell=not text) or changed
                para=paras[0]
                if len(paras)==1:intent['expectedText']=v02_text(para)
                else:intent['expectedParagraphs']=[v02_text(p) for p in paras]
        elif kind == 'paragraph.format':
            need(hp(para, 'p'), 'INVALID_TARGET', 'format target must be paragraph')
            need(expected.header is not None, 'UNSUPPORTED', 'missing header')
            styles_parent = one([n for n in expected.xml[expected.header].root.walk()
                                 if n.tag == q(HH, 'paraProperties')], 'expected paraProperties')
            styles = styles_parent.children
            need(all(n.tag == q(HH, 'paraPr') for n in styles), 'UNSUPPORTED', 'opaque style collection child')
            old = one([n for n in styles if n.attrs.get('id') == para.attrs.get('paraPrIDRef')], 'missing style')
            wanted = copy.deepcopy(old)
            alignment, spacing = op.get('alignment'), op.get('lineSpacingPercent')
            need(alignment is not None or spacing is not None, 'INVALID_REQUEST', 'empty format request')
            if alignment is not None:
                values = {'left': 'LEFT', 'right': 'RIGHT', 'center': 'CENTER', 'justify': 'JUSTIFY',
                          'distribute': 'DISTRIBUTE', 'distributeSpace': 'DISTRIBUTE_SPACE'}
                need(isinstance(alignment, str) and alignment in values, 'INVALID_REQUEST', 'unknown alignment')
                one([n for n in wanted.children if n.tag == q(HH, 'align')], 'missing align').set_attr('horizontal', values[alignment])
            if spacing is not None:
                need(type(spacing) is int and 1 <= spacing <= 1000, 'INVALID_REQUEST', 'lineSpacingPercent must be 1..1000')
                line = one([n for n in wanted.children if n.tag == q(HH, 'lineSpacing')], 'missing lineSpacing')
                line.set_attr('type', 'PERCENT')
                line.set_attr('value', str(spacing))
            intent.update(alignment=alignment, lineSpacingPercent=spacing)
            changed = semantic(old, True) != semantic(wanted, True)
            if changed:
                actual_para = candidate.xml[entry].root.at(path)
                ref = actual_para.attrs.get('paraPrIDRef')
                matches = [n for n in styles if semantic(n, True) == semantic(wanted, True)]
                if matches:
                    need(ref in {n.attrs['id'] for n in matches}, 'FORMAT_MISMATCH', 'must reuse equivalent paragraph style')
                else:
                    need(isinstance(ref, str) and ref.isdecimal() and ref not in {n.attrs['id'] for n in styles},
                         'FORMAT_MISMATCH', 'must allocate new paragraph style')
                    wanted.set_attr('id', ref)
                    styles_parent.append(wanted)
                    need(styles_parent.attrs.get('itemCnt') == str(len(styles) - 1), 'UNSUPPORTED', 'bad original style count')
                    styles_parent.set_attr('itemCnt', str(len(styles)))
                    new_styles.add(ref)
                para.set_attr('paraPrIDRef', ref)
        else:
            need(hp(target, 'pic'), 'INVALID_TARGET', 'image target must be picture')
            need(not any(n.tag.rsplit('}', 1)[-1] in ('imgClip', 'effects', 'imgEffect', 'effect') for n in target.walk()),
                 'UNSUPPORTED', 'image clipping/effects unsupported')
            media = one([n for n in target.walk() if n.tag == q(HC, 'img')], 'expected one image reference')
            need('binaryItemIDRef' in media.attrs, 'UNSUPPORTED', 'missing image reference')
            image = png_bytes(op['image'])
            intent['imageSha256'] = sha(image)
            old_entry = original.items[media.attrs['binaryItemIDRef']][0]
            changed = original.payload[old_entry] != image
            if changed:
                actual_pic = candidate.xml[entry].root.at(path)
                actual_media = one([n for n in actual_pic.walk() if n.tag == q(HC, 'img')], 'expected candidate image')
                ref = actual_media.attrs.get('binaryItemIDRef')
                need(ref in candidate.items and ref not in expected.items, 'IMAGE_MISMATCH', 'replacement needs fresh media id')
                media_entry, manifest_item = candidate.items[ref]
                need(media_entry not in original.payload and media_entry not in new_images and media_entry.startswith('BinData/'),
                     'IMAGE_MISMATCH', 'replacement must append fresh media entry')
                need(candidate.payload[media_entry] == image, 'IMAGE_MISMATCH', 'replacement bytes differ from request')
                attrs = {'id': ref, 'href': manifest_item.attrs.get('href'), 'media-type': 'image/png'}
                if 'isEmbeded' in manifest_item.attrs:
                    attrs['isEmbeded'] = '1'
                need(manifest_item.attrs == attrs and not manifest_item.children and not manifest_item.text,
                     'IMAGE_MISMATCH', 'unexpected new manifest attributes/content')
                node = Node(expected.manifest.tag.rsplit('}', 1)[0] + '}item', attrs, gaps=[b''], generated=True)
                expected.manifest.append(node)
                expected.items[ref] = (media_entry, node)
                expected.names.append(media_entry)
                expected.payload[media_entry] = image
                new_images.add(media_entry)
                media.set_attr('binaryItemIDRef', ref)
        if changed:
            intents.append(intent)
            changes.append({'op': kind, 'entry': entry, 'path': path})
            if kind != 'image.replace':
                need(path[0] < len(root.children) and hp(root.children[path[0]], 'p'),
                     'UNSUPPORTED', 'target must descend from top-level paragraph')
                anchor = anchors.setdefault(entry, {'first': path[0], 'nested': []})
                anchor['first'] = min(anchor['first'], path[0])
                if para is not root.children[path[0]]:
                    anchor['nested'].extend(paras if kind == 'cell.setText' else [para])
    cache_count = 0
    for entry, anchor in anchors.items():
        root = expected.xml[entry].root
        for para in root.children[anchor['first']:] + anchor['nested']:
            if not hp(para, 'p'):
                continue
            for i in reversed(range(len(para.children))):
                if hp(para.children[i], 'linesegarray'):
                    need(i == len(para.children) - 1, 'UNSUPPORTED', 'cache must be final paragraph child')
                    para.remove(i)
                    cache_count += 1
    update_preview(original, expected, changes)
    if 'intents' in request:
        need(request['intents'] == intents, 'INTENT_MISMATCH', 'reported Intent does not equal independently derived applied intents')
    return expected, {'changes': changes, 'expectedIntents': intents, 'cacheRemovals': cache_count,
                      'addedMedia': sorted(new_images), 'addedStyles': sorted(new_styles)}


def check(original, candidate, request):
    """Return JSON-serializable report; never import or invoke the engine.

    original/candidate: bytes, str path, or pathlib.Path. request: schemaVersion=1 object.
    Status is pass/fail/unsupported/error; see README for the trust boundary.
    """
    phase = 'original'
    try:
        before = Package(original)
        phase = 'candidate'
        after = Package(candidate)
        phase = 'expectation'
        wanted, evidence = expected_state(before, after, request)
        phase = 'comparison'
        need(set(wanted.names) <= set(after.names), 'MISSING_ENTRY', 'original or expected entry missing')
        need(set(wanted.names) == set(after.names), 'UNEXPECTED_ENTRY', 'unexpected added ZIP entry')
        need(before.names == [n for n in after.names if n in before.payload]
             and after.names[:len(before.names)] == before.names, 'ENTRY_ORDER_CHANGED', 'original ZIP order / append-only media')
        for name in before.names:
            need(before.methods[name] == after.methods[name], 'ZIP_METHOD_CHANGED', name)
            if name in before.xml:
                a, b = wanted.xml[name], after.xml[name]
                need(a.before == b.before and a.after == b.after, 'LEXICAL_CHANGED', name)
                compare_node(a.root, b.root, name)
            else:
                need(wanted.payload[name] == after.payload[name], 'PAYLOAD_CHANGED', name)
            if before.payload[name] == after.payload[name]:
                need(before.compressed[name] == after.compressed[name], 'RECOMPRESSED_ENTRY', name)
        if not evidence['changes']:
            need(before.raw == after.raw, 'NOOP_BYTES_CHANGED', 'no-op must preserve entire original file')
        return {'schemaVersion': 1, 'ok': True, 'status': 'pass', 'diagnostics': [], **evidence}
    except OracleError as exc:
        status = ('unsupported' if exc.code.startswith('UNSUPPORTED') else
                  'error' if phase == 'original' or exc.code in ('INVALID_REQUEST', 'INVALID_TARGET', 'INVALID_RANGE') else 'fail')
        return {'schemaVersion': 1, 'ok': False, 'status': status,
                'diagnostics': [{'code': exc.code, 'phase': phase, 'message': exc.message}]}
    except (OSError, ValueError, TypeError, KeyError, IndexError, OverflowError) as exc:
        return {'schemaVersion': 1, 'ok': False, 'status': 'error',
                'diagnostics': [{'code': 'INPUT_ERROR', 'phase': phase, 'message': str(exc)}]}


def read_json(path):
    def pairs(items):
        result = {}
        for k, v in items:
            need(k not in result, 'INVALID_REQUEST', 'duplicate JSON key')
            result[k] = v
        return result
    need(Path(path).stat().st_size <= 1024 * 1024, 'INVALID_REQUEST', 'JSON request exceeds 1 MiB')
    return json.loads(Path(path).read_text(encoding='utf-8'), object_pairs_hook=pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(OracleError('INVALID_REQUEST', 'non-finite JSON number')))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('original')
    parser.add_argument('candidate')
    parser.add_argument('request', help='JSON declarative operations; never engine-generated expectations alone')
    args = parser.parse_args(argv)
    try:
        report = check(args.original, args.candidate, read_json(args.request))
    except (OracleError, OSError, ValueError) as exc:
        report = {'schemaVersion': 1, 'ok': False, 'status': 'error', 'diagnostics': [
            {'code': getattr(exc, 'code', 'INPUT_ERROR'), 'phase': 'request', 'message': str(exc)}]}
    print(json.dumps(report, ensure_ascii=False, sort_keys=True))
    return 0 if report['ok'] else 1 if report['status'] == 'fail' else 2


if __name__ == '__main__':
    sys.exit(main())
