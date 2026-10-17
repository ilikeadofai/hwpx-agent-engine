"""Hand-authored 0.2 originals and expected outputs; no engine/oracle imports."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
from fixtures import entries, archive, SECTION, CACHE, HEADER
HP='http://www.hancom.co.kr/hwpml/2011/paragraph'
HS='http://www.hancom.co.kr/hwpml/2011/section'

def scenario(kind):
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

def mutants(kind,payload):
    xml=payload[SECTION]
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
    for kind in ['insert','empty','preserve','object_empty','cell']:
        original,valid,request,payload=scenario(kind)
        (out/(kind+'.original.hwpx')).write_bytes(original)
        (out/(kind+'.valid.hwpx')).write_bytes(valid)
        (out/(kind+'.request.json')).write_text(json.dumps(request))
        names=[]
        for i,xml in enumerate(mutants(kind,payload)):
            p=payload.copy();p[SECTION]=xml;name=f'{kind}.mutant-{i}.hwpx';(out/name).write_bytes(archive(p));names.append(name)
        cases.append({'kind':kind,'mutants':names})
    (out/'v02-manifest.json').write_text(json.dumps(cases))
