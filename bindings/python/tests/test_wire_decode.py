"""Malformed-wire decode inventory and the `engine_detail` property (spec revision 1.7 §11.1, 1.7/E-6)."""

from __future__ import annotations

import json
import os
import pathlib
import unittest
from typing import Any

from yamlprims import (
    Document,
    Node,
    Options,
    Tag,
    Tagged,
    YamlprimsError,
    decode_wire,
    default_options,
    encode,
    encode_error,
    load_bytes,
    validate_options,
)

ROOT = pathlib.Path(__file__).resolve().parents[3]
CASES_FILE = pathlib.Path(
    os.environ.get(
        "YAMLPRIMS_WIRE_DECODE_CASES_FILE",
        ROOT / "conformance/vectors/wire-decode-cases.jsonl",
    )
)
EXPECTED_CASES = 35
REFUSAL = "invalid yamlprims wire payload"


def _options(rec: dict[str, Any]) -> Options | YamlprimsError:
    try:
        return validate_options({"limits": rec.get("limits") or {}})
    except YamlprimsError as err:
        return err


def _decode(rec: dict[str, Any]) -> tuple[str, str]:
    """(decode outcome, encoded result) for one record, as the crosscheck drivers report it."""
    opts = _options(rec)
    wide = default_options()
    if isinstance(opts, YamlprimsError):
        return "refused", encode_error(opts, wide)
    try:
        env = decode_wire(rec["input"], opts)
    except YamlprimsError as err:
        return "refused", encode_error(err, wide)
    if env.kind == "documents":
        return "documents", encode(list(env.documents or ()), wide)
    assert env.error is not None
    return "decoded_error", encode_error(env.error, wide)


class TestWireDecodeInventory(unittest.TestCase):
    def test_every_case_meets_its_expectation(self) -> None:
        records = [json.loads(line) for line in CASES_FILE.read_text(encoding="utf-8").splitlines() if line.strip()]
        self.assertEqual(len(records), EXPECTED_CASES)
        for rec in records:
            exp = rec["expect"]
            with self.subTest(rec["id"]):
                outcome, wire = _decode(rec)
                self.assertEqual(outcome, exp["outcome"])
                if exp.get("reencodes_input"):
                    self.assertEqual(wire, rec["input"])
                env = json.loads(wire)
                if exp["outcome"] == "documents":
                    self.assertIn("documents", env)
                    continue
                err = env["error"]
                for key in ("kind", "detail", "site", "related", "path", "message"):
                    if key in exp:
                        self.assertEqual(err[key], exp[key], key)


class TestEngineDetail(unittest.TestCase):
    def test_refusal_keeps_reason_out_of_the_wire(self) -> None:
        with self.assertRaises(YamlprimsError) as cm:
            decode_wire("{}")
        err = cm.exception
        self.assertEqual(err.kind, "internal")
        self.assertEqual(err.message, REFUSAL)
        self.assertEqual(str(err), REFUSAL)
        self.assertTrue(err.engine_detail)
        self.assertNotIn(err.engine_detail, encode_error(err, default_options()))

    def test_absent_unless_set(self) -> None:
        with self.assertRaises(YamlprimsError) as cm:
            load_bytes(b"a: [", default_options())
        self.assertIsNone(cm.exception.engine_detail)
        self.assertIsNone(YamlprimsError("internal", "x").engine_detail)

    def test_read_only(self) -> None:
        err = YamlprimsError("internal", "x", engine_detail="why")
        with self.assertRaises(AttributeError):
            err.engine_detail = "other"  # type: ignore[misc]
        self.assertEqual(err.engine_detail, "why")

    def test_bounded_on_a_utf8_boundary(self) -> None:
        err = YamlprimsError("internal", "x", engine_detail="é" * 5000)
        detail = err.engine_detail or ""
        raw = detail.encode("utf-8")
        self.assertLessEqual(len(raw), 4096)
        self.assertTrue(detail.endswith("…"))
        raw.decode("utf-8")
        long_key = "k" * 20000
        with self.assertRaises(YamlprimsError) as cm:
            decode_wire('{"format":"yamlprims-value","version":1,"' + long_key + '":0,"documents":[]}')
        self.assertEqual(cm.exception.message, REFUSAL)
        self.assertLessEqual(len((cm.exception.engine_detail or "").encode("utf-8")), 4096)

    def test_decoded_error_envelope_keeps_caller_message(self) -> None:
        text = (
            '{"format":"yamlprims-value","version":1,"error":{"kind":"internal","detail":{},'
            '"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"'
            + REFUSAL
            + '"}}'
        )
        env = decode_wire(text)
        self.assertEqual(env.kind, "error")
        assert env.error is not None
        self.assertEqual(env.error.message, REFUSAL)
        self.assertIsNone(env.error.engine_detail)
        self.assertEqual(encode_error(env.error, default_options()), text)

    def test_limits_and_options_are_not_folded(self) -> None:
        with self.assertRaises(YamlprimsError) as cm:
            decode_wire("nope", {"limits": {"max_input_bytes": 3}})
        self.assertEqual(cm.exception.kind, "limit_exceeded")
        self.assertEqual(cm.exception.message, "wire payload exceeds max_input_bytes (3)")
        with self.assertRaises(YamlprimsError) as cm:
            decode_wire("nope", {"limits": {"max_depth": 0}})
        self.assertEqual(cm.exception.kind, "invalid_options")


CONSTRUCTOR_CASES_FILE = pathlib.Path(
    os.environ.get(
        "YAMLPRIMS_CONSTRUCTOR_CASES_FILE",
        ROOT / "conformance/vectors/constructor-error-cases.json",
    )
)


class TestConstructorRefusals(unittest.TestCase):
    def test_constructor_refusals_match_the_shared_table(self) -> None:
        table = json.loads(CONSTRUCTOR_CASES_FILE.read_text(encoding="utf-8"))
        local = Tag.try_new("!t")
        merge = Tag.try_new("tag:yaml.org,2002:merge")
        calls = {
            "tag.empty-uri": lambda: Tag.try_new(""),
            "tag.core-uri": lambda: Tag.try_new("tag:yaml.org,2002:str"),
            "tag.inconsistent-spelling": lambda: Tag.with_spelling("!x", "!!", "y"),
            "tagged.bool-inner": lambda: Tagged.new(local, Node.boolean(True)),
            "tagged.merge-sequence-inner": lambda: Tagged.new(merge, Node.sequence([])),
            "document.version-2-0": lambda: Document.with_metadata(Node.null(), (2, 0), False),
        }
        self.assertEqual(sorted(calls), sorted(c["id"] for c in table["cases"]))
        for case in table["cases"]:
            with self.subTest(case["id"]):
                with self.assertRaises(YamlprimsError) as cm:
                    calls[case["id"]]()
                want = (
                    '{"format":"yamlprims-value","version":1,"error":{"kind":"' + case["kind"]
                    + '","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"'
                    + case["message"] + '"}}'
                )
                self.assertEqual(encode_error(cm.exception, default_options()), want)


class TestOutOfRangeMetadata(unittest.TestCase):
    def test_encode_refusals_match_the_shared_table(self) -> None:
        from yamlprims import Location, PathSegment

        table = json.loads(CONSTRUCTOR_CASES_FILE.read_text(encoding="utf-8"))
        big = 2**53
        ns = {"not_source": {"last_known": None}}
        errors = {
            "path-index-2^53": YamlprimsError(
                "internal", "m", site=ns, path=[PathSegment(kind="index", index=big, key=None)]
            ),
            "site-offset-2^53": YamlprimsError(
                "internal", "m", site={"source": Location(byte_offset=big, line=1, column=1)}
            ),
            "configured-2^53": YamlprimsError(
                "limit_exceeded", "m", detail={"limit": "max_nodes", "configured": big}, site=ns
            ),
            "configured-zero": YamlprimsError(
                "limit_exceeded", "m", detail={"limit": "max_nodes", "configured": 0}, site=ns
            ),
        }
        self.assertEqual(sorted(errors), sorted(c["id"] for c in table["encode_error_cases"]))
        for case in table["encode_error_cases"]:
            with self.subTest(case["id"]):
                with self.assertRaises(YamlprimsError) as cm:
                    encode_error(errors[case["id"]], default_options())
                self.assertEqual(cm.exception.kind, "internal")
                self.assertEqual(cm.exception.message, case["message"])
                self.assertEqual(cm.exception.site, ns)


class TestEmitVersionMessage(unittest.TestCase):
    def test_emit_refuses_an_unsupported_version_as_the_core_does(self) -> None:
        from yamlprims import dump_all

        doc = Document(root=Node.null(), yaml_version=(2, 0), explicit_start=False)
        with self.assertRaises(YamlprimsError) as cm:
            dump_all([doc])
        self.assertEqual(cm.exception.kind, "unsupported_yaml_version")
        self.assertEqual(cm.exception.message, "unsupported YAML directive version 2.0")


if __name__ == "__main__":
    unittest.main()
