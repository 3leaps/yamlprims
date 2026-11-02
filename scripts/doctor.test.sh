#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON_BIN="$(command -v python3)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-doctor-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/bin"
cat >"$tmp/floors.txt" <<'EOF'
rustc=1.88.0
cargo=1.88.0
python3=3.9.0
git=2.39.0
bash=3.2.0
make=3.81.0
shellcheck=0.11.0
EOF

write_stub() {
	local name="$1"
	local output="$2"
	cat >"$tmp/bin/$name" <<EOF
#!/bin/sh
echo "$output"
EOF
	chmod +x "$tmp/bin/$name"
}

write_stub rustc "rustc 1.88.0 (6b00bc388 2025-06-23)"
write_stub cargo "cargo 1.88.0 (873a06493 2025-05-10)"
write_stub python3 "Python 3.12.2"
write_stub git "git version 2.44.0"
write_stub bash "GNU bash, version 5.2.26(1)-release"
write_stub make "GNU Make 4.4.1"

PATH="$tmp/bin" "$PYTHON_BIN" "$ROOT/scripts/doctor.py" --floors "$tmp/floors.txt"
if PATH="$tmp/bin" "$PYTHON_BIN" "$ROOT/scripts/doctor.py" --floors "$tmp/floors.txt" --require-all >/dev/null 2>&1; then
	echo "expected missing optional tool to fail under --require-all" >&2
	exit 1
fi

write_stub shellcheck "version: 0.10.0"
if PATH="$tmp/bin" "$PYTHON_BIN" "$ROOT/scripts/doctor.py" --floors "$tmp/floors.txt" >/dev/null 2>&1; then
	echo "expected installed below-floor tool to fail" >&2
	exit 1
fi

write_stub shellcheck "version: 0.11.0"
PATH="$tmp/bin" "$PYTHON_BIN" "$ROOT/scripts/doctor.py" --floors "$tmp/floors.txt" --require-all
echo "doctor tests ok"
