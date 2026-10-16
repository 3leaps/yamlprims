#!/usr/bin/env bash
# Reproduce the strict core baseline. Any failed phase stops the run.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
prefix_only=false
if [ "${1:-}" = --prefix-only ]; then
  prefix_only=true
elif [ "$#" -ne 3 ]; then
  echo "usage: bash conformance/requalify.sh CORPUS HYDRATED OUTPUT_DIR" >&2
  exit 2
fi

cargo +1.89.0 test --locked --workspace -- --test-threads=1
cargo +1.88.0 test --release --locked --workspace -- --test-threads=1
cargo +1.89.0 build --release --locked -p yamlprims --example structcorpus
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
pin="$root/conformance/pins/pin-400ae34.json"
"$python" -I conformance/scripts/structcorpus_gate.py "$pin" "$hydrated" "$corpus" "$driver" --record "$out/gate.json"
"$python" -I conformance/scripts/structcorpus_probes_v13.py "$corpus" "$driver" > "$out/probes-v13.tsv"
"$python" -I conformance/scripts/structcorpus_gate_probes.py "$pin" "$hydrated" "$corpus" "$driver" > "$out/gate-probes.tsv"
"$python" -I conformance/scripts/node_selectors.py "$corpus" "$driver" "$out/selectors.json"
echo "Pinned conformance gate and driver controls passed."
