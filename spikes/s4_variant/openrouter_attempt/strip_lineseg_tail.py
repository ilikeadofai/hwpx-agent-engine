#!/usr/bin/env python3
import sys
import os
import re
import json
import zipfile

TOKEN_RE = re.compile(rb'<\?.*?\?>|<!--.*?-->|<(/?)([A-Za-z0-9_:.-]+)[^>]*?(/?)>', re.S)
SECTION_RE = re.compile(r'^Contents/section\d+\.xml$')


def error(msg):
    print(json.dumps({"error": msg}, ensure_ascii=False))
    sys.exit(2)


def parse_section(data):
    depth = 0
    stack = []
    paragraphs = []
    linesegs = []

    for m in TOKEN_RE.finditer(data):
        name = m.group(2)
        if name is None:
            continue

        is_close = m.group(1) == b'/'
        is_self = m.group(3) == b'/'

        if is_close:
            depth -= 1
            if not stack:
                raise ValueError("unbalanced close tag")
            elem = stack.pop()
            if elem['depth'] != depth:
                raise ValueError("depth mismatch")

            if elem['name'] == b'hp:p' and elem['depth'] == 1:
                paragraphs[elem['para_index']]['end'] = m.end()
            elif elem['name'] == b'hp:linesegarray' and elem['depth'] == 2:
                parent = stack[-1] if stack else None
                if parent and parent['name'] == b'hp:p' and parent['depth'] == 1:
                    linesegs.append({
                        'start': elem['start'],
                        'end': m.end(),
                        'para_index': parent['para_index']
                    })
        else:
            if is_self:
                if name == b'hp:p' and depth == 1:
                    para_index = len(paragraphs)
                    paragraphs.append({
                        'index': para_index,
                        'start': m.start(),
                        'end': m.end()
                    })
                elif name == b'hp:linesegarray' and depth == 2:
                    parent = stack[-1] if stack else None
                    if parent and parent['name'] == b'hp:p' and parent['depth'] == 1:
                        linesegs.append({
                            'start': m.start(),
                            'end': m.end(),
                            'para_index': parent['para_index']
                        })
            else:
                elem_depth = depth
                if name == b'hp:p' and elem_depth == 1:
                    para_index = len(paragraphs)
                    paragraphs.append({
                        'index': para_index,
                        'start': m.start(),
                        'end': None
                    })
                    entry = {
                        'name': name,
                        'depth': elem_depth,
                        'start': m.start(),
                        'para_index': para_index
                    }
                else:
                    entry = {
                        'name': name,
                        'depth': elem_depth,
                        'start': m.start()
                    }
                stack.append(entry)
                depth += 1

    for p in paragraphs:
        if p['end'] is None:
            raise ValueError("unclosed paragraph")

    return paragraphs, linesegs


def main():
    if len(sys.argv) != 4:
        error("usage: python3 strip_lineseg_tail.py SRC.hwpx DST.hwpx MARKER")

    src, dst, marker_str = sys.argv[1], sys.argv[2], sys.argv[3]
    marker_bytes = marker_str.encode('utf-8')

    try:
        if os.path.exists(dst):
            error(f"destination already exists: {dst}")

        with zipfile.ZipFile(src, 'r') as zf:
            infos = zf.infolist()
            if not infos:
                error("empty zip")

            first = infos[0]
            if first.filename != 'mimetype' or first.compress_type != zipfile.ZIP_STORED:
                error("first entry must be 'mimetype' stored")

            matches = []
            for info in infos:
                if not SECTION_RE.match(info.filename):
                    continue

                with zf.open(info) as f:
                    data = f.read()

                try:
                    paragraphs, linesegs = parse_section(data)
                except Exception as e:
                    error(f"failed to parse {info.filename}: {e}")

                for p in paragraphs:
                    span = data[p['start']:p['end']]
                    if marker_bytes in span:
                        matches.append({
                            'section': info.filename,
                            'k': p['index'],
                            'data': data,
                            'paragraphs': len(paragraphs),
                            'linesegs': linesegs
                        })

            if len(matches) != 1:
                error(f"expected exactly one matching paragraph, found {len(matches)}")

            match = matches[0]
            k = match['k']
            data = match['data']
            linesegs = match['linesegs']

            to_remove = [ls for ls in linesegs if ls['para_index'] >= k]
            to_remove.sort(key=lambda x: x['start'])

            new_parts = []
            prev = 0
            for ls in to_remove:
                new_parts.append(data[prev:ls['start']])
                prev = ls['end']
            new_parts.append(data[prev:])
            new_data = b''.join(new_parts)

            removed_count = len(to_remove)
            paragraphs_in_section = match['paragraphs']
            edited_section = match['section']

            with zipfile.ZipFile(dst, 'w') as out:
                for info in infos:
                    if info.filename == edited_section:
                        out.writestr(info, new_data, compress_type=info.compress_type)
                    else:
                        with zf.open(info) as f:
                            orig_data = f.read()
                        out.writestr(info, orig_data, compress_type=info.compress_type)

        print(json.dumps({
            "section": edited_section,
            "paragraphIndex": k,
            "removed": removed_count,
            "paragraphsInSection": paragraphs_in_section
        }, ensure_ascii=False))
        sys.exit(0)

    except Exception as e:
        error(str(e))


if __name__ == '__main__':
    main()
