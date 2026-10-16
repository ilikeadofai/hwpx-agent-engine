#!/usr/bin/env python3
"""Remove trailing line-layout cache (hp:linesegarray) from one paragraph.

Usage: python3 strip_lineseg_tail.py SRC.hwpx DST.hwpx MARKER

Standard library only. The section XML is spliced at the byte level; it is
never parsed and re-serialized.
"""

import io
import json
import os
import re
import sys
import zipfile

TOKEN_RE = re.compile(
    rb'<\?.*?\?>|<!--.*?-->|<(/?)([A-Za-z0-9_:.-]+)[^>]*?(/?)>', re.S
)
SECTION_RE = re.compile(r'^Contents/section\d+\.xml$')


def fail(message):
    sys.stdout.write(json.dumps({"error": message}, ensure_ascii=False) + "\n")
    sys.exit(2)


def enclosing_paragraph(stack):
    """Innermost enclosing top-level hp:p record on the open-element stack."""
    for name, _start, _depth, rec in reversed(stack):
        if name == b'hp:p':
            return rec
    return None


def scan_section(data):
    """Byte-scan one section XML.

    Returns (paragraphs, linesegs):
      paragraphs: top-level hp:p records {"index", "start", "end"} in order.
      linesegs:   depth-2 hp:linesegarray records {"para", "start", "end"},
                  where "para" is the enclosing top-level paragraph record.
    """
    depth = 0
    stack = []  # (name, open_tag_start, open_depth, record_or_None)
    paragraphs = []
    linesegs = []

    for match in TOKEN_RE.finditer(data):
        name = match.group(2)
        if name is None:  # processing instruction or comment
            continue
        closing = match.group(1) == b'/'
        self_closing = match.group(3) == b'/'

        if closing:
            depth -= 1
            if stack and stack[-1][0] == name and stack[-1][2] == depth:
                _n, _s, _d, rec = stack.pop()
                if rec is not None:
                    rec["end"] = match.end()
                    if name == b'hp:p':
                        paragraphs.append(rec)
                    else:
                        linesegs.append(rec)
            continue

        if self_closing:
            if name == b'hp:p' and depth == 1:
                paragraphs.append(
                    {"index": len(paragraphs), "start": match.start(), "end": match.end()}
                )
            elif name == b'hp:linesegarray' and depth == 2:
                linesegs.append(
                    {
                        "para": enclosing_paragraph(stack),
                        "start": match.start(),
                        "end": match.end(),
                    }
                )
            continue

        rec = None
        if name == b'hp:p' and depth == 1:
            rec = {"index": len(paragraphs), "start": match.start(), "end": None}
        elif name == b'hp:linesegarray' and depth == 2:
            rec = {"para": enclosing_paragraph(stack), "start": match.start(), "end": None}
        stack.append((name, match.start(), depth, rec))
        depth += 1

    return paragraphs, linesegs


def main(argv):
    if len(argv) != 4:
        fail("usage: strip_lineseg_tail.py SRC.hwpx DST.hwpx MARKER")
    src, dst, marker_text = argv[1], argv[2], argv[3]

    if os.path.exists(dst):
        fail("destination already exists: " + dst)

    marker = marker_text.encode('utf-8')

    try:
        with open(src, 'rb') as handle:
            src_bytes = handle.read()
    except OSError as exc:
        fail("cannot read source: %s" % exc)

    entries = []      # (ZipInfo, uncompressed bytes) in source order
    section_idxs = []  # positions in entries that are section XML
    try:
        with zipfile.ZipFile(io.BytesIO(src_bytes)) as archive:
            infos = archive.infolist()
            if not infos:
                fail("source archive has no entries")
            first = infos[0]
            if first.filename != "mimetype":
                fail("first entry is not 'mimetype': %r" % first.filename)
            if first.compress_type != zipfile.ZIP_STORED:
                fail("first entry 'mimetype' is not stored")
            for info in infos:
                try:
                    data = archive.read(info)
                except Exception as exc:  # bad CRC, unsupported compression, ...
                    fail("cannot read entry %r: %s" % (info.filename, exc))
                if SECTION_RE.match(info.filename):
                    section_idxs.append(len(entries))
                entries.append((info, data))
    except zipfile.BadZipFile as exc:
        fail("not a valid zip file: %s" % exc)

    matches = []
    for idx in section_idxs:
        data = entries[idx][1]
        paragraphs, linesegs = scan_section(data)
        for para in paragraphs:
            if marker in data[para["start"]:para["end"]]:
                matches.append((idx, paragraphs, linesegs, para))

    if len(matches) != 1:
        fail("expected exactly one matching paragraph, found %d" % len(matches))
    idx, paragraphs, linesegs, para = matches[0]
    k = para["index"]
    section_name = entries[idx][0].filename
    section_bytes = entries[idx][1]

    spans = [
        (ls["start"], ls["end"])
        for ls in linesegs
        if ls["para"] is not None and ls["para"]["index"] >= k
    ]
    spans.sort()

    out = bytearray()
    pos = 0
    for start, end in spans:
        out += section_bytes[pos:start]
        pos = end
    out += section_bytes[pos:]
    new_section = bytes(out)

    try:
        with zipfile.ZipFile(dst, 'w', zipfile.ZIP_DEFLATED) as archive:
            for i, (info, data) in enumerate(entries):
                payload = new_section if i == idx else data
                archive.writestr(info, payload, compress_type=info.compress_type)
    except OSError as exc:
        fail("cannot write destination: %s" % exc)

    sys.stdout.write(
        json.dumps(
            {
                "section": section_name,
                "paragraphIndex": k,
                "removed": len(spans),
                "paragraphsInSection": len(paragraphs),
            },
            ensure_ascii=False,
        )
        + "\n"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
