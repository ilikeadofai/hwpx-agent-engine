#!/usr/bin/env python3
"""Deterministic one-defect candidates for cross-checking another verifier.

Does not import the oracle or Rust engine. Run with an output directory.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys

# Support both unittest discovery and python -I execution.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from fixtures import (CACHE, HEADER, HPF, NEW_PNG, OLD_PNG, P0, P4, SECTION,
                      STYLE0, archive, case, unpack)


# name, valid scenario, defect category, expected oracle diagnostic
MUTATIONS = (
    ('out_of_target_text', 'text', 'text', 'TEXT_CHANGED'),
    ('wrong_target_text', 'text', 'text', 'TEXT_CHANGED'),
    ('wrong_run_distribution', 'cross_run', 'text', 'TEXT_CHANGED'),
    ('non_target_format', 'text', 'format', 'ATTRIBUTE_CHANGED'),
    ('shared_style_mutation', 'format_clone', 'format', 'ATTRIBUTE_CHANGED'),
    ('clone_opaque_attribute', 'format_clone', 'opaque', 'ATTRIBUTE_CHANGED'),
    ('clone_opaque_lexical', 'format_clone', 'opaque', 'LEXICAL_CHANGED'),
    ('unrequested_style_value', 'format_clone', 'format', 'ATTRIBUTE_CHANGED'),
    ('shared_media_payload', 'image', 'media', 'PAYLOAD_CHANGED'),
    ('shared_picture_retarget', 'image', 'media', 'ATTRIBUTE_CHANGED'),
    ('wrong_image_payload', 'image', 'media', 'IMAGE_MISMATCH'),
    ('picture_geometry', 'image', 'format', 'ATTRIBUTE_CHANGED'),
    ('opaque_attribute', 'text', 'opaque', 'ATTRIBUTE_CHANGED'),
    ('opaque_lexical', 'text', 'opaque', 'LEXICAL_CHANGED'),
    ('opaque_payload', 'text', 'opaque', 'PAYLOAD_CHANGED'),
    ('missing_entry', 'text', 'entries', 'MISSING_ENTRY'),
    ('missing_media_entry', 'image', 'entries', 'INVALID_REFERENCE'),
    ('invalid_media_reference', 'image', 'references', 'INVALID_MEDIA_REFERENCE'),
    ('invalid_manifest_href', 'image', 'references', 'INVALID_REFERENCE'),
    ('spine_reorder', 'text', 'references', 'ATTRIBUTE_CHANGED'),
    ('extra_entry', 'text', 'entries', 'UNEXPECTED_ENTRY'),
    ('extra_manifest_attribute', 'image', 'opaque', 'IMAGE_MISMATCH'),
    ('cache_retained', 'text', 'cache', 'STRUCTURE_CHANGED'),
    ('cache_removed_before_anchor', 'text', 'cache', 'STRUCTURE_CHANGED'),
    ('nested_cache_removed', 'text', 'cache', 'STRUCTURE_CHANGED'),
    ('image_cache_removed', 'image', 'cache', 'STRUCTURE_CHANGED'),
    ('other_section_text', 'text', 'text', 'TEXT_CHANGED'),
    ('stale_preview', 'text', 'preview', 'PAYLOAD_CHANGED'),
    ('thumbnail_mutation', 'text', 'preview', 'PAYLOAD_CHANGED'),
    ('zip_order', 'text', 'zip', 'ENTRY_ORDER_CHANGED'),
)


def mutate(name):
    row = next(row for row in MUTATIONS if row[0] == name)
    original, valid, req = case(row[1])
    payload = unpack(valid)
    if name == 'out_of_target_text':
        payload[SECTION] = payload[SECTION].replace(b'KEEP', b'CORRUPT')
    elif name == 'wrong_target_text':
        payload[SECTION] = payload[SECTION].replace(b'WORLD', b'WRONG')
    elif name == 'wrong_run_distribution':
        payload[SECTION] = payload[SECTION].replace(b'AlpX', b'Alp').replace(b'>ta<', b'>Xta<')
    elif name == 'non_target_format':
        payload[SECTION] = payload[SECTION].replace(b'charPrIDRef="0"', b'charPrIDRef="99"', 1)
    elif name == 'shared_style_mutation':
        payload[HEADER] = payload[HEADER].replace(b'horizontal="LEFT"', b'horizontal="CENTER"', 1)
    elif name in ('clone_opaque_attribute', 'clone_opaque_lexical', 'unrequested_style_value'):
        head, clone = payload[HEADER].split(b'<hh:paraPr id="2"', 1)
        clone = clone.replace(b'x:opaque="&#65;"', b'x:opaque="B"') if name == 'clone_opaque_attribute' else clone.replace(b'flag=\'&#65;\'', b'flag=\'A\'') if name == 'clone_opaque_lexical' else clone.replace(b'vertical="BASELINE"', b'vertical="TOP"')
        payload[HEADER] = head + b'<hh:paraPr id="2"' + clone
    elif name == 'shared_media_payload':
        payload['BinData/old.png'] = NEW_PNG
    elif name == 'shared_picture_retarget':
        payload[SECTION] = payload[SECTION].replace(b'binaryItemIDRef="old-image"', b'binaryItemIDRef="new-image"')
    elif name == 'wrong_image_payload':
        payload['BinData/new.png'] = OLD_PNG
    elif name == 'picture_geometry':
        payload[SECTION] = payload[SECTION].replace(b'width="100"', b'width="101"', 1)
    elif name == 'opaque_attribute':
        payload[SECTION] = payload[SECTION].replace(b'x:keep="&#65;"', b'x:keep="B"')
    elif name == 'opaque_lexical':
        payload[SECTION] = payload[SECTION].replace(b'attr=\'&#65;\'', b'attr=\'A\'')
    elif name == 'opaque_payload':
        payload['Opaque/data.bin'] = b'corrupt'
    elif name == 'missing_entry':
        del payload['Opaque/data.bin']
    elif name == 'missing_media_entry':
        del payload['BinData/new.png']
    elif name == 'invalid_media_reference':
        payload[SECTION] = payload[SECTION].replace(b'binaryItemIDRef="new-image"', b'binaryItemIDRef="missing"')
    elif name == 'invalid_manifest_href':
        payload[HPF] = payload[HPF].replace(b'href="BinData/new.png"', b'href="BinData/missing.png"')
    elif name == 'spine_reorder':
        payload[HPF] = payload[HPF].replace(b'<opf:itemref idref="s0"/><opf:itemref idref="s1"/>', b'<opf:itemref idref="s1"/><opf:itemref idref="s0"/>')
    elif name == 'extra_entry':
        payload['Opaque/extra.bin'] = b'extra'
    elif name == 'extra_manifest_attribute':
        payload[HPF] = payload[HPF].replace(b'id="new-image"', b'id="new-image" surprise="1"')
    elif name == 'cache_retained':
        payload[SECTION] = payload[SECTION].replace(b'<hp:t>WORLD</hp:t></hp:run></hp:p>', b'<hp:t>WORLD</hp:t></hp:run>' + CACHE.encode() + b'</hp:p>')
    elif name == 'cache_removed_before_anchor':
        payload[SECTION] = payload[SECTION].replace(CACHE.encode(), b'', 1)
    elif name == 'nested_cache_removed':
        payload[SECTION] = payload[SECTION].replace(CACHE.encode(), b'')
        # Restore preceding top-level cache so this isolates nested over-invalidation.
        payload[SECTION] = payload[SECTION].replace(P0.replace(CACHE, '').encode(), P0.encode())
    elif name == 'image_cache_removed':
        payload[SECTION] = payload[SECTION].replace(CACHE.encode(), b'', 1)
    elif name == 'other_section_text':
        payload['Contents/section1.xml'] = payload['Contents/section1.xml'].replace(b'OTHER', b'WRONG')
    elif name == 'stale_preview':
        payload['Preview/PrvText.txt'] = unpack(original)['Preview/PrvText.txt']
    elif name == 'thumbnail_mutation':
        payload['Preview/PrvImage.png'] = NEW_PNG
    elif name == 'zip_order':
        value = payload.pop('Opaque/data.bin')
        pairs = list(payload.items())
        pairs.insert(1, ('Opaque/data.bin', value))
        payload = dict(pairs)
    return original, archive(payload), req, row


def generate(directory):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    manifest = {'schemaVersion': 1, 'provenance': 'hand-authored synthetic; no external document content', 'cases': []}
    for name, _, _, _ in MUTATIONS:
        original, candidate, request, row = mutate(name)
        paths = {}
        for suffix, data in (('original.hwpx', original), ('candidate.hwpx', candidate),
                             ('request.json', (json.dumps(request, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode())):
            path = directory / (name + '.' + suffix)
            path.write_bytes(data)
            paths[suffix] = {'file': path.name, 'sha256': hashlib.sha256(data).hexdigest()}
        manifest['cases'].append({'name': name, 'category': row[2], 'expect': 'reject', 'oracleCode': row[3], 'files': paths})
    (directory / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True, indent=2) + '\n', encoding='utf-8')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output')
    args = parser.parse_args()
    manifest = generate(args.output)
    print(json.dumps({'generated': len(manifest['cases']), 'manifest': str(Path(args.output) / 'manifest.json')}, sort_keys=True))


if __name__ == '__main__':
    main()
