# SPDX-License-Identifier: MIT OR Apache-2.0
"""Rust/Go differential over fuzz corpora (run through scripts/fuzz-differential.sh).

Usage:
  python3 -I scripts/fuzz_differential.py run KNOWN RUST_DRIVER GO_DRIVER OUT MODE:DIR...
  python3 -I scripts/fuzz_differential.py check-known KNOWN
  python3 -I scripts/fuzz_differential.py resolve-out ROOT PATH
  python3 -I scripts/fuzz_differential.py claim-out ROOT PATH
  python3 -I scripts/fuzz_differential.py check-tree OUT
  python3 -I scripts/fuzz_differential.py mkdir-under OUT REL...
  python3 -I scripts/fuzz_differential.py hold-lock OUT -- CMD...
  python3 -I scripts/fuzz_differential.py lock-held OUT
  python3 -I scripts/fuzz_differential.py lock-owned OUT FD
  python3 -I scripts/fuzz_differential.py import-new OUT REL SRC

Output never contains input bytes. A failure prints an input SHA-256 and a
fixed operation or failure kind; full details go to OUT.
"""
import hashlib
import json
import os
import re
import stat
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
        report = write_new(out, "differential-failures.jsonl", "".join(json.dumps(d) + "\n" for d in details))
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


def resolve_out(root, path, quiet=False):
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
    if not quiet:
        print(out_real)
    return out_real


# Marker written into every output root this harness creates. A pre-existing
# output root is reused only if it carries the marker.
MARKER = ".yamlprims-fuzz-out"


def refuse(msg):
    sys.exit(f"fuzz output: {msg}")


def check_tree(out):
    """Refuse OUT unless it is a real directory with no symlink, and no
    regular file with more than one link, anywhere below it."""
    try:
        st = os.lstat(out)
    except FileNotFoundError:
        refuse("output directory does not exist")
    if not stat.S_ISDIR(st.st_mode):
        refuse("output path is not a directory")
    for dirpath, dirnames, filenames in os.walk(out, followlinks=False):
        for name in dirnames + filenames:
            st = os.lstat(os.path.join(dirpath, name))
            if stat.S_ISLNK(st.st_mode):
                refuse("symlink under the output directory")
            # A hard link shares its inode with a file elsewhere (possibly in
            # the tree); writing to it would change that file.
            if stat.S_ISREG(st.st_mode) and st.st_nlink > 1:
                refuse("hard-linked file under the output directory")


def claim_out(root, path):
    """Create the output root exclusively, or reuse an empty directory or one
    this harness marked. Containment is checked first, by identity."""
    out = resolve_out(root, path, quiet=True)
    try:
        os.mkdir(out, 0o700)
    except FileExistsError:
        st = os.lstat(out)
        if not stat.S_ISDIR(st.st_mode):
            refuse("output path exists and is not a directory")
        entries = os.listdir(out)
        if entries:
            try:
                marked = stat.S_ISREG(os.lstat(os.path.join(out, MARKER)).st_mode)
            except FileNotFoundError:
                marked = False
            if not marked:
                refuse("output directory is not empty and was not created by this harness")
    except FileNotFoundError:
        refuse("the output directory's parent does not exist")
    if not os.path.lexists(os.path.join(out, MARKER)):
        fd = os.open(os.path.join(out, MARKER), os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        os.close(fd)
    check_tree(out)
    return out


def mkdir_under(out, rel):
    """Create OUT/REL one component at a time, refusing symlinks and non-directories."""
    check_tree(out)
    cur = out
    for part in Path(rel).parts:
        if part in ("", ".", "..") or os.sep in part:
            refuse("bad relative path")
        cur = os.path.join(cur, part)
        try:
            st = os.lstat(cur)
        except FileNotFoundError:
            os.mkdir(cur, 0o700)
            continue
        if not stat.S_ISDIR(st.st_mode):
            refuse("output component is a symlink or not a directory")
    return cur


def write_new(out, name, text):
    """Write a new file directly under OUT without following links; never
    overwrite. Returns the path written."""
    check_tree(out)
    base, ext = os.path.splitext(name)
    for i in range(1000):
        path = os.path.join(out, name if i == 0 else f"{base}-{i}{ext}")
        try:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        except FileExistsError:
            continue
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(text)
        return path
    refuse("too many existing report files")


LOCK = ".yamlprims-fuzz-lock"


def _open_lock(out):
    """Open (creating if needed) the lock file under OUT without following
    links, and refuse anything that is not a plain, singly linked file."""
    check_tree(out)
    path = os.path.join(out, LOCK)
    fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    st = os.fstat(fd)
    if not stat.S_ISREG(st.st_mode) or st.st_nlink != 1:
        os.close(fd)
        refuse("lock file is not a plain file")
    return fd


def hold_lock(out, cmd):
    """Run CMD while holding an exclusive, non-blocking flock on OUT's lock
    file. The locked descriptor is passed to CMD (its number is in
    YAMLPRIMS_FUZZ_LOCK_FD) and is inherited by subprocesses that keep
    inherited descriptors open. A flock belongs to the open file description,
    so the lock stays held while any process that still has it open is alive,
    even if this wrapper is killed, and the kernel releases it when the last
    holder exits. Subprocesses started with close_fds (e.g. run_batch's
    drivers) do not hold it. The file is never removed.
    A child killed by signal N exits this wrapper with status 128+N."""
    import fcntl
    import signal

    fd = _open_lock(out)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        refuse("another run holds this output directory")
    os.ftruncate(fd, 0)
    os.write(fd, f"{os.getpid()}\n".encode())
    os.set_inheritable(fd, True)
    env = dict(os.environ, YAMLPRIMS_FUZZ_LOCK_FD=str(fd))
    child = subprocess.Popen(cmd, pass_fds=(fd,), env=env)
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, lambda n, _f: child.send_signal(n))
    rc = child.wait()
    sys.exit(128 - rc if rc < 0 else rc)


def lock_owned(out, fd):
    """Exit 0 if descriptor FD is OUT's lock file and holds its flock (a
    re-lock of the same open file description succeeds), 1 otherwise."""
    import fcntl

    try:
        st = os.fstat(fd)
        path_st = os.lstat(os.path.join(out, LOCK))
    except OSError:
        sys.exit(1)
    if (st.st_dev, st.st_ino) != (path_st.st_dev, path_st.st_ino):
        sys.exit(1)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        sys.exit(1)
    sys.exit(0)


def lock_held(out):
    """Exit 0 if some process holds OUT's lock, 1 if it is free."""
    import fcntl

    fd = _open_lock(out)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        sys.exit(0)
    fcntl.flock(fd, fcntl.LOCK_UN)
    sys.exit(1)


def import_new(out, rel, src):
    """Copy each file in SRC into OUT/REL by exclusive creation. A destination
    name that already exists is skipped, never opened, so an existing file or
    link there is not written through."""
    dest = mkdir_under(out, rel)
    added = skipped = 0
    for name in sorted(os.listdir(src)):
        data = Path(src, name).read_bytes()
        try:
            fd = os.open(os.path.join(dest, name), os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        except FileExistsError:
            skipped += 1
            continue
        with os.fdopen(fd, "wb") as f:
            f.write(data)
        added += 1
    print(f"{added} added, {skipped} already present")


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "resolve-out":
        resolve_out(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 4 and sys.argv[1] == "claim-out":
        print(claim_out(sys.argv[2], sys.argv[3]))
    elif len(sys.argv) == 3 and sys.argv[1] == "check-tree":
        check_tree(sys.argv[2])
    elif len(sys.argv) >= 5 and sys.argv[1] == "hold-lock" and sys.argv[3] == "--":
        hold_lock(sys.argv[2], sys.argv[4:])
    elif len(sys.argv) == 4 and sys.argv[1] == "lock-owned":
        lock_owned(sys.argv[2], int(sys.argv[3]))
    elif len(sys.argv) == 3 and sys.argv[1] == "lock-held":
        lock_held(sys.argv[2])
    elif len(sys.argv) == 5 and sys.argv[1] == "import-new":
        import_new(sys.argv[2], sys.argv[3], sys.argv[4])
    elif len(sys.argv) >= 4 and sys.argv[1] == "mkdir-under":
        for rel in sys.argv[3:]:
            mkdir_under(sys.argv[2], rel)
    elif len(sys.argv) == 3 and sys.argv[1] == "check-known":
        print(f"known-differences: {len(load_known(sys.argv[2]))} suppressing entries")
    elif len(sys.argv) >= 7 and sys.argv[1] == "run":
        main_run(*sys.argv[2:6], sys.argv[6:])
    else:
        sys.exit(__doc__)
