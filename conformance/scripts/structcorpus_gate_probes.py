"""Controls for the structcorpus driver and gate: each corruption must fail (exit != 0).

Usage: python3 -I conformance/scripts/structcorpus_gate_probes.py PIN.json HYDRATED_DIR SOURCE_DIR DRIVER [LIMITS_SOURCE]

LIMITS_SOURCE is a structcorpus checkout that has `yamlprims-core-limits.r1.json` (added
after 0.1 at 444bc12); the limits-profile probes take that one profile file from it.

Driver probes use a one-case corpus built in a temporary directory from SOURCE_DIR's
profiles and fixtures. Gate probes use the real pin with a copied, altered inventory or
source. Neither SOURCE_DIR nor HYDRATED_DIR is modified. Prints one row per probe and
exits 1 if any control does not behave as expected.
"""
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
pin, hyd, src, driver = map(Path, sys.argv[1:5])
limits_src = Path(sys.argv[5]) if len(sys.argv) > 5 else src
cases = {json.loads(l)["id"]: json.loads(l) for l in open(src / "manifests/cases.jsonl") if l.strip()}
results = []
EXTRA = {}


def tiny(tmp, records, profile_edit=None, profile="yamlprims-core.r2"):
    """A corpus of the given case records: empty hydration, real profiles and fixtures,
    plus the probe fixtures in EXTRA."""
    s, h = tmp / "src", tmp / "hyd"
    shutil.rmtree(s, ignore_errors=True)
    shutil.rmtree(h, ignore_errors=True)
    (s / "manifests").mkdir(parents=True)
    h.mkdir()
    shutil.copytree(src / "manifests/profiles", s / "manifests/profiles")
    lp = "manifests/profiles/yamlprims-core-limits.r1.json"
    if not (s / lp).exists():
        shutil.copy(limits_src / lp, s / lp)
    shutil.copytree(src / "fixtures", s / "fixtures")
    (h / "cases.jsonl").write_text("")
    (s / "manifests/cases.jsonl").write_text("".join(json.dumps(r) + "\n" for r in records))
    for rel, data in EXTRA.items():
        (s / rel).parent.mkdir(parents=True, exist_ok=True)
        (s / rel).write_bytes(data)
    if profile_edit:
        f = s / f"manifests/profiles/{profile}.json"
        d = json.loads(f.read_text())
        profile_edit(d)
        f.write_text(json.dumps(d))
    return h, s


def drive(label, records, want_ok, profile_edit=None, profile="yamlprims-core.r2"):
    with tempfile.TemporaryDirectory() as t:
        h, s = tiny(Path(t), records, profile_edit, profile)
        r = subprocess.run([str(driver), str(h), str(s)], capture_output=True, text=True)
    ok = (r.returncode == 0) == want_ok
    last = [l for l in r.stdout.splitlines() if l.startswith(("problem\t", "result\t"))]
    results.append((f"driver:{label}", r.returncode, ok, " | ".join(last)[:160]))


def edit(case_id, f):
    c = json.loads(json.dumps(cases[case_id]))
    f(c)
    return c


def first_value(c):
    return c["expectations"]["value"][0]


control = cases["yaml.documents.empty-stream"]
drive("control", [control], True)
drive("wrong-value", [edit("yaml.documents.empty-stream", lambda c: first_value(c).update(documents=[]))], False)
drive("wrong-error-phase", [edit("yaml.documents.two-explicit-empty", lambda c: first_value(c).update(error={"phase": "syntax"}))], False)
drive("wrong-error-kind", [edit("yaml.documents.two-explicit-empty", lambda c: first_value(c)["error"].update(kind="duplicate_key"))], False)
drive("value-instead-of-error", [edit("yaml.syntax.flow-under-indented", lambda c: (first_value(c).pop("error"), first_value(c).update(documents=[{"t": "null"}])))], False)
drive("empty-inventory", [], False)
drive("unknown-revision", [edit("yaml.documents.empty-stream", lambda c: first_value(c).update(profile_revision=999))], False)
drive("unknown-profile", [edit("yaml.documents.empty-stream", lambda c: first_value(c).update(profile="yamlprims-other"))], False)
drive("duplicate-id", [control, control], False)
drive("malformed-expectation", [edit("yaml.documents.empty-stream", lambda c: first_value(c).pop("documents"))], False)
drive("unknown-phase", [edit("yaml.documents.two-explicit-empty", lambda c: first_value(c)["error"].update(phase="parse"))], False)
drive("input-bytes", [edit("yaml.documents.empty-stream", lambda c: c["input"].update(bytes=1))], False)
drive("changed-profile-policy", [control], False, lambda d: d["policy"].update(merge_keys="apply"))

# Explicit-limits profile (yamlprims-core-limits r1, max_depth 8): a 12-deep flow sequence
# is a max_depth error with configured value 8.
EXTRA = {"fixtures/probe/deep.yaml": b"[" * 12 + b"x" + b"]" * 12 + b"\n"}


def limits_case(error):
    data = EXTRA["fixtures/probe/deep.yaml"]
    exp = {"profile": "yamlprims-core-limits", "profile_revision": 1, "error": error}
    return {"id": "probe.limits.deep", "input": {"path": "fixtures/probe/deep.yaml", "bytes": len(data)}, "expectations": {"value": [exp]}}


LIMITS = "yamlprims-core-limits.r1"
deep = {"phase": "limit", "kind": "limit_exceeded", "limit": "max_depth", "configured": 8}
drive("limits-control", [limits_case(deep)], True)
drive("limits-wrong-configured", [limits_case({**deep, "configured": 9})], False)
drive("limits-wrong-limit", [limits_case({**deep, "limit": "max_nodes"})], False)
drive("limits-unknown-field", [limits_case(deep)], False, lambda d: d["limits"].update(max_cpu=1), LIMITS)
drive("limits-missing-field", [limits_case(deep)], False, lambda d: d["limits"].pop("max_warnings"), LIMITS)
drive("limits-invalid", [limits_case(deep)], False, lambda d: d["limits"].update(max_scalar_bytes=10**6), LIMITS)
drive("limits-removed", [limits_case(deep)], False, lambda d: d.pop("limits"), LIMITS)

# max_input_bytes (65,536 in the limits profile) is phase `input`. The expectation cannot
# name the limit: the case schema allows `limit` only with phase `limit`.
lim = json.loads((limits_src / "manifests/profiles/yamlprims-core-limits.r1.json").read_text())["limits"]["max_input_bytes"]
head = b"a: 1\n#"
EXTRA["fixtures/probe/input-at.yaml"] = head + b"x" * (lim - len(head) - 1) + b"\n"
EXTRA["fixtures/probe/input-over.yaml"] = head + b"x" * (lim - len(head)) + b"\n"
assert len(EXTRA["fixtures/probe/input-at.yaml"]) == lim and len(EXTRA["fixtures/probe/input-over.yaml"]) == lim + 1


def input_case(name, exp):
    data = EXTRA[f"fixtures/probe/{name}.yaml"]
    exp = {"profile": "yamlprims-core-limits", "profile_revision": 1, **exp}
    return {"id": f"probe.limits.{name}", "input": {"path": f"fixtures/probe/{name}.yaml", "bytes": len(data)}, "expectations": {"value": [exp]}}


a1 = {"documents": [{"t": "map", "entries": [[{"t": "str", "v": "a"}, {"t": "int", "v": "1"}]]}]}
drive("input-at-limit-loads", [input_case("input-at", a1)], True)
drive("input-over-is-phase-input", [input_case("input-over", {"error": {"phase": "input", "kind": "limit_exceeded"}})], True)
drive("input-over-not-phase-limit", [input_case("input-over", {"error": {"phase": "limit", "limit": "max_input_bytes"}})], False)
drive("input-at-not-error", [input_case("input-at", {"error": {"phase": "input"}})], False)


def gate(label, h, s):
    r = subprocess.run([sys.executable, "-I", str(ROOT / "scripts/structcorpus_gate.py"), str(pin), str(h), str(s), str(driver)], capture_output=True, text=True)
    fails = [l for l in r.stdout.splitlines() if l.startswith("FAIL")]
    ok = (r.returncode == 0) == (label == "control")
    results.append((f"gate:{label}", r.returncode, ok, " | ".join(fails)[:160] or r.stdout.strip()[-120:]))


with tempfile.TemporaryDirectory() as t:
    t = Path(t)
    gate("control", hyd, src)
    # Missing inventory: one hydrated case removed; inputs shared by symlink.
    h = t / "hyd"
    h.mkdir()
    (h / "inputs").symlink_to(hyd / "inputs")
    shutil.copy(hyd / ".structcorpus-hydrated.json", h)
    lines = (hyd / "cases.jsonl").read_text().splitlines(keepends=True)
    (h / "cases.jsonl").write_text("".join(lines[:-1]))
    gate("missing-case", h, src)
    # Altered source: a fixture byte changed in a copied checkout.
    s = t / "src"
    shutil.copytree(src, s, symlinks=True)
    f = s / "fixtures/yaml/indent-flow-indented.yaml"
    f.write_bytes(f.read_bytes().replace(b"a", b"x", 1))
    gate("altered-fixture", hyd, s)

bad = 0
print("probe\texit\tas_expected\tdetail")
for label, code, ok, detail in results:
    bad += not ok
    print(f"{label}\t{code}\t{'yes' if ok else 'NO'}\t{detail}")
sys.exit(1 if bad else 0)
