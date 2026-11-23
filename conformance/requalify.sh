#!/usr/bin/env bash
# Reproduce the strict core baseline. Any failed phase stops the run.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
prefix_only=false
pin="$root/conformance/pins/pin-d04cd32.json"
if [ "${1:-}" = --prefix-only ] && [ "$#" -eq 1 ]; then
  prefix_only=true
else
  if [ "${1:-}" = --pin ]; then
    if [ "$#" -lt 2 ] || [ -z "${2:-}" ]; then
      echo "usage: bash conformance/requalify.sh [--pin PIN.json] CORPUS HYDRATED OUTPUT_DIR" >&2
      exit 2
    fi
    pin=$2
    shift 2
  fi
  if [ "$#" -ne 3 ]; then
    echo "usage: bash conformance/requalify.sh [--pin PIN.json] CORPUS HYDRATED OUTPUT_DIR" >&2
    exit 2
  fi
  if [ ! -f "$pin" ]; then
    echo "pin file does not exist: $pin" >&2
    exit 2
  fi
fi

cargo +1.89.0 test --locked --workspace -- --test-threads=1
cargo +1.88.0 test --release --locked --workspace -- --test-threads=1
cargo +1.89.0 build --release --locked -p yamlprims --example structcorpus
CGO_ENABLED=0 go test ./yaml/... ./cmd/...
CGO_ENABLED=0 go build -o "${CARGO_TARGET_DIR:-$root/target}/release/structcorpus-driver-go" ./cmd/structcorpus-driver
if "$prefix_only"; then
  echo "Test/build prefix passed; corpus qualification was not run."
  exit 0
fi

python=${PYTHON:-python3}
corpus=$1 hydrated=$2 out=$3
# The corpus's YAML profile directory: named by manifests/formats.json when the corpus has
# the format registry (a malformed registry stops the run), else manifests/profiles.
profiles=$("$python" - "$corpus" <<'PY'
import json
import sys
from pathlib import Path


def structcorpus_layout(src):
    """YAML inventory paths of a structcorpus source: (cases, profiles, span_conventions, registry).

    With manifests/formats.json the registry must be formats/v1 with a yaml entry, and its
    cases, profiles and span_conventions paths are used (relative, no empty, . or ..
    segments, resolving inside src); any problem raises ValueError. Without the registry, the
    legacy single-format paths. The same function is in every conformance script that reads
    a source checkout; keep the copies identical.
    """
    reg = src / "manifests/formats.json"
    if not reg.exists():
        return src / "manifests/cases.jsonl", src / "manifests/profiles", src / "manifests/span-conventions", False
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


try:
    print(structcorpus_layout(Path(sys.argv[1]))[1])
except ValueError as e:
    sys.exit(f"structcorpus layout: {e}")
PY
)
# Evidence directories are fresh and outside the working tree.
"$python" - "$root" "$out" <<'PY'
from pathlib import Path
import sys
root, out = (Path(p).resolve() for p in sys.argv[1:])
if out == root or root in out.parents:
    sys.exit("OUTPUT_DIR must be outside the working tree")
out.mkdir(parents=True, exist_ok=False)
PY
driver="${CARGO_TARGET_DIR:-$root/target}/release/examples/structcorpus"
"$python" -I conformance/scripts/structcorpus_gate.py "$pin" "$hydrated" "$corpus" "$driver" --record "$out/gate.json"
"$python" -I conformance/scripts/structcorpus_probes_v13.py "$corpus" "$driver" > "$out/probes-v13.tsv"
if [ -f "$profiles/yamlprims-core-depth128.r1.json" ]; then
  v14_flags=()
  if [ -f "$profiles/yamlprims-core-depth255.r1.json" ]; then
    v14_flags=(--require-depth255)
  fi
  if [ -f "$profiles/yamlprims-core.r5.json" ]; then
    v14_flags+=(--require-r5)
  fi
  "$python" -I conformance/scripts/structcorpus_probes_v14.py ${v14_flags[@]+"${v14_flags[@]}"} "$corpus" "$driver" > "$out/probes-v14.tsv"
fi
"$python" -I conformance/scripts/structcorpus_gate_probes.py "$pin" "$hydrated" "$corpus" "$driver" > "$out/gate-probes.tsv"
"$python" -I conformance/scripts/node_selectors.py "$corpus" "$driver" "$out/selectors.json"

go_driver="${CARGO_TARGET_DIR:-$root/target}/release/structcorpus-driver-go"
"$python" -I conformance/scripts/structcorpus_gate.py "$pin" "$hydrated" "$corpus" "$go_driver" --record "$out/go-gate.json"
"$python" -I conformance/scripts/structcorpus_probes_v13.py "$corpus" "$go_driver" > "$out/go-probes-v13.tsv"
if [ -f "$profiles/yamlprims-core-depth128.r1.json" ]; then
  "$python" -I conformance/scripts/structcorpus_probes_v14.py ${v14_flags[@]+"${v14_flags[@]}"} "$corpus" "$go_driver" > "$out/go-probes-v14.tsv"
fi
"$python" -I conformance/scripts/structcorpus_gate_probes.py "$pin" "$hydrated" "$corpus" "$go_driver" > "$out/go-gate-probes.tsv"
"$python" -I conformance/scripts/node_selectors.py "$corpus" "$go_driver" "$out/go-selectors.json"
echo "Pinned conformance gate and driver controls passed."
