#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Rust/Go differential over fuzz corpora.
#
# Usage: scripts/fuzz-differential.sh MODE:DIR [MODE:DIR ...]
#        scripts/fuzz-differential.sh --seeds
#   MODE is "selector" (the first byte picks the load options, as in the
#   load/emit/encode fuzz targets) or "raw" (default options).
#   Files may be raw bytes or Go fuzz corpus files ("go test fuzz v1").
#
# Every input runs through the crosscheck drivers (Rust structcorpus example
# and cmd/structcorpus-driver) as a multi-document and a single-document
# record. Any byte difference in wire, plain, emit, I-JSON or lint output, or
# in either twin's wire round trip, fails unless fuzz/known-differences.jsonl
# lists that exact input digest, operation and options (see fuzz/README.md).
#
# Output never contains input bytes: a failure prints the input SHA-256 and
# the operation only. Full details go to $YAMLPRIMS_FUZZ_OUT (default: a fresh
# temporary directory), which must be outside the working tree.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[ "$#" -gt 0 ] || { echo "usage: $0 MODE:DIR [MODE:DIR ...]" >&2; exit 2; }

out=${YAMLPRIMS_FUZZ_OUT:-$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-fuzz-diff.XXXXXX")}
case "$out" in /*) ;; *) out="$PWD/$out" ;; esac
case "$out/" in
  "$PWD/"* | "$(pwd -P)/"*) echo "fuzz-differential: output directory must be outside the working tree" >&2; exit 2 ;;
esac
mkdir -p "$out"

tmp=$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-fuzz-diff-bin.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

if [ "$1" = --seeds ]; then
  # Differential over the committed seeds only.
  python3 -I fuzz/seeds.py "$tmp/seeds" >/dev/null
  set -- "selector:$tmp/seeds/load" "raw:$tmp/seeds/lint_yaml11"
fi

cargo build --quiet --locked -p yamlprims --example structcorpus
rust_driver="${CARGO_TARGET_DIR:-$root/target}/debug/examples/structcorpus"
go_driver="$tmp/structcorpus-driver-go"
GOWORK=off CGO_ENABLED=0 go build -o "$go_driver" ./cmd/structcorpus-driver

python3 -I - "$root/fuzz/known-differences.jsonl" "$rust_driver" "$go_driver" "$out" "$@" <<'PY'
import hashlib
import json
import subprocess
import sys
from pathlib import Path

known_path, rust_driver, go_driver, out = sys.argv[1:5]
specs = sys.argv[5:]

OPTIONS = {
    0: ("default", {"merge": "reject", "unknown_tags": "preserve"}),
    1: ("ijson-default", {"merge": "reject", "unknown_tags": "reject", "limits": {"max_output_bytes": 16 << 20}}),
    2: ("merge-as-key", {"merge": "as-key", "unknown_tags": "preserve"}),
    3: ("merge-apply", {"merge": "apply", "unknown_tags": "preserve"}),
}
GO_HEADER = b"go test fuzz v1\n"
ESC = {"a": 7, "b": 8, "f": 12, "n": 10, "r": 13, "t": 9, "v": 11, "\\": 92, '"': 34, "'": 39}


def go_unquote(s):
    """Decode one Go %q literal into bytes."""
    out = bytearray()
    i = 0
    while i < len(s):
        c = s[i]
        if c != "\\":
            out += c.encode("utf-8")
            i += 1
            continue
        e = s[i + 1]
        if e in ESC:
            out.append(ESC[e])
            i += 2
        elif e == "x":
            out.append(int(s[i + 2:i + 4], 16))
            i += 4
        elif e in "uU":
            n = 4 if e == "u" else 8
            out += chr(int(s[i + 2:i + 2 + n], 16)).encode("utf-8")
            i += 2 + n
        elif e in "01234567":
            out.append(int(s[i + 1:i + 4], 8))
            i += 4
        else:
            raise ValueError(f"unknown escape \\{e}")
    return bytes(out)


def read_input(path):
    data = path.read_bytes()
    if not data.startswith(GO_HEADER):
        return data
    line = data[len(GO_HEADER):].decode("utf-8").strip()
    for prefix in ("[]byte(", "string("):
        if line.startswith(prefix) and line.endswith(")"):
            lit = line[len(prefix):-1]
            if lit.startswith('"') and lit.endswith('"'):
                return go_unquote(lit[1:-1])
            if lit.startswith("`") and lit.endswith("`"):
                return lit[1:-1].encode("utf-8")
    raise ValueError(f"unsupported Go corpus entry in {path.name}")


REQUIRED = ("id", "input_sha256", "operation", "options", "revisions", "owner", "status")
known = set()
for n, line in enumerate(Path(known_path).read_text(encoding="utf-8").split("\n"), 1):
    if not line.strip():
        continue
    k = json.loads(line)
    missing = [f for f in REQUIRED if not k.get(f)]
    if missing:
        sys.exit(f"known-differences line {n}: missing {', '.join(missing)}")
    if k["options"] not in {o for o, _ in OPTIONS.values()}:
        sys.exit(f"known-differences line {n}: unknown options {k['options']!r}")
    if len(k["input_sha256"]) != 64:
        sys.exit(f"known-differences line {n}: input_sha256 must be one exact digest")
    known.add((k["input_sha256"], k["options"], k["operation"]))

records, meta = [], []
for spec in specs:
    mode, _, d = spec.partition(":")
    if mode not in ("selector", "raw") or not d:
        sys.exit(f"bad corpus spec {spec!r}: want selector:DIR or raw:DIR")
    for path in sorted(p for p in Path(d).iterdir() if p.is_file()):
        data = read_input(path)
        if mode == "selector":
            sel, data = (data[0] % 4, data[1:]) if data else (0, data)
        else:
            sel = 0
        name, opts = OPTIONS[sel]
        digest = hashlib.sha256(data).hexdigest()
        for single in (False, True):
            rid = f"{digest}:{name}:{'single' if single else 'stream'}"
            records.append({"id": rid, "input_hex": data.hex(), "single": single, "check_plain": single, **opts})
            meta.append((digest, name))

if not records:
    sys.exit("fuzz-differential: no inputs")


def run_batch(driver, recs):
    text = "".join(json.dumps(r) + "\n" for r in recs)
    p = subprocess.run([driver, "--crosscheck-jsonl"], input=text, capture_output=True, text=True)
    if p.returncode != 0:
        return None, p.stderr
    # Split on "\n" only: JSON may carry raw U+2028/U+2029, which splitlines() breaks on.
    lines = [json.loads(l) for l in p.stdout.split("\n") if l.strip()]
    if len(lines) != len(recs):
        sys.exit(f"{Path(driver).name}: {len(lines)} results for {len(recs)} records")
    return lines, ""


def run(driver, recs=None):
    """The drivers stop a batch when their own wire round trip fails. Bisect
    the batch so the failing records are isolated and reported, not the batch."""
    recs = records if recs is None else recs
    lines, err = run_batch(driver, recs)
    if lines is not None:
        return lines
    if len(recs) == 1:
        return [{"id": recs[0]["id"], "driver_error": err.strip()}]
    mid = len(recs) // 2
    return run(driver, recs[:mid]) + run(driver, recs[mid:])


rust, go = run(rust_driver), run(go_driver)
found, details = [], []
for (digest, name), r, g in zip(meta, rust, go):
    errs = [f"{who}-driver-error" for who, x in (("rust", r), ("go", g)) if "driver_error" in x]
    if errs:
        for f in errs:
            if (digest, name, f) not in known:
                found.append((r["id"], f))
                details.append({"id": r["id"], "operation": f, "rust": r, "go": g})
        continue
    fields = [f for f in ("wire", "plain", "emit", "ijson", "lint") if r.get(f, "") != g.get(f, "")]
    if r["roundtrip"] != r["wire"]:
        fields.append("rust-roundtrip")
    if g["roundtrip"] != r["wire"]:
        fields.append("go-roundtrip")
    for f in fields:
        if (digest, name, f) in known:
            continue
        found.append((r["id"], f))
        details.append({"id": r["id"], "operation": f, "rust": r, "go": g})

if found:
    report = Path(out) / "differential-failures.jsonl"
    report.write_text("".join(json.dumps(d) + "\n" for d in details), encoding="utf-8")
    for rid, f in found[:20]:
        print(f"DIFF {f}: {rid}", file=sys.stderr)
    sys.exit(f"fuzz-differential FAILED: {len(found)} unlisted differences in {len(records)} records; details in {report}")
print(f"fuzz-differential PASS: {len(records)} records ({len(records) // 2} inputs), no unlisted Rust/Go differences")
PY
