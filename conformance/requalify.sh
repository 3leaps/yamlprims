#!/usr/bin/env bash
# Reproduce the strict core baseline. Any failed phase stops the run.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
prefix_only=false
pin="$root/conformance/pins/pin-c8b7421.json"
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
if [ -f "$corpus/manifests/profiles/yamlprims-core-depth128.r1.json" ]; then
  "$python" -I conformance/scripts/structcorpus_probes_v14.py "$corpus" "$driver" > "$out/probes-v14.tsv"
fi
"$python" -I conformance/scripts/structcorpus_gate_probes.py "$pin" "$hydrated" "$corpus" "$driver" > "$out/gate-probes.tsv"
"$python" -I conformance/scripts/node_selectors.py "$corpus" "$driver" "$out/selectors.json"

go_driver="${CARGO_TARGET_DIR:-$root/target}/release/structcorpus-driver-go"
"$python" -I conformance/scripts/structcorpus_gate.py "$pin" "$hydrated" "$corpus" "$go_driver" --record "$out/go-gate.json"
"$python" -I conformance/scripts/structcorpus_probes_v13.py "$corpus" "$go_driver" > "$out/go-probes-v13.tsv"
if [ -f "$corpus/manifests/profiles/yamlprims-core-depth128.r1.json" ]; then
  "$python" -I conformance/scripts/structcorpus_probes_v14.py "$corpus" "$go_driver" > "$out/go-probes-v14.tsv"
fi
"$python" -I conformance/scripts/structcorpus_gate_probes.py "$pin" "$hydrated" "$corpus" "$go_driver" > "$out/go-gate-probes.tsv"
"$python" -I conformance/scripts/node_selectors.py "$corpus" "$go_driver" "$out/go-selectors.json"
echo "Pinned conformance gate and driver controls passed."
