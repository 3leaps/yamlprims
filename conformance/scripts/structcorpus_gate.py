"""Gate a structcorpus run against an exact pin. Fails closed (exit 1) on any difference.

Usage:
  python3 -I conformance/scripts/structcorpus_gate.py PIN.json HYDRATED_DIR SOURCE_DIR DRIVER [--record OUT.json]

DRIVER is the built `structcorpus` example (yamlprims/examples/structcorpus.rs).
Checks, in order; every failure is reported and the exit status is 1:
- when the pin names a `corpus_tag`, SOURCE_DIR's `refs/tags/<name>` (an exact ref,
  not a revision expression) is that annotated tag object and it peels to the pinned
  commit; a moved, lightweight or missing tag, or a malformed `corpus_tag`, fails before
  anything from SOURCE_DIR runs;
- SOURCE_DIR's YAML inventory is located: with `manifests/formats.json` (a `formats/v1`
  registry with a `yaml` entry) the YAML case shard, profile and span-convention directories
  it names, otherwise the legacy `manifests/cases.jsonl`, `manifests/profiles` and
  `manifests/span-conventions`; a malformed registry fails;
- SOURCE_DIR is a clean checkout at the pinned commit, and the pinned files (the registry
  when present, the YAML case manifest, every profile and span convention, every recipe)
  have their pinned sha256;
- structcorpus's own `tools/corpus.py verify` accepts HYDRATED_DIR, and the hydrated
  case inventory and hydration record have their pinned sha256;
- the inventory (records with `"format": "yaml"`; other formats in the hydrated manifest are
  skipped and counted, a non-yaml record in the YAML source manifest fails, and a record
  without `format` fails when the source has the registry): exactly the pinned number of
  cases, no duplicate ids, every input present
  with the byte count and sha256 its record states; the review-status counts (all
  `provisional` at 0.1) match the pin, so a change in oracle status is visible;
- the driver exits 0, prints `result PASS`, and its per-profile rows (status, pass, fail,
  excluded) equal the pinned rows exactly: no new, missing, failing or re-labelled rows.
`--record` writes the run record: pins, digests, counts, toolchain-independent results.
Pin generation is disabled; changed expectations require independent review.
"""
import collections
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path


def sha256(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# Git runs without inherited GIT_* variables (GIT_DIR, GIT_INDEX_FILE, ...), so every
# question is answered by SOURCE_DIR's own repository, and `git status` takes no optional
# index lock or refresh write.
GIT_ENV = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")} | {"GIT_OPTIONAL_LOCKS": "0"}


def git(src, *args):
    return subprocess.run(["git", "-C", str(src), *args], capture_output=True, text=True, check=True, env=GIT_ENV).stdout.strip()


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


def pinned_files(src, layout):
    cases, profiles, spans, registry = layout
    out = ["manifests/formats.json"] if registry else []
    out.append(cases.relative_to(src).as_posix())
    for d in (profiles, spans, src / "recipes/yaml"):
        out += sorted(p.relative_to(src).as_posix() for p in d.glob("*.json"))
    return out


def observe(hyd, src, driver):
    """Everything the pin fixes, measured from the inputs; plus problems found on the way."""
    problems = []
    try:
        layout = structcorpus_layout(src)
    except ValueError as e:
        print("FAIL", e)
        sys.exit(1)
    registry = layout[3]
    obs = {"corpus_commit": git(src, "rev-parse", "HEAD")}
    if git(src, "status", "--porcelain", "--untracked-files=no"):
        problems.append("source checkout has local changes")
    obs["files"] = {f: sha256(src / f) for f in pinned_files(src, layout)}
    v = subprocess.run([sys.executable, "-I", str(src / "tools/corpus.py"), "verify", "--root", str(hyd)], capture_output=True, text=True)
    if v.returncode != 0:
        problems.append(f"corpus.py verify failed: {(v.stdout + v.stderr).strip()[-300:]}")
    obs["hydrated"] = {f: sha256(hyd / f) for f in ("cases.jsonl", ".structcorpus-hydrated.json")}

    ids = collections.Counter()
    status = collections.Counter()
    other = collections.Counter()
    for manifest, shard in ((hyd / "cases.jsonl", False), (layout[0], True)):
        for n, line in enumerate(open(manifest, encoding="utf-8"), 1):
            if not line.strip():
                continue
            c = json.loads(line)
            try:
                run = yaml_record(c, registry)
            except ValueError as e:
                problems.append(f"{manifest}:{n}: {e}")
                continue
            if not run:
                if shard:
                    problems.append(f"{manifest}:{n}: {c['format']} record in the YAML case manifest")
                other[c["format"]] += 1
                continue
            ids[c["id"]] += 1
            status[c.get("review", {}).get("status", "missing")] += 1
            inp = c["input"]
            if "path" in inp:
                p = hyd / inp["path"] if (hyd / inp["path"]).exists() else src / inp["path"]
            else:
                recipe, k = c["id"].rsplit(".", 1)
                p = hyd / "inputs/yaml" / recipe / f"{k}.yaml"
            if not p.is_file():
                problems.append(f"{c['id']}: input {p} missing")
                continue
            data = p.read_bytes()
            if len(data) != inp["bytes"] or hashlib.sha256(data).hexdigest() != inp["sha256"]:
                problems.append(f"{c['id']}: input {p} does not match its record (bytes or sha256)")
    dups = sorted(i for i, n in ids.items() if n > 1)
    if dups:
        problems.append(f"duplicate case ids: {dups[:5]}")
    obs["cases"] = sum(ids.values())
    obs["review_status"] = dict(sorted(status.items()))
    obs["other_formats"] = dict(sorted(other.items()))

    r = subprocess.run([str(driver), str(hyd), str(src), "--details", "50"], capture_output=True, text=True)
    lines = r.stdout.splitlines()
    rows = []
    for line in lines[1:]:
        f = line.split("\t")
        if len(f) == 7 and f[1].isdigit():
            rows.append({"profile": f[0], "revision": int(f[1]), "kind": f[2], "status": f[3], "pass": int(f[4]), "fail": int(f[5]), "excluded": int(f[6])})
    obs["rows"] = rows
    result = [l for l in lines if l.startswith("result\t")]
    if r.returncode != 0 or not result or not result[-1].startswith("result\tPASS"):
        problems.append(f"driver exit {r.returncode}: " + "; ".join(l for l in lines if l.startswith(("problem\t", "result\t")))[:800])
    return obs, problems


def tag_problems(src, tag, commit):
    """The pinned tag must exist in SOURCE_DIR as the pinned annotated tag object on the pinned commit.

    The name is a literal tag name (`git check-ref-format`), looked up as exactly
    `refs/tags/<name>` with `git show-ref --verify`, never as a revision expression.
    """
    if not isinstance(tag, dict):
        return [f"corpus_tag is not an object: {json.dumps(tag)[:200]}"]
    name, obj = tag.get("name"), tag.get("object")
    if (not isinstance(name, str) or not isinstance(obj, str) or not name or "\x00" in name
            or not re.fullmatch(r"[0-9a-f]{40}", obj)):
        return [f"corpus_tag is malformed: {json.dumps(tag)[:200]}"]
    ref = f"refs/tags/{name}"
    def run(*args):
        r = subprocess.run(["git", "-C", str(src), *args], capture_output=True, text=True, env=GIT_ENV)
        return r.stdout.strip() if r.returncode == 0 else None
    if run("check-ref-format", ref) is None:
        return [f"corpus_tag name {json.dumps(name)} is not a valid tag name"]
    got = run("show-ref", "--verify", "--hash", ref)
    if got is None:
        return [f"corpus_tag {name} is missing from the source checkout"]
    problems = []
    if got != obj:
        problems.append(f"corpus_tag {name} is object {got}, pinned {obj}")
    if run("cat-file", "-t", got) != "tag":
        problems.append(f"corpus_tag {name} is not an annotated tag")
    peeled = run("rev-parse", "--verify", "--quiet", f"{got}^{{commit}}")
    if peeled != commit:
        problems.append(f"corpus_tag {name} peels to {peeled}, pinned commit {commit}")
    return problems


def main():
    args = sys.argv[1:]
    if "--write-pin" in args:
        sys.exit("pin generation is disabled; use independently reviewed locks")
    record = None
    if "--record" in args:
        i = args.index("--record")
        record = Path(args[i + 1])
        del args[i : i + 2]
    if len(args) != 4:
        sys.exit(__doc__)
    pin_path, hyd, src, driver = Path(args[0]), Path(args[1]), Path(args[2]), Path(args[3])
    pin = json.loads(pin_path.read_text())
    # The tag is checked before anything from the source checkout runs.
    if "corpus_tag" in pin:
        early = tag_problems(src, pin["corpus_tag"], pin["corpus_commit"])
        if early:
            if record:
                record.write_text(json.dumps({"pin": str(pin_path), "problems": early, "result": "FAIL"}, indent=1) + "\n")
            for p in early:
                print("FAIL", p)
            print("gate FAIL: corpus tag")
            sys.exit(1)
    obs, problems = observe(hyd, src, driver)
    for key in ("corpus_commit", "files", "hydrated", "cases", "review_status", "rows"):
        if obs[key] != pin[key]:
            if isinstance(pin[key], dict):
                diff = sorted(k for k in set(pin[key]) | set(obs[key]) if pin[key].get(k) != obs[key].get(k))
                problems.append(f"{key} differs from the pin: {diff[:8]}")
            else:
                problems.append(f"{key} differs from the pin: got {json.dumps(obs[key])[:400]}, pinned {json.dumps(pin[key])[:400]}")
    ran = sum(r["pass"] + r["fail"] for r in obs["rows"])
    summary = {
        "pin": str(pin_path),
        "corpus_commit": obs["corpus_commit"],
        "cases": obs["cases"],
        "review_status": obs["review_status"],
        "other_formats_skipped": obs["other_formats"],
        "expectations_run": ran,
        "rows": obs["rows"],
        "problems": problems,
        "result": "FAIL" if problems else "PASS",
    }
    if record:
        record.write_text(json.dumps(summary, indent=1) + "\n")
    for p in problems:
        print("FAIL", p)
    print(f"gate {summary['result']}: {obs['cases']} cases, {ran} expectations run, review status {obs['review_status']}")
    sys.exit(1 if problems else 0)


main()
