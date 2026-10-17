"""Synthetic oracle tests, independently authored expected candidates and mutants."""
import base64
import copy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from oracle import Package, check, read_json
from fixtures import (CASES, CACHE, CELL_P_PATH, HEADER, HPF, NEW_PNG, OLD_PNG,
                      P1, SECTION, STYLE0, archive, case, entries, request, unpack)
from mutations import MUTATIONS, generate, mutate


class OracleTests(unittest.TestCase):
    def assert_pass(self, o, c, r):
        result = check(o, c, r)
        self.assertEqual(result['status'], 'pass', result)
        self.assertTrue(result['ok'])
        return result

    def assert_reject(self, o, c, r, code=None, status=None):
        result = check(o, c, r)
        self.assertFalse(result['ok'], result)
        if code:
            self.assertEqual(result['diagnostics'][0]['code'], code, result)
        if status:
            self.assertEqual(result['status'], status, result)
        return result

    def test_request_not_candidate_is_source_of_expectation(self):
        o, c, r = case('text')
        self.assert_reject(o, o, r, 'STRUCTURE_CHANGED')
        r['operations'][0]['text'] = 'WRONG'
        self.assert_reject(o, c, r, 'TEXT_CHANGED')

    def test_spoofed_engine_intent_cannot_authorize_corruption(self):
        o, c, r = case('text')
        r['intents'] = [{'op': 'text.replace', 'entry': SECTION, 'path': [1], 'expectedText': 'WRONG'}]
        self.assert_reject(o, c, r, 'INTENT_MISMATCH')

    def test_explicit_expected_intent(self):
        o, c, r = case('cross_run')
        r['intents'] = [{'op': 'text.replace', 'entry': SECTION, 'path': [1], 'expectedText': 'AlpXta'}]
        self.assert_pass(o, c, r)

    def test_invalid_intent_only_request(self):
        o, c, _ = case('text')
        self.assert_reject(o, c, {'schemaVersion': 1, 'intents': []}, 'INVALID_REQUEST', 'error')

    def test_report_cache_boundaries(self):
        report = self.assert_pass(*case('text'))
        self.assertEqual(report['cacheRemovals'], 4)
        report = self.assert_pass(*case('image'))
        self.assertEqual(report['cacheRemovals'], 0)

    def test_precomposed_korean_range(self):
        o, c, r = case('text')
        a, b = unpack(o), unpack(c)
        for p in (a, b):
            p[SECTION] = p[SECTION].replace(b'Alpha ', '한글 문장 '.encode())
            p['Preview/PrvText.txt'] = p['Preview/PrvText.txt'].replace(b'Alpha ', '한글 문장 '.encode())
        self.assert_pass(archive(a), archive(b), r)

    def test_complex_unicode_explicitly_unsupported(self):
        for value in ('e\u0301', '\u1100\u1161', '👩\u200d💻', '🇰🇷', 'a\ufe0f'):
            with self.subTest(value=value):
                o, c, r = case('text')
                a = unpack(o)
                a[SECTION] = a[SECTION].replace(b'Alpha ', value.encode())
                self.assert_reject(archive(a), c, r, 'UNSUPPORTED_GRAPHEME', 'unsupported')

    def test_new_text_unicode_is_not_normalized(self):
        o, c, r = case('text')
        text = 'e\u0301'
        r['operations'][0]['text'] = text
        b = unpack(c)
        b[SECTION] = b[SECTION].replace(b'WORLD', text.encode())
        b['Preview/PrvText.txt'] = b['Preview/PrvText.txt'].replace(b'WORLD', text.encode())
        self.assert_pass(o, archive(b), r)

    def test_bad_ranges(self):
        for start, end in ((-1, 2), (1, 20), (True, 3), (0, 1.5)):
            with self.subTest(start=start, end=end):
                o, c, r = case('text')
                r['operations'][0]['range'] = {'start': start, 'end': end}
                self.assert_reject(o, c, r, 'INVALID_RANGE', 'error')

    def test_bad_replacement_characters(self):
        for text in ('tab\t', 'line\n', '\r', '\x00', '\ud800', '\ufffe'):
            with self.subTest(text=repr(text)):
                o, c, r = case('text')
                r['operations'][0]['text'] = text
                self.assert_reject(o, c, r, 'INVALID_REQUEST', 'error')

    def test_unknown_fields_and_types(self):
        o, c, r = case('text')
        for value in ({**r, 'surprise': 1}, {**r, 'schemaVersion': True}, {**r, 'operations': None}):
            with self.subTest(value=value):
                self.assert_reject(o, c, value, 'INVALID_REQUEST', 'error')
        r['operations'][0]['unexpected'] = 1
        self.assert_reject(o, c, r, 'INVALID_REQUEST', 'error')

    def test_target_validation(self):
        for path in ([100], [-1], [True], [], ['1']):
            o, c, r = case('text')
            r['operations'][0]['path'] = path
            self.assert_reject(o, c, r, 'INVALID_TARGET')

    def test_noop_operations_preserve_bytes(self):
        o, _, r = case('text')
        r['operations'][0]['text'] = 'beta'
        self.assert_pass(o, o, r)
        _, _, r = case('format_reuse')
        r['operations'][0]['alignment'] = 'left'
        self.assert_pass(o, o, r)
        _, _, r = case('image')
        r['operations'][0]['image']['base64'] = base64.b64encode(OLD_PNG).decode()
        self.assert_pass(o, o, r)

    def test_noop_whole_zip_bytes_required(self):
        o, _, r = case('noop')
        b = io.BytesIO(o)
        with zipfile.ZipFile(b, 'a') as z:
            z.comment = b'modified'
        self.assert_reject(o, b.getvalue(), r, 'NOOP_BYTES_CHANGED')

    def test_multiple_writes_same_target_explicitly_unsupported(self):
        o, c, r = case('text')
        r['operations'].append(copy.deepcopy(r['operations'][0]))
        self.assert_reject(o, c, r, 'UNSUPPORTED', 'unsupported')

    def test_independent_batch(self):
        o, text, req = case('text')
        _, image, image_req = case('image')
        b, im = unpack(text), unpack(image)
        b[SECTION] = b[SECTION].replace(b'binaryItemIDRef="old-image"', b'binaryItemIDRef="new-image"', 1)
        b[HPF] = im[HPF]
        b['BinData/new.png'] = NEW_PNG
        req['operations'] += image_req['operations']
        self.assertEqual(len(self.assert_pass(o, archive(b), req)['changes']), 2)

    def test_text_target_inside_cell(self):
        o, c, _ = case('cell')
        req = request('text.replace', path=CELL_P_PATH, range={'start': 0, 'end': 4}, text='NEW')
        self.assert_pass(o, c, req)

    def test_empty_self_closing_cell_text(self):
        o, c, req = case('empty_cell')
        a = unpack(o)
        a[SECTION] = a[SECTION].replace(b'<hp:t></hp:t>', b'<hp:t/>')
        self.assert_pass(archive(a), c, req)

    def test_merged_cell_is_supported_and_geometry_preserved(self):
        o, c, req = case('cell')
        a = unpack(o)
        a[SECTION] = a[SECTION].replace(b'colSpan="1"', b'colSpan="2"', 1)
        b=unpack(c)
        b[SECTION]=b[SECTION].replace(b'colSpan="1"',b'colSpan="2"',1)
        self.assert_pass(archive(a),archive(b),req)

    def test_multiple_cell_paragraphs_unsupported(self):
        o, c, req = case('cell')
        a = unpack(o)
        a[SECTION] = a[SECTION].replace(b'</hp:subList>', b'<hp:p id="extra" paraPrIDRef="0"/></hp:subList>', 1)
        self.assert_reject(archive(a), c, req, 'UNSUPPORTED')

    def test_empty_cell_without_run_unsupported(self):
        o, c, req = case('empty_cell')
        a = unpack(o)
        a[SECTION] = a[SECTION].replace(b'<hp:run charPrIDRef="0"><hp:t></hp:t></hp:run>', b'')
        self.assert_reject(archive(a), c, req, 'UNSUPPORTED')

    def test_nested_table_unsupported(self):
        o, c, req = case('cell')
        a = unpack(o)
        a[SECTION] = a[SECTION].replace(b'</hp:tc>', b'<hp:tbl/></hp:tc>', 1)
        self.assert_reject(archive(a), c, req, 'UNSUPPORTED')

    def test_controls_and_opaque_text_attributes_unsupported(self):
        for old, new in ((b'<hp:t>beta', b'<hp:t><hp:tab/>beta'),
                         (b'<hp:t>beta', b'<hp:t x:opaque="a">beta'),
                         (b'charPrIDRef="1"', b'charPrIDRef="1" x:opaque="a"')):
            o, c, r = case('text')
            a = unpack(o)
            a[SECTION] = a[SECTION].replace(old, new)
            self.assert_reject(archive(a), c, r, 'UNSUPPORTED')

    def test_namespace_spoof_unsupported(self):
        o, c, r = case('text')
        a = unpack(o)
        a[SECTION] = a[SECTION].replace(b'<hp:t>beta</hp:t>', b'<hp:t xmlns:hp="urn:spoof">beta</hp:t>')
        self.assert_reject(archive(a), c, r, 'UNSUPPORTED')

    def test_utf16_preview_remains_stale(self):
        o, c, r = case('text')
        a, b = unpack(o), unpack(c)
        for payload in (a, b):
            payload['Preview/PrvText.txt'] = b'\xff\xfe' + payload['Preview/PrvText.txt'].decode().replace('\n', '\r\n').encode('utf-16-le')
        b['Preview/PrvText.txt'] = a['Preview/PrvText.txt']
        self.assert_pass(archive(a), archive(b), r)

    def test_ambiguous_preview_remains_stale(self):
        o, c, r = case('text')
        a = unpack(o)
        a['Preview/PrvText.txt'] = b'Alpha beta appears twice Alpha beta'
        b = unpack(c)
        b['Preview/PrvText.txt'] = a['Preview/PrvText.txt']
        self.assert_pass(archive(a), archive(b), r)
        self.assert_reject(archive(a), c, r, 'PAYLOAD_CHANGED')

    def test_missing_preview_is_not_synthesized(self):
        o, c, r = case('text')
        a, b = unpack(o), unpack(c)
        del a['Preview/PrvText.txt']
        del b['Preview/PrvText.txt']
        self.assert_pass(archive(a), archive(b), r)

    def test_format_clone_id_is_not_predetermined(self):
        o, c, r = case('format_clone')
        b = unpack(c)
        b[HEADER] = b[HEADER].replace(b'id="2"', b'id="42"')
        b[SECTION] = b[SECTION].replace(b'paraPrIDRef="2"', b'paraPrIDRef="42"')
        self.assert_pass(o, archive(b), r)

    def test_format_requires_reuse(self):
        o, c, r = case('format_clone')
        r['operations'][0] = {'op': 'paragraph.format', 'entry': SECTION, 'path': [1], 'alignment': 'right'}
        b = unpack(c)
        b[HEADER] = b[HEADER].replace(b'horizontal="CENTER"', b'horizontal="RIGHT"').replace(b'value="180"', b'value="160"')
        self.assert_reject(o, archive(b), r, 'FORMAT_MISMATCH')

    def test_format_unknown_option_rejected(self):
        o, c, r = case('format_clone')
        r['operations'][0]['alignment'] = 'middle'
        self.assert_reject(o, c, r, 'INVALID_REQUEST')

    def test_invalid_format_spacing(self):
        for value in (0, -1, True, 1001, '160'):
            o, c, r = case('format_clone')
            r['operations'][0]['lineSpacingPercent'] = value
            self.assert_reject(o, c, r, 'INVALID_REQUEST')

    def test_image_generated_name_is_not_predetermined(self):
        o, c, r = case('image')
        b = unpack(c)
        for n in (SECTION, HPF):
            b[n] = b[n].replace(b'new-image', b'fresh-resource').replace(b'BinData/new.png', b'BinData/fresh.png')
        b['BinData/fresh.png'] = b.pop('BinData/new.png')
        self.assert_pass(o, archive(b), r)

    def test_image_existing_media_cannot_be_reused(self):
        o, _, r = case('image')
        self.assert_reject(o, o, r, 'IMAGE_MISMATCH')

    def test_jpeg_explicitly_unsupported(self):
        o, c, r = case('image')
        r['operations'][0]['image']['mediaType'] = 'image/jpeg'
        self.assert_reject(o, c, r, 'UNSUPPORTED_IMAGE', 'unsupported')

    def test_bad_png_crc(self):
        o, c, r = case('image')
        data = bytearray(NEW_PNG)
        data[29] ^= 1
        r['operations'][0]['image']['base64'] = base64.b64encode(data).decode()
        self.assert_reject(o, c, r, 'INVALID_IMAGE')

    def test_image_clipping_unsupported(self):
        o, c, r = case('image')
        a = unpack(o)
        a[SECTION] = a[SECTION].replace(b'</hp:pic>', b'<hp:imgClip left="0"/></hp:pic>', 1)
        self.assert_reject(archive(a), c, r, 'UNSUPPORTED')

    def test_dtd_rejected(self):
        o, c, r = case('noop')
        b = unpack(c)
        b[HEADER] = b'<!DOCTYPE head [<!ENTITY test "bad">]>' + b[HEADER]
        self.assert_reject(o, archive(b), r, 'INVALID_XML')

    def test_xml_encoding_rejected(self):
        o, c, r = case('noop')
        b = unpack(c)
        b[HEADER] = b'<?xml version="1.0" encoding="ISO-8859-1"?>' + b[HEADER]
        self.assert_reject(o, archive(b), r, 'INVALID_XML')

    def test_duplicate_zip_entries_rejected(self):
        o, c, r = case('noop')
        b = io.BytesIO(c)
        import warnings
        with warnings.catch_warnings():
            warnings.simplefilter('ignore', UserWarning)
            with zipfile.ZipFile(b, 'a') as z:
                z.writestr('Opaque/data.bin', b'duplicate')
        self.assert_reject(o, b.getvalue(), r, 'INVALID_PACKAGE')

    def test_path_escape_and_case_collision(self):
        o, c, r = case('noop')
        for name in ('../escape', 'Contents/../escape', 'Opaque/DATA.bin'):
            b = unpack(c)
            b[name] = b'bad'
            self.assert_reject(o, archive(b), r, 'INVALID_PACKAGE')

    def test_ambiguous_manifest_href_rejected(self):
        o, c, r = case('noop')
        b = unpack(c)
        b['Contents/BinData/old.png'] = OLD_PNG
        self.assert_reject(o, archive(b), r, 'INVALID_REFERENCE')

    def test_invalid_original_is_not_candidate_failure(self):
        _, c, r = case('noop')
        self.assert_reject(b'not a zip', c, r, 'INVALID_PACKAGE', 'error')

    def test_zip_method_preserved(self):
        o, c, r = case('text')
        c = archive(unpack(c), zipfile.ZIP_DEFLATED)
        self.assert_reject(o, c, r, 'ZIP_METHOD_CHANGED')

    def test_unchanged_compressed_data_preserved(self):
        o, c, r = case('text')
        a, b = unpack(o), unpack(c)
        a['Opaque/data.bin'] = b['Opaque/data.bin'] = b'ABCD' * 10000
        o = archive(a, zipfile.ZIP_DEFLATED, 1)
        c = archive(b, zipfile.ZIP_DEFLATED, 9)
        self.assert_reject(o, c, r, 'RECOMPRESSED_ENTRY')

    def test_signed_edit_unsupported(self):
        o, c, r = case('text')
        a = unpack(o)
        a['META-INF/signatures.xml'] = b'<signature/>'
        self.assert_reject(archive(a), c, r, 'UNSUPPORTED')

    def test_generator_is_deterministic_and_manifest_hashes_match(self):
        with tempfile.TemporaryDirectory() as temp:
            a, b = Path(temp) / 'a', Path(temp) / 'b'
            manifest = generate(a)
            generate(b)
            self.assertEqual(sorted(p.name for p in a.iterdir()), sorted(p.name for p in b.iterdir()))
            for p in a.iterdir():
                self.assertEqual(p.read_bytes(), (b / p.name).read_bytes())
            for row in manifest['cases']:
                for item in row['files'].values():
                    self.assertEqual(item['sha256'], hashlib.sha256((a / item['file']).read_bytes()).hexdigest())

    def test_json_duplicate_keys_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            p = Path(temp) / 'r.json'
            p.write_text('{"schemaVersion":1,"schemaVersion":1,"operations":[]}', encoding='utf-8')
            with self.assertRaisesRegex(Exception, 'duplicate JSON key'):
                read_json(p)

    def test_cli_exit_codes_and_json_stdout(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            for scenario, exit_code, status in (('text', 0, 'pass'), ('out_of_target_text', 1, 'fail'), ('unsupported', 2, 'unsupported')):
                if scenario == 'out_of_target_text':
                    o, c, r, _ = mutate(scenario)
                else:
                    o, c, r = case('image' if scenario == 'unsupported' else scenario)
                    if scenario == 'unsupported':
                        r['operations'][0]['image']['mediaType'] = 'image/jpeg'
                (temp / 'a.hwpx').write_bytes(o)
                (temp / 'b.hwpx').write_bytes(c)
                (temp / 'r.json').write_text(json.dumps(r), encoding='utf-8')
                proc = subprocess.run([sys.executable, '-I', str(Path(__file__).with_name('oracle.py')),
                                       str(temp / 'a.hwpx'), str(temp / 'b.hwpx'), str(temp / 'r.json')],
                                      capture_output=True, text=True, check=False)
                self.assertEqual(proc.returncode, exit_code, proc.stderr + proc.stdout)
                self.assertEqual(proc.stderr, '')
                self.assertEqual(json.loads(proc.stdout)['status'], status)


# Give each valid case and defect its own named unittest result.
def positive(name):
    def test(self):
        self.assert_pass(*case(name))
    return test


def negative(name, code):
    def test(self):
        original, candidate, req, _ = mutate(name)
        self.assertNotEqual(original, candidate)
        self.assert_reject(original, candidate, req, code, 'fail')
    return test


for _name in CASES:
    setattr(OracleTests, 'test_valid_' + _name, positive(_name))
for _name, _, _, _code in MUTATIONS:
    setattr(OracleTests, 'test_mutation_' + _name, negative(_name, _code))


if __name__ == '__main__':
    unittest.main()
