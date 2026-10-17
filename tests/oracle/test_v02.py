"""Independent synthetic 0.2 expectations and structural mutations."""
import unittest
from fixtures import entries, archive, SECTION, CACHE
from oracle import check

from fixtures_v02 import scenario, mutants

class V02Tests(unittest.TestCase):
    def test_new_behaviors_have_independent_expected_candidates(self):
        for kind in ['insert','empty','preserve','object_empty','cell']:
            with self.subTest(kind=kind):
                a,b,r,_=scenario(kind);result=check(a,b,r)
                self.assertTrue(result['ok'],result)
    def test_new_behavior_mutations_are_rejected(self):
        for kind in ['insert','empty','preserve','object_empty','cell']:
            a,b,r,payload=scenario(kind)
            candidates=mutants(kind,payload)
            for mutant in candidates:
                with self.subTest(kind=kind,mutation=candidates.index(mutant)):
                    self.assertNotEqual(mutant,payload[SECTION]);p=payload.copy();p[SECTION]=mutant
                    self.assertFalse(check(a,archive(p),r)['ok'])

class Phase71Tests(unittest.TestCase):
    def test_object_boundary_insertion_independently_preserves_original_object(self):
        a,b,r,payload=scenario('object_empty')
        from fixtures import unpack
        original=unpack(a);candidate=original.copy()
        candidate[SECTION]=original[SECTION].replace(b'<hp:equation id="E"/>',b'<hp:t>X</hp:t><hp:equation id="E"/>').replace(CACHE.encode(),b'')
        r['operations'][0]['range']={'start':0,'end':0};r['operations'][0].pop('controls')
        self.assertTrue(check(a,archive(candidate),r)['ok'])
        bad=candidate.copy();bad[SECTION]=bad[SECTION].replace(b'id="E"',b'id="WRONG"')
        self.assertFalse(check(a,archive(bad),r)['ok'])
    def test_preserve_rejects_fields_including_hyperlink_markers(self):
        from fixtures import unpack
        for marker in [b'<hp:fieldBegin type="HYPERLINK" id="1"/>',b'<hp:fieldEnd beginIDRef="1"/>']:
            a,b,r,_=scenario('preserve');original=unpack(a);candidate=unpack(b)
            for p in [original,candidate]:p[SECTION]=p[SECTION].replace(b'<hp:equation id="E"/>',marker)
            result=check(archive(original),archive(candidate),r)
            self.assertFalse(result['ok'])
            self.assertIn('field_boundary_in_range',result['diagnostics'][0]['message'])
