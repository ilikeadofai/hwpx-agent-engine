import collections, hashlib, json, pathlib, zipfile, xml.etree.ElementTree as E
r=pathlib.Path.cwd(); path=r/'tests/fixtures/corpus/research-school-plan-reference.hwpx'
responses=json.loads((r/'.queries2-corpus-output.json').read_text())
assert all(v['ok'] for v in responses),[(v['command'],v.get('diagnostics')) for v in responses if not v['ok']]
style={v['result']['items'][0]['kind']:v['result'] for v in responses if v['command']=='style.list'}
tables=next(v['result']['items'] for v in responses if v['command']=='table.get')
pictures=next(v['result']['items'] for v in responses if v['command']=='picture.list')
original=path.read_bytes(); out=r/'.queries2-tmp/corpus-evidence'
assert (out/'before.hwpx').read_bytes()==original==(out/'after.hwpx').read_bytes()
# Independently count literal references from stdlib XML, and hash ZIP payloads.
z=zipfile.ZipFile(path); HH='http://www.hancom.co.kr/hwpml/2011/head'; HP='http://www.hancom.co.kr/hwpml/2011/paragraph'; HC='http://www.hancom.co.kr/hwpml/2011/core'; HP10='http://www.hancom.co.kr/hwpml/2016/paragraph'
refs={'styleIDRef':'style','nextStyleIDRef':'style','charPrIDRef':'charPr','paraPrIDRef':'paraPr','borderFillIDRef':'borderFill'}
counts=collections.Counter(); defs=set(); header=E.fromstring(z.read('Contents/header.xml'))
for e in header.iter():
 ns,_,local=e.tag[1:].partition('}')
 if ns==HH and local in style and 'id' in e.attrib: defs.add((local,e.attrib['id']))
for entry in z.namelist():
 if not entry.lower().endswith(('.xml','.hpf','.rdf')):continue
 for e in E.fromstring(z.read(entry)).iter():
  if not e.tag.startswith('{'):continue
  ns,_,local=e.tag[1:].partition('}')
  if ns not in (HH,HP,HC,HP10):continue
  for k,v in e.attrib.items():
   if k in refs:counts[refs[k],v]+=1
for kind,v in style.items():
 assert v['nextCursor'] is None
 for item in v['items']:assert item['useCount']==counts[kind,item['id']]
unresolved=sum(n for key,n in counts.items() if key not in defs)
assert unresolved==next(iter(style.values()))['unresolvedReferenceCount']
for p in pictures:
 if p['media']['entry']:
  assert p['media']['sha256']==hashlib.sha256(z.read(p['media']['entry'])).hexdigest()
summary={
 'fixture':path.name,'bytes':len(original),'sha256':hashlib.sha256(original).hexdigest(),
 'all_commands_ok':True,'export_before_after_exact_original':True,
 'resource_definitions':{k:v['total'] for k,v in style.items()},
 'unresolved_references':unresolved,'unresolved_body_references':next(iter(style.values()))['unresolvedBodyReferenceCount'],
 'unresolved_kinds':dict(collections.Counter({k:sum(n for (kind,id),n in counts.items() if kind==k and (kind,id) not in defs) for k in style})),
 'table_count':len(tables),'actual_cells':sum(len(t['cells']) for t in tables),
 'merged_anchors':sum(any((c['address'][a] or 0)>1 for a in ['rowSpan','colSpan']) for t in tables for c in t['cells']),
 'covered_coordinates':sum(len(t['coveredCells']) for t in tables),
 'tables':[{'section':t['section'],'rows':t['rows'],'cols':t['cols'],'cells':len(t['cells']),'covered':len(t['coveredCells'])} for t in tables],
 'picture_count':len(pictures),'payload_hashes':sum(p['media']['sha256'] is not None for p in pictures),
 'unique_media_entries':len({p['media']['entry'] for p in pictures if p['media']['entry']}),
 'pictures_with_duplicate_refs':sum(bool(p['media']['otherPictureNodeIds']) for p in pictures),
 'cropped':sum(p['cropped'] is True for p in pictures),'external':sum(p['media']['external'] is True for p in pictures),
 'independent_stdlib_reference_counts_and_payload_hashes_match':True,
}
(r/'.queries2-corpus-summary.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2))
print(json.dumps(summary,ensure_ascii=False,indent=2))
