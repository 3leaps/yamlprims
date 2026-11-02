#!/usr/bin/env python3
"""Check local yamlprims development and release-verification prerequisites."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FLOORS = ROOT / "config/doctor-versions.txt"

COMMANDS: dict[str, tuple[str, ...]] = {
    "rustc": ("rustc", "--version"),
    "cargo": ("cargo", "--version"),
    "python3": ("python3", "--version"),
    "git": ("git", "--version"),
    "bash": ("bash", "--version"),
    "make": ("make", "--version"),
    "gpg": ("gpg", "--version"),
    "jq": ("jq", "--version"),
    "shasum": ("shasum", "--version"),
    "minisign": ("minisign", "-v"),
    "actionlint": ("actionlint", "-version"),
    "shellcheck": ("shellcheck", "--version"),
    "yamlfmt": ("yamlfmt", "-version"),
    "yamllint": ("yamllint", "--version"),
    "goneat": ("goneat", "version"),
    "gh": ("gh", "--version"),
    "cargo-deny": ("cargo", "deny", "--version"),
}

RELEASE_REQUIRED = (
    "rustc",
    "cargo",
    "python3",
    "git",
    "bash",
    "make",
    "gpg",
    "jq",
    "shasum",
    "minisign",
    "gh",
)

CORE_REQUIRED = (
    "rustc",
    "cargo",
    "python3",
    "git",
    "bash",
    "make",
)

VERSION_RE = re.compile(r"(?:^|[^0-9])([0-9]+)\.([0-9]+)(?:\.([0-9]+))?")


def parse_semver(raw: str) -> tuple[int, int, int] | None:
    match = VERSION_RE.search(raw)
    if not match:
        return None
    major, minor, patch = match.groups()
    return int(major), int(minor), int(patch or "0")


def format_semver(version: tuple[int, int, int]) -> str:
    return f"{version[0]}.{version[1]}.{version[2]}"


def load_floors(path: Path) -> dict[str, tuple[int, int, int]]:
    floors: dict[str, tuple[int, int, int]] = {}
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            raise ValueError(f"{path}:{lineno}: expected tool=version")
        tool, version_text = [part.strip() for part in line.split("=", 1)]
        if tool not in COMMANDS:
            raise ValueError(f"{path}:{lineno}: unknown tool {tool!r}")
        parsed = parse_semver(version_text)
        if parsed is None:
            raise ValueError(f"{path}:{lineno}: invalid version {version_text!r}")
        floors[tool] = parsed
    return floors


def first_version_line(output: str) -> str:
    for line in output.splitlines():
        if parse_semver(line) is not None:
            return line.strip()
    return output.splitlines()[0].strip() if output.splitlines() else ""


def probe_tool(tool: str) -> tuple[bool, tuple[int, int, int] | None, str]:
    cmd = COMMANDS[tool]
    binary = cmd[0]
    if shutil.which(binary) is None:
        return False, None, f"{binary} not found in PATH"
    proc = subprocess.run(
        cmd,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env={**os.environ, "LC_ALL": "C"},
    )
    line = first_version_line(proc.stdout)
    if proc.returncode != 0:
        return False, None, line or f"{' '.join(cmd)} exited {proc.returncode}"
    return True, parse_semver(proc.stdout), line


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--floors", type=Path, default=DEFAULT_FLOORS)
    parser.add_argument(
        "--require-all",
        action="store_true",
        help="require optional lint/audit tools in addition to core build/test tools",
    )
    parser.add_argument(
        "--release",
        action="store_true",
        help="require release signing/verification/publishing tools",
    )
    args = parser.parse_args()

    floors = load_floors(args.floors)
    required = set(CORE_REQUIRED)
    if args.release:
        required.update(RELEASE_REQUIRED)
    if args.require_all:
        required.update(floors)

    failures = 0
    warnings = 0
    for tool, floor in floors.items():
        found, version, detail = probe_tool(tool)
        is_required = tool in required
        floor_text = format_semver(floor)
        if not found:
            label = "FAIL" if is_required else "WARN"
            print(f"[{label}] {tool:<12} missing (require >= {floor_text}): {detail}")
            if is_required:
                failures += 1
            else:
                warnings += 1
            continue
        if version is None:
            print(
                f"[FAIL] {tool:<12} could not parse version from {detail!r} (require >= {floor_text})"
            )
            failures += 1
            continue
        if version < floor:
            print(
                f"[FAIL] {tool:<12} {format_semver(version)} < {floor_text} ({detail})"
            )
            failures += 1
            continue
        print(f"[OK]   {tool:<12} {format_semver(version)} >= {floor_text}")

    if failures:
        print(
            f"doctor failed: {failures} problem(s), {warnings} warning(s)",
            file=sys.stderr,
        )
        return 1
    print(f"doctor ok ({warnings} optional warning(s))")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
