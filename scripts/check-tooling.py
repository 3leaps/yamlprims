#!/usr/bin/env python3
"""Verify the pinned goneat version and optional full pre-push prerequisites."""

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


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
    if result.returncode or first_line != expected:
        parser.exit(
            1,
            "Expected "
            + expected
            + "; got "
            + repr(first_line)
            + ". See CONTRIBUTING.md.\n",
        )
    print(expected + " verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
