"""Detect test/build failure propagation before a stale driver can be used."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.name == "posix", "shell qualification controls require Unix")
class RequalifyTests(unittest.TestCase):
    def test_failed_phase_stops_prefix(self):
        for fail_phase in ("", "test", "build"):
            with self.subTest(fail_phase=fail_phase), tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp)
                cargo = tmp / "cargo"
                cargo.write_text(
                    '#!/bin/sh\n'
                    'echo "$*" >> "$CALL_LOG"\n'
                    'for arg in "$@"; do\n'
                    '  if [ -n "$FAIL_PHASE" ] && [ "$arg" = "$FAIL_PHASE" ]; then exit 101; fi\n'
                    'done\n'
                    'exit 0\n'
                )
                cargo.chmod(0o755)
                log = tmp / "calls"
                env = dict(os.environ, PATH=str(tmp) + os.pathsep + os.environ["PATH"],
                           FAIL_PHASE=fail_phase, CALL_LOG=str(log))
                result = subprocess.run(
                    ["bash", str(ROOT / "conformance/requalify.sh"), "--prefix-only"],
                    env=env, capture_output=True, text=True, timeout=10,
                )
                self.assertEqual(result.returncode, 101 if fail_phase else 0, result.stderr)
                calls = log.read_text().splitlines()
                self.assertEqual(len(calls), 1 if fail_phase == "test" else 3)
                self.assertEqual("prefix passed" in result.stdout, not fail_phase)


if __name__ == "__main__":
    unittest.main()
