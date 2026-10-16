"""Gate a structcorpus run against an exact pin. Fails closed (exit 1) on any difference.

Usage:
  python3 -I conformance/scripts/structcorpus_gate.py PIN.json HYDRATED_DIR SOURCE_DIR DRIVER [--record OUT.json]

DRIVER is the built `structcorpus` example (yamlprims/examples/structcorpus.rs).
Checks, in order; every failure is reported and the exit status is 1:
- SOURCE_DIR is a clean checkout at the pinned commit, and the pinned files (case
  manifest, every profile and span convention, every recipe) have their pinned sha256;
- structcorpus's own `tools/corpus.py verify` accepts HYDRATED_DIR, and the hydrated
  case inventory and hydration record have their pinned sha256;
- the inventory: exactly the pinned number of cases, no duplicate ids, every input present
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
import subprocess
import sys
from pathlib import Path


def sha256(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def git(src, *args):
    return subprocess.run(["git", "-C", str(src), *args], capture_output=True, text=True, check=True).stdout.strip()


def pinned_files(src):
    out = ["manifests/cases.jsonl"]
    for d in ("manifests/profiles", "manifests/span-conventions", "recipes/yaml"):
        out += sorted(p.relative_to(src).as_posix() for p in (src / d).glob("*.json"))
    return out


def observe(hyd, src, driver):
    """Everything the pin fixes, measured from the inputs; plus problems found on the way."""
    problems = []
    obs = {"corpus_commit": git(src, "rev-parse", "HEAD")}
    if git(src, "status", "--porcelain", "--untracked-files=no"):
        problems.append("source checkout has local changes")
    obs["files"] = {f: sha256(src / f) for f in pinned_files(src)}
    v = subprocess.run([sys.executable, "-I", str(src / "tools/corpus.py"), "verify", "--root", str(hyd)], capture_output=True, text=True)
    if v.returncode != 0:
        problems.append(f"corpus.py verify failed: {(v.stdout + v.stderr).strip()[-300:]}")
    obs["hydrated"] = {f: sha256(hyd / f) for f in ("cases.jsonl", ".structcorpus-hydrated.json")}

    ids = collections.Counter()
    status = collections.Counter()
    for manifest, base in ((hyd / "cases.jsonl", hyd), (src / "manifests/cases.jsonl", src)):
        for n, line in enumerate(open(manifest, encoding="utf-8"), 1):
            if not line.strip():
                continue
            c = json.loads(line)
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
    obs, problems = observe(hyd, src, driver)
    pin = json.loads(pin_path.read_text())
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
