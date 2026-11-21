# SPDX-License-Identifier: MIT OR Apache-2.0
"""Rust/Go differential over fuzz corpora (run through scripts/fuzz-differential.sh).

Usage:
  python3 -I scripts/fuzz_differential.py run KNOWN RUST_DRIVER GO_DRIVER OUT MODE:DIR...
  python3 -I scripts/fuzz_differential.py check-known KNOWN
  python3 -I scripts/fuzz_differential.py resolve-out ROOT PATH

Output never contains input bytes. A failure prints an input SHA-256 and a
fixed operation or failure kind; full details go to OUT.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

OPTIONS = {
    0: ("default", {"merge": "reject", "unknown_tags": "preserve"}),
    1: ("ijson-default", {"merge": "reject", "unknown_tags": "reject", "limits": {"max_output_bytes": 16 << 20}}),
    2: ("merge-as-key", {"merge": "as-key", "unknown_tags": "preserve"}),
    3: ("merge-apply", {"merge": "apply", "unknown_tags": "preserve"}),
}
OPTION_NAMES = {name for name, _ in OPTIONS.values()}
# Cross-language comparisons: the same output field from both twins.
CROSS = ("wire", "plain", "emit", "ijson", "lint")
OPERATIONS = set(CROSS) | {"rust-roundtrip", "go-roundtrip", "rust-driver-error", "go-driver-error"}
# Only these statuses suppress a difference; "closed" entries are kept as history.
SUPPRESSING = {"open", "accepted"}
STATUSES = SUPPRESSING | {"closed"}
KNOWN_FIELDS = {"id", "input_sha256", "operation", "options", "revisions", "owner", "status", "note"}
DIGEST = re.compile(r"^[0-9a-f]{64}$")
REVISION = re.compile(r"^(?:[0-9a-f]{7,40}|v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?)$")

GO_HEADER = b"go test fuzz v1\n"
ESC = {"a": 7, "b": 8, "f": 12, "n": 10, "r": 13, "t": 9, "v": 11, "\\": 92, '"': 34, "'": 39}


class CorpusError(Exception):
    """A corpus file that cannot be decoded. Carries no input bytes."""


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
            v = int(s[i + 1:i + 4], 8)
            if v > 255:
                raise CorpusError
            out.append(v)
            i += 4
        else:
            raise CorpusError
    return bytes(out)


def decode_corpus_file(raw):
    if not raw.startswith(GO_HEADER):
        return raw
    line = raw[len(GO_HEADER):].decode("utf-8").strip()
    for prefix in ("[]byte(", "string("):
        if line.startswith(prefix) and line.endswith(")"):
            lit = line[len(prefix):-1]
            if len(lit) >= 2 and lit[0] == lit[-1] == '"':
                return go_unquote(lit[1:-1])
            if len(lit) >= 2 and lit[0] == lit[-1] == "`":
                return lit[1:-1].encode("utf-8")
    raise CorpusError


def read_input(raw):
    """Decode a corpus file. Any failure becomes CorpusError, whose message
    and traceback never carry the input."""
    try:
        return decode_corpus_file(raw)
    except Exception:
        raise CorpusError from None


def load_known(path):
    """Validate known-differences.jsonl. Returns the suppressing keys."""
    keys = set()
    errors = []
    for n, line in enumerate(Path(path).read_text(encoding="utf-8").split("\n"), 1):
        if not line.strip():
            continue
        try:
            k = json.loads(line)
        except json.JSONDecodeError:
            errors.append(f"line {n}: not JSON")
            continue
        if not isinstance(k, dict):
            errors.append(f"line {n}: not an object")
            continue
        unknown = sorted(set(k) - KNOWN_FIELDS)
        if unknown:
            errors.append(f"line {n}: unknown fields {unknown}")
        if not (isinstance(k.get("id"), str) and k["id"].strip()):
            errors.append(f"line {n}: id must be a non-empty string")
        if not (isinstance(k.get("input_sha256"), str) and DIGEST.match(k["input_sha256"])):
            errors.append(f"line {n}: input_sha256 must be one lowercase SHA-256 digest")
        if k.get("operation") not in OPERATIONS:
            errors.append(f"line {n}: operation must be one of {sorted(OPERATIONS)}")
        if k.get("options") not in OPTION_NAMES:
            errors.append(f"line {n}: options must be one of {sorted(OPTION_NAMES)}")
        revs = k.get("revisions")
        if not (isinstance(revs, list) and revs and all(isinstance(r, str) and REVISION.match(r) for r in revs)):
            errors.append(f"line {n}: revisions must be a non-empty list of commit ids or vX.Y.Z tags")
        if not (isinstance(k.get("owner"), str) and k["owner"].strip()):
            errors.append(f"line {n}: owner must be a non-empty string")
        if k.get("status") not in STATUSES:
            errors.append(f"line {n}: status must be one of {sorted(STATUSES)}")
        if "note" in k and not isinstance(k["note"], str):
            errors.append(f"line {n}: note must be a string")
        if not errors and k["status"] in SUPPRESSING:
            keys.add((k["input_sha256"], k["options"], k["operation"]))
    if errors:
        sys.exit("known-differences: " + "; ".join(errors))
    return keys


def compare(r, g):
    """Operations that differ for one record. Each twin's round trip is checked
    against its own wire; the cross-language wire check is separate."""
    errs = [f"{who}-driver-error" for who, x in (("rust", r), ("go", g)) if "driver_error" in x]
    if errs:
        return errs
    ops = [f for f in CROSS if r.get(f, "") != g.get(f, "")]
    if r["roundtrip"] != r["wire"]:
        ops.append("rust-roundtrip")
    if g["roundtrip"] != g["wire"]:
        ops.append("go-roundtrip")
    return ops


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


def run_driver(driver, recs):
    """The drivers stop a batch when their own wire round trip fails. Bisect
    the batch so the failing records are isolated and reported."""
    lines, err = run_batch(driver, recs)
    if lines is not None:
        return lines
    if len(recs) == 1:
        return [{"id": recs[0]["id"], "driver_error": err.strip()}]
    mid = len(recs) // 2
    return run_driver(driver, recs[:mid]) + run_driver(driver, recs[mid:])


def main_run(known_path, rust_driver, go_driver, out, specs):
    known = load_known(known_path)
    records, meta, found, details = [], [], [], []
    for spec in specs:
        mode, _, d = spec.partition(":")
        if mode not in ("selector", "raw") or not d:
            sys.exit(f"bad corpus spec {spec!r}: want selector:DIR or raw:DIR")
        for path in sorted(p for p in Path(d).iterdir() if p.is_file()):
            raw = path.read_bytes()
            try:
                data = read_input(raw)
            except CorpusError:
                digest = hashlib.sha256(raw).hexdigest()
                found.append((f"{digest}:file", "corpus-decode"))
                details.append({"id": f"{digest}:file", "operation": "corpus-decode"})
                continue
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
    if not records and not found:
        sys.exit("fuzz-differential: no inputs")
    rust = run_driver(rust_driver, records) if records else []
    go = run_driver(go_driver, records) if records else []
    for (digest, name), r, g in zip(meta, rust, go):
        for op in compare(r, g):
            if (digest, name, op) in known:
                continue
            found.append((r["id"], op))
            details.append({"id": r["id"], "operation": op, "rust": r, "go": g})
    if found:
        report = Path(out) / "differential-failures.jsonl"
        report.write_text("".join(json.dumps(d) + "\n" for d in details), encoding="utf-8")
        for rid, op in found[:20]:
            print(f"DIFF {op}: {rid}", file=sys.stderr)
        sys.exit(f"fuzz-differential FAILED: {len(found)} unlisted differences in {len(records)} records; details in {report}")
    print(f"fuzz-differential PASS: {len(records)} records ({len(records) // 2} inputs), no unlisted Rust/Go differences")


def _ancestors(path):
    """PATH and each of its parents, nearest first."""
    while True:
        yield path
        parent = os.path.dirname(path)
        if parent == path:
            return
        path = parent


def _same(a, b):
    try:
        return os.path.samefile(a, b)
    except OSError:
        return False


def resolve_out(root, path):
    """Resolve PATH (symlinks, ".", "..", relative to the current directory)
    and refuse it if it is inside ROOT or ROOT is inside it. Containment is
    decided by filesystem identity (device and inode), not by comparing path
    strings, so case aliases and other spellings of the same directory are
    caught. Nothing is created here."""
    root_real = os.path.realpath(root)
    out_real = os.path.realpath(path)
    # Out is ROOT or below it: some existing ancestor of out is ROOT.
    if any(_same(a, root_real) for a in _ancestors(out_real) if os.path.exists(a)):
        sys.exit("output directory must be outside the working tree")
    # ROOT is below out: some ancestor of ROOT is out.
    if os.path.exists(out_real) and any(_same(a, out_real) for a in _ancestors(root_real)):
        sys.exit("output directory must be outside the working tree")
    print(out_real)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "resolve-out":
        resolve_out(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 3 and sys.argv[1] == "check-known":
        print(f"known-differences: {len(load_known(sys.argv[2]))} suppressing entries")
    elif len(sys.argv) >= 7 and sys.argv[1] == "run":
        main_run(*sys.argv[2:6], sys.argv[6:])
    else:
        sys.exit(__doc__)
