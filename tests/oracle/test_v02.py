"""Independent synthetic 0.2 expectations and structural mutations."""
import unittest
from fixtures import entries, archive, SECTION, CACHE
from oracle import check

from fixtures_v02 import scenario, mutants

class V02Tests(unittest.TestCase):
    def test_new_behaviors_have_independent_expected_candidates(self):
        for kind in ['insert','empty','preserve','object_empty','cell','replace_all','replace_all_scope']:
            with self.subTest(kind=kind):
                a,b,r,_=scenario(kind);result=check(a,b,r)
                self.assertTrue(result['ok'],result)
    def test_new_behavior_mutations_are_rejected(self):
        for kind in ['insert','empty','preserve','object_empty','cell','replace_all','replace_all_scope']:
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


class ReplaceAllTests(unittest.TestCase):
    P=staticmethod(lambda inner,cache=True:'<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0">'+inner+'</hp:run>'+(CACHE if cache else '')+'</hp:p>')
    def case(self,old,new,**op):
        from fixtures_v02 import custom_replace_all
        return custom_replace_all(old,new,{'op':'text.replaceAll',**op})
    def test_greedy_non_overlapping_deletion_and_exact_bytes(self):
        P=self.P
        a,b,r,_=self.case(P('<hp:t>aaaa</hp:t>'),P('<hp:t>XX</hp:t>',False),find='aa',text='X')
        self.assertTrue(check(a,b,r)['ok'])
        a,b,r,_=self.case(P('<hp:t>aaaa</hp:t>'),P('<hp:t>XXa</hp:t>',False),find='aa',text='X')
        self.assertFalse(check(a,b,r)['ok'])
        a,b,r,_=self.case(P('<hp:t>abcb</hp:t>'),P('<hp:t>ac</hp:t>',False),find='b',text='')
        self.assertTrue(check(a,b,r)['ok'])
    def test_limits_and_invalid_requests(self):
        P=self.P
        a,b,r,_=self.case(P('<hp:t>aa</hp:t>'),P('<hp:t>XX</hp:t>',False),find='a',text='X',maxMatches=1)
        result=check(a,b,r);self.assertFalse(result['ok']);self.assertEqual(result['diagnostics'][0]['code'],'RESOURCE_LIMIT')
        for op in [dict(find='',text='X'),dict(find='a',text='\t'),dict(find='a',text='X',maxMatches=0),dict(find='a',text='X',extra=1)]:
            a,b,r,_=self.case(P('<hp:t>aa</hp:t>'),P('<hp:t>XX</hp:t>',False),**op)
            self.assertFalse(check(a,b,r)['ok'],op)
    def test_no_match_and_identical_replacement_must_leave_bytes_untouched(self):
        P=self.P
        for op in [dict(find='zzz',text='X'),dict(find='aa',text='aa')]:
            a,_,r,_=self.case(P('<hp:t>aa</hp:t>'),P('<hp:t>aa</hp:t>'),**op)
            self.assertTrue(check(a,a,r)['ok'],op)
            _,b,_,_=self.case(P('<hp:t>aa</hp:t>'),P('<hp:t>XX</hp:t>',False),**op)
            self.assertFalse(check(a,b,r)['ok'],op)
    def test_paragraphs_outside_the_text_model_are_reported_unsupported_not_guessed(self):
        P=self.P
        # CDATA, and a combining mark: the oracle's conservative model cannot judge these.
        a,b,r,_=self.case(P('<hp:t>a<![CDATA[b]]>a</hp:t>'),P('<hp:t>X<![CDATA[b]]>X</hp:t>',False),find='a',text='X')
        self.assertEqual(check(a,b,r)['status'],'unsupported')
        a,b,r,_=self.case(P('<hp:t>e\u0301</hp:t>'),P('<hp:t>X\u0301</hp:t>',False),find='e',text='X')
        self.assertEqual(check(a,b,r)['status'],'unsupported')
        # A paragraph with no possible match is not an obstacle.
        a,b,r,_=self.case(P('<hp:t>a<![CDATA[b]]>a</hp:t>')+P('<hp:t>cat</hp:t>'),P('<hp:t>a<![CDATA[b]]>a</hp:t>')+P('<hp:t>lion</hp:t>',False),find='cat',text='lion')
        self.assertTrue(check(a,b,r)['ok'],check(a,b,r))
    def test_entity_text_and_reported_intents(self):
        P=self.P
        a,b,r,_=self.case(P('<hp:t>a&amp;b a&amp;b</hp:t>'),P('<hp:t>&lt;&amp;&gt; &lt;&amp;&gt;</hp:t>',False),find='a&b',text='<&>')
        result=check(a,b,r);self.assertTrue(result['ok'],result)
        r['intents']=result['expectedIntents'];self.assertTrue(check(a,b,r)['ok'])
        r['intents']=[{**result['expectedIntents'][0],'replacementCount':1}];self.assertFalse(check(a,b,r)['ok'])

    def test_empty_paragraph_insertion_leaves_preview_stale(self):
        from fixtures_v02 import custom_replace_all, _para
        from fixtures import unpack
        before = _para('<hp:t>pre</hp:t>') + _para('') + _para('<hp:t>post</hp:t>')
        after = _para('<hp:t>pre</hp:t>') + _para('<hp:t>X</hp:t>', cache=False) + _para('<hp:t>post</hp:t>', cache=False)
        a,b,r,_ = custom_replace_all(before, after, {'op':'text.replace','entry':SECTION,'path':[1],'range':{'start':0,'end':0},'text':'X'})
        original, candidate = unpack(a), unpack(b)
        original['Preview/PrvText.txt'] = b'pre\n\npost'
        candidate['Preview/PrvText.txt'] = b'pre\n\npost'
        self.assertTrue(check(archive(original), archive(candidate), r)['ok'])
        candidate['Preview/PrvText.txt'] = b'pre\nX\npost'
        result = check(archive(original), archive(candidate), r)
        self.assertFalse(result['ok'], result)
