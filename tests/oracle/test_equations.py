"""Handwritten equation candidates, independent of Rust editor/model/verifier."""
from pathlib import Path
import unittest
from fixtures import archive, unpack, SECTION
from oracle import check

SOURCE = Path(__file__).parents[1] / 'fixtures/editing/equations.hwpx'
OLD = '{a} over {b}  TIMES  {c} over {d} = {ac} over {bd}'
PREVIEW = 'Preview/PrvText.txt'


def case(preview, new_preview=None, props=False):
    before = unpack(SOURCE.read_bytes())
    before[PREVIEW] = preview
    after = dict(before)
    after[SECTION] = after[SECTION].replace(b'<hp:linesegarray/>', b'')
    if props:
        after[SECTION] = after[SECTION].replace(b'baseLine="66"', b'baseLine="77"', 1)
        op = {'op': 'equation.setProperties', 'props': {'baseLine': 77}}
    else:
        after[SECTION] = after[SECTION].replace(OLD.encode(), b'x &lt; y &amp; z', 1)
        op = {'op': 'equation.setScript', 'script': 'x < y & z'}
    if new_preview is not None:
        after[PREVIEW] = new_preview
    op.update(entry=SECTION, path=[0, 0, 2])
    return before, after, {'schemaVersion': 1, 'operations': [op]}


class EquationOracleTests(unittest.TestCase):
    def verify(self, before, after, request):
        return check(archive(before), archive(after), request)

    def test_unique_script_updates_preview(self):
        result = self.verify(*case(('before '+OLD+' after').encode(), b'before x < y & z after'))
        self.assertTrue(result['ok'], result)
        self.assertFalse(result['prvTextStale'])
        self.assertEqual(result['cacheRemovals'], 4)

    def test_absent_duplicate_and_non_utf8_stay_stale(self):
        for preview in (b'Equation 1:', (OLD+' '+OLD).encode(), b'\xff\xfe'):
            result = self.verify(*case(preview))
            self.assertTrue(result['ok'], result)
            self.assertTrue(result['prvTextStale'])

    def test_missing_preview_not_synthesized(self):
        before, after, req = case(b'ignored')
        del before[PREVIEW], after[PREVIEW]
        result = self.verify(before, after, req)
        self.assertTrue(result['ok'], result)
        self.assertFalse(result['prvTextStale'])

    def test_retained_unique_preview_rejected(self):
        result = self.verify(*case(OLD.encode()))
        self.assertFalse(result['ok'], result)

    def test_unrequested_preview_rejected(self):
        result = self.verify(*case(b'no script', b'x < y & z'))
        self.assertFalse(result['ok'], result)

    def test_script_and_unrelated_equation_tampering_rejected(self):
        before, after, req = case(b'no script')
        for a, b in ((b'x &lt; y &amp; z', b'WRONG'), (b'id="101"', b'id="999"')):
            mutant = dict(after)
            mutant[SECTION] = mutant[SECTION].replace(a, b, 1)
            self.assertFalse(self.verify(before, mutant, req)['ok'])

    def test_properties_keep_preview(self):
        result = self.verify(*case(OLD.encode(), props=True))
        self.assertTrue(result['ok'], result)
        self.assertFalse(result['prvTextStale'])

    def test_properties_preview_tampering_rejected(self):
        result = self.verify(*case(OLD.encode(), b'changed', props=True))
        self.assertFalse(result['ok'], result)

    def test_properties_add_missing_attribute(self):
        before, after, req = case(b'no script', props=True)
        before[SECTION] = before[SECTION].replace(b' baseLine="66"', b'', 1)
        result = self.verify(before, after, req)
        self.assertTrue(result['ok'], result)

    def test_empty_script_stale_and_entity_decoding(self):
        before, after, req = case(b'empty')
        before[SECTION] = before[SECTION].replace(OLD.encode(), b'', 1)
        result = self.verify(before, after, req)
        self.assertTrue(result['ok'], result)
        self.assertTrue(result['prvTextStale'])
        before, after, req = case(b'a < b & c > d + '+ 'α'.encode())
        req['operations'][0]['path'] = [2, 0, 1]
        after[SECTION] = before[SECTION].replace(b'a &lt; b &amp; c &gt; d + &#x3B1;', b'x &lt; y &amp; z').replace(b'<hp:linesegarray/>', b'', 0)
        # Only the edited third paragraph and the following paragraph lose caches.
        parts = after[SECTION].split(b'<hp:linesegarray/>')
        after[SECTION] = b'<hp:linesegarray/>'.join(parts[:3]) + b''.join(parts[3:])
        after[PREVIEW] = b'x < y & z'
        result = self.verify(before, after, req)
        self.assertTrue(result['ok'], result)

    def test_noop_preserves_archive(self):
        before, _, req = case(OLD.encode())
        req['operations'][0]['script'] = OLD
        result = self.verify(before, before, req)
        self.assertTrue(result['ok'], result)

    def test_invalid_notation_rejected(self):
        for script in ('', '\\frac{a}{b}', '{x', 'x}', '"x"', '<!--x-->', 'x\ny'):
            before, after, req = case(b'no script')
            req['operations'][0]['script'] = script
            self.assertFalse(self.verify(before, after, req)['ok'])

    def test_property_intent_includes_omitted_properties_as_null(self):
        before, after, req = case(b'unchanged preview', props=True)
        req['intents'] = [{'op': 'equation.setProperties', 'entry': SECTION,
                           'path': [0, 0, 2], 'props': {'baseLine': 77, 'textColor': None,
                           'font': None, 'baseUnit': None, 'lineMode': None, 'version': None}}]
        result = self.verify(before, after, req)
        self.assertTrue(result['ok'], result)
        req['intents'][0]['props']['baseLine'] = 78
        self.assertFalse(self.verify(before, after, req)['ok'])
