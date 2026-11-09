"""Controls for the driver's spec 1.4 profile bindings: each corruption must fail (exit != 0).

Usage: python3 -I conformance/scripts/structcorpus_probes_v14.py [--require-depth255] SOURCE_DIR DRIVER

SOURCE_DIR is a structcorpus checkout at c8b7421 or later. It must have the spec 1.4 profiles:
`yamlprims-core-depth128` r1, `yamlprims-core-entries2` r1, and the four r4 policy profiles
(`yamlprims-core`, `yamlprims-core-stream`, `yamlprims-core-merge-apply`,
`yamlprims-core-merge-as-key` r4). When `yamlprims-core-depth255` r1 is present (6b1266e or
later, or required via `--require-depth255`), 10 additional depth-255 ceiling controls run.

The four r4 profiles carry no corpus cases; these synthetic driver controls exercise their
strict policy, merge mode (`Reject`/`Apply`/`AsKey`), single/stream cardinality, and profile
file validation without adding synthetic rows to the frozen corpus pin.
"""
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

args = sys.argv[1:]
require_depth255 = False
if args and args[0] == "--require-depth255":
    require_depth255 = True
    args = args[1:]
src, driver = map(Path, args[:2])
cases = {
    json.loads(line)["id"]: json.loads(line)
    for line in (src / "manifests/cases.jsonl").read_bytes().split(b"\n")
    if line.strip()
}
results = []
EXTRA = {
    "fixtures/probe/deep128.yaml": b"[" * 128 + b"0" + b"]" * 128 + b"\n",
    "fixtures/probe/deep129.yaml": b"[" * 129 + b"0" + b"]" * 129 + b"\n",
    "fixtures/probe/deep255.yaml": b"[" * 255 + b"0" + b"]" * 255 + b"\n",
    "fixtures/probe/deep256.yaml": b"[" * 256 + b"0" + b"]" * 256 + b"\n",
    "fixtures/probe/entries2.yaml": b"a: 1\nb: 2\n",
    "fixtures/probe/entries3.yaml": b"a: 1\nb: 2\nc: 3\n",
    "fixtures/probe/stream2.yaml": b"---\na\n---\nb\n",
    "fixtures/probe/merge-apply.yaml": b"base: &b\n  a: 1\nout:\n  <<: *b\n  b: 2\n",
    "fixtures/probe/merge-key.yaml": b"<<: 1\n",
}


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


def drive(label, records, want_ok, edit_file=None):
    with tempfile.TemporaryDirectory() as t:
        h, s = tiny(Path(t), records, edit_file)
        r = subprocess.run([str(driver), str(h), str(s)], capture_output=True, text=True)
    ok = (r.returncode == 0) == want_ok
    last = [l for l in r.stdout.splitlines() if l.startswith(("problem\t", "result\t"))]
    results.append((label, r.returncode, ok, " | ".join(last)[:200]))


def probe_case(name, profile, rev, exp):
    data = EXTRA[f"fixtures/probe/{name}.yaml"]
    return {
        "id": f"probe.{name}.{profile}.r{rev}",
        "input": {"path": f"fixtures/probe/{name}.yaml", "bytes": len(data)},
        "expectations": {"value": [{"profile": profile, "profile_revision": rev, **exp}]},
    }


def nested_seq(depth):
    node = {"t": "int", "v": "0"}
    for _ in range(depth):
        node = {"t": "seq", "items": [node]}
    return {"documents": [node]}


def limit_err(limit_name, configured):
    return {
        "error": {
            "phase": "limit",
            "kind": "limit_exceeded",
            "limit": limit_name,
            "configured": configured,
        }
    }


def struct_err(kind):
    return {"error": {"phase": "structure", "kind": kind}}


def rebind_first(case_id, profile, rev):
    c = json.loads(json.dumps(cases[case_id]))
    c["expectations"].pop("node_locations", None)
    c["expectations"].pop("typed", None)
    exp = c["expectations"]["value"][0]
    exp["profile"] = profile
    exp["profile_revision"] = rev
    c["expectations"]["value"] = [exp]
    return c


DEPTH128_FILE = "manifests/profiles/yamlprims-core-depth128.r1.json"
DEPTH255_FILE = "manifests/profiles/yamlprims-core-depth255.r1.json"
ENTRIES2_FILE = "manifests/profiles/yamlprims-core-entries2.r1.json"

has_depth255 = (src / DEPTH255_FILE).is_file()
if require_depth255 and not has_depth255:
    sys.exit(f"missing required profile file: {src / DEPTH255_FILE}")

# 1. yamlprims-core-depth128 r1 (explicit 12 limits, max_depth 128, strict policy).
drive(
    "depth128-at-limit",
    [probe_case("deep128", "yamlprims-core-depth128", 1, nested_seq(128))],
    True,
)
drive(
    "depth128-over-limit",
    [probe_case("deep129", "yamlprims-core-depth128", 1, limit_err("max_depth", 128))],
    True,
)
drive(
    "depth128-strict-indent",
    [rebind_first("yaml.syntax.flow-under-indented", "yamlprims-core-depth128", 1)],
    True,
)
drive(
    "depth128-wrong-configured",
    [probe_case("deep129", "yamlprims-core-depth128", 1, limit_err("max_depth", 127))],
    False,
)
drive(
    "depth128-wrong-limit",
    [probe_case("deep129", "yamlprims-core-depth128", 1, limit_err("max_nodes", 128))],
    False,
)
drive(
    "depth128-unknown-field",
    [probe_case("deep129", "yamlprims-core-depth128", 1, limit_err("max_depth", 128))],
    False,
    (DEPTH128_FILE, lambda d: d["limits"].update(max_cpu=1)),
)
drive(
    "depth128-missing-field",
    [probe_case("deep129", "yamlprims-core-depth128", 1, limit_err("max_depth", 128))],
    False,
    (DEPTH128_FILE, lambda d: d["limits"].pop("max_warnings")),
)
drive(
    "depth128-invalid-limit",
    [probe_case("deep129", "yamlprims-core-depth128", 1, limit_err("max_depth", 128))],
    False,
    (DEPTH128_FILE, lambda d: d["limits"].update(max_scalar_bytes=10**6)),
)
drive(
    "depth128-removed-limits",
    [probe_case("deep129", "yamlprims-core-depth128", 1, limit_err("max_depth", 128))],
    False,
    (DEPTH128_FILE, lambda d: d.pop("limits")),
)
drive(
    "depth128-changed-policy",
    [probe_case("deep129", "yamlprims-core-depth128", 1, limit_err("max_depth", 128))],
    False,
    (DEPTH128_FILE, lambda d: d["policy"].pop("flow_indentation")),
)

# 1b. yamlprims-core-depth255 r1 (explicit 12 limits, max_depth 255, strict policy).
if has_depth255:
    drive(
        "depth255-at-limit",
        [probe_case("deep255", "yamlprims-core-depth255", 1, nested_seq(255))],
        True,
    )
    drive(
        "depth255-over-limit",
        [probe_case("deep256", "yamlprims-core-depth255", 1, limit_err("max_depth", 255))],
        True,
    )
    drive(
        "depth255-strict-indent",
        [rebind_first("yaml.syntax.flow-under-indented", "yamlprims-core-depth255", 1)],
        True,
    )
    drive(
        "depth255-wrong-configured",
        [probe_case("deep256", "yamlprims-core-depth255", 1, limit_err("max_depth", 254))],
        False,
    )
    drive(
        "depth255-wrong-limit",
        [probe_case("deep256", "yamlprims-core-depth255", 1, limit_err("max_nodes", 255))],
        False,
    )
    drive(
        "depth255-unknown-field",
        [probe_case("deep256", "yamlprims-core-depth255", 1, limit_err("max_depth", 255))],
        False,
        (DEPTH255_FILE, lambda d: d["limits"].update(max_cpu=1)),
    )
    drive(
        "depth255-missing-field",
        [probe_case("deep256", "yamlprims-core-depth255", 1, limit_err("max_depth", 255))],
        False,
        (DEPTH255_FILE, lambda d: d["limits"].pop("max_warnings")),
    )
    drive(
        "depth255-invalid-limit",
        [probe_case("deep256", "yamlprims-core-depth255", 1, limit_err("max_depth", 255))],
        False,
        (DEPTH255_FILE, lambda d: d["limits"].update(max_scalar_bytes=10**6)),
    )
    drive(
        "depth255-removed-limits",
        [probe_case("deep256", "yamlprims-core-depth255", 1, limit_err("max_depth", 255))],
        False,
        (DEPTH255_FILE, lambda d: d.pop("limits")),
    )
    drive(
        "depth255-changed-policy",
        [probe_case("deep256", "yamlprims-core-depth255", 1, limit_err("max_depth", 255))],
        False,
        (DEPTH255_FILE, lambda d: d["policy"].pop("flow_indentation")),
    )

# 2. yamlprims-core-entries2 r1 (explicit 12 limits, max_mapping_entries 2, strict policy).
map2 = {
    "documents": [
        {
            "t": "map",
            "entries": [
                [{"t": "str", "v": "a"}, {"t": "int", "v": "1"}],
                [{"t": "str", "v": "b"}, {"t": "int", "v": "2"}],
            ],
        }
    ]
}
drive(
    "entries2-at-limit",
    [probe_case("entries2", "yamlprims-core-entries2", 1, map2)],
    True,
)
drive(
    "entries2-over-limit",
    [probe_case("entries3", "yamlprims-core-entries2", 1, limit_err("max_mapping_entries", 2))],
    True,
)
drive(
    "entries2-strict-indent",
    [rebind_first("yaml.syntax.flow-under-indented", "yamlprims-core-entries2", 1)],
    True,
)
drive(
    "entries2-wrong-configured",
    [probe_case("entries3", "yamlprims-core-entries2", 1, limit_err("max_mapping_entries", 3))],
    False,
)
drive(
    "entries2-wrong-limit",
    [probe_case("entries3", "yamlprims-core-entries2", 1, limit_err("max_nodes", 2))],
    False,
)
drive(
    "entries2-unknown-field",
    [probe_case("entries3", "yamlprims-core-entries2", 1, limit_err("max_mapping_entries", 2))],
    False,
    (ENTRIES2_FILE, lambda d: d["limits"].update(max_cpu=1)),
)
drive(
    "entries2-missing-field",
    [probe_case("entries3", "yamlprims-core-entries2", 1, limit_err("max_mapping_entries", 2))],
    False,
    (ENTRIES2_FILE, lambda d: d["limits"].pop("max_documents")),
)
drive(
    "entries2-invalid-limit",
    [probe_case("entries3", "yamlprims-core-entries2", 1, limit_err("max_mapping_entries", 2))],
    False,
    (ENTRIES2_FILE, lambda d: d["limits"].update(max_key_bytes=10**6)),
)
drive(
    "entries2-removed-limits",
    [probe_case("entries3", "yamlprims-core-entries2", 1, limit_err("max_mapping_entries", 2))],
    False,
    (ENTRIES2_FILE, lambda d: d.pop("limits")),
)
drive(
    "entries2-changed-policy",
    [probe_case("entries3", "yamlprims-core-entries2", 1, limit_err("max_mapping_entries", 2))],
    False,
    (ENTRIES2_FILE, lambda d: d["policy"].update(duplicate_keys="allow")),
)

# 3. Four r4 policy bindings (synthetic dispatch/policy controls only; 0 shipped corpus cases).
CORE_R4 = "manifests/profiles/yamlprims-core.r4.json"
STREAM_R4 = "manifests/profiles/yamlprims-core-stream.r4.json"
APPLY_R4 = "manifests/profiles/yamlprims-core-merge-apply.r4.json"
ASKEY_R4 = "manifests/profiles/yamlprims-core-merge-as-key.r4.json"

drive(
    "r4-core-control",
    [
        probe_case("entries2", "yamlprims-core", 4, map2),
        probe_case("merge-apply", "yamlprims-core", 4, struct_err("merge_rejected")),
        probe_case("stream2", "yamlprims-core", 4, struct_err("multiple_documents")),
        rebind_first("yaml.syntax.flow-under-indented", "yamlprims-core", 4),
    ],
    True,
)
drive(
    "r4-core-wrong-value",
    [probe_case("entries2", "yamlprims-core", 4, {"documents": [{"t": "null"}]})],
    False,
)
drive(
    "r4-core-changed-policy",
    [probe_case("entries2", "yamlprims-core", 4, map2)],
    False,
    (CORE_R4, lambda d: d["policy"].update(merge_keys="apply")),
)
drive(
    "r4-core-unexpected-limits",
    [probe_case("entries2", "yamlprims-core", 4, map2)],
    False,
    (
        CORE_R4,
        lambda d: d.update(
            limits=json.loads((src / DEPTH128_FILE).read_text())["limits"]
        ),
    ),
)

drive(
    "r4-stream-control",
    [
        probe_case(
            "stream2",
            "yamlprims-core-stream",
            4,
            {"documents": [{"t": "str", "v": "a"}, {"t": "str", "v": "b"}]},
        ),
        probe_case("merge-apply", "yamlprims-core-stream", 4, struct_err("merge_rejected")),
    ],
    True,
)
drive(
    "r4-stream-changed-policy",
    [
        probe_case(
            "stream2",
            "yamlprims-core-stream",
            4,
            {"documents": [{"t": "str", "v": "a"}, {"t": "str", "v": "b"}]},
        )
    ],
    False,
    (STREAM_R4, lambda d: d["policy"].update(documents="single")),
)

merged_doc = {
    "documents": [
        {
            "t": "map",
            "entries": [
                [
                    {"t": "str", "v": "base"},
                    {"t": "map", "entries": [[{"t": "str", "v": "a"}, {"t": "int", "v": "1"}]]},
                ],
                [
                    {"t": "str", "v": "out"},
                    {
                        "t": "map",
                        "entries": [
                            [{"t": "str", "v": "a"}, {"t": "int", "v": "1"}],
                            [{"t": "str", "v": "b"}, {"t": "int", "v": "2"}],
                        ],
                    },
                ],
            ],
        }
    ]
}
drive(
    "r4-merge-apply-control",
    [
        probe_case("merge-apply", "yamlprims-core-merge-apply", 4, merged_doc),
        probe_case("stream2", "yamlprims-core-merge-apply", 4, struct_err("multiple_documents")),
    ],
    True,
)
drive(
    "r4-merge-apply-changed-policy",
    [probe_case("merge-apply", "yamlprims-core-merge-apply", 4, merged_doc)],
    False,
    (APPLY_R4, lambda d: d["policy"].update(merge_keys="reject")),
)

askey_doc = {
    "documents": [
        {"t": "map", "entries": [[{"t": "str", "v": "<<"}, {"t": "int", "v": "1"}]]}
    ]
}
drive(
    "r4-merge-as-key-control",
    [
        probe_case("merge-key", "yamlprims-core-merge-as-key", 4, askey_doc),
        probe_case("stream2", "yamlprims-core-merge-as-key", 4, struct_err("multiple_documents")),
    ],
    True,
)
drive(
    "r4-merge-as-key-changed-policy",
    [probe_case("merge-key", "yamlprims-core-merge-as-key", 4, askey_doc)],
    False,
    (ASKEY_R4, lambda d: d["policy"].update(merge_keys="apply")),
)

def drive_raw_line(label, line_text, want_ok):
    with tempfile.TemporaryDirectory() as t:
        h, s = tiny(Path(t), [])
        (s / "manifests/cases.jsonl").write_text(line_text + "\n")
        r = subprocess.run([str(driver), str(h), str(s)], capture_output=True, text=True)
    ok = (r.returncode == 0) == want_ok
    last = [l for l in r.stdout.splitlines() if l.startswith(("problem\t", "result\t"))]
    results.append((label, r.returncode, ok, " | ".join(last)[:200]))


# node_locations remains bound to spec 1.3 profiles only.
sib_r4 = json.loads(json.dumps(cases["yaml.anchors.after-sibling"]))
sib_r4["expectations"]["value"][0]["profile_revision"] = 4
sib_r4["expectations"]["node_locations"]["profile_revision"] = 4
drive("r4-node-locations-rejected", [sib_r4], False)

# 4. Paired shallow/deep JSON transport controls (strict RFC 8259 whitespace and 1024-depth cap).
base_case = probe_case("entries2", "yamlprims-core", 4, map2)
base_json = json.dumps(base_case, separators=(",", ":"))
for kind, nest in [("shallow", 1), ("deep", 130)]:
    pad = "[" * nest + "0" + "]" * nest
    text = '{"padding":' + pad + "," + base_json[1:]
    drive_raw_line(f"json-{kind}-valid-ws", text + " \t\r", True)
    drive_raw_line(f"json-{kind}-vtab", text.replace(',"id"', ",\x0b\"id\"", 1), False)
    drive_raw_line(f"json-{kind}-nbsp", text.replace(',"id"', ",\u00a0\"id\"", 1), False)
    drive_raw_line(f"json-{kind}-trailing-nbsp", text + "\u00a0", False)

for nest, want_ok in [(1024, True), (1025, False)]:
    pad = "[" * nest + "]" * nest
    text = '{"padding":' + pad + "," + base_json[1:]
    drive_raw_line(
        f"json-depth{nest}-{'at-limit' if want_ok else 'over-limit'}", text, want_ok
    )

bad = 0
print("probe\texit\tas_expected\tdetail")
for label, code, ok, detail in results:
    bad += not ok
    print(f"{label}\t{code}\t{'yes' if ok else 'NO'}\t{detail}")
sys.exit(1 if bad else 0)
