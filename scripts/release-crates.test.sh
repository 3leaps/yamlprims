#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# The ordered crate plan: order, publishability and the release version.
set -euo pipefail
# shellcheck source=scripts/release-test-fixture.sh
source "$(dirname "$0")/release-test-fixture.sh"
python3 -B - "$fixture_root/scripts" "$fixture_dir" <<'PY'
import importlib.util
import pathlib
import sys
spec = importlib.util.spec_from_file_location('rc', pathlib.Path(sys.argv[1]) / 'release-crates.py')
rc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rc)

def pkg(name, deps=(), publish=None):
    return {'name': name, 'publish': publish, 'dependencies': [{'name': d} for d in deps]}

def manifests(names, flag=True):
    return {n: {'package': {'publish': flag}} for n in names}

order = ['yamlprims', 'yamlprims-ffi']
graph = [pkg('yamlprims'), pkg('yamlprims-ffi', ['yamlprims']),
         pkg('yamlprims-cli', ['yamlprims', 'yamlprims-ffi'], publish=[])]
rc.validate(graph, order, manifests(order))

def rejects(packages, names, mans, needle):
    try:
        rc.validate(packages, names, mans)
    except ValueError as error:
        assert needle in str(error), (needle, str(error))
        return
    raise SystemExit(f'expected rejection: {needle}')

rejects(graph, order + ['yamlprims-node'], manifests(order), 'not in this workspace: yamlprims-node')
swapped = ['yamlprims-ffi', 'yamlprims']
rejects(graph, swapped, manifests(swapped), 'yamlprims-ffi must follow yamlprims')
rejects(graph, order[:-1], manifests(order[:-1]), 'missing from the list: yamlprims-ffi')
rejects(graph, order, {**manifests(order), 'yamlprims': {'package': {'publish': False}}}, 'explicitly set publish = true')
leaky = graph[:-1] + [pkg('yamlprims-cli', ['yamlprims'])]
rejects(leaky, order, manifests(order), 'missing from the list: yamlprims-cli')
internal = [p if p['name'] != 'yamlprims-ffi' else pkg('yamlprims-ffi', ['yamlprims-cli']) for p in graph]
rejects(internal, order, manifests(order), 'depends on unpublished workspace crate yamlprims-cli')

plan = pathlib.Path(sys.argv[1]).parent / 'config/release/publishable-crates.txt'
names = rc.read_plan(plan)
assert names == ['yamlprims'], names
for bad in ('yamlprims\n\nyamlprims-ffi\n', 'yamlprims\nyamlprims\n', ' yamlprims\n', 'other-crate\n'):
    tmp = pathlib.Path(sys.argv[2]) / 'plan'
    try:
        tmp.write_text(bad)
        try:
            rc.read_plan(tmp)
        except ValueError:
            continue
        raise SystemExit(f'expected plan rejection for {bad!r}')
    finally:
        tmp.unlink(missing_ok=True)
PY

# A tagged tree whose crate version differs from VERSION or whose publishable
# crate sets publish = false is refused.
src="$fixture_dir/source"
mkdir -p "$src/crates" "$src/config/release"
cp "$fixture_root/Cargo.toml" "$fixture_root/Cargo.lock" "$fixture_root/VERSION" "$src/"
cp -R "$fixture_root/crates/." "$src/crates/"
cp -R "$fixture_root/scripts" "$src/scripts"
cp "$fixture_root/config/release/publishable-crates.txt" "$src/config/release/"
PYTHONDONTWRITEBYTECODE=1 python3 "$src/scripts/release-crates.py" --source-root "$src" check
python3 - "$src/crates/yamlprims/Cargo.toml" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
p.write_text(p.read_text().replace("publish = true", "publish = false"))
PY
expect_fail_with 'yamlprims must explicitly set publish = true' \
	env PYTHONDONTWRITEBYTECODE=1 python3 "$src/scripts/release-crates.py" --source-root "$src" check
python3 - "$src/crates/yamlprims/Cargo.toml" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
p.write_text(p.read_text().replace("publish = false", "publish = true"))
PY
printf '9.9.9\n' >"$src/VERSION"
expect_fail_with 'crate version differs from VERSION 9.9.9: yamlprims' \
	env PYTHONDONTWRITEBYTECODE=1 python3 "$src/scripts/release-crates.py" --source-root "$src" check
echo '[ok] crate plan controls passed'
