"""Detect test/build failure propagation and pin selection before a stale driver can be used."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.name == "posix", "shell qualification controls require Unix")
class RequalifyTests(unittest.TestCase):
    def _make_fake_cargo(self, tmp: Path) -> Path:
        cargo = tmp / "cargo"
        cargo.write_text(
            '#!/bin/sh\n'
            'echo "$*" >> "$CALL_LOG"\n'
            'for arg in "$@"; do\n'
            '  if [ -n "${FAIL_PHASE:-}" ] && [ "$arg" = "$FAIL_PHASE" ]; then exit 101; fi\n'
            'done\n'
            'exit 0\n'
        )
        cargo.chmod(0o755)
        go = tmp / "go"
        go.write_text(
            '#!/bin/sh\n'
            'echo "go $*" >> "$CALL_LOG"\n'
            'for arg in "$@"; do\n'
            '  if [ -n "${FAIL_GO_PHASE:-}" ] && [ "$arg" = "$FAIL_GO_PHASE" ]; then exit 103; fi\n'
            'done\n'
            'exit 0\n'
        )
        go.chmod(0o755)
        return cargo

    def _make_fake_python(self, tmp: Path) -> Path:
        py = tmp / "fake-python"
        py.write_text(
            '#!/bin/sh\n'
            'if [ "${1:-}" = "-" ]; then\n'
            f'  exec "{sys.executable}" "$@"\n'
            'fi\n'
            'echo "$*" >> "$PY_LOG"\n'
            'for arg in "$@"; do\n'
            '  if [ -n "${FAIL_SCRIPT:-}" ] && [ "$arg" = "$FAIL_SCRIPT" ]; then exit 102; fi\n'
            'done\n'
            'exit 0\n'
        )
        py.chmod(0o755)
        return py

    def test_failed_phase_stops_prefix(self):
        for fail_phase in ("", "test", "build"):
            with self.subTest(fail_phase=fail_phase), tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp)
                self._make_fake_cargo(tmp)
                log = tmp / "calls"
                env = dict(os.environ, PATH=str(tmp) + os.pathsep + os.environ["PATH"],
                           FAIL_PHASE=fail_phase, CALL_LOG=str(log))
                result = subprocess.run(
                    ["bash", str(ROOT / "conformance/requalify.sh"), "--prefix-only"],
                    env=env, capture_output=True, text=True, timeout=10,
                )
                self.assertEqual(result.returncode, 101 if fail_phase else 0, result.stderr)
                calls = log.read_text().splitlines()
                expected_len = 1 if fail_phase == "test" else (3 if fail_phase == "build" else 5)
                self.assertEqual(len(calls), expected_len)
                self.assertEqual("prefix passed" in result.stdout, not fail_phase)
        for fail_go_phase in ("test", "build"):
            with self.subTest(fail_go_phase=fail_go_phase), tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp)
                self._make_fake_cargo(tmp)
                log = tmp / "calls"
                env = dict(os.environ, PATH=str(tmp) + os.pathsep + os.environ["PATH"],
                           FAIL_GO_PHASE=fail_go_phase, CALL_LOG=str(log))
                result = subprocess.run(
                    ["bash", str(ROOT / "conformance/requalify.sh"), "--prefix-only"],
                    env=env, capture_output=True, text=True, timeout=10,
                )
                self.assertEqual(result.returncode, 103, result.stderr)
                calls = log.read_text().splitlines()
                self.assertEqual(len(calls), 4 if fail_go_phase == "test" else 5)
                self.assertFalse("prefix passed" in result.stdout)

    def test_default_pin_and_v14_probes_when_depth128_profile_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            self._make_fake_cargo(tmp)
            py = self._make_fake_python(tmp)
            corpus = tmp / "corpus"
            (corpus / "manifests/profiles").mkdir(parents=True)
            (corpus / "manifests/profiles/yamlprims-core-depth128.r1.json").write_text("{}\n")
            (corpus / "manifests/profiles/yamlprims-core-depth255.r1.json").write_text("{}\n")
            hydrated = tmp / "hydrated"
            hydrated.mkdir()
            out = tmp / "evidence"
            cargo_log = tmp / "cargo.log"
            py_log = tmp / "py.log"
            env = dict(
                os.environ,
                PATH=str(tmp) + os.pathsep + os.environ["PATH"],
                PYTHON=str(py),
                CALL_LOG=str(cargo_log),
                PY_LOG=str(py_log),
            )
            result = subprocess.run(
                [
                    "bash",
                    str(ROOT / "conformance/requalify.sh"),
                    str(corpus),
                    str(hydrated),
                    str(out),
                ],
                env=env,
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            py_calls = py_log.read_text().splitlines()
            self.assertEqual(len(py_calls), 10, py_calls)
            expected_pin = str(ROOT / "conformance/pins/pin-6b1266e.json")
            self.assertIn(f"conformance/scripts/structcorpus_gate.py {expected_pin}", py_calls[0])
            self.assertIn("conformance/scripts/structcorpus_probes_v13.py", py_calls[1])
            self.assertIn("conformance/scripts/structcorpus_probes_v14.py --require-depth255", py_calls[2])
            self.assertIn(f"conformance/scripts/structcorpus_gate_probes.py {expected_pin}", py_calls[3])
            self.assertIn("conformance/scripts/node_selectors.py", py_calls[4])
            self.assertIn(f"conformance/scripts/structcorpus_gate.py {expected_pin}", py_calls[5])
            self.assertIn("structcorpus-driver-go", py_calls[5])
            self.assertIn("conformance/scripts/structcorpus_probes_v13.py", py_calls[6])
            self.assertIn("conformance/scripts/structcorpus_probes_v14.py --require-depth255", py_calls[7])
            self.assertIn(f"conformance/scripts/structcorpus_gate_probes.py {expected_pin}", py_calls[8])
            self.assertIn("conformance/scripts/node_selectors.py", py_calls[9])

    def test_explicit_pin_and_historical_corpus_skips_v14_probes(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            self._make_fake_cargo(tmp)
            py = self._make_fake_python(tmp)
            corpus = tmp / "corpus-400ae34"
            (corpus / "manifests/profiles").mkdir(parents=True)
            hydrated = tmp / "hydrated"
            hydrated.mkdir()
            out = tmp / "evidence"
            cargo_log = tmp / "cargo.log"
            py_log = tmp / "py.log"
            hist_pin = str(ROOT / "conformance/pins/pin-400ae34.json")
            env = dict(
                os.environ,
                PATH=str(tmp) + os.pathsep + os.environ["PATH"],
                PYTHON=str(py),
                CALL_LOG=str(cargo_log),
                PY_LOG=str(py_log),
            )
            result = subprocess.run(
                [
                    "bash",
                    str(ROOT / "conformance/requalify.sh"),
                    "--pin",
                    hist_pin,
                    str(corpus),
                    str(hydrated),
                    str(out),
                ],
                env=env,
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            py_calls = py_log.read_text().splitlines()
            self.assertEqual(len(py_calls), 8, py_calls)
            self.assertIn(f"conformance/scripts/structcorpus_gate.py {hist_pin}", py_calls[0])
            self.assertIn("conformance/scripts/structcorpus_probes_v13.py", py_calls[1])
            self.assertIn(f"conformance/scripts/structcorpus_gate_probes.py {hist_pin}", py_calls[2])
            self.assertIn("conformance/scripts/node_selectors.py", py_calls[3])
            self.assertIn(f"conformance/scripts/structcorpus_gate.py {hist_pin}", py_calls[4])
            self.assertIn("structcorpus-driver-go", py_calls[4])
            self.assertIn("conformance/scripts/structcorpus_probes_v13.py", py_calls[5])
            self.assertIn(f"conformance/scripts/structcorpus_gate_probes.py {hist_pin}", py_calls[6])
            self.assertIn("conformance/scripts/node_selectors.py", py_calls[7])

    def test_missing_or_nonexistent_pin_fails_before_cargo(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            self._make_fake_cargo(tmp)
            cargo_log = tmp / "cargo.log"
            env = dict(
                os.environ,
                PATH=str(tmp) + os.pathsep + os.environ["PATH"],
                CALL_LOG=str(cargo_log),
            )
            for argv in (
                ["--pin"],
                ["--pin", "", "a", "b", "c"],
                ["--pin", str(tmp / "missing-pin.json"), "a", "b", "c"],
            ):
                with self.subTest(argv=argv):
                    result = subprocess.run(
                        ["bash", str(ROOT / "conformance/requalify.sh"), *argv],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=10,
                    )
                    self.assertEqual(result.returncode, 2)
                    self.assertFalse(cargo_log.exists())

    def test_failed_control_script_stops_run(self):
        for fail_script in (
            "conformance/scripts/structcorpus_gate.py",
            "conformance/scripts/structcorpus_probes_v13.py",
            "conformance/scripts/structcorpus_probes_v14.py",
            "conformance/scripts/structcorpus_gate_probes.py",
            "conformance/scripts/node_selectors.py",
        ):
            with self.subTest(fail_script=fail_script), tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp)
                self._make_fake_cargo(tmp)
                py = self._make_fake_python(tmp)
                corpus = tmp / "corpus"
                (corpus / "manifests/profiles").mkdir(parents=True)
                (corpus / "manifests/profiles/yamlprims-core-depth128.r1.json").write_text("{}\n")
                hydrated = tmp / "hydrated"
                hydrated.mkdir()
                out = tmp / "evidence"
                cargo_log = tmp / "cargo.log"
                py_log = tmp / "py.log"
                env = dict(
                    os.environ,
                    PATH=str(tmp) + os.pathsep + os.environ["PATH"],
                    PYTHON=str(py),
                    CALL_LOG=str(cargo_log),
                    PY_LOG=str(py_log),
                    FAIL_SCRIPT=fail_script,
                )
                result = subprocess.run(
                    [
                        "bash",
                        str(ROOT / "conformance/requalify.sh"),
                        str(corpus),
                        str(hydrated),
                        str(out),
                    ],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                self.assertEqual(result.returncode, 102, result.stderr)
                self.assertNotIn("Pinned conformance gate and driver controls passed.", result.stdout)


if __name__ == "__main__":
    unittest.main()
