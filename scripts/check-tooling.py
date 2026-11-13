#!/usr/bin/env python3
"""Verify the pinned goneat version and optional full pre-push prerequisites."""

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--goneat", default="goneat", help="goneat executable")
    parser.add_argument(
        "--prepush", action="store_true", help="require Unix signing-test support"
    )
    parser.add_argument(
        "--require", action="append", default=[], help="additional required executable"
    )
    args = parser.parse_args()
    if args.prepush and os.name != "posix":
        parser.exit(
            1,
            "Full prepush requires Unix (the signing tests use pty). Run it on Linux or macOS.\n",
        )
    missing = [
        tool for tool in [args.goneat] + args.require if shutil.which(tool) is None
    ]
    if missing:
        parser.exit(
            1,
            "Missing pre-push tools: "
            + ", ".join(missing)
            + ". See CONTRIBUTING.md.\n",
        )
    if args.prepush:
        # python-check and the wheel builds create venvs with this interpreter
        # (pip included). Stock Debian/Ubuntu ship Python without ensurepip
        # until python3-venv is installed; fail here, not after the Rust, Go
        # and TypeScript gates.
        with tempfile.TemporaryDirectory(prefix="yamlprims-venv-probe.") as probe:
            try:
                venv = subprocess.run(
                    [sys.executable, "-I", "-m", "venv", str(Path(probe) / "v")],
                    capture_output=True,
                    text=True,
                    timeout=180,
                )
            except (OSError, subprocess.TimeoutExpired) as exc:
                parser.exit(1, "Could not create a Python venv: " + str(exc) + "\n")
        if venv.returncode:
            detail = (venv.stderr or venv.stdout).strip().splitlines()
            parser.exit(
                1,
                "Python "
                + sys.executable
                + " cannot create a venv with pip (prepush needs it): "
                + (detail[-1] if detail else "no detail")
                + ". On Debian/Ubuntu install the python3-venv package for this Python"
                + " (e.g. python3.14-venv). See CONTRIBUTING.md.\n",
            )
    pin = (
        (Path(__file__).resolve().parents[1] / ".goneat-version")
        .read_text(encoding="ascii")
        .strip()
    )
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", pin):
        parser.exit(1, "Invalid .goneat-version: expected MAJOR.MINOR.PATCH.\n")
    try:
        result = subprocess.run(
            [args.goneat, "version"], capture_output=True, text=True, timeout=15
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        parser.exit(1, "Could not check goneat: " + str(exc) + "\n")
    first_line = result.stdout.splitlines()[0] if result.stdout else ""
    expected = "goneat v" + pin
    match = re.match(r"^goneat v([0-9]+)\.([0-9]+)\.([0-9]+)(?:\s|$)", first_line)
    pin_tuple = tuple(int(part) for part in pin.split("."))
    got_tuple = tuple(int(part) for part in match.groups()) if match else ()
    if result.returncode or not match or got_tuple < pin_tuple:
        parser.exit(
            1,
            "Expected "
            + expected
            + " (or newer); got "
            + repr(first_line)
            + ". See CONTRIBUTING.md.\n",
        )
    print(first_line + " verified (minimum " + expected + ")")
    return 0


if __name__ == "__main__":
    sys.exit(main())
