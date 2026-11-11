# SPDX-License-Identifier: MIT OR Apache-2.0
"""API tests for the Python `yamlprims` binding."""

from __future__ import annotations

import array
import math
import mmap
import tempfile
import unittest

import yamlprims
from yamlprims import (
    CANONICAL_NAN_BITS,
    DEFAULT_EMIT_MAX_OUTPUT_BYTES,
    DEFAULT_EMIT_MAX_TAG_BYTES,
    DEFAULT_IJSON_MAX_OUTPUT_BYTES,
    I128_MAX,
    I128_MIN,
    U128_MAX,
    Document,
    EmitLimits,
    EmitOptions,
    Limits,
    Location,
    Mapping,
    Node,
    Options,
    PathSegment,
    Span,
    Tag,
    Tagged,
    YamlprimsError,
    decode_wire,
    default_emit_limits,
    default_emit_options,
    default_ijson_options,
    default_limits,
    default_options,
    dump,
    dump_all,
    encode,
    encode_error,
    encode_plain,
    from_plain,
    key_eq,
    load,
    load_all,
    load_all_bytes,
    load_bytes,
    load_ijson,
    load_plain,
    to_plain,
    validate_emit_options,
)


class TestApi(unittest.TestCase):
    def test_load_and_buffer_protocols(self) -> None:
        raw = b"a: 1\nb: hello\n"
        with tempfile.TemporaryFile() as f:
            f.write(raw)
            f.flush()
            with mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as mapped:
                for buf in (
                    raw,
                    bytearray(raw),
                    memoryview(raw),
                    memoryview(b"xx" + raw)[2:],
                    array.array("B", raw),
                    mapped,
                ):
                    doc = load_bytes(buf)
                    self.assertEqual(to_plain(doc), {"a": 1, "b": "hello"})
                    docs = load_all_bytes(buf)
                    self.assertEqual(len(docs), 1)
                    self.assertEqual(load_plain(buf), '{"a":1,"b":"hello"}')

        # str input works on load / load_all / load_plain, rejected on load_bytes / load_all_bytes
        doc_str = load("a: 1\nb: hello\n")
        self.assertEqual(to_plain(doc_str), {"a": 1, "b": "hello"})
        self.assertEqual(len(load_all("a: 1\n---\nb: 2\n")), 2)
        with self.assertRaises(TypeError):
            load_bytes("a: 1\n")  # type: ignore[arg-type]
        with self.assertRaises(TypeError):
            load_all_bytes("a: 1\n")  # type: ignore[arg-type]

        # Lone surrogate in str input raises TypeError
        with self.assertRaises(TypeError):
            load("a: \ud800\n")

    def test_mapping_no_bool_int_float_key_collision(self) -> None:
        # In Python dict, True == 1 == 1.0 collide. In yamlprims.Mapping they are distinct!
        # Meanwhile -0.0 and 0.0 ARE equal keys in YAML 1.2.2 and must be rejected as duplicate_key.
        with self.assertRaises(YamlprimsError) as cm:
            load("-0.0: neg_zero_val\n0.0: pos_zero_val\n")
        self.assertEqual(cm.exception.kind, "duplicate_key")

    def test_mapping_distinct_keys_and_lookup(self) -> None:
        doc = load("true: bool_val\n1: int_val\n1.0: float_val\n'1': str_val\n")
        m = doc.root.as_mapping()
        self.assertEqual(len(m), 4)
        self.assertEqual(m[True].as_str(), "bool_val")
        self.assertEqual(m[1].as_str(), "int_val")
        self.assertEqual(m[1.0].as_str(), "float_val")
        self.assertEqual(m["1"].as_str(), "str_val")
        self.assertIn(True, m)
        self.assertIn(1, m)
        self.assertIn(1.0, m)
        self.assertIn("1", m)
        self.assertNotIn(False, m)
        self.assertIsNone(m.get(False))

        # -0.0 and +0.0 are equal under key_eq (numeric zero equality)
        self.assertTrue(key_eq(Node.from_float(-0.0), Node.from_float(0.0)))
        self.assertFalse(key_eq(Node.from_bool(True), Node.from_int(1)))
        self.assertFalse(key_eq(Node.from_int(1), Node.from_float(1.0)))

        # NaN keys are never key_eq to anything, including NaN
        nan_node = Node.from_float(float("nan"))
        self.assertFalse(key_eq(nan_node, nan_node))

        # Order-independent composite mapping key equality
        m_ab = Node(
            kind="mapping",
            value=[(Node.from_str("a"), Node.from_int(1)), (Node.from_str("b"), Node.from_int(2))],
        )
        m_ba = Node(
            kind="mapping",
            value=[(Node.from_str("b"), Node.from_int(2)), (Node.from_str("a"), Node.from_int(1))],
        )
        self.assertTrue(key_eq(m_ab, m_ba))
        with self.assertRaises(YamlprimsError) as cm_dup:
            Mapping([(m_ab, Node.from_str("first")), (m_ba, Node.from_str("second"))])
        self.assertEqual(cm_dup.exception.kind, "duplicate_key")

        # Deep immutability of public Node(kind="sequence", value=...) and Mapping keys:
        # mutating the caller's list after construction cannot mutate the Node or Mapping keys.
        caller_items = [Node.from_str("a")]
        k1 = Node(kind="sequence", value=caller_items)
        k2 = Node(kind="sequence", value=[Node.from_str("b")])
        comp_map = Mapping([(k1, Node.from_int(10)), (k2, Node.from_int(20))])
        caller_items[0] = Node.from_str("b")
        caller_items.append(Node.from_str("extra"))

        self.assertEqual(k1.as_sequence(), (Node.from_str("a"),))
        self.assertEqual(comp_map[Node.from_sequence([Node.from_str("a")])].as_int(), 10)
        self.assertEqual(comp_map[Node.from_sequence([Node.from_str("b")])].as_int(), 20)
        wire = encode([Document(root=Node.from_mapping(comp_map))])
        self.assertEqual(decode_wire(wire).kind, "documents")

    def test_128_bit_integers(self) -> None:
        doc = load(f"min: {I128_MIN}\nmax_i: {I128_MAX}\nmax_u: {U128_MAX}\n")
        m = doc.root.as_mapping()
        self.assertEqual(m["min"].as_int(), I128_MIN)
        self.assertEqual(m["max_i"].as_int(), I128_MAX)
        self.assertEqual(m["max_u"].as_int(), U128_MAX)

        # Out-of-range integers rejected by Node.from_int and encode
        with self.assertRaises(YamlprimsError) as cm:
            Node.from_int(U128_MAX + 1)
        self.assertEqual(cm.exception.kind, "integer_out_of_range")

        with self.assertRaises(YamlprimsError) as cm2:
            Node.from_int(I128_MIN - 1)
        self.assertEqual(cm2.exception.kind, "integer_out_of_range")

        # Bool rejected by Node.from_int
        with self.assertRaises(TypeError):
            Node.from_int(True)

    def test_float_bits_and_special_values(self) -> None:
        doc = load("neg_zero: -0.0\nnan_val: .nan\npinf: .inf\nninf: -.inf\n")
        m = doc.root.as_mapping()
        nz = m["neg_zero"]
        self.assertEqual(nz.bits, "8000000000000000")
        self.assertTrue(math.copysign(1.0, nz.as_float()) < 0)

        nan_node = m["nan_val"]
        self.assertEqual(nan_node.bits, CANONICAL_NAN_BITS)
        self.assertTrue(math.isnan(nan_node.as_float()))

        self.assertEqual(m["pinf"].bits, "7ff0000000000000")
        self.assertEqual(m["ninf"].bits, "fff0000000000000")

        # Signaled NaN bits are canonicalized on Node.from_float
        snan = Node.from_float(float("nan"), bits="7ff0000000000001")
        self.assertEqual(snan.bits, CANONICAL_NAN_BITS)

    def test_to_plain_and_encode_plain_loss_matrix(self) -> None:
        # Non-string mapping key rejected by to_plain and encode_plain
        doc_int_key = load("1: one\n")
        with self.assertRaises(YamlprimsError) as cm:
            to_plain(doc_int_key)
        self.assertEqual(cm.exception.kind, "type")
        self.assertEqual(cm.exception.detail, {"expected": "plain JSON value", "found": "non-string key"})

        with self.assertRaises(YamlprimsError) as cm2:
            encode_plain(doc_int_key)
        self.assertEqual(cm2.exception.kind, "type")

        # Tagged value rejected by to_plain and encode_plain
        doc_tag = load("a: !custom hello\n")
        with self.assertRaises(YamlprimsError) as cm3:
            to_plain(doc_tag)
        self.assertEqual(cm3.exception.kind, "type")
        self.assertEqual(
            cm3.exception.detail,
            {"expected": "plain JSON value", "found": "tagged value"},
        )

        with self.assertRaises(YamlprimsError) as cm4:
            encode_plain(doc_tag)
        self.assertEqual(cm4.exception.kind, "type")

        # Non-finite float allowed in to_plain (Python float supports nan/inf), rejected in encode_plain
        doc_nan = load("a: .nan\n")
        plain_obj = to_plain(doc_nan)
        self.assertTrue(math.isnan(plain_obj["a"]))
        with self.assertRaises(YamlprimsError) as cm5:
            encode_plain(doc_nan)
        self.assertEqual(cm5.exception.kind, "type")
        self.assertEqual(
            cm5.exception.detail,
            {"expected": "plain JSON value", "found": "non-finite float"},
        )

    def test_cycle_detection_and_tree_limits(self) -> None:
        # Public Node("sequence", value=items) freezes items into an immutable tuple at construction.
        items: list[Node] = []
        seq_node = Node(kind="sequence", value=items)
        items.append(seq_node)
        self.assertEqual(seq_node.as_sequence(), ())

        # Defense-in-depth: if a cyclic reference is injected via object.__setattr__,
        # encode, to_plain, and encode_plain still detect the cycle and fail with max_depth.
        object.__setattr__(seq_node, "value", (seq_node,))

        with self.assertRaises(YamlprimsError) as cm:
            encode([Document(root=seq_node)])
        self.assertEqual(cm.exception.kind, "limit_exceeded")
        self.assertEqual(cm.exception.detail["limit"], "max_depth")

        with self.assertRaises(YamlprimsError) as cm2:
            to_plain(seq_node)
        self.assertEqual(cm2.exception.kind, "limit_exceeded")
        self.assertEqual(cm2.exception.detail["limit"], "max_depth")

        with self.assertRaises(YamlprimsError) as cm3:
            encode_plain(seq_node)
        self.assertEqual(cm3.exception.kind, "limit_exceeded")
        self.assertEqual(cm3.exception.detail["limit"], "max_depth")

        # Shared DAG (diamond, non-cyclic) succeeds within max_nodes and fails when exceeding max_nodes
        leaf = Node.from_str("shared")
        pair = Node.from_sequence([leaf, leaf])
        root = Node.from_sequence([pair, pair])
        self.assertEqual(to_plain(root), [["shared", "shared"], ["shared", "shared"]])
        with self.assertRaises(YamlprimsError) as cm4:
            to_plain(root, Options(limits=Limits(max_nodes=4)))
        self.assertEqual(cm4.exception.kind, "limit_exceeded")
        self.assertEqual(cm4.exception.detail["limit"], "max_nodes")

    def test_options_and_limits_validation(self) -> None:
        self.assertEqual(default_limits().max_depth, 128)
        self.assertEqual(default_options().merge_keys, "reject")

        with self.assertRaises(YamlprimsError) as cm:
            Limits(max_depth=0).validate()
        self.assertEqual(cm.exception.kind, "invalid_options")

        with self.assertRaises(YamlprimsError) as cm2:
            Limits(max_depth=256).validate()
        self.assertEqual(cm2.exception.kind, "invalid_options")

        with self.assertRaises(YamlprimsError) as cm3:
            Limits(max_depth=True).validate()  # type: ignore[arg-type]
        self.assertEqual(cm3.exception.kind, "invalid_options")

        with self.assertRaises(YamlprimsError) as cm4:
            Limits(max_scalar_bytes=2048, max_owned_bytes=1024).validate()
        self.assertEqual(cm4.exception.kind, "invalid_options")

    def test_strict_decode_wire_negative_controls(self) -> None:
        bad_envelopes = [
            # Duplicate JSON key
            '{"format":"yamlprims-value","format":"yamlprims-value","version":1,"documents":[]}',
            # Unknown top-level field
            '{"format":"yamlprims-value","version":1,"documents":[],"extra":1}',
            # Unknown format / version
            '{"format":"other","version":1,"documents":[]}',
            '{"format":"yamlprims-value","version":2,"documents":[]}',
            # Both documents and error
            '{"format":"yamlprims-value","version":1,"documents":[],"error":{}}',
            # Unknown node kind
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"mystery","span":null,"via":null,"tag_start":null}}]}',
            # Non-canonical integer (leading zero / + sign / > u128::MAX)
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"int","value":"01","span":null,"via":null,"tag_start":null}}]}',
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"int","value":"+1","span":null,"via":null,"tag_start":null}}]}',
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"int","value":"340282366920938463463374607431768211456","span":null,"via":null,"tag_start":null}}]}',
            # Invalid / uppercase float bits or extra value field on float
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"float","bits":"3FF0000000000000","span":null,"via":null,"tag_start":null}}]}',
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"float","bits":"3ff000","span":null,"via":null,"tag_start":null}}]}',
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"float","value":1.0,"bits":"3ff0000000000000","span":null,"via":null,"tag_start":null}}]}',
            # yaml_version component > u32::MAX
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":[1,4294967296],"explicit_start":false,"root":{"kind":"null","span":null,"via":null,"tag_start":null}}]}',
            # NaN mapping key in wire
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"mapping","entries":[{"key":{"kind":"float","value":"nan","bits":"7ff8000000000000","span":null,"via":null,"tag_start":null},"value":{"kind":"null","span":null,"via":null,"tag_start":null}}],"span":null,"via":null,"tag_start":null}}]}',
            # Duplicate mapping key in wire
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"mapping","entries":[{"key":{"kind":"int","value":"1","span":null,"via":null,"tag_start":null},"value":{"kind":"null","span":null,"via":null,"tag_start":null}},{"key":{"kind":"int","value":"1","span":null,"via":null,"tag_start":null},"value":{"kind":"null","span":null,"via":null,"tag_start":null}}],"span":null,"via":null,"tag_start":null}}]}',
            # Nested Tagged-in-Tagged in wire
            '{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"tagged","tag":{"uri":"!a","handle":"!","suffix":"a"},"value":{"kind":"tagged","tag":{"uri":"!b","handle":"!","suffix":"b"},"value":{"kind":"null","span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null}}]}',
            # Unknown syntax_kind in error envelope
            '{"format":"yamlprims-value","version":1,"error":{"kind":"syntax","detail":{"syntax_kind":"not_a_real_syntax_kind"},"site":{"source":[0,1,1]},"related":[],"path":[],"message":"bad"}}',
            # Non-null location on error PathSegment.key node
            '{"format":"yamlprims-value","version":1,"error":{"kind":"duplicate_key","detail":{},"site":{"source":[0,1,1]},"related":[],"path":[{"key":{"kind":"string","value":"a","span":[0,1,1,1,1,2],"via":null,"tag_start":null}}],"message":"dup"}}',
        ]
        for wire in bad_envelopes:
            with self.subTest(wire=wire):
                with self.assertRaises(YamlprimsError) as cm:
                    decode_wire(wire)
                self.assertEqual(cm.exception.kind, "internal")

        # Nested Tagged-in-Tagged rejected by Node.from_tagged, Node(kind="tagged", ...), and encode
        inner_tagged = Node(
            kind="tagged",
            value=Tagged(tag=Tag(uri="!b", handle="!", suffix="b"), value=Node.from_str("x")),
        )
        with self.assertRaises(YamlprimsError) as cm_tag:
            Node.from_tagged(Tag(uri="!a", handle="!", suffix="a"), inner_tagged)
        self.assertEqual(cm_tag.exception.kind, "tag_kind_mismatch")

        with self.assertRaises(YamlprimsError) as cm_ctor:
            Node(
                kind="tagged",
                value=Tagged(tag=Tag(uri="!a", handle="!", suffix="a"), value=inner_tagged),
            )
        self.assertEqual(cm_ctor.exception.kind, "tag_kind_mismatch")

        bypass_outer = Node.from_tagged(Tag(uri="!a", handle="!", suffix="a"), Node.from_str("x"))
        bypass_inner = object.__new__(Tagged)
        object.__setattr__(bypass_inner, "tag", Tag(uri="!a", handle="!", suffix="a"))
        object.__setattr__(bypass_inner, "value", inner_tagged)
        object.__setattr__(bypass_outer, "value", bypass_inner)
        with self.assertRaises(YamlprimsError) as cm_enc:
            encode([Document(root=bypass_outer)])
        self.assertEqual(cm_enc.exception.kind, "internal")

    def test_native_status_3_maps_to_internal_yamlprims_error(self) -> None:
        import yamlprims as yp

        with self.assertRaises(YamlprimsError) as cm:
            yp._handle_native_status(3, "native panic in yamlprims", default_options())
        self.assertEqual(cm.exception.kind, "internal")
        self.assertEqual(cm.exception.message, "native panic in yamlprims")
        self.assertIsNone(cm.exception.location())

    def test_spec_v15_checked_constructors_and_deep_trees(self) -> None:
        t1 = Tag.try_new("!custom")
        t2 = Tag.with_spelling("!custom", "!", "custom")
        self.assertEqual(t1, t2)
        self.assertEqual(hash(t1), hash(t2))

        with self.assertRaises(YamlprimsError) as cm:
            Tag.try_new("tag:yaml.org,2002:str")
        self.assertEqual(cm.exception.kind, "invalid_options")

        with self.assertRaises(YamlprimsError) as cm2:
            Tagged.new(Tag.try_new("!custom"), Node.null())
        self.assertEqual(cm2.exception.kind, "tag_kind_mismatch")

        doc = Document.with_metadata(Node.string("ok"), (1, 2), True)
        self.assertEqual(doc.yaml_version, (1, 2))
        self.assertTrue(doc.explicit_start)
        with self.assertRaises(YamlprimsError) as cm_doc:
            Document.with_metadata(Node.string("no"), (2, 0), False)
        self.assertEqual(cm_doc.exception.kind, "unsupported_yaml_version")

        # 2,000-deep sequence should compare and repr without Python RecursionError
        cur_a = Node.string("leaf")
        cur_b = Node.string("leaf")
        for _ in range(2000):
            cur_a = Node.sequence([cur_a])
            cur_b = Node.sequence([cur_b])
        self.assertEqual(cur_a, cur_b)
        self.assertIn("...", repr(cur_a))

    def test_spec_v15_decode_wire_limits_and_invariants(self) -> None:
        wire = encode([Document.new(Node.sequence([Node.sequence([])]))])
        # max_input_bytes enforced on wire_json
        with self.assertRaises(YamlprimsError) as cm_in:
            decode_wire(wire, Options(limits=Limits(max_input_bytes=10)))
        self.assertEqual(cm_in.exception.kind, "limit_exceeded")
        self.assertEqual(cm_in.exception.detail["limit"], "max_input_bytes")

        # [[]] has loader height H = 2 -> rejected at max_depth=1
        with self.assertRaises(YamlprimsError) as cm_h:
            decode_wire(wire, Options(limits=Limits(max_depth=1)))
        self.assertEqual(cm_h.exception.kind, "limit_exceeded")
        self.assertEqual(cm_h.exception.detail["limit"], "max_depth")

        # Error path key [[]] has metric P = 1 -> accepted at max_depth=1
        err_wire = encode_error(
            YamlprimsError(
                kind="duplicate_key",
                message="dup",
                path=[PathSegment.from_key(Node.sequence([Node.sequence([])]))],
            ),
            Options(limits=Limits(max_depth=1)),
        )
        env = decode_wire(err_wire, Options(limits=Limits(max_depth=1)))
        self.assertEqual(env.kind, "error")

    def test_load_ijson_six_rules(self) -> None:
        self.assertEqual(default_ijson_options().limits.max_output_bytes, DEFAULT_IJSON_MAX_OUTPUT_BYTES)
        self.assertEqual(load_ijson("a: 1.0\nb: -0.0\nc: 1e20\nd: 1e21\n"), '{"a":1,"b":0,"c":100000000000000000000,"d":1e+21}')
        self.assertEqual(load_ijson("---\n"), "null")

        # Rule 1: empty stream and multi-doc
        for empty in ("", "# comment only\n", "...\n"):
            with self.assertRaises(YamlprimsError) as cm_empty:
                load_ijson(empty)
            self.assertEqual(cm_empty.exception.kind, "type")
            self.assertEqual(cm_empty.exception.detail["found"], "empty_stream")

        with self.assertRaises(YamlprimsError) as cm_multi:
            load_ijson("1\n---\n2\n")
        self.assertEqual(cm_multi.exception.kind, "multiple_documents")

        # Rule 2: IEEE 754 safe integer bounds
        self.assertEqual(load_ijson("9007199254740991\n"), "9007199254740991")
        with self.assertRaises(YamlprimsError) as cm_int:
            load_ijson("9007199254740992\n")
        self.assertEqual(cm_int.exception.kind, "integer_out_of_range")

        # Rule 3: Non-finite floats
        for nf in (".nan\n", ".inf\n", "-.inf\n"):
            with self.assertRaises(YamlprimsError) as cm_fl:
                load_ijson(nf)
            self.assertEqual(cm_fl.exception.kind, "float_out_of_range")

        # Rule 4: Non-string mapping keys
        with self.assertRaises(YamlprimsError) as cm_key:
            load_ijson("1: one\n")
        self.assertEqual(cm_key.exception.kind, "type")
        self.assertEqual(cm_key.exception.detail["found"], "non_string_key")

        # Rule 5: Tags and merge keys
        with self.assertRaises(YamlprimsError) as cm_tag:
            load_ijson("!custom hello\n")
        self.assertEqual(cm_tag.exception.kind, "unknown_tag")

        with self.assertRaises(YamlprimsError) as cm_as_key:
            load_ijson("<<: 1\n", merge_keys="as_key")
        self.assertEqual(cm_as_key.exception.kind, "invalid_options")

        self.assertEqual(
            load_ijson("base: &b {x: 1}\nmerged: {<<: *b, y: 2}\n", merge_keys="apply"),
            '{"base":{"x":1},"merged":{"x":1,"y":2}}',
        )

        # Rule 6: Unicode noncharacters
        with self.assertRaises(YamlprimsError) as cm_nc:
            load_ijson('"\ufdd0"\n')
        self.assertEqual(cm_nc.exception.kind, "type")
        self.assertEqual(cm_nc.exception.detail["found"], "noncharacter")

    def test_from_plain_dump_and_dump_all(self) -> None:
        self.assertEqual(default_emit_limits().max_tag_bytes, DEFAULT_EMIT_MAX_TAG_BYTES)
        self.assertEqual(default_emit_limits().max_output_bytes, DEFAULT_EMIT_MAX_OUTPUT_BYTES)
        self.assertEqual(default_emit_options().compat_quoting, "yaml11_safe")

        n = from_plain({"yes_str": "yes", "count": 42, "items": [True, None, -0.0]})
        self.assertEqual(n.kind, "mapping")

        yaml_safe = dump({"flag": "yes", "time": "10:20", "iso": "2026-11-09"})
        self.assertEqual(yaml_safe, "flag: 'yes'\ntime: '10:20'\niso: '2026-11-09'\n")

        yaml_core = dump(
            {"flag": "yes", "time": "10:20", "iso": "2026-11-09"},
            EmitOptions(compat_quoting="core_only"),
        )
        self.assertEqual(yaml_core, "flag: yes\ntime: 10:20\niso: 2026-11-09\n")

        # Multi-document dump_all + round-trip
        d1 = Document.with_metadata(from_plain({"a": 1}), (1, 2), True)
        d2 = Document.new(from_plain(["x", "y"]))
        emitted = dump_all([d1, d2])
        self.assertIn("%YAML 1.2\n---\n", emitted)
        reloaded = load_all(emitted)
        self.assertEqual(len(reloaded), 2)
        self.assertEqual(to_plain(reloaded[0]), {"a": 1})
        self.assertEqual(to_plain(reloaded[1]), ["x", "y"])

        # Lone surrogate in from_plain / dump raises InvalidUtf8 before max_scalar_bytes
        with self.assertRaises(YamlprimsError) as cm_surr:
            dump("\ud800", EmitOptions(limits=EmitLimits(max_scalar_bytes=1)))
        self.assertEqual(cm_surr.exception.kind, "invalid_utf8")

        # Output limit exceeded
        with self.assertRaises(YamlprimsError) as cm_out:
            dump(
                "hello world",
                EmitOptions(limits=EmitLimits(max_scalar_bytes=4, max_key_bytes=4, max_output_bytes=4)),
            )
        self.assertEqual(cm_out.exception.kind, "limit_exceeded")

        with self.assertRaises(YamlprimsError) as cm_out2:
            dump("hello", EmitOptions(limits=EmitLimits(max_scalar_bytes=5, max_key_bytes=5, max_output_bytes=5)))
        self.assertEqual(cm_out2.exception.kind, "output_limit_exceeded")

        # Bare Tagged is charged once (2 nodes total for Tagged + inner scalar)
        bare_tagged = Tagged(Tag.try_new("!t"), Node.string("v"))
        self.assertEqual(
            dump(bare_tagged, EmitOptions(limits=EmitLimits(max_nodes=2))),
            "!t v\n",
        )

        # dump_all bounds lazy iterables via islice and converts non-iterable TypeError to YamlprimsError("type")
        import itertools
        with self.assertRaises(YamlprimsError) as cm_gen:
            dump_all(itertools.repeat("x"), EmitOptions(limits=EmitLimits(max_documents=2)))
        self.assertEqual(cm_gen.exception.kind, "limit_exceeded")
        self.assertEqual(cm_gen.exception.detail["limit"], "max_documents")

        with self.assertRaises(YamlprimsError) as cm_noniter:
            dump_all(42)  # type: ignore[arg-type]
        self.assertEqual(cm_noniter.exception.kind, "type")

        # Malformed non-core tag URI in wire decode raises internal
        bad_tag_wire = (
            '{"format":"yamlprims-value","version":1,"documents":[{"root":{'
            '"kind":"tagged","tag":{"uri":"$:?","handle":"","suffix":"$:?"},'
            '"inner":{"kind":"string","value":"x","span":null,"via":null,"tag_start":null},'
            '"span":null,"via":null,"tag_start":null},"explicit_start":false,"yaml_version":null}]}'
        )
        with self.assertRaises(YamlprimsError) as cm_bad_tag:
            decode_wire(bad_tag_wire)
        self.assertEqual(cm_bad_tag.exception.kind, "internal")

        # Incremental preflight rejects max_nodes=1 on large list immediately and 5,000-key dict completes quickly
        with self.assertRaises(YamlprimsError) as cm_inc_nodes:
            dump([0] * 100_000, EmitOptions(limits=EmitLimits(max_nodes=1)))
        self.assertEqual(cm_inc_nodes.exception.kind, "limit_exceeded")
        self.assertEqual(cm_inc_nodes.exception.detail["limit"], "max_nodes")

        large_dict_yaml = dump({f"k{i}": i for i in range(2_000)})
        self.assertTrue(large_dict_yaml.startswith("k0: 0\n"))

        # Keyed 64-bit structural hash prevents sys.hash_info.modulus integer hash-flooding
        # and keeps key_eq comparisons O(1) (<= 5 comparisons vs 499,500 on unkeyed int hash).
        import sys
        from unittest.mock import patch
        import yamlprims as _yp_mod
        modulus = sys.hash_info.modulus
        mod_entries = [(Node.integer(i * modulus), Node.null()) for i in range(1_000)]
        with patch.object(_yp_mod, "key_eq", wraps=key_eq) as spy_eq:
            mod_map = Mapping(mod_entries)
            self.assertEqual(len(mod_map), 1_000)
            self.assertLessEqual(spy_eq.call_count, 5)
        with patch.object(_yp_mod, "key_eq", wraps=key_eq) as spy_eq_load:
            mod_yaml = "".join(f"{i * modulus}: null\n" for i in range(1_000))
            mod_doc = load(mod_yaml)
            self.assertEqual(len(mod_doc.root.value), 1_000)
            self.assertLessEqual(spy_eq_load.call_count, 5)

        # Tag.with_spelling rejects trailing newline in named handle
        with self.assertRaises(YamlprimsError) as cm_nl_handle:
            Tag.with_spelling("x:thing", "!e!\n", "thing")
        self.assertEqual(cm_nl_handle.exception.kind, "invalid_options")


if __name__ == "__main__":
    unittest.main()
