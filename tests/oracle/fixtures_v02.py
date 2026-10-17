"""Hand-authored 0.2 originals and expected outputs; no engine/oracle imports."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
from fixtures import entries, archive, SECTION, CACHE, HEADER
HP='http://www.hancom.co.kr/hwpml/2011/paragraph'
HS='http://www.hancom.co.kr/hwpml/2011/section'

def _para(inner,attrs='',cache=True):
    return f'<hp:p {attrs}paraPrIDRef="0"><hp:run charPrIDRef="0">{inner}</hp:run>'+(CACHE if cache else '')+'</hp:p>'

def replace_all_scenario(kind):
    """Hand-written document-wide find/replace: cross-run, object, tab, nested cell, locked paragraph."""
    ref='<x:ref xmlns:x="urn:oracle:opaque" to="ref"/>'
    def cell(inner,cache):return '<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl><hp:tr><hp:tc><hp:subList>'+inner+'</hp:subList></hp:tc></hp:tr></hp:tbl></hp:run>'+(CACHE if cache else '')+'</hp:p>'
    def split(a,b,cache):return '<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>'+a+'</hp:t></hp:run><hp:run charPrIDRef="1"><hp:t>'+b+'</hp:t></hp:run>'+(CACHE if cache else '')+'</hp:p>'
    def body(word,keep_before,keep_nested):
        # keep_before: how many leading top-level paragraphs keep their cache; word: replacement or None
        w=lambda old,new:new if word else old
        rows=[(_para,('<hp:t>'+w('cat dog cat','lion dog lion')+'</hp:t>','')),
              (split,(w('xca','xlion'),w('t y &amp;cat',' y &amp;lion'))),
              (_para,('<hp:t>'+w('cat','lion')+'</hp:t><hp:equation id="E"/><hp:t>'+w('cat','lion')+'</hp:t>','')),
              (_para,('<hp:t>'+w('cat','lion')+'<hp:tab/>'+w('cat','lion')+'</hp:t>','')),
              (cell,(_para('<hp:t>'+w('cat in cell','lion in cell')+'</hp:t>',cache=keep_nested),)),
              (_para,('<hp:t>cat</hp:t>','id="ref" ')),
              (_para,('<hp:t>'+w('tail cat','tail lion')+'</hp:t>',''))]
        out=[]
        for index,(fn,args) in enumerate(rows):
            cache=index<keep_before
            if fn is _para:out.append(fn(args[0],args[1],cache))
            elif fn is split:out.append(fn(args[0],args[1],cache))
            else:out.append(fn(args[0],cache))
            if index==5:out.append(ref)
        return ''.join(out)
    scoped=kind=='replace_all_scope'
    old=body(False,7,True)
    if scoped:
        # Only the cell below the fifth top-level paragraph: earlier caches stay.
        new=body(False,4,False).replace('cat in cell','lion in cell')
        op={'op':'text.replaceAll','find':'cat','text':'lion','scope':{'entry':SECTION,'path':[4]}}
    else:
        new=body(True,0,False)
        op={'op':'text.replaceAll','find':'cat','text':'lion'}
    def section(inner):return (f'<hs:sec xmlns:hs="{HS}" xmlns:hp="{HP}">'+inner+'</hs:sec>').encode()
    a=entries();a.pop('Preview/PrvText.txt');a[SECTION]=section(old)
    a[HEADER]=a[HEADER].replace(b'</hh:refList>',b'<hh:charProperties itemCnt="2"><hh:charPr id="0"/><hh:charPr id="1"/></hh:charProperties></hh:refList>')
    b=a.copy();b[SECTION]=section(new)
    return archive(a),archive(b),{'schemaVersion':1,'operations':[op]},b

def custom_replace_all(old_body,new_body,op):
    """Hand-written originals/expectations for single-purpose replaceAll cases."""
    def section(inner):return (f'<hs:sec xmlns:hs="{HS}" xmlns:hp="{HP}">'+inner+'</hs:sec>').encode()
    a=entries();a.pop('Preview/PrvText.txt');a[SECTION]=section(old_body)
    a[HEADER]=a[HEADER].replace(b'</hh:refList>',b'<hh:charProperties itemCnt="2"><hh:charPr id="0"/><hh:charPr id="1"/></hh:charProperties></hh:refList>')
    b=a.copy();b[SECTION]=section(new_body)
    return archive(a),archive(b),{'schemaVersion':1,'operations':[op]},b

def scenario(kind):
    if kind in ('replace_all','replace_all_scope'):return replace_all_scenario(kind)
    if kind=='insert':
        old='<hp:run charPrIDRef="0"><hp:t>ab</hp:t></hp:run><hp:run charPrIDRef="1"><hp:t>cd</hp:t></hp:run>'
        new=old.replace('>cd<','>Xcd<')
        op={'op':'text.replace','path':[0],'range':{'start':2,'end':2},'text':'X'}
    elif kind=='empty':
        old='<hp:run charPrIDRef="0"><hp:secPr/></hp:run>'
        new=old.replace('</hp:run>','<hp:t>X</hp:t></hp:run>')
        op={'op':'text.replace','path':[0],'range':{'start':0,'end':0},'text':'X'}
    elif kind=='preserve':
        old='<hp:run charPrIDRef="0"><hp:t>a<hp:tab/>b</hp:t><hp:equation id="E"/><hp:t>c</hp:t></hp:run>'
        new=old.replace('>a<','>X<').replace('/>b<','/><').replace('>c<','><')
        op={'op':'text.replace','path':[0],'range':{'start':0,'end':5},'text':'X','controls':'preserve'}
    elif kind=='object_empty':
        old='<hp:run charPrIDRef="0"><hp:equation id="E"/><hp:t/></hp:run>'
        new=old.replace('<hp:t/>','<hp:t>X</hp:t>')
        op={'op':'text.replace','path':[0],'range':{'start':0,'end':1},'text':'X','controls':'preserve'}
    elif kind=='cell':
        def para(t):return '<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>'+t+'</hp:t></hp:run>'+CACHE+'</hp:p>'
        old='<hp:run><hp:tbl><hp:tr><hp:tc><hp:cellSpan colSpan="2" rowSpan="1"/><hp:subList>'+para('one')+para('')+'</hp:subList></hp:tc></hp:tr></hp:tbl></hp:run>'
        new=old.replace('>one<','>X<').replace('<hp:t></hp:t>','<hp:t>Y</hp:t>').replace(CACHE,'')
        op={'op':'cell.setText','path':[0,0,0,0,0],'paragraphs':['X','Y']}
    else:raise ValueError(kind)
    def section(inner,cache):return (f'<hs:sec xmlns:hs="{HS}" xmlns:hp="{HP}"><hp:p paraPrIDRef="0">'+inner+cache+'</hp:p></hs:sec>').encode()
    a=entries();a.pop('Preview/PrvText.txt');a[SECTION]=section(old,CACHE)
    a[HEADER]=a[HEADER].replace(b'</hh:refList>',b'<hh:charProperties itemCnt="2"><hh:charPr id="0"/><hh:charPr id="1"/></hh:charProperties></hh:refList>')
    b=a.copy();b[SECTION]=section(new,'')
    op['entry']=SECTION
    return archive(a),archive(b),{'schemaVersion':1,'operations':[op]},b

def replace_all_mutants(kind,xml):
    cache=CACHE.encode()
    def sub(old,new):
        assert old in xml,old
        return xml.replace(old,new,1)
    if kind=='replace_all':
        return [sub(b'>lion dog lion<',b'>lion dog cat<'),                                   # omitted match
                sub(b'<hp:t>xlion</hp:t>',b'<hp:t>x</hp:t>').replace(b'<hp:t> y &amp;lion</hp:t>',b'<hp:t>lion y &amp;lion</hp:t>'),  # wrong run position
                sub(b'<hp:t>cat</hp:t></hp:run></hp:p><x:ref',b'<hp:t>lion</hp:t></hp:run></hp:p><x:ref'),  # locked paragraph
                sub(b'<hp:t>tail lion</hp:t></hp:run>',b'<hp:t>tail lion</hp:t></hp:run>'+cache),   # cache kept at tail
                sub(b'<hp:t>lion in cell</hp:t></hp:run>',b'<hp:t>lion in cell</hp:t></hp:run>'+cache),  # nested cache kept
                sub(b'<hp:t>lion dog lion</hp:t></hp:run>',b'<hp:t>lion dog lion</hp:t></hp:run>'+cache),  # cache before first edit kept
                sub(b'<hp:equation id="E"/>',b''),
                sub(b'tail lion',b'tail Lion'),
                sub(b'lion<hp:tab/>lion',b'lion lion'),
                xml.replace(b'lion',b'cat'),                                                      # caches gone, nothing replaced
                sub(b'dog',b'DOG')]
    return [sub(b'tail cat',b'tail lion'),                                                       # outside the scope
            sub(b'lion in cell',b'cat in cell'),                                                 # omitted in scope
            sub(b'<hp:t>cat dog cat</hp:t></hp:run>'+cache,b'<hp:t>cat dog cat</hp:t></hp:run>'),  # earlier cache removed
            sub(b'<hp:t>lion in cell</hp:t></hp:run>',b'<hp:t>lion in cell</hp:t></hp:run>'+cache),  # nested cache kept
            sub(b'<hp:t>tail cat</hp:t></hp:run>',b'<hp:t>tail cat</hp:t></hp:run>'+cache)]    # tail cache kept

def mutants(kind,payload):
    xml=payload[SECTION]
    if kind in ('replace_all','replace_all_scope'):
        result=replace_all_mutants(kind,xml);assert all(value!=xml for value in result);return result
    result=[xml.replace(b'>X',b'>WRONG')]
    if kind=='insert':result.append(xml.replace(b'>ab<',b'>abX<').replace(b'>Xcd<',b'>cd<'))
    if kind=='empty':result.append(xml.replace(b'<hp:secPr/><hp:t>X</hp:t>',b'<hp:t>X</hp:t><hp:secPr/>'))
    if kind in ['preserve','object_empty']:
        result.extend([xml.replace(b'id="E"',b'id="BAD"'),xml.replace(b'<hp:equation id="E"/>',b'')])
    if kind=='preserve':result.append(xml.replace(b'<hp:tab/>',b'<hp:lineBreak/>'))
    if kind=='cell':result.extend([xml.replace(b'colSpan="2"',b'colSpan="1"'),xml.replace(b'>Y<',b'>WRONG<')])
    assert all(value!=xml for value in result)
    return result

if __name__=='__main__':
    import json,sys
    from pathlib import Path
    out=Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True);cases=[]
    for kind in ['insert','empty','preserve','object_empty','cell','replace_all','replace_all_scope']:
        original,valid,request,payload=scenario(kind)
        (out/(kind+'.original.hwpx')).write_bytes(original)
        (out/(kind+'.valid.hwpx')).write_bytes(valid)
        (out/(kind+'.request.json')).write_text(json.dumps(request))
        names=[]
        for i,xml in enumerate(mutants(kind,payload)):
            p=payload.copy();p[SECTION]=xml;name=f'{kind}.mutant-{i}.hwpx';(out/name).write_bytes(archive(p));names.append(name)
        cases.append({'kind':kind,'mutants':names})
    (out/'v02-manifest.json').write_text(json.dumps(cases))
