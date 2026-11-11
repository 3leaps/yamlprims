#!/usr/bin/env bash
# Differential parity gate between the Rust reference implementation
# (crates/yamlprims) and the pure-Go twin (github.com/3leaps/yamlprims/yaml).
#
# Verifies byte-for-byte identity of:
#   1. yamlprims-value/1 wire output (Encode / EncodeError)
#   2. spec-v1.4 §11.4 wire round-trip (Rust wire -> Go DecodeWire -> Go Encode)
#   3. plain JSON output (EncodePlain)
# across conformance/vectors/cases.jsonl and structcorpus (when present).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

tmp=$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-crosscheck.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

cargo build --locked -p yamlprims --example structcorpus >/dev/null
rust_driver="${CARGO_TARGET_DIR:-$root/target}/debug/examples/structcorpus"
go_driver="$tmp/structcorpus-driver-go"
CGO_ENABLED=0 go build -o "$go_driver" ./cmd/structcorpus-driver

python=${PYTHON:-python3}
corpus_root="${STRUCTCORPUS_ROOT:-$root/../structcorpus}"

"$python" - "$root" "$tmp" "$rust_driver" "$go_driver" "$corpus_root" <<'PY'
import json
import os
import subprocess
import sys
from pathlib import Path

root, tmp, rust_driver, go_driver, corpus_root = (Path(p) for p in sys.argv[1:])

records = []
vector_file = root / "conformance" / "vectors" / "cases.jsonl"
for line in vector_file.read_text(encoding="utf-8").split("\n"):
    if line.strip():
        records.append(json.loads(line))
vector_count = len(records)

corpus_count = 0
if (corpus_root / "manifests" / "cases.jsonl").is_file() and (corpus_root / "tools" / "corpus.py").is_file():
    hyd_dir = tmp / "hydrated-smoke"
    subprocess.run(
        [sys.executable, "-I", str(corpus_root / "tools" / "corpus.py"), "hydrate", "--tier", "smoke", "--output", str(hyd_dir)],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    profiles = {}
    for pf in (corpus_root / "manifests" / "profiles").glob("*.json"):
        pdoc = json.loads(pf.read_text(encoding="utf-8"))
        profiles[(pdoc["id"], pdoc["revision"])] = pdoc

    for manifest in (hyd_dir / "cases.jsonl", corpus_root / "manifests" / "cases.jsonl"):
        for line in manifest.read_text(encoding="utf-8").split("\n"):
            if not line.strip():
                continue
            c = json.loads(line)
            cid = c["id"]
            inp = c["input"]
            if "path" in inp:
                p = hyd_dir / inp["path"] if (hyd_dir / inp["path"]).exists() else corpus_root / inp["path"]
            else:
                recipe, suffix = cid.rsplit(".", 1)
                p = hyd_dir / "inputs" / "yaml" / recipe / f"{suffix}.yaml"
            raw_hex = p.read_bytes().hex()
            for idx, exp in enumerate(c.get("expectations", {}).get("value", [])):
                prof = exp.get("profile")
                rev = exp.get("profile_revision")
                if (prof, rev) == ("yaml-core", 1):
                    continue
                pdoc = profiles.get((prof, rev))
                if not pdoc:
                    continue
                pol = pdoc["policy"]
                single = pol.get("documents") == "single"
                merge = pol.get("merge_keys", "reject")
                rec = {
                    "id": f"structcorpus:{cid}:{prof}.r{rev}:{idx}",
                    "input_hex": raw_hex,
                    "single": single,
                    "merge": merge,
                    "unknown_tags": "preserve",
                    "check_plain": single,
                }
                if "limits" in pdoc:
                    rec["limits"] = pdoc["limits"]
                records.append(rec)
                corpus_count += 1
elif os.environ.get("YAMLPRIMS_CROSSCHECK_REQUIRE_CORPUS") == "1":
    sys.exit(f"crosscheck error: YAMLPRIMS_CROSSCHECK_REQUIRE_CORPUS=1 but structcorpus was not found at {corpus_root}")

jsonl_payload = "\n".join(json.dumps(r) for r in records) + "\n"

rust_proc = subprocess.run(
    [str(rust_driver), "--crosscheck-jsonl"],
    input=jsonl_payload,
    capture_output=True,
    text=True,
)
if rust_proc.returncode != 0:
    sys.stderr.write(rust_proc.stderr)
    sys.exit(f"rust_driver exited with {rust_proc.returncode}")
go_proc = subprocess.run(
    [str(go_driver), "--crosscheck-jsonl"],
    input=jsonl_payload,
    capture_output=True,
    text=True,
)
if go_proc.returncode != 0:
    sys.stderr.write(go_proc.stderr)
    sys.exit(f"go_driver exited with {go_proc.returncode}")

rust_lines = [json.loads(l) for l in rust_proc.stdout.split("\n") if l.strip()]
go_lines = [json.loads(l) for l in go_proc.stdout.split("\n") if l.strip()]

if len(rust_lines) != len(records) or len(go_lines) != len(records):
    sys.exit(f"crosscheck count mismatch: records={len(records)} rust={len(rust_lines)} go={len(go_lines)}")

mismatches = []
for r, g in zip(rust_lines, go_lines):
    rid = r["id"]
    if r["id"] != g["id"]:
        mismatches.append(f"id mismatch: {r['id']} vs {g['id']}")
        continue
    if r["wire"] != g["wire"]:
        mismatches.append(f"{rid} [wire]:\n  rust: {r['wire']}\n  go:   {g['wire']}")
    if r["roundtrip"] != r["wire"]:
        mismatches.append(f"{rid} [rust-roundtrip]:\n  rust: {r['wire']}\n  rt:   {r['roundtrip']}")
    if g["roundtrip"] != r["wire"]:
        mismatches.append(f"{rid} [go-roundtrip]:\n  rust: {r['wire']}\n  rt:   {g['roundtrip']}")
    if r["plain"] != g["plain"]:
        mismatches.append(f"{rid} [plain]:\n  rust: {r['plain']}\n  go:   {g['plain']}")
    if r["emit"] != g["emit"]:
        mismatches.append(f"{rid} [emit]:\n  rust: {r['emit']!r}\n  go:   {g['emit']!r}")
    if r["ijson"] != g["ijson"]:
        mismatches.append(f"{rid} [ijson]:\n  rust: {r['ijson']}\n  go:   {g['ijson']}")

if mismatches:
    for m in mismatches[:20]:
        print("FAIL:", m, file=sys.stderr)
    sys.exit(f"crosscheck FAILED: {len(mismatches)} mismatches out of {len(records)} checks")

print(f"crosscheck PASS: {vector_count} local vectors + {corpus_count} structcorpus expectations ({len(records)} total) byte-identical across wire, DecodeWire round-trip, plain JSON, emit_all, and encode_ijson")
PY
