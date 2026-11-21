#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Write per-target seed corpora for the Rust fuzz targets.

Usage: python3 -I fuzz/seeds.py OUT_DIR

Seeds come from conformance/vectors/cases.jsonl (load, emit_round_trip,
encode: one file per input and options selector byte; lint_yaml11: the raw
input) and crates/yamlprims/tests/fixtures/wire/*.json (decode_wire). Nothing
generated here is committed. OUT_DIR must be outside the working tree and
must not exist (or be an empty directory); nothing existing is adopted.
"""
import json
import os
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
out = Path(sys.argv[1]).resolve()


def same(a, b):
    try:
        return os.path.samefile(a, b)
    except OSError:
        return False


# Containment by filesystem identity, not path strings (case aliases, links).
if any(same(a, root) for a in (out, *out.parents) if a.exists()) or (
    out.exists() and any(same(a, out) for a in (root, *root.parents))
):
    sys.exit("seeds.py: OUT_DIR must be outside the working tree")

# OUT_DIR is created here, or must be an empty real directory: existing
# content (including symlinks) is never adopted.
try:
    os.mkdir(out, 0o700)
except FileExistsError:
    if out.is_symlink() or not out.is_dir() or any(out.iterdir()):
        sys.exit("seeds.py: OUT_DIR must not exist or must be an empty directory")

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
    os.mkdir(d, 0o700)  # fails if anything is already there
    for i, data in enumerate(items):
        fd = os.open(d / f"seed-{i:04d}", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, "wb") as f:
            f.write(data)
    return len(items)


counts = {}
selected = [bytes([sel]) + data for data in inputs for sel in range(4)]
for target in ("load", "emit_round_trip", "encode"):
    counts[target] = write(target, selected)
counts["lint_yaml11"] = write("lint_yaml11", inputs)
wire = sorted((root / "crates" / "yamlprims" / "tests" / "fixtures" / "wire").glob("*.json"))
counts["decode_wire"] = write("decode_wire", [p.read_bytes() for p in wire])
print(" ".join(f"{k}={v}" for k, v in counts.items()))
