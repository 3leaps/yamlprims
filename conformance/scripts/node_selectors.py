"""Independent, hand-derived node selector and byte-offset controls.
Usage: node_selectors.py CORPUS_SRC DRIVER OUT_JSON
CORPUS_SRC's profiles and span conventions are read through its format registry when it has
one; the temporary corpora use the legacy single-format layout.
"""
from pathlib import Path
import copy,hashlib,json,shutil,subprocess,sys,tempfile


def structcorpus_layout(src):
    """YAML inventory paths of a structcorpus source: (cases, profiles, span_conventions, registry).

    With manifests/formats.json the registry must be formats/v1 with a yaml entry, and its
    cases, profiles and span_conventions paths are used (relative, no empty, . or ..
    segments, resolving inside src); any problem raises ValueError. Without the registry, the
    legacy single-format paths. The same function is in every conformance script that reads
    a source checkout; keep the copies identical.
    """
    reg = src / "manifests/formats.json"
    try:
        reg.lstat()
    except FileNotFoundError:
        return src / "manifests/cases.jsonl", src / "manifests/profiles", src / "manifests/span-conventions", False
    except OSError as e:
        raise ValueError(f"{reg}: {e}") from None
    try:
        doc = json.loads(reg.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise ValueError(f"{reg}: {e}") from None
    version = doc.get("schema_version") if isinstance(doc, dict) else None
    if version != "formats/v1":
        raise ValueError(f"{reg}: unsupported schema_version {json.dumps(version)}")
    formats = doc.get("formats")
    entry = formats.get("yaml") if isinstance(formats, dict) else None
    if not isinstance(entry, dict):
        raise ValueError(f"{reg}: no yaml format entry")
    root = src.resolve()
    out = []
    for key in ("cases", "profiles", "span_conventions"):
        rel = entry.get(key)
        if not isinstance(rel, str):
            raise ValueError(f"{reg}: yaml.{key} is missing or not a string")
        if not rel or rel.startswith("/") or "\\" in rel or ":" in rel or any(p in ("", ".", "..") for p in rel.split("/")):
            raise ValueError(f"{reg}: yaml.{key}: {rel!r} is not a normalized repository-relative path")
        p = src / rel
        if p.exists() and root not in p.resolve().parents:
            raise ValueError(f"{reg}: yaml.{key}: {rel} resolves outside the source directory")
        out.append(p)
    return out[0], out[1], out[2], True


def yaml_record(c, registry):
    """True for a `"format": "yaml"` case record, False for another format. A record without
    format is run from a legacy source and raises ValueError from a registry source."""
    if "format" not in c:
        if registry:
            raise ValueError("no format")
        return True
    if not isinstance(c["format"], str):
        raise ValueError(f"format {json.dumps(c['format'])} is not a string")
    return c["format"] == "yaml"


src,driver,out=map(Path,sys.argv[1:]);rows=[]
try:_,PROFILES,SPANS,_=structcorpus_layout(src)
except ValueError as e:sys.exit(f"structcorpus layout: {e}")
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
   shutil.copytree(PROFILES,p/'src/manifests/profiles');shutil.copytree(SPANS,p/'src/manifests/span-conventions')
   (p/'src/fixtures/probe.yaml').write_bytes(data);(p/'src/manifests/cases.jsonl').write_text(json.dumps(case)+'\n');(p/'hyd/cases.jsonl').write_text('')
   r=subprocess.run([str(driver),str(p/'hyd'),str(p/'src')],capture_output=True,text=True,timeout=5)
  rows.append(dict(case=name,corrupt=corrupt,exit=r.returncode,stdout=r.stdout,stderr=r.stderr))
  assert r.returncode==(1 if corrupt else 0),rows[-1]
out.write_text(json.dumps(rows,indent=2)+'\n')
