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
    I128_MAX,
    I128_MIN,
    U128_MAX,
    Document,
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
    default_limits,
    default_options,
    encode,
    encode_error,
    encode_plain,
    key_eq,
    load,
    load_all,
    load_all_bytes,
    load_bytes,
    load_plain,
    to_plain,
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
            value=Tagged(tag=Tag(uri="!b", handle="!", suffix="b"), value=Node.from_null()),
        )
        with self.assertRaises(YamlprimsError) as cm_tag:
            Node.from_tagged(Tag(uri="!a", handle="!", suffix="a"), inner_tagged)
        self.assertEqual(cm_tag.exception.kind, "internal")

        with self.assertRaises(YamlprimsError) as cm_ctor:
            Node(
                kind="tagged",
                value=Tagged(tag=Tag(uri="!a", handle="!", suffix="a"), value=inner_tagged),
            )
        self.assertEqual(cm_ctor.exception.kind, "internal")

        bypass_outer = Node.from_tagged(Tag(uri="!a", handle="!", suffix="a"), Node.from_null())
        object.__setattr__(
            bypass_outer,
            "value",
            Tagged(tag=Tag(uri="!a", handle="!", suffix="a"), value=inner_tagged),
        )
        with self.assertRaises(YamlprimsError) as cm_enc:
            encode([Document(root=bypass_outer)])
        self.assertEqual(cm_enc.exception.kind, "internal")


if __name__ == "__main__":
    unittest.main()
