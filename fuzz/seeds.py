#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Write per-target seed corpora for the Rust fuzz targets.

Usage: python3 -I fuzz/seeds.py OUT_DIR

Seeds come from conformance/vectors/cases.jsonl (load, emit_round_trip,
encode: one file per input and options selector byte; lint_yaml11: the raw
input) and crates/yamlprims/tests/fixtures/wire/*.json (decode_wire). Nothing
generated here is committed; OUT_DIR must be outside the working tree.
"""
import json
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
out = Path(sys.argv[1]).resolve()
if out == root or root in out.parents:
    sys.exit("seeds.py: OUT_DIR must be outside the working tree")

inputs = []
for line in (root / "conformance" / "vectors" / "cases.jsonl").read_text(encoding="utf-8").splitlines():
    if not line.strip():
        continue
    rec = json.loads(line)
    if "input" in rec:
        inputs.append(rec["input"].encode("utf-8"))
    elif "input_hex" in rec:
        inputs.append(bytes.fromhex(rec["input_hex"]))
if not inputs:
    sys.exit("seeds.py: no conformance inputs")


def write(target, items):
    d = out / target
    d.mkdir(parents=True, exist_ok=True)
    for i, data in enumerate(items):
        (d / f"seed-{i:04d}").write_bytes(data)
    return len(items)


counts = {}
selected = [bytes([sel]) + data for data in inputs for sel in range(4)]
for target in ("load", "emit_round_trip", "encode"):
    counts[target] = write(target, selected)
counts["lint_yaml11"] = write("lint_yaml11", inputs)
wire = sorted((root / "crates" / "yamlprims" / "tests" / "fixtures" / "wire").glob("*.json"))
counts["decode_wire"] = write("decode_wire", [p.read_bytes() for p in wire])
print(" ".join(f"{k}={v}" for k, v in counts.items()))
