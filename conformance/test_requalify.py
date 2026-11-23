"""Detect test/build failure propagation and pin selection before a stale driver can be used."""
import ast
import json
import os
import re
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

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

    def test_v14_probes_without_r5_profiles_omit_require_r5(self):
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
            py_log = tmp / "py.log"
            env = dict(
                os.environ,
                PATH=str(tmp) + os.pathsep + os.environ["PATH"],
                PYTHON=str(py),
                CALL_LOG=str(tmp / "cargo.log"),
                PY_LOG=str(py_log),
            )
            result = subprocess.run(
                ["bash", str(ROOT / "conformance/requalify.sh"), str(corpus), str(hydrated), str(tmp / "evidence")],
                env=env,
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            v14 = [c for c in py_log.read_text().splitlines() if "structcorpus_probes_v14.py" in c]
            self.assertEqual(len(v14), 2, v14)
            for call in v14:
                self.assertIn("structcorpus_probes_v14.py --require-depth255 ", call)
                self.assertNotIn("--require-r5", call)

    def test_default_pin_and_v14_probes_when_depth128_profile_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            self._make_fake_cargo(tmp)
            py = self._make_fake_python(tmp)
            corpus = tmp / "corpus"
            (corpus / "manifests/profiles").mkdir(parents=True)
            (corpus / "manifests/profiles/yamlprims-core-depth128.r1.json").write_text("{}\n")
            (corpus / "manifests/profiles/yamlprims-core-depth255.r1.json").write_text("{}\n")
            (corpus / "manifests/profiles/yamlprims-core.r5.json").write_text("{}\n")
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
            expected_pin = str(ROOT / "conformance/pins/pin-d04cd32.json")
            self.assertIn(f"conformance/scripts/structcorpus_gate.py {expected_pin}", py_calls[0])
            self.assertIn("conformance/scripts/structcorpus_probes_v13.py", py_calls[1])
            self.assertIn("conformance/scripts/structcorpus_probes_v14.py --require-depth255 --require-r5", py_calls[2])
            self.assertIn(f"conformance/scripts/structcorpus_gate_probes.py {expected_pin}", py_calls[3])
            self.assertIn("conformance/scripts/node_selectors.py", py_calls[4])
            self.assertIn(f"conformance/scripts/structcorpus_gate.py {expected_pin}", py_calls[5])
            self.assertIn("structcorpus-driver-go", py_calls[5])
            self.assertIn("conformance/scripts/structcorpus_probes_v13.py", py_calls[6])
            self.assertIn("conformance/scripts/structcorpus_probes_v14.py --require-depth255 --require-r5", py_calls[7])
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

    def _run_registry_corpus(self, tmp: Path, registry: str):
        self._make_fake_cargo(tmp)
        py = self._make_fake_python(tmp)
        corpus = tmp / "corpus"
        profiles = corpus / "manifests/yaml/profiles"
        profiles.mkdir(parents=True)
        (corpus / "manifests/yaml/span-conventions").mkdir()
        (corpus / "manifests/yaml/cases.jsonl").write_text("")
        for name in ("yamlprims-core-depth128.r1.json", "yamlprims-core-depth255.r1.json", "yamlprims-core.r5.json"):
            (profiles / name).write_text("{}\n")
        (corpus / "manifests/formats.json").write_text(registry)
        hydrated = tmp / "hydrated"
        hydrated.mkdir()
        py_log = tmp / "py.log"
        env = dict(
            os.environ,
            PATH=str(tmp) + os.pathsep + os.environ["PATH"],
            PYTHON=str(py),
            CALL_LOG=str(tmp / "cargo.log"),
            PY_LOG=str(py_log),
        )
        result = subprocess.run(
            ["bash", str(ROOT / "conformance/requalify.sh"), str(corpus), str(hydrated), str(tmp / "evidence")],
            env=env,
            capture_output=True,
            text=True,
            timeout=10,
        )
        calls = py_log.read_text().splitlines() if py_log.exists() else []
        return result, calls

    def test_registry_layout_resolves_profiles(self):
        with tempfile.TemporaryDirectory() as tmp:
            result, calls = self._run_registry_corpus(Path(tmp), json.dumps(_registry(_yaml_entry())))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(len(calls), 10, calls)
            v14 = [c for c in calls if "structcorpus_probes_v14.py" in c]
            self.assertEqual(len(v14), 2, v14)
            for call in v14:
                self.assertIn("structcorpus_probes_v14.py --require-depth255 --require-r5 ", call)

    def test_malformed_registry_stops_before_controls(self):
        for label, doc in _bad_registries():
            with self.subTest(label), tempfile.TemporaryDirectory() as tmp:
                result, calls = self._run_registry_corpus(Path(tmp), doc)
                self.assertNotEqual(result.returncode, 0, label)
                self.assertIn("structcorpus layout:", result.stderr)
                self.assertEqual(calls, [])
                self.assertFalse((Path(tmp) / "evidence").exists())


def _yaml_entry():
    return {
        "cases": "manifests/yaml/cases.jsonl",
        "profiles": "manifests/yaml/profiles",
        "span_conventions": "manifests/yaml/span-conventions",
    }


def _registry(entry, version="formats/v1", name="yaml"):
    return {"schema_version": version, "formats": {name: entry}}


def _bad_registries():
    out = [
        ("schema-version", json.dumps(_registry(_yaml_entry(), version="formats/v2"))),
        ("no-yaml-entry", json.dumps(_registry(_yaml_entry(), name="json"))),
        ("not-json", "{"),
    ]
    for bad in ("../outside", "/abs/profiles", "manifests/./yaml/profiles", "manifests/yaml//profiles", ""):
        out.append((f"profiles={bad!r}", json.dumps(_registry({**_yaml_entry(), "profiles": bad}))))
    for key in ("cases", "span_conventions"):
        out.append((f"{key}=../x", json.dumps(_registry({**_yaml_entry(), key: "../x"}))))
    out.append(("span_conventions-missing", json.dumps(_registry({k: v for k, v in _yaml_entry().items() if k != "span_conventions"}))))
    return out


SOURCE_READERS = (
    "conformance/scripts/structcorpus_gate.py",
    "conformance/scripts/structcorpus_gate_probes.py",
    "conformance/scripts/structcorpus_probes_v13.py",
    "conformance/scripts/structcorpus_probes_v14.py",
    "conformance/scripts/node_selectors.py",
)


def _function_sources(text):
    tree = ast.parse(text)
    return {n.name: ast.get_source_segment(text, n) for n in tree.body if isinstance(n, ast.FunctionDef)}


def _script_names(path, *names):
    """Functions and the GIT_ENV assignment of a conformance script, evaluated now."""
    text = (ROOT / path).read_text()
    tree = ast.parse(text)
    env = [ast.get_source_segment(text, n) for n in tree.body
           if isinstance(n, ast.Assign) and any(getattr(t, "id", None) == "GIT_ENV" for t in n.targets)]
    ns = {"json": json, "os": os, "re": re, "shutil": shutil, "subprocess": subprocess, "sys": sys, "Path": Path}
    for src in env + [_function_sources(text)[n] for n in names]:
        exec(src, ns)
    return ns


def _embedded_python(path):
    """The Python heredoc bodies of a shell script."""
    text = (ROOT / path).read_text()
    return re.findall(r"<<'PY'\n(.*?)\nPY\n", text, re.S)


class LayoutTests(unittest.TestCase):
    """The duplicated source-layout readers agree, and the gate fails closed on a bad registry."""

    def test_resolver_copies_are_identical(self):
        sources = [(ROOT / f).read_text() for f in SOURCE_READERS]
        sources += [b for b in _embedded_python("conformance/requalify.sh") if "structcorpus_layout" in b]
        sources += [b for b in _embedded_python("scripts/crosscheck.sh") if "structcorpus_layout" in b]
        self.assertEqual(len(sources), len(SOURCE_READERS) + 2)
        for name in ("structcorpus_layout", "yaml_record"):
            copies = {_function_sources(t)[name] for t in sources}
            self.assertEqual(len(copies), 1, name)

    def _layout(self):
        ns = {}
        exec(_function_sources((ROOT / SOURCE_READERS[0]).read_text())["structcorpus_layout"], {"json": json}, ns)
        return ns["structcorpus_layout"]

    def test_resolver(self):
        layout = self._layout()
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp)
            (src / "manifests/yaml/profiles").mkdir(parents=True)
            self.assertEqual(
                layout(src),
                (src / "manifests/cases.jsonl", src / "manifests/profiles", src / "manifests/span-conventions", False),
            )
            (src / "manifests/formats.json").write_text(json.dumps(_registry(_yaml_entry())))
            self.assertEqual(
                layout(src),
                (src / "manifests/yaml/cases.jsonl", src / "manifests/yaml/profiles", src / "manifests/yaml/span-conventions", True),
            )
            for label, doc in _bad_registries():
                with self.subTest(label):
                    (src / "manifests/formats.json").write_text(doc)
                    with self.assertRaises(ValueError):
                        layout(src)
            if os.name == "posix":
                outside = tempfile.mkdtemp()
                try:
                    (src / "manifests/out").symlink_to(outside)
                    (src / "manifests/formats.json").write_text(json.dumps(_registry({**_yaml_entry(), "profiles": "manifests/out"})))
                    with self.assertRaisesRegex(ValueError, "outside"):
                        layout(src)
                finally:
                    os.rmdir(outside)
                # A dangling registry link is not a missing registry: it fails closed.
                (src / "manifests/formats.json").unlink()
                (src / "manifests/formats.json").symlink_to(src / "manifests/no-such-registry.json")
                (src / "manifests/cases.jsonl").write_text("")
                with self.assertRaisesRegex(ValueError, "formats.json"):
                    layout(src)

    def test_yaml_record(self):
        ns = {}
        exec(_function_sources((ROOT / SOURCE_READERS[0]).read_text())["yaml_record"], {"json": json}, ns)
        yaml_record = ns["yaml_record"]
        self.assertTrue(yaml_record({"format": "yaml"}, True))
        self.assertFalse(yaml_record({"format": "json"}, True))
        self.assertFalse(yaml_record({"format": "json"}, False))
        self.assertTrue(yaml_record({}, False))
        with self.assertRaises(ValueError):
            yaml_record({}, True)
        with self.assertRaises(ValueError):
            yaml_record({"format": 1}, False)

    def test_gate_fails_closed_on_bad_registry(self):
        for label, doc in _bad_registries():
            with self.subTest(label), tempfile.TemporaryDirectory() as tmp:
                src = Path(tmp) / "src"
                (src / "manifests").mkdir(parents=True)
                (src / "manifests/formats.json").write_text(doc)
                result = subprocess.run(
                    [sys.executable, "-I", str(ROOT / SOURCE_READERS[0]), str(ROOT / "conformance/pins/pin-2f055c1.json"),
                     str(Path(tmp) / "hyd"), str(src), str(Path(tmp) / "driver")],
                    capture_output=True, text=True, timeout=10,
                )
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("FAIL", result.stdout)
                self.assertIn("formats.json", result.stdout)

    def test_probe_scripts_fail_closed_on_bad_registry(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            (src / "manifests").mkdir(parents=True)
            (src / "manifests/formats.json").write_text(json.dumps(_registry(_yaml_entry(), version="formats/v2")))
            pin = str(ROOT / "conformance/pins/pin-2f055c1.json")
            hyd, drv, out = (str(Path(tmp) / n) for n in ("hyd", "driver", "out.json"))
            for script, args in (
                (SOURCE_READERS[1], [pin, hyd, str(src), drv]),
                (SOURCE_READERS[2], [str(src), drv]),
                (SOURCE_READERS[3], [str(src), drv]),
                (SOURCE_READERS[4], [str(src), drv, out]),
            ):
                with self.subTest(script):
                    result = subprocess.run([sys.executable, "-I", str(ROOT / script), *args],
                                            capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn("structcorpus layout:", result.stderr)

    def test_probe_copy_has_private_refs(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            main, wt = tmp / "main", tmp / "wt"
            run = lambda d, *a: subprocess.run(["git", "-C", str(d), *a], capture_output=True, text=True, check=True).stdout.strip()
            ident = ["-c", "user.name=t", "-c", "user.email=t@example.invalid"]
            main.mkdir()
            run(main, "init", "-q")
            (main / "f.txt").write_text("x\n")
            run(main, "add", "f.txt")
            run(main, *ident, "commit", "-q", "-m", "c1")
            run(main, *ident, "tag", "-a", "-m", "t", "v1")
            obj, c1 = run(main, "rev-parse", "refs/tags/v1"), run(main, "rev-parse", "HEAD")
            run(main, "worktree", "add", "-q", "--detach", str(wt), "v1")
            self.assertTrue((wt / ".git").is_file())
            src_gitdir = Path(run(wt, "rev-parse", "--absolute-git-dir"))
            run(wt, "update-index", "--assume-unchanged", "f.txt")
            index_before = (src_gitdir / "index").read_bytes()
            head_before = (src_gitdir / "HEAD").read_bytes()
            # Inherited variables that would point git at the source repository are dropped.
            with mock.patch.dict(os.environ, {"GIT_INDEX_FILE": str(src_gitdir / "index"), "GIT_DIR": str(src_gitdir)}):
                private_copy = _script_names(SOURCE_READERS[1], "private_copy")["private_copy"]
                copy = private_copy(wt, tmp / "copy", tmp / "meta")
            self.assertEqual((src_gitdir / "index").read_bytes(), index_before)
            self.assertEqual((src_gitdir / "HEAD").read_bytes(), head_before)
            self.assertTrue((copy / ".git" / "index").is_file())
            self.assertTrue((copy / ".git").is_dir())
            self.assertEqual(run(copy, "status", "--porcelain", "--untracked-files=no"), "")
            self.assertEqual(run(copy, "rev-parse", "HEAD"), c1)
            self.assertEqual(run(copy, "rev-parse", "refs/tags/v1"), obj)
            run(copy, "update-ref", "refs/tags/v1", c1)
            (copy / "f.txt").write_text("y\n")
            self.assertEqual(run(main, "rev-parse", "refs/tags/v1"), obj)
            self.assertEqual(run(main, "cat-file", "-t", "v1"), "tag")
            self.assertEqual((wt / "f.txt").read_text(), "x\n")

    def test_probe_writes_refuse_symlinks(self):
        write_inside = _script_names(SOURCE_READERS[1], "write_inside")["write_inside"]
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            root, outside = tmp / "root", tmp / "outside"
            (root / "d").mkdir(parents=True)
            outside.mkdir()
            (outside / "f").write_text("x\n")
            (root / "d/real").write_text("a\n")
            write_inside(root, "d/real", b"b\n")
            self.assertEqual((root / "d/real").read_text(), "b\n")
            (root / "d/link").symlink_to(outside / "f")
            (root / "dirlink").symlink_to(outside)
            for rel in ("d/link", "dirlink/f"):
                with self.subTest(rel), self.assertRaises(SystemExit):
                    write_inside(root, rel, b"Z\n")
            self.assertEqual((outside / "f").read_text(), "x\n")

    def test_registry_pin_shape(self):
        pins = ROOT / "conformance/pins"
        new = json.loads((pins / "pin-2f055c1.json").read_text())
        old = json.loads((pins / "pin-b52fcd0.json").read_text())
        self.assertEqual(new["corpus_commit"], "2f055c10bffb494dbe395a5d8851b374fde07322")
        for key in ("cases", "review_status", "rows", "hydrated", "tier", "schema", "repository"):
            self.assertEqual(new[key], old[key], key)
        self.assertIn("manifests/formats.json", new["files"])
        self.assertIn("manifests/yaml/cases.jsonl", new["files"])
        self.assertFalse([f for f in new["files"] if f.startswith(("manifests/profiles/", "manifests/span-conventions/"))])
        self.assertEqual(len(new["files"]), len(old["files"]) + 1)

    def test_release_pin_shape(self):
        pins = ROOT / "conformance/pins"
        new = json.loads((pins / "pin-d04cd32.json").read_text())
        old = json.loads((pins / "pin-2f055c1.json").read_text())
        self.assertEqual(new["corpus_commit"], "d04cd325262717b2fcec1884536c6f72398e3b63")
        self.assertEqual(new["corpus_tag"], {"name": "v0.1.0", "object": "d9d37e9d937041cebc41c31380dd02381f34dc23"})
        self.assertEqual({k: v for k, v in new.items() if k not in ("corpus_commit", "corpus_tag")},
                         {k: v for k, v in old.items() if k != "corpus_commit"})

    def test_gate_checks_corpus_tag(self):
        tag_problems = _script_names(SOURCE_READERS[0], "tag_problems")["tag_problems"]
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp)
            run = lambda *a: subprocess.run(["git", "-C", tmp, *a], capture_output=True, text=True, check=True).stdout.strip()
            run("init", "-q")
            run("-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "-q", "--allow-empty", "-m", "c1")
            c1 = run("rev-parse", "HEAD")
            run("-c", "user.name=t", "-c", "user.email=t@example.invalid", "tag", "-a", "-m", "t", "v1")
            obj = run("rev-parse", "refs/tags/v1")
            self.assertEqual(tag_problems(src, {"name": "v1", "object": obj}, c1), [])
            self.assertTrue(tag_problems(src, {"name": "v2", "object": obj}, c1))
            self.assertTrue(tag_problems(src, {"name": "v1", "object": obj}, "0" * 40))
            self.assertTrue(tag_problems(src, {"name": "v1"}, c1))
            for bad in (None, [], "v1", 1):
                self.assertTrue(tag_problems(src, bad, c1), bad)
            # A revision expression naming the same object is not the tag name.
            for name in ("v1^{tag}", "v1^0", "refs/tags/v1", "v1~0", "@", "v1..v1"):
                self.assertTrue(tag_problems(src, {"name": name, "object": obj}, c1), name)
            self.assertTrue(tag_problems(src, {"name": "v1", "object": obj[:12]}, c1))
            self.assertTrue(tag_problems(src, {"name": "v1\x00", "object": obj}, c1))
            # An inherited GIT_DIR naming another repository is not consulted.
            other = Path(tmp) / "other"
            other.mkdir()
            run_o = lambda *a: subprocess.run(["git", "-C", str(other), *a], capture_output=True, text=True, check=True)
            run_o("init", "-q")
            with mock.patch.dict(os.environ, {"GIT_DIR": str(other / ".git")}):
                scoped = _script_names(SOURCE_READERS[0], "tag_problems")["tag_problems"]
                self.assertEqual(scoped(src, {"name": "v1", "object": obj}, c1), [])
            run("tag", "-f", "v1", c1)
            problems = tag_problems(src, {"name": "v1", "object": obj}, c1)
            self.assertTrue(any("not an annotated tag" in p for p in problems), problems)


if __name__ == "__main__":
    unittest.main()
