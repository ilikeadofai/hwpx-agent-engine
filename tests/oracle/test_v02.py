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
