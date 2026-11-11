# SPDX-License-Identifier: MIT OR Apache-2.0
"""Conformance vector parity harness for Python `yamlprims` against `conformance/vectors/cases.jsonl`."""

from __future__ import annotations

import json
import os
import pathlib
import unittest
from typing import Any

from yamlprims import (
    Limits,
    Location,
    Options,
    YamlprimsError,
    decode_wire,
    default_emit_options,
    default_options,
    dump_all,
    encode,
    encode_error,
    encode_plain,
    load_all_bytes,
    load_bytes,
    load_ijson,
    load_plain,
    validate_options,
)

ROOT = pathlib.Path(__file__).resolve().parents[3]
VECTORS_FILE = pathlib.Path(
    os.environ.get("YAMLPRIMS_VECTORS_FILE", ROOT / "conformance/vectors/cases.jsonl")
)


def _build_options(rec: dict[str, Any]) -> Options:
    raw_merge = rec.get("merge") or "reject"
    merge_keys = "as_key" if raw_merge in ("as-key", "plain-key") else raw_merge
    unknown_tags = "reject" if rec.get("unknown_tags") == "reject" else "preserve"
    return validate_options(
        {
            "merge_keys": merge_keys,
            "unknown_tags": unknown_tags,
            "limits": rec.get("limits") or {},
        }
    )


def _widen_limits_for_round_trip(opts: Options) -> Options:
    def_lim = default_options().limits
    kw: dict[str, int] = {}
    for field_name in def_lim.__dataclass_fields__:
        kw[field_name] = max(getattr(opts.limits, field_name), getattr(def_lim, field_name))
    return Options(
        merge_keys=opts.merge_keys,
        unknown_tags=opts.unknown_tags,
        limits=Limits(**kw),
    )


class TestCorpus(unittest.TestCase):
    def test_conformance_vectors_parity(self) -> None:
        raw_lines = [
            line
            for line in VECTORS_FILE.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
        self.assertGreaterEqual(len(raw_lines), 150, f"expected >= 150 vectors, got {len(raw_lines)}")

        def_opts = default_options()
        out_lines: list[str] = []

        for line in raw_lines:
            rec = json.loads(line)
            cid = rec["id"]

            if isinstance(rec.get("syntax_code"), str):
                code = rec["syntax_code"]
                err = YamlprimsError(
                    kind="syntax",
                    message=f"syntax error: {code}",
                    detail={"syntax_kind": code},
                    site={"source": Location(byte_offset=0, line=1, column=1)},
                )
                wire = encode_error(err, def_opts)
                decoded = decode_wire(wire, def_opts)
                self.assertEqual(decoded.kind, "error", f"{cid}: expected error envelope")
                assert decoded.error is not None
                rt = encode_error(decoded.error, def_opts)
                self.assertEqual(rt, wire, f"{cid}: syntax_code round-trip mismatch")
                out_lines.append(
                    json.dumps(
                        {"id": cid, "plain": "", "wire": wire, "emit": "", "ijson": ""},
                        separators=(",", ":"),
                        ensure_ascii=False,
                    )
                )
                continue

            if isinstance(rec.get("input_hex"), str):
                input_bytes = bytes.fromhex(rec["input_hex"])
            else:
                input_bytes = rec["input"].encode("utf-8")

            opts = _build_options(rec)
            single = bool(rec.get("single"))
            check_plain = bool(rec.get("check_plain"))
            rt_opts = _widen_limits_for_round_trip(opts)

            loaded_docs = None
            try:
                docs = [load_bytes(input_bytes, opts)] if single else load_all_bytes(input_bytes, opts)
                loaded_docs = docs
                try:
                    wire = encode(docs, opts)
                except YamlprimsError as enc_err:
                    wire = encode_error(enc_err, rt_opts)
            except YamlprimsError as load_err:
                wire = encode_error(load_err, rt_opts)

            decoded = decode_wire(wire, rt_opts)
            if decoded.kind == "documents":
                assert decoded.documents is not None
                rt = encode(decoded.documents, rt_opts)
            else:
                assert decoded.error is not None
                rt = encode_error(decoded.error, rt_opts)
            self.assertEqual(rt, wire, f"{cid}: decode_wire -> encode round-trip mismatch")

            plain = ""
            kw_wide_out = {
                k: getattr(opts.limits, k)
                for k in opts.limits.__dataclass_fields__
            }
            kw_wide_out["max_output_bytes"] = max(
                opts.limits.max_output_bytes, def_opts.limits.max_output_bytes
            )
            load_opts_wide_out = Options(
                merge_keys=opts.merge_keys,
                unknown_tags=opts.unknown_tags,
                limits=Limits(**kw_wide_out),
            )
            if check_plain:
                try:
                    doc = load_bytes(input_bytes, load_opts_wide_out)
                    plain_via_host = encode_plain(doc, opts)
                except YamlprimsError as err:
                    plain_via_host = encode_error(err, rt_opts)

                try:
                    plain_via_native = load_plain(input_bytes, opts)
                except YamlprimsError as err:
                    plain_via_native = encode_error(err, rt_opts)

                self.assertEqual(
                    plain_via_native,
                    plain_via_host,
                    f"{cid}: load_plain vs load_bytes+encode_plain mismatch",
                )
                plain = plain_via_host

            emit_out = ""
            try:
                docs_for_emit = (
                    [load_bytes(input_bytes, load_opts_wide_out)]
                    if single
                    else load_all_bytes(input_bytes, load_opts_wide_out)
                )
                try:
                    emit_out = dump_all(docs_for_emit, default_emit_options())
                except YamlprimsError as emit_err:
                    emit_out = encode_error(emit_err, rt_opts)
            except YamlprimsError:
                emit_out = ""

            ijson_opts = Options(
                merge_keys=opts.merge_keys,
                unknown_tags="reject",
                limits=opts.limits,
            )
            try:
                ijson_out = load_ijson(input_bytes, ijson_opts)
            except YamlprimsError as ijson_err:
                ijson_out = encode_error(ijson_err, rt_opts)

            out_lines.append(
                json.dumps(
                    {
                        "id": cid,
                        "plain": plain,
                        "wire": wire,
                        "emit": emit_out,
                        "ijson": ijson_out,
                    },
                    separators=(",", ":"),
                    ensure_ascii=False,
                )
            )

        results_out = os.environ.get("YAMLPRIMS_RESULTS_OUT")
        if results_out:
            pathlib.Path(results_out).write_text("\n".join(out_lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    unittest.main()
