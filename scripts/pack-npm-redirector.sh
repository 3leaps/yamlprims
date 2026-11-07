#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Build the bare-name npm redirector tarball (`yamlprims-<version>.tgz`), which
# claims the unscoped `yamlprims` name on npm under the `3leaps` org, re-exports
# `@3leaps/yamlprims`, and is immediately deprecated with:
#   npm deprecate yamlprims "Canonical package is @3leaps/yamlprims — install @3leaps/yamlprims instead."
#
# Note on 0.x SemVer: `"@3leaps/yamlprims": "^0.Y.Z"` resolves to `<0.(Y+1).0`.
# Immediate deprecation of the bare name is mandatory so consumers never depend
# on the unscoped package across 0.x minor bumps.
#
# Usage: scripts/pack-npm-redirector.sh <out-dir> [version]
set -euo pipefail

out="${1:?output directory required}"
root="$(cd "$(dirname "$0")/.." && pwd -P)"
version="${2:-$(tr -d ' \t\r\n' < "$root/VERSION")}"
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || {
	echo "pack-npm-redirector: invalid semantic version $version" >&2
	exit 1
}

work="$(mktemp -d "${TMPDIR:-/tmp}/yamlprims-redirector.XXXXXX")"
trap 'rm -rf "$work"' EXIT
pkg="$work/pkg"
mkdir -p "$pkg"

cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$root/NOTICE" "$pkg/"

cat >"$pkg/index.js" <<'EOF'
// SPDX-License-Identifier: MIT OR Apache-2.0
export * from "@3leaps/yamlprims";
EOF

cat >"$pkg/index.d.ts" <<'EOF'
// SPDX-License-Identifier: MIT OR Apache-2.0
export * from "@3leaps/yamlprims";
EOF

cat >"$pkg/README.md" <<'EOF'
# yamlprims (redirector)

> **Deprecated bare-name package:** The canonical npm package for `yamlprims` is [`@3leaps/yamlprims`](https://www.npmjs.com/package/@3leaps/yamlprims).

Install the scoped package instead:

```sh
npm install @3leaps/yamlprims
```

This unscoped `yamlprims` package is maintained by [3 Leaps](https://github.com/3leaps/yamlprims) solely to prevent scope-stripping or typosquatting and re-exports `@3leaps/yamlprims`.

## License

MIT OR Apache-2.0
EOF

python3 -I - "$pkg/package.json" "$version" <<'PY'
import json
import pathlib
import sys

out = pathlib.Path(sys.argv[1])
version = sys.argv[2]
manifest = {
    "name": "yamlprims",
    "version": version,
    "description": "Deprecated bare-name redirector — install @3leaps/yamlprims instead",
    "license": "MIT OR Apache-2.0",
    "repository": {
        "type": "git",
        "url": "git+https://github.com/3leaps/yamlprims.git",
        "directory": "bindings/typescript",
    },
    "keywords": ["yaml", "yaml-1.2", "parser", "strict", "wasm"],
    "type": "module",
    "sideEffects": False,
    "exports": {
        ".": {
            "types": "./index.d.ts",
            "default": "./index.js",
        }
    },
    "types": "./index.d.ts",
    "files": [
        "index.js",
        "index.d.ts",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "NOTICE",
        "README.md",
    ],
    "dependencies": {
        "@3leaps/yamlprims": f"^{version}",
    },
    "engines": {
        "node": ">=20",
    },
}
out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY

tarball="$(cd "$pkg" && npm pack --silent --pack-destination "$work")"
tarball="$work/${tarball##*/}"
[[ "${tarball##*/}" == "yamlprims-${version}.tgz" ]] || {
	echo "pack-npm-redirector: unexpected tarball ${tarball##*/}" >&2
	exit 1
}
listed="$(tar -tzf "$tarball" | LC_ALL=C sort | tr '\n' ' ')"
want="package/LICENSE-APACHE package/LICENSE-MIT package/NOTICE package/README.md package/index.d.ts package/index.js package/package.json "
[[ "$listed" == "$want" ]] || {
	echo "pack-npm-redirector: unexpected tarball contents: $listed" >&2
	exit 1
}

mkdir -p "$out"
cp "$tarball" "$out/"
echo "[ok] ${out%/}/yamlprims-${version}.tgz"
