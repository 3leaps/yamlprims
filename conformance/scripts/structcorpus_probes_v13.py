"""Controls for the driver's spec 1.3 support: each corruption must fail (exit != 0).

Usage: python3 -I conformance/scripts/structcorpus_probes_v13.py SOURCE_DIR DRIVER

SOURCE_DIR is a structcorpus checkout at 400ae34 or later. It must have the r3 profiles,
`yamlprims-core-limits` r2, `yamlprims-core-stress` r1, the `yamlprims-node` r1 node
convention and the `yaml.anchors.*` node-span cases.

Probes cover the r3 and explicit-limits profiles, `node_locations` (spans, paths, node
kinds, convention and profile binding) and event-only cases. Each probe runs a small
corpus built in a temporary directory from SOURCE_DIR's profiles, span conventions and
fixtures. SOURCE_DIR is not modified. Prints one row per probe and exits 1 if any control
does not behave as expected. scripts/structcorpus_gate_probes.py keeps the earlier
controls.
"""
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

src, driver = map(Path, sys.argv[1:3])
cases = {json.loads(l)["id"]: json.loads(l) for l in open(src / "manifests/cases.jsonl") if l.strip()}
results = []
EXTRA = {}


def tiny(tmp, records, edit_file=None):
    s, h = tmp / "src", tmp / "hyd"
    (s / "manifests").mkdir(parents=True)
    h.mkdir()
    for d in ("manifests/profiles", "manifests/span-conventions", "fixtures"):
        shutil.copytree(src / d, s / d)
    (h / "cases.jsonl").write_text("")
    (s / "manifests/cases.jsonl").write_text("".join(json.dumps(r) + "\n" for r in records))
    for rel, data in EXTRA.items():
        (s / rel).parent.mkdir(parents=True, exist_ok=True)
        (s / rel).write_bytes(data)
    if edit_file:
        rel, f = edit_file
        d = json.loads((s / rel).read_text())
        f(d)
        (s / rel).write_text(json.dumps(d))
    return h, s


def drive(label, records, want_ok, edit_file=None, case_file=False):
    with tempfile.TemporaryDirectory() as t:
        h, s = tiny(Path(t), records, edit_file)
        argv = [str(driver), str(h), str(s)]
        if case_file:
            argv += ["--case-file", str(s / "manifests/cases.jsonl")]
        r = subprocess.run(argv, capture_output=True, text=True)
    ok = (r.returncode == 0) == want_ok
    last = [l for l in r.stdout.splitlines() if l.startswith(("problem\t", "result\t", "load-only\t"))]
    results.append((label, r.returncode, ok, " | ".join(last)[:200]))


def edit(case_id, f):
    c = json.loads(json.dumps(cases[case_id]))
    f(c)
    return c


def nl(c):
    return c["expectations"]["node_locations"]


TAGGED, SIB = "yaml.anchors.tagged", "yaml.anchors.after-sibling"

# Profiles citing spec 1.3.
r3 = edit(SIB, lambda c: c["expectations"].pop("node_locations"))
drive("r3-value-control", [r3], True)
drive("r3-wrong-value", [edit(SIB, lambda c: (c["expectations"].pop("node_locations"), c["expectations"]["value"][0]["documents"][0]["entries"].pop()))], False)
drive("r3-changed-policy", [r3], False, ("manifests/profiles/yamlprims-core.r3.json", lambda d: d["policy"].update(merge_keys="apply")))

# Node spans.
drive("node-control", [cases[TAGGED], cases[SIB]], True)
drive("node-wrong-start", [edit(SIB, lambda c: nl(c)["spans"][0].update(start=nl(c)["spans"][0]["start"] - 1))], False)
drive("node-wrong-end", [edit(SIB, lambda c: nl(c)["spans"][0].update(end=nl(c)["spans"][0]["end"] + 1))], False)
drive("node-inner-moved", [edit(TAGGED, lambda c: nl(c)["spans"][1].update(start=0, end=0))], False)
drive("node-unresolved-path", [edit(SIB, lambda c: nl(c)["spans"][0].update(path=[{"value": 7}]))], False)
drive("node-inner-on-untagged", [edit(SIB, lambda c: nl(c)["spans"][0]["path"].append({"inner": True}))], False)
drive("node-undefined-kind", [edit(SIB, lambda c: nl(c)["spans"][0].update(path=[{"value": 0}]))], False)
drive("node-unknown-step", [edit(SIB, lambda c: nl(c)["spans"][0].update(path=[{"entry": 1}]))], False)
drive("node-outside-input", [edit(SIB, lambda c: nl(c)["spans"][0].update(start=99, end=99))], False)
drive("node-document-index", [edit(SIB, lambda c: nl(c)["spans"][0].update(document=1))], False)
drive("node-no-spans", [edit(SIB, lambda c: nl(c).update(spans=[]))], False)
drive("node-no-value-expectation", [edit(SIB, lambda c: c["expectations"].pop("value"))], False)
drive("node-convention-revision", [edit(SIB, lambda c: nl(c).update(convention_revision=2))], False)
drive("node-convention-name", [edit(SIB, lambda c: nl(c).update(convention="scalar-alias"))], False)
drive("node-bound-to-r2", [edit(SIB, lambda c: (nl(c).update(profile_revision=2), c["expectations"]["value"][0].update(profile_revision=2)))], False)
drive("node-convention-file-kinds", [cases[SIB]], False, ("manifests/span-conventions/yamlprims-node.r1.json", lambda d: d["defined_nodes"].append("int")))
drive("node-convention-file-selects", [cases[SIB]], False, ("manifests/span-conventions/yamlprims-node.r1.json", lambda d: d.update(selects="event")))

# Explicit-limits profiles: a 12-deep flow sequence under limits r2 (max_depth 8), and a
# 130-deep one under stress r1 (max_depth 128).
EXTRA = {"fixtures/probe/deep12.yaml": b"[" * 12 + b"x" + b"]" * 12 + b"\n",
         "fixtures/probe/deep130.yaml": b"[" * 130 + b"x" + b"]" * 130 + b"\n"}


def deep_case(name, profile, rev, error):
    data = EXTRA[f"fixtures/probe/{name}.yaml"]
    exp = {"profile": profile, "profile_revision": rev, "error": error}
    return {"id": f"probe.{name}", "input": {"path": f"fixtures/probe/{name}.yaml", "bytes": len(data)}, "expectations": {"value": [exp]}}


def depth(n):
    return {"phase": "limit", "kind": "limit_exceeded", "limit": "max_depth", "configured": n}


LIM2, STRESS = "manifests/profiles/yamlprims-core-limits.r2.json", "manifests/profiles/yamlprims-core-stress.r1.json"
drive("limits-r2-control", [deep_case("deep12", "yamlprims-core-limits", 2, depth(8))], True)
drive("limits-r2-wrong-configured", [deep_case("deep12", "yamlprims-core-limits", 2, depth(9))], False)
drive("limits-r2-removed", [deep_case("deep12", "yamlprims-core-limits", 2, depth(8))], False, (LIM2, lambda d: d.pop("limits")))
drive("stress-control", [deep_case("deep130", "yamlprims-core-stress", 1, depth(128))], True)
drive("stress-wrong-configured", [deep_case("deep130", "yamlprims-core-stress", 1, depth(255))], False)
drive("stress-missing-field", [deep_case("deep130", "yamlprims-core-stress", 1, depth(128))], False, (STRESS, lambda d: d["limits"].pop("max_warnings")))
drive("stress-changed-policy", [deep_case("deep130", "yamlprims-core-stress", 1, depth(128))], False, (STRESS, lambda d: d["policy"].update(documents="stream")))

# Event-only cases: never a pass on their own in a full run; with --case-file they are
# loaded under yamlprims-core r3 and the outcome is reported.
ev = {"id": "probe.events-only", "input": {"path": "fixtures/probe/deep12.yaml", "bytes": len(EXTRA["fixtures/probe/deep12.yaml"])},
      "expectations": {"events": {"notation": "yaml-test-suite", "status": "valid", "events": []}}}
drive("events-only-full-run", [ev], False)
drive("events-only-case-file", [ev], True, case_file=True)

bad = 0
print("probe\texit\tas_expected\tdetail")
for label, code, ok, detail in results:
    bad += not ok
    print(f"{label}\t{code}\t{'yes' if ok else 'NO'}\t{detail}")
sys.exit(1 if bad else 0)
