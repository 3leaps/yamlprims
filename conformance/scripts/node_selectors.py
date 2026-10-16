"""Independent, hand-derived node selector and byte-offset controls.
Usage: node_selectors.py CORPUS_SRC DRIVER OUT_JSON
"""
from pathlib import Path
import copy,hashlib,json,shutil,subprocess,sys,tempfile
src,driver,out=map(Path,sys.argv[1:]);rows=[]
S=lambda x:{'t':'str','v':x}
inputs=[
 ('item-unicode','a: [é]\n','yamlprims-core',[{'t':'map','entries':[[S('a'),{'t':'seq','items':[S('é')]}]]}],0,[{'value':0},{'item':0}],4,6),
 ('alias-definition','a: &x foo\nb: *x\n','yamlprims-core',[{'t':'map','entries':[[S('a'),S('foo')],[S('b'),S('foo')]]}],0,[{'value':1}],6,9),
 ('stream-second-document','---\nfoo\n---\nbar\n','yamlprims-core-stream',[S('foo'),S('bar')],1,[],12,15),
]
for name,text,profile,docs,document,path,start,end in inputs:
 for corrupt in (False,True):
  span=dict(document=document,path=path,start=start,end=end)
  if corrupt:
   if name=='item-unicode':span['end']=5
   elif name=='alias-definition':span.update(start=13,end=15)
   else:span['document']=0
  data=text.encode();case=dict(id='probe.'+name,input={'path':'fixtures/probe.yaml','bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()},expectations={'value':[dict(profile=profile,profile_revision=3,documents=docs)],'node_locations':dict(profile=profile,profile_revision=3,convention='yamlprims-node',convention_revision=1,spans=[span])})
  with tempfile.TemporaryDirectory() as t:
   p=Path(t);(p/'src/fixtures').mkdir(parents=True);(p/'hyd').mkdir()
   shutil.copytree(src/'manifests/profiles',p/'src/manifests/profiles');shutil.copytree(src/'manifests/span-conventions',p/'src/manifests/span-conventions')
   (p/'src/fixtures/probe.yaml').write_bytes(data);(p/'src/manifests/cases.jsonl').write_text(json.dumps(case)+'\n');(p/'hyd/cases.jsonl').write_text('')
   r=subprocess.run([str(driver),str(p/'hyd'),str(p/'src')],capture_output=True,text=True,timeout=5)
  rows.append(dict(case=name,corrupt=corrupt,exit=r.returncode,stdout=r.stdout,stderr=r.stderr))
  assert r.returncode==(1 if corrupt else 0),rows[-1]
out.write_text(json.dumps(rows,indent=2)+'\n')
