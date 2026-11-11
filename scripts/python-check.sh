#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Build the Python binding as a wheel with the pinned maturin, install it into
# a throwaway virtual environment, and test the installed package:
#
# 0. the wheel is abi3 for 3.10 and carries both licenses, NOTICE, and py.typed;
# 1. bindings/python/tests (the API tests and the conformance vectors, no skips);
# 2. its conformance vector result lines equal Rust's, byte for byte.
#
# The tests run from outside the source tree with -I, so they import the
# installed wheel, never bindings/python/python.
# PYTHON names the interpreter the wheel is installed into (>= 3.10).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
python="${PYTHON:-python3}"
work="$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-python.XXXXXX")"
trap 'rm -rf "${work}"' EXIT
cd "${root}"

# The wheel's license and notice files are copies of the root ones.
for license in LICENSE-MIT LICENSE-APACHE NOTICE; do
  cmp -s "${license}" "bindings/python/${license}" ||
    { echo "python-check: bindings/python/${license} differs from ${license}" >&2; exit 1; }
done
"${python}" -I -c 'import sys; sys.exit(sys.version_info < (3, 10))' ||
  { echo "python-check: Python 3.10 or later required" >&2; exit 1; }
"${python}" -I -m venv "${work}/venv"
venv_python="${work}/venv/bin/python"
[ -x "${venv_python}" ] || venv_python="${work}/venv/Scripts/python.exe"
"${venv_python}" -I -m pip install --quiet --disable-pip-version-check --require-hashes \
  --only-binary :all: -r config/python-build-requirements.txt
# Map the checkout, the Cargo home and the rustup home to fixed names so the
# extension carries no build-host path.
# Git Bash on Windows spells a drive path as /d/a while rustc sees D:\a or
# D:/a, so on Windows each directory is remapped in both native spellings.
here="$(pwd -P)" cargo_home="${CARGO_HOME:-${HOME}/.cargo}" rustup_home="${RUSTUP_HOME:-${HOME}/.rustup}" home="${HOME}"
rustflags=""
for pair in "${here}=/yamlprims" "${cargo_home}=/cargo" "${rustup_home}=/rustup"; do
  dir="${pair%=*}" to="${pair##*=}"
  if command -v cygpath >/dev/null 2>&1; then
    rustflags+=" --remap-path-prefix=$(cygpath -w "${dir}")=${to} --remap-path-prefix=$(cygpath -m "${dir}")=${to}"
  else
    rustflags+=" --remap-path-prefix=${dir}=${to}"
  fi
done
if command -v cygpath >/dev/null 2>&1; then
  here="$(cygpath -m "${here}")" cargo_home="$(cygpath -m "${cargo_home}")"
  rustup_home="$(cygpath -m "${rustup_home}")" home="$(cygpath -m "${home}")"
fi
# The MSVC linker records the PDB's full path in the DLL; record its name only.
# RUSTFLAGS already holds native paths: Git Bash must not convert it.
case "$(rustc -vV | awk '/^host:/{print $2}')" in *-windows-msvc) rustflags+=" -Clink-arg=/PDBALTPATH:%_PDB%" ;; esac
MSYS2_ENV_CONV_EXCL=RUSTFLAGS RUSTFLAGS="${rustflags}" \
  "${venv_python}" -I -m maturin build --release --locked --quiet \
  --manifest-path bindings/python/Cargo.toml --interpreter "${venv_python}" --out "${work}/wheels"
wheels=("${work}"/wheels/yamlprims-*.whl)
# The extension must carry no build-host path (panic locations are remapped).
"${venv_python}" -I scripts/host-path-check.py "${wheels[0]}" \
  "${here}" "${cargo_home}" "${rustup_home}" "${home}" ||
  { echo "python-check: the wheel embeds a build-host path" >&2; exit 1; }
[ "${#wheels[@]}" -eq 1 ] && [ -f "${wheels[0]}" ] || { echo "python-check: expected one wheel" >&2; exit 1; }
case "${wheels[0]##*/}" in *-cp310-abi3-*) ;; *) echo "python-check: wheel is not abi3 for 3.10: ${wheels[0]##*/}" >&2; exit 1 ;; esac
"${venv_python}" -I -m pip install --quiet --disable-pip-version-check --no-index --no-deps "${wheels[0]}"
"${venv_python}" -I - <<'PY'
import importlib.metadata as md
files = {str(f).rsplit("/", 1)[-1] for f in md.files("yamlprims") or []}
missing = {"LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "py.typed"} - files
if missing:
    raise SystemExit(f"python-check: wheel lacks {sorted(missing)}")
PY

tests="${root}/bindings/python/tests"
vectors="${root}/conformance/vectors/cases.jsonl"
(cd "${work}" && YAMLPRIMS_VECTORS_FILE="${vectors}" YAMLPRIMS_RESULTS_OUT="${work}/py.results" \
  "${venv_python}" -I -m unittest discover -v -s "${tests}" -t "${tests}") 2>"${work}/unittest.log" ||
  { cat "${work}/unittest.log" >&2; exit 1; }
if grep -q 'skipped' "${work}/unittest.log"; then
  echo "python-check: the Python tests must not skip" >&2
  exit 1
fi

cargo build --quiet --locked -p yamlprims --example structcorpus >/dev/null
rust_driver="${CARGO_TARGET_DIR:-${root}/target}/debug/examples/structcorpus"
"${rust_driver}" --crosscheck-jsonl < "${vectors}" > "${work}/rust.results"

"${python}" -I - "${work}/rust.results" "${work}/py.results" <<'PY'
import json, pathlib, sys
rust_lines = [json.loads(l) for l in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if l.strip()]
py_lines = [json.loads(l) for l in pathlib.Path(sys.argv[2]).read_text(encoding="utf-8").splitlines() if l.strip()]
if len(rust_lines) != len(py_lines):
    sys.exit(f"python-check: count mismatch rust={len(rust_lines)} py={len(py_lines)}")
mismatches = []
for r, p in zip(rust_lines, py_lines):
    if r["id"] != p["id"]:
        mismatches.append(f"id mismatch: {r['id']} vs {p['id']}")
    elif r["wire"] != p["wire"]:
        mismatches.append(f"{r['id']} [wire]:\n  rust: {r['wire']}\n  py:   {p['wire']}")
    elif r["plain"] != p["plain"]:
        mismatches.append(f"{r['id']} [plain]:\n  rust: {r['plain']}\n  py:   {p['plain']}")
    elif r.get("emit", "") != p.get("emit", ""):
        mismatches.append(f"{r['id']} [emit]:\n  rust: {r.get('emit', '')}\n  py:   {p.get('emit', '')}")
    elif r.get("ijson", "") != p.get("ijson", ""):
        mismatches.append(f"{r['id']} [ijson]:\n  rust: {r.get('ijson', '')}\n  py:   {p.get('ijson', '')}")
if mismatches:
    for m in mismatches[:10]:
        print(m, file=sys.stderr)
    sys.exit(f"python-check: {len(mismatches)} mismatches vs Rust")
print(f"python-check: conformance vectors agree with Rust ({len(py_lines)} cases)", file=sys.stderr)
PY
echo "[ok] python-check"
