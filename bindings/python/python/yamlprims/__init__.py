# SPDX-License-Identifier: MIT OR Apache-2.0
"""YAML 1.2.2 Core Schema value loader, strict wire decoder and canonical encoder."""

from __future__ import annotations

from dataclasses import dataclass, field
import hashlib
import itertools
import math
import os
import re
import struct
from typing import Any, Final, Iterator, Mapping as TypingMapping, Sequence, Union

from . import _yamlprims as _native

__all__ = [
    "ABI_VERSION",
    "DEFAULT_EMIT_MAX_OUTPUT_BYTES",
    "DEFAULT_EMIT_MAX_TAG_BYTES",
    "DEFAULT_IJSON_MAX_OUTPUT_BYTES",
    "Document",
    "EmitLimits",
    "EmitOptions",
    "I128_MAX",
    "I128_MIN",
    "LIMIT_KINDS",
    "Limits",
    "Location",
    "MAX_SAFE_METADATA_INT",
    "Mapping",
    "Node",
    "Options",
    "PathSegment",
    "Related",
    "SYNTAX_KINDS",
    "Span",
    "Tag",
    "Tagged",
    "U128_MAX",
    "Via",
    "WireEnvelope",
    "YamlprimsError",
    "decode_wire",
    "default_emit_limits",
    "default_emit_options",
    "default_ijson_options",
    "default_limits",
    "default_options",
    "dump",
    "dump_all",
    "encode",
    "encode_error",
    "encode_plain",
    "from_plain",
    "key_eq",
    "load",
    "load_all",
    "load_all_bytes",
    "load_bytes",
    "load_ijson",
    "load_plain",
    "to_plain",
    "validate_emit_options",
    "validate_options",
]

__version__: Final[str] = _native.VERSION

ABI_VERSION: Final[int] = 1
MAX_SAFE_METADATA_INT: Final[int] = 9_007_199_254_740_991  # 2^53 - 1
I128_MIN: Final[int] = -(1 << 127)
I128_MAX: Final[int] = (1 << 127) - 1
U128_MAX: Final[int] = (1 << 128) - 1
CANONICAL_NAN_BITS: Final[str] = "7ff8000000000000"
CANONICAL_NAN_INT: Final[int] = 0x7FF8000000000000
NODE_OVERHEAD: Final[int] = 64
ENTRY_OVERHEAD: Final[int] = 24
MAX_MESSAGE_BYTES: Final[int] = 4096
MAX_FIELD_BYTES: Final[int] = 1024
MAX_DEPTH_CEILING: Final[int] = 255
DEFAULT_IJSON_MAX_OUTPUT_BYTES: Final[int] = _native.DEFAULT_IJSON_MAX_OUTPUT_BYTES
DEFAULT_EMIT_MAX_TAG_BYTES: Final[int] = _native.DEFAULT_EMIT_MAX_TAG_BYTES
DEFAULT_EMIT_MAX_OUTPUT_BYTES: Final[int] = _native.DEFAULT_EMIT_MAX_OUTPUT_BYTES

Input = Union[bytes, bytearray, memoryview, str]

SYNTAX_KINDS: Final[frozenset[str]] = frozenset(
    {
        "unexpected_eof_flow_sequence",
        "unexpected_eof_flow_mapping",
        "unexpected_eof_implicit_flow_mapping",
        "unexpected_eof_block_sequence",
        "unexpected_eof_block_mapping",
        "unexpected_eof",
        "expected_stream_start",
        "duplicate_version_directive",
        "duplicate_tag_directive",
        "expected_document_start",
        "missing_document_end_before_directive",
        "anchor_count_overflow",
        "expected_node_content",
        "expected_block_mapping_key",
        "expected_flow_mapping_separator",
        "expected_flow_sequence_separator",
        "expected_block_sequence_entry",
        "undeclared_tag_handle",
        "expected_tag_bang",
        "expected_tag_directive_bang",
        "invalid_global_tag_character",
        "simple_key_expected",
        "invalid_simple_key",
        "invalid_document_end",
        "invalid_indentation",
        "bom_inside_document",
        "unexpected_character",
        "tab_not_allowed",
        "tab_in_block_indentation",
        "comment_intercepted_scalar",
        "expected_whitespace",
        "comment_not_separated",
        "invalid_directive_terminator",
        "missing_yaml_version_separator",
        "missing_directive_name",
        "invalid_directive_name",
        "directive_byte_limit_exceeded",
        "too_many_reserved_directive_params",
        "yaml_version_too_long",
        "missing_yaml_version",
        "invalid_tag_directive_terminator",
        "invalid_tag_terminator",
        "missing_tag_uri",
        "unclosed_verbatim_tag",
        "invalid_tag_escape",
        "invalid_tag_utf8_leading_byte",
        "invalid_tag_utf8_trailing_byte",
        "invalid_tag_utf8",
        "missing_anchor_or_alias_name",
        "misplaced_flow_collection_end",
        "mismatched_flow_collection_end",
        "unclosed_flow_collection",
        "block_entry_in_flow_collection",
        "block_sequence_entry_not_allowed",
        "invalid_block_entry_whitespace",
        "zero_block_scalar_indent",
        "invalid_block_scalar_header",
        "tab_at_block_scalar_start",
        "invalid_block_scalar_indent",
        "document_indicator_in_quoted_scalar",
        "unclosed_quoted_scalar",
        "tab_in_indentation",
        "invalid_quoted_scalar_indent",
        "invalid_trailing_single_quoted_scalar",
        "invalid_trailing_double_quoted_scalar",
        "unknown_quoted_scalar_escape",
        "invalid_quoted_scalar_hex_escape",
        "invalid_low_surrogate_hex_escape",
        "invalid_low_surrogate",
        "missing_low_surrogate",
        "unpaired_low_surrogate",
        "invalid_unicode_escape",
        "invalid_flow_scalar_indent",
        "plain_scalar_starts_with_dash_flow_indicator",
        "tab_in_plain_scalar",
        "unexpected_end_of_plain_scalar",
        "mapping_key_not_allowed",
        "flow_mapping_value_adjacent_collection",
        "invalid_mapping_value_whitespace",
        "invalid_colon_placement",
        "mapping_value_not_allowed",
    }
)

LIMIT_KINDS: Final[frozenset[str]] = frozenset(
    {
        "max_input_bytes",
        "max_depth",
        "max_nodes",
        "max_expanded_nodes",
        "max_alias_events",
        "max_owned_bytes",
        "max_scalar_bytes",
        "max_key_bytes",
        "max_mapping_entries",
        "max_documents",
    }
)

_EMPTY_DETAIL_ERROR_KINDS: Final[frozenset[str]] = frozenset(
    {
        "unsupported_yaml_version",
        "invalid_options",
        "duplicate_key",
        "invalid_key",
        "unknown_anchor",
        "recursive_alias",
        "merge_rejected",
        "merge_source",
        "unknown_tag",
        "tag_kind_mismatch",
        "tag_value_mismatch",
        "integer_out_of_range",
        "float_out_of_range",
        "multiple_documents",
        "custom",
        "internal",
    }
)

_LIMIT_FIELD_NAMES: Final[tuple[str, ...]] = (
    "max_input_bytes",
    "max_depth",
    "max_nodes",
    "max_expanded_nodes",
    "max_alias_events",
    "max_owned_bytes",
    "max_scalar_bytes",
    "max_key_bytes",
    "max_mapping_entries",
    "max_documents",
    "max_output_bytes",
    "max_warnings",
)


@dataclass(frozen=True)
class Location:
    """Zero-based byte offset and 1-based line and Unicode scalar column."""

    byte_offset: int
    line: int
    column: int


@dataclass(frozen=True)
class Span:
    """Half-open source span `[start, end)`."""

    start: Location
    end: Location


@dataclass(frozen=True)
class Via:
    """Provenance for nodes materialized via `*alias` or `<<` merge."""

    kind: str  # "alias" | "merge"
    span: Span


_CORE_TAG_URIS: Final[frozenset[str]] = frozenset(
    {
        "tag:yaml.org,2002:null",
        "tag:yaml.org,2002:bool",
        "tag:yaml.org,2002:int",
        "tag:yaml.org,2002:float",
        "tag:yaml.org,2002:str",
        "tag:yaml.org,2002:seq",
        "tag:yaml.org,2002:map",
    }
)
_URI_SCHEME_RE: Final[re.Pattern[str]] = re.compile(r"\A[A-Za-z][A-Za-z0-9+.-]*:")
_NAMED_HANDLE_RE: Final[re.Pattern[str]] = re.compile(r"\A![0-9A-Za-z-]+!\Z")
_KEY_HASH_SECRET: Final[bytes] = os.urandom(16)


def _prf_u64(payload: bytes) -> int:
    return int.from_bytes(
        hashlib.blake2b(payload, digest_size=8, key=_KEY_HASH_SECRET).digest(),
        "little",
    )


def _has_unpaired_surrogate(s: str) -> bool:
    return any(0xD800 <= ord(c) <= 0xDFFF for c in s)


def _ensure_valid_utf8_str(s: str, label: str = "string") -> int:
    if _has_unpaired_surrogate(s):
        raise _not_source_error(
            "invalid_utf8",
            {"invalid_len": None},
            f"{label} contains unpaired UTF-16 surrogate",
        )
    return len(s.encode("utf-8"))


def _is_valid_non_core_tag_uri(uri: str) -> bool:
    if not uri or uri in _CORE_TAG_URIS:
        return False
    if uri.startswith("!"):
        return len(uri) >= 2
    return _URI_SCHEME_RE.match(uri) is not None


def _is_valid_tag_spelling(uri: str, handle: str, suffix: str) -> bool:
    if handle == "":
        return uri == suffix
    if handle in ("!", "!!") or _NAMED_HANDLE_RE.fullmatch(handle) is not None:
        return bool(suffix) and uri.endswith(suffix) and len(uri) > len(suffix)
    return False


@dataclass(frozen=True)
class Tag:
    """Retained YAML tag (`uri`, `handle`, `suffix`; equality compares `uri` only per spec-v1.5 §3)."""

    uri: str
    handle: str = field(default="", compare=False)
    suffix: str = field(default="", compare=False)

    def __post_init__(self) -> None:
        if self.handle == "" and self.suffix == "" and self.uri:
            object.__setattr__(self, "suffix", self.uri)

    @staticmethod
    def new(uri: str) -> Tag:
        if not isinstance(uri, str):
            raise TypeError("Tag.new requires a str")
        return Tag(uri=uri, handle="", suffix=uri)

    @staticmethod
    def try_new(uri: str) -> Tag:
        if not isinstance(uri, str):
            raise TypeError("Tag.try_new requires a str")
        _ensure_valid_utf8_str(uri, "Tag.uri")
        if not _is_valid_non_core_tag_uri(uri):
            raise _invalid_options_error(
                "tag URI must be a non-empty local (!name) or RFC 3986 global non-core URI"
            )
        return Tag(uri=uri, handle="", suffix=uri)

    @staticmethod
    def with_spelling(uri: str, handle: str, suffix: str) -> Tag:
        if not isinstance(uri, str) or not isinstance(handle, str) or not isinstance(suffix, str):
            raise TypeError("Tag.with_spelling requires str arguments")
        _ensure_valid_utf8_str(uri, "Tag.uri")
        _ensure_valid_utf8_str(handle, "Tag.handle")
        _ensure_valid_utf8_str(suffix, "Tag.suffix")
        if not _is_valid_non_core_tag_uri(uri):
            raise _invalid_options_error(
                "tag URI must be a non-empty local (!name) or RFC 3986 global non-core URI"
            )
        if not _is_valid_tag_spelling(uri, handle, suffix):
            raise _invalid_options_error(
                "tag handle and suffix are inconsistent with tag URI"
            )
        obj = object.__new__(Tag)
        object.__setattr__(obj, "uri", uri)
        object.__setattr__(obj, "handle", handle)
        object.__setattr__(obj, "suffix", suffix)
        return obj


@dataclass(frozen=True, eq=False, repr=False)
class Tagged:
    """A tagged YAML node (`tag` + inner `Node`)."""

    tag: Tag
    value: Node

    @staticmethod
    def new(tag: Tag, value: Node) -> Tagged:
        if not isinstance(tag, Tag):
            raise TypeError("Tagged.new requires a Tag")
        if not isinstance(value, Node):
            raise TypeError("Tagged.new requires a Node")
        _ensure_valid_utf8_str(tag.uri, "Tag.uri")
        _ensure_valid_utf8_str(tag.handle, "Tag.handle")
        _ensure_valid_utf8_str(tag.suffix, "Tag.suffix")
        if not _is_valid_non_core_tag_uri(tag.uri) or not _is_valid_tag_spelling(
            tag.uri, tag.handle, tag.suffix
        ):
            raise _invalid_options_error("invalid or core tag on Tagged node")
        valid_inner = (
            value.kind == "string"
            if tag.uri == "tag:yaml.org,2002:merge"
            else value.kind in ("string", "sequence", "mapping")
        )
        if not valid_inner:
            raise _not_source_error(
                "tag_kind_mismatch",
                {},
                "tagged value inner kind is not permitted for this tag",
            )
        return Tagged(tag=tag, value=value)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Tagged):
            return NotImplemented
        return self.tag == other.tag and _nodes_equal_iterative(self.value, other.value)

    def __repr__(self) -> str:
        return f"Tagged(tag={self.tag!r}, value={_node_repr_bounded(self.value, 0)})"


@dataclass(frozen=True)
class Related:
    """Related source span attached to a `YamlprimsError`."""

    role: str
    span: Span


@dataclass(frozen=True)
class PathSegment:
    """Error path segment (`kind="index"` with `index` or `kind="key"` with synthetic `key` `Node`)."""

    kind: str  # "index" | "key"
    index: int | None = None
    key: Node | None = None

    @staticmethod
    def from_index(index: int) -> PathSegment:
        return PathSegment(kind="index", index=index, key=None)

    @staticmethod
    def from_key(key: Node) -> PathSegment:
        return PathSegment(kind="key", index=None, key=key)


_INT_CANONICAL_RE: Final[re.Pattern[str]] = re.compile(r"^-?(?:0|[1-9][0-9]*)$")
_HEX16_LOWER_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{16}$")


def _float_to_bits_hex(val: float) -> str:
    if math.isnan(val):
        return CANONICAL_NAN_BITS
    bits = struct.unpack(">Q", struct.pack(">d", val))[0]
    return f"{bits:016x}"


def _bits_hex_to_float(bits_hex: str) -> float:
    bits = int(bits_hex, 16)
    if (bits & 0x7FF0000000000000) == 0x7FF0000000000000 and (bits & 0x000FFFFFFFFFFFFF) != 0:
        return float("nan")
    return struct.unpack(">d", struct.pack(">Q", bits))[0]


def _is_neg_zero(val: float) -> bool:
    return val == 0.0 and math.copysign(1.0, val) < 0.0


@dataclass(frozen=True)
class Node:
    """Lossless YAML 1.2.2 Core Schema value node with optional source coordinates."""

    kind: str  # "null" | "bool" | "int" | "float" | "string" | "sequence" | "mapping" | "tagged"
    value: Any = None
    bits: str | None = None
    span: Span | None = None
    via: Via | None = None
    tag_start: Location | None = None

    def __post_init__(self) -> None:
        if self.kind == "null":
            if self.value is not None:
                raise TypeError("null Node value must be None")
        elif self.kind == "bool":
            if type(self.value) is not bool:
                raise TypeError("Node.boolean requires a bool")
        elif self.kind == "int":
            if type(self.value) is not int:
                raise TypeError("Node.integer requires an int (not bool)")
            if self.value < I128_MIN or self.value > U128_MAX:
                raise _not_source_error("integer_out_of_range", {}, "integer outside 128-bit range")
        elif self.kind == "float":
            if type(self.value) is bool or not isinstance(self.value, (int, float)):
                raise TypeError("Node.floating requires a float")
            fval = float(self.value)
            if self.bits is not None:
                if not isinstance(self.bits, str) or not _HEX16_LOWER_RE.match(self.bits):
                    raise _not_source_error("internal", {}, f"invalid float bits: {self.bits!r}")
                fval = _bits_hex_to_float(self.bits)
                bhex = CANONICAL_NAN_BITS if math.isnan(fval) else self.bits
            else:
                bhex = _float_to_bits_hex(fval)
            object.__setattr__(self, "value", fval)
            object.__setattr__(self, "bits", bhex)
        elif self.kind == "string":
            if not isinstance(self.value, str):
                raise TypeError("Node.string requires a str")
        elif self.kind == "sequence":
            if not isinstance(self.value, Sequence) or isinstance(
                self.value, (str, bytes, bytearray)
            ):
                raise TypeError("sequence Node value must be a sequence of Nodes")
            items_tuple = tuple(self.value)
            for it in items_tuple:
                if not isinstance(it, Node):
                    raise TypeError("sequence Node elements must be Node instances")
            object.__setattr__(self, "value", items_tuple)
        elif self.kind == "mapping":
            if isinstance(self.value, Mapping):
                m = self.value
            elif isinstance(self.value, Sequence) and not isinstance(
                self.value, (str, bytes, bytearray)
            ):
                m = Mapping(self.value)
            else:
                raise TypeError(
                    "mapping Node value must be a Mapping or sequence of (Node, Node) pairs"
                )
            object.__setattr__(self, "value", m)
        elif self.kind == "tagged":
            if not isinstance(self.value, Tagged):
                raise TypeError("tagged Node value must be a Tagged instance")
            Tagged.new(self.value.tag, self.value.value)
        else:
            raise _not_source_error("internal", {}, f"unknown Node kind: {self.kind!r}")

    @staticmethod
    def null(
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node(kind="null", value=None, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def from_null(
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node.null(span=span, via=via, tag_start=tag_start)

    @staticmethod
    def boolean(
        val: bool,
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        if type(val) is not bool:
            raise TypeError("Node.boolean requires a bool")
        return Node(kind="bool", value=val, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def from_bool(
        val: bool,
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node.boolean(val, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def integer(
        val: int,
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        if type(val) is not int:
            raise TypeError("Node.integer requires an int (not bool)")
        if val < I128_MIN or val > U128_MAX:
            raise _not_source_error("integer_out_of_range", {}, "integer outside 128-bit range")
        return Node(kind="int", value=val, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def from_int(
        val: int,
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node.integer(val, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def floating(
        val: float,
        *,
        bits: str | None = None,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        fval = float(val)
        if bits is not None:
            if not isinstance(bits, str) or not _HEX16_LOWER_RE.match(bits):
                raise _not_source_error("internal", {}, f"invalid float bits: {bits!r}")
            fval = _bits_hex_to_float(bits)
            bhex = CANONICAL_NAN_BITS if math.isnan(fval) else bits
        else:
            bhex = _float_to_bits_hex(fval)
        return Node(
            kind="float",
            value=fval,
            bits=bhex,
            span=span,
            via=via,
            tag_start=tag_start,
        )

    @staticmethod
    def from_float(
        val: float,
        *,
        bits: str | None = None,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node.floating(val, bits=bits, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def string(
        val: str,
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        if not isinstance(val, str):
            raise TypeError("Node.string requires a str")
        return Node(kind="string", value=val, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def from_str(
        val: str,
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node.string(val, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def sequence(
        items: Sequence[Node],
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node(
            kind="sequence",
            value=tuple(items),
            span=span,
            via=via,
            tag_start=tag_start,
        )

    @staticmethod
    def from_sequence(
        items: Sequence[Node],
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node.sequence(items, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def mapping(
        entries: Mapping | Sequence[tuple[Node, Node]],
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        m = entries if isinstance(entries, Mapping) else Mapping(entries)
        return Node(kind="mapping", value=m, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def from_mapping(
        entries: Mapping | Sequence[tuple[Node, Node]],
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node.mapping(entries, span=span, via=via, tag_start=tag_start)

    @staticmethod
    def tagged(
        tag: Tag,
        inner: Node,
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node(
            kind="tagged",
            value=Tagged.new(tag, inner),
            span=span,
            via=via,
            tag_start=tag_start,
        )

    @staticmethod
    def from_tagged(
        tag: Tag,
        inner: Node,
        *,
        span: Span | None = None,
        via: Via | None = None,
        tag_start: Location | None = None,
    ) -> Node:
        return Node.tagged(tag, inner, span=span, via=via, tag_start=tag_start)

    def _expect_kind(self, expected: str) -> None:
        if self.kind != expected:
            loc = self.span.start if self.span is not None else None
            site = {"source": loc} if loc is not None else {"not_source": {"last_known": None}}
            raise YamlprimsError(
                kind="type",
                message=f"expected {expected}, found {self.kind}",
                detail={"expected": expected, "found": self.kind},
                site=site,
            )

    def as_bool(self) -> bool:
        self._expect_kind("bool")
        return bool(self.value)

    def as_int(self) -> int:
        self._expect_kind("int")
        return int(self.value)

    def as_float(self) -> float:
        self._expect_kind("float")
        return float(self.value)

    def as_str(self) -> str:
        self._expect_kind("string")
        return str(self.value)

    def as_sequence(self) -> tuple[Node, ...]:
        self._expect_kind("sequence")
        return tuple(self.value)

    def as_mapping(self) -> Mapping:
        self._expect_kind("mapping")
        return self.value if isinstance(self.value, Mapping) else Mapping(self.value)

    def as_tagged(self) -> Tagged:
        self._expect_kind("tagged")
        return self.value

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Node):
            return NotImplemented
        return _nodes_equal_iterative(self, other)

    def __hash__(self) -> int:
        return _node_hash_bounded(self)

    def __repr__(self) -> str:
        return _node_repr_bounded(self, 0)


def _nodes_equal_iterative(a: Node, b: Node) -> bool:
    stack: list[tuple[Node, Node]] = [(a, b)]
    while stack:
        x, y = stack.pop()
        if x is y:
            continue
        if (
            x.kind != y.kind
            or x.span != y.span
            or x.via != y.via
            or x.tag_start != y.tag_start
        ):
            return False
        kind = x.kind
        if kind == "null":
            continue
        if kind == "bool":
            if bool(x.value) is not bool(y.value):
                return False
        elif kind == "int":
            if int(x.value) != int(y.value):
                return False
        elif kind == "float":
            if x.bits != y.bits or (not math.isnan(x.value) and x.value != y.value):
                return False
        elif kind == "string":
            if x.value != y.value:
                return False
        elif kind == "tagged":
            xt: Tagged = x.value
            yt: Tagged = y.value
            if xt.tag != yt.tag:
                return False
            stack.append((xt.value, yt.value))
        elif kind == "sequence":
            xs: Sequence[Node] = x.value
            ys: Sequence[Node] = y.value
            if len(xs) != len(ys):
                return False
            for xi, yi in zip(xs, ys):
                stack.append((xi, yi))
        elif kind == "mapping":
            xm: Mapping = x.value if isinstance(x.value, Mapping) else Mapping(x.value)
            ym: Mapping = y.value if isinstance(y.value, Mapping) else Mapping(y.value)
            if len(xm.entries) != len(ym.entries):
                return False
            for (xk, xv), (yk, yv) in zip(xm.entries, ym.entries):
                stack.append((xv, yv))
                stack.append((xk, yk))
        else:
            return False
    return True


def _node_hash_bounded(root: Node) -> int:
    h = 0x345678
    stack: list[tuple[Node, int]] = [(root, 0)]
    visited = 0
    while stack and visited < 1024:
        cur, depth = stack.pop()
        visited += 1
        h ^= hash((cur.kind, cur.bits, cur.span, cur.via, cur.tag_start, depth))
        if cur.kind in ("null", "bool", "int", "string"):
            h ^= hash(cur.value)
        elif cur.kind == "tagged":
            t: Tagged = cur.value
            h ^= hash(t.tag.uri)
            stack.append((t.value, depth))
        elif cur.kind == "sequence" and depth < MAX_DEPTH_CEILING:
            h ^= len(cur.value)
            for it in reversed(cur.value):
                stack.append((it, depth + 1))
        elif cur.kind == "mapping" and depth < MAX_DEPTH_CEILING:
            m: Mapping = cur.value if isinstance(cur.value, Mapping) else Mapping(cur.value)
            h ^= len(m.entries)
            for k, v in reversed(m.entries):
                stack.append((v, depth + 1))
                stack.append((k, depth + 1))
    return h


def _node_repr_bounded(node: Node, depth: int) -> str:
    if node.kind == "sequence":
        if depth >= MAX_DEPTH_CEILING:
            val_repr = "('...',)"
        else:
            inner = ", ".join(_node_repr_bounded(it, depth + 1) for it in node.value)
            if len(node.value) == 1:
                inner += ","
            val_repr = f"({inner})"
    elif node.kind == "mapping":
        m: Mapping = node.value if isinstance(node.value, Mapping) else Mapping(node.value)
        if depth >= MAX_DEPTH_CEILING:
            val_repr = "Mapping(['...'])"
        else:
            pairs = ", ".join(
                f"({_node_repr_bounded(k, depth + 1)}, {_node_repr_bounded(v, depth + 1)})"
                for k, v in m.entries
            )
            val_repr = f"Mapping([{pairs}])"
    elif node.kind == "tagged":
        t: Tagged = node.value
        val_repr = f"Tagged(tag={t.tag!r}, value={_node_repr_bounded(t.value, depth)})"
    else:
        val_repr = repr(node.value)
    return (
        f"Node(kind={node.kind!r}, value={val_repr}, bits={node.bits!r}, "
        f"span={node.span!r}, via={node.via!r}, tag_start={node.tag_start!r})"
    )


def _contains_nan(node: Node) -> bool:
    stack: list[Node] = [node]
    seen: set[int] = set()
    while stack:
        cur = stack.pop()
        cid = id(cur)
        if cid in seen:
            continue
        seen.add(cid)
        if cur.kind == "float":
            if math.isnan(cur.value) or cur.bits == CANONICAL_NAN_BITS:
                return True
        elif cur.kind == "tagged":
            stack.append(cur.value.value)
        elif cur.kind == "sequence":
            stack.extend(cur.value)
        elif cur.kind == "mapping":
            for k, v in cur.value.entries:
                stack.append(k)
                stack.append(v)
    return False


def _key_hash(root: Node) -> int:
    """Iterative keyed 64-bit structural hash consistent with `key_eq` (`spec-v1.5 §5.1`)."""
    kind = root.kind
    if kind == "string":
        return _prf_u64(b"\x05" + root.value.encode("utf-8", "surrogatepass"))
    if kind == "int":
        return _prf_u64(b"\x03" + int(root.value).to_bytes(17, "big", signed=True))
    if kind == "bool":
        return _prf_u64(b"\x02\x01" if root.value else b"\x02\x00")
    if kind == "null":
        return _prf_u64(b"\x01")
    if kind == "float":
        xf = float(root.value)
        if math.isnan(xf):
            return _prf_u64(b"\x04nan" + id(root).to_bytes(8, "little", signed=False))
        if xf == 0.0:
            return _prf_u64(b"\x040000000000000000")
        xb = root.bits if root.bits is not None else _float_to_bits_hex(xf)
        return _prf_u64(b"\x04" + xb.encode("ascii"))

    work: list[Any] = [("eval", root)]
    hashes: list[int] = []
    while work:
        frame = work.pop()
        op = frame[0]
        if op == "finish_tagged":
            _, uri = frame
            inner_h = hashes.pop()
            hashes.append(
                _prf_u64(
                    b"\x06"
                    + inner_h.to_bytes(8, "little", signed=False)
                    + uri.encode("utf-8", "surrogatepass")
                )
            )
            continue
        if op == "finish_seq":
            _, count = frame
            h = hashlib.blake2b(digest_size=8, key=_KEY_HASH_SECRET)
            h.update(b"\x07" + count.to_bytes(8, "little", signed=False))
            if count > 0:
                seq_hs = hashes[-count:]
                del hashes[-count:]
                for item_h in seq_hs:
                    h.update(item_h.to_bytes(8, "little", signed=False))
            hashes.append(int.from_bytes(h.digest(), "little"))
            continue
        if op == "finish_map":
            _, count = frame
            if count == 0:
                hashes.append(_prf_u64(b"\x09" + (0).to_bytes(24, "little", signed=False)))
            else:
                flat = hashes[-(2 * count) :]
                del hashes[-(2 * count) :]
                total_sum = 0
                total_xor = 0
                for i in range(count):
                    eh = _prf_u64(
                        b"\x08"
                        + flat[2 * i].to_bytes(8, "little", signed=False)
                        + flat[2 * i + 1].to_bytes(8, "little", signed=False)
                    )
                    total_sum = (total_sum + eh) & 0xFFFFFFFFFFFFFFFF
                    total_xor ^= eh
                hashes.append(
                    _prf_u64(
                        b"\x09"
                        + count.to_bytes(8, "little", signed=False)
                        + total_sum.to_bytes(8, "little", signed=False)
                        + total_xor.to_bytes(8, "little", signed=False)
                    )
                )
            continue

        _, n = frame
        nk = n.kind
        if nk == "null":
            hashes.append(_prf_u64(b"\x01"))
        elif nk == "bool":
            hashes.append(_prf_u64(b"\x02\x01" if n.value else b"\x02\x00"))
        elif nk == "int":
            hashes.append(_prf_u64(b"\x03" + int(n.value).to_bytes(17, "big", signed=True)))
        elif nk == "float":
            xf = float(n.value)
            if math.isnan(xf):
                hashes.append(_prf_u64(b"\x04nan" + id(n).to_bytes(8, "little", signed=False)))
            elif xf == 0.0:
                hashes.append(_prf_u64(b"\x040000000000000000"))
            else:
                xb = n.bits if n.bits is not None else _float_to_bits_hex(xf)
                hashes.append(_prf_u64(b"\x04" + xb.encode("ascii")))
        elif nk == "string":
            hashes.append(_prf_u64(b"\x05" + n.value.encode("utf-8", "surrogatepass")))
        elif nk == "tagged":
            xt: Tagged = n.value
            work.append(("finish_tagged", xt.tag.uri))
            work.append(("eval", xt.value))
        elif nk == "sequence":
            xs: Sequence[Node] = n.value
            work.append(("finish_seq", len(xs)))
            for item in reversed(xs):
                work.append(("eval", item))
        elif nk == "mapping":
            entries = n.value.entries if isinstance(n.value, Mapping) else n.value
            work.append(("finish_map", len(entries)))
            for k, v in reversed(entries):
                work.append(("eval", v))
                work.append(("eval", k))
        else:
            hashes.append(_prf_u64(b"\xff" + str(nk).encode("utf-8", "surrogatepass")))
    return hashes.pop()


def key_eq(a: Node, b: Node) -> bool:
    """YAML 1.2.2 Core Schema structural key equality (`spec-v1.4 §5.1`).

    - `Bool(True) != Int(1) != Float(1.0)` (distinct kinds never collide).
    - `Float`: `NaN` is never equal to anything (including `NaN`); `-0.0` and `+0.0` are equal.
    - `Mapping`: order-independent over `(key, value)` pairs.
    - `Sequence` / `Tagged`: iterative structural equality; spans/via/tag_start are ignored.
    """
    stack: list[Any] = [("cmp", a, b)]
    backtrack_stack: list[Any] = []

    def backtrack() -> bool:
        while backtrack_stack:
            (
                saved_len,
                coll_entries,
                coll_idx,
                ym_entries,
                ym_buckets,
                used,
                cands,
                next_cand_idx,
                prev_j,
            ) = backtrack_stack.pop()
            used[prev_j] = False
            del stack[saved_len:]
            while next_cand_idx < len(cands) and used[cands[next_cand_idx]]:
                next_cand_idx += 1
            if next_cand_idx < len(cands):
                stack.append(
                    (
                        "map_cand",
                        coll_entries,
                        coll_idx,
                        ym_entries,
                        ym_buckets,
                        used,
                        cands,
                        next_cand_idx,
                    )
                )
                return True
        return False

    while stack:
        frame = stack.pop()
        op = frame[0]
        if op == "map_coll":
            _, coll_entries, coll_idx, ym_entries, ym_buckets, used = frame
            if coll_idx >= len(coll_entries):
                continue
            xk, _ = coll_entries[coll_idx]
            cands = ym_buckets.get(_key_hash(xk), [])
            cand_idx = 0
            while cand_idx < len(cands) and used[cands[cand_idx]]:
                cand_idx += 1
            if cand_idx >= len(cands):
                if not backtrack():
                    return False
                continue
            stack.append(
                ("map_cand", coll_entries, coll_idx, ym_entries, ym_buckets, used, cands, cand_idx)
            )
            continue

        if op == "map_cand":
            _, coll_entries, coll_idx, ym_entries, ym_buckets, used, cands, cand_idx = frame
            while cand_idx < len(cands) and used[cands[cand_idx]]:
                cand_idx += 1
            if cand_idx >= len(cands):
                if not backtrack():
                    return False
                continue
            j = cands[cand_idx]
            xk, xv = coll_entries[coll_idx]
            yk, yv = ym_entries[j]
            next_cand_idx = cand_idx + 1
            while next_cand_idx < len(cands) and used[cands[next_cand_idx]]:
                next_cand_idx += 1
            if next_cand_idx < len(cands):
                backtrack_stack.append(
                    (
                        len(stack),
                        coll_entries,
                        coll_idx,
                        ym_entries,
                        ym_buckets,
                        used,
                        cands,
                        next_cand_idx,
                        j,
                    )
                )
            used[j] = True
            if coll_idx + 1 < len(coll_entries):
                stack.append(
                    ("map_coll", coll_entries, coll_idx + 1, ym_entries, ym_buckets, used)
                )
            stack.append(("cmp", xv, yv))
            stack.append(("cmp", xk, yk))
            continue

        _, x, y = frame
        if x.kind != y.kind:
            if not backtrack():
                return False
            continue
        kind = x.kind
        if kind == "null":
            continue
        if kind == "bool":
            if bool(x.value) is not bool(y.value):
                if not backtrack():
                    return False
        elif kind == "int":
            if int(x.value) != int(y.value):
                if not backtrack():
                    return False
        elif kind == "float":
            xf = float(x.value)
            yf = float(y.value)
            if math.isnan(xf) or math.isnan(yf):
                if not backtrack():
                    return False
                continue
            if xf == 0.0 and yf == 0.0:
                continue
            xb = x.bits if x.bits is not None else _float_to_bits_hex(xf)
            yb = y.bits if y.bits is not None else _float_to_bits_hex(yf)
            if xb != yb:
                if not backtrack():
                    return False
        elif kind == "string":
            if x.value != y.value:
                if not backtrack():
                    return False
        elif kind == "tagged":
            xt: Tagged = x.value
            yt: Tagged = y.value
            if xt.tag.uri != yt.tag.uri:
                if not backtrack():
                    return False
                continue
            stack.append(("cmp", xt.value, yt.value))
        elif kind == "sequence":
            xs: Sequence[Node] = x.value
            ys: Sequence[Node] = y.value
            if len(xs) != len(ys):
                if not backtrack():
                    return False
                continue
            for xi, yi in zip(xs, ys):
                stack.append(("cmp", xi, yi))
        elif kind == "mapping":
            xm_entries = x.value.entries if isinstance(x.value, Mapping) else x.value
            ym_entries = y.value.entries if isinstance(y.value, Mapping) else y.value
            if len(xm_entries) != len(ym_entries):
                if not backtrack():
                    return False
                continue
            ym_buckets: dict[int, list[int]] = {}
            for j, (yk, _) in enumerate(ym_entries):
                ym_buckets.setdefault(_key_hash(yk), []).append(j)
            used = [False] * len(ym_entries)
            coll_entries: list[tuple[Node, Node]] = []
            failed = False
            for xk, xv in xm_entries:
                cands = ym_buckets.get(_key_hash(xk))
                if not cands:
                    failed = True
                    break
                if len(cands) == 1:
                    j = cands[0]
                    if used[j]:
                        failed = True
                        break
                    used[j] = True
                    yk, yv = ym_entries[j]
                    stack.append(("cmp", xv, yv))
                    stack.append(("cmp", xk, yk))
                else:
                    coll_entries.append((xk, xv))
            if failed:
                if not backtrack():
                    return False
                continue
            if coll_entries:
                stack.append(("map_coll", coll_entries, 0, ym_entries, ym_buckets, used))
        else:
            if not backtrack():
                return False
    return True


class _KeyIndex:
    """Hash-bucketed duplicate-key detector using `_key_hash` + `key_eq` across all Node kinds."""

    __slots__ = ("_buckets",)

    def __init__(self) -> None:
        self._buckets: dict[int, list[Node]] = {}

    def check_and_insert(self, key: Node) -> bool:
        h = _key_hash(key)
        bucket = self._buckets.get(h)
        if bucket is None:
            self._buckets[h] = [key]
            return True
        for existing in bucket:
            if key_eq(existing, key):
                return False
        bucket.append(key)
        return True


def _coerce_lookup_key(key: Any) -> Node:
    if isinstance(key, Node):
        return key
    if key is None:
        return Node.null()
    if type(key) is bool:
        return Node.boolean(key)
    if type(key) is int:
        return Node.integer(key)
    if type(key) is float:
        return Node.floating(key)
    if isinstance(key, str):
        return Node.string(key)
    raise TypeError(f"unsupported Mapping lookup key type: {type(key).__name__}")


class Mapping:
    """Lossless ordered YAML mapping backed by `(key_node, value_node)` pairs.

    Uses `yamlprims.key_eq` rather than Python's `hash`/`==` so YAML `true`,
    `1`, and `1.0` keys never collide as they would in a built-in Python `dict`.
    """

    __slots__ = ("_entries",)

    def __init__(self, entries: Sequence[tuple[Node, Node]] = ()) -> None:
        validated: list[tuple[Node, Node]] = []
        key_index = _KeyIndex()
        for k, v in entries:
            if not isinstance(k, Node) or not isinstance(v, Node):
                raise TypeError("Mapping entries must be (Node, Node) pairs")
            if _contains_nan(k):
                raise YamlprimsError(
                    kind="invalid_key",
                    detail={},
                    site={"not_source": {"last_known": None}},
                    message="mapping key contains NaN",
                )
            if not key_index.check_and_insert(k):
                raise YamlprimsError(
                    kind="duplicate_key",
                    detail={},
                    site={"not_source": {"last_known": None}},
                    message="duplicate mapping key",
                )
            validated.append((k, v))
        self._entries: tuple[tuple[Node, Node], ...] = tuple(validated)

    @classmethod
    def _from_trusted_entries(cls, entries: Sequence[tuple[Node, Node]]) -> Mapping:
        obj = object.__new__(cls)
        obj._entries = tuple(entries)
        return obj

    @property
    def entries(self) -> tuple[tuple[Node, Node], ...]:
        return self._entries

    def __len__(self) -> int:
        return len(self._entries)

    def __iter__(self) -> Iterator[tuple[Node, Node]]:
        return iter(self._entries)

    def keys(self) -> tuple[Node, ...]:
        return tuple(k for k, _ in self._entries)

    def values(self) -> tuple[Node, ...]:
        return tuple(v for _, v in self._entries)

    def items(self) -> tuple[tuple[Node, Node], ...]:
        return self._entries

    def at(self, index: int) -> tuple[Node, Node]:
        return self._entries[index]

    def get(self, key: Any, default: Any = None) -> Node | Any:
        target = _coerce_lookup_key(key)
        for k, v in self._entries:
            if key_eq(k, target):
                return v
        return default

    def __getitem__(self, key: Any) -> Node:
        target = _coerce_lookup_key(key)
        for k, v in self._entries:
            if key_eq(k, target):
                return v
        raise KeyError(key)

    def __contains__(self, key: Any) -> bool:
        try:
            target = _coerce_lookup_key(key)
        except TypeError:
            return False
        return any(key_eq(k, target) for k, _ in self._entries)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Mapping):
            return NotImplemented
        if len(self._entries) != len(other._entries):
            return False
        for (xk, xv), (yk, yv) in zip(self._entries, other._entries):
            if not _nodes_equal_iterative(xk, yk) or not _nodes_equal_iterative(xv, yv):
                return False
        return True

    def __repr__(self) -> str:
        pairs = ", ".join(
            f"({_node_repr_bounded(k, 1)}, {_node_repr_bounded(v, 1)})" for k, v in self._entries
        )
        return f"Mapping([{pairs}])"


@dataclass(frozen=True, eq=False, repr=False)
class Document:
    """A loaded YAML 1.2.2 document."""

    root: Node
    yaml_version: tuple[int, int] | None = None
    explicit_start: bool = False

    @staticmethod
    def new(root: Node) -> Document:
        if not isinstance(root, Node):
            raise TypeError("Document.new requires a Node")
        return Document(root=root, yaml_version=None, explicit_start=False)

    @staticmethod
    def with_metadata(
        root: Node,
        yaml_version: tuple[int, int] | None = None,
        explicit_start: bool = False,
    ) -> Document:
        if not isinstance(root, Node):
            raise TypeError("Document.with_metadata requires a Node")
        if type(explicit_start) is not bool:
            raise TypeError("Document.with_metadata explicit_start must be a bool")
        if yaml_version is not None:
            if (
                not isinstance(yaml_version, tuple)
                or len(yaml_version) != 2
                or type(yaml_version[0]) is not int
                or type(yaml_version[1]) is not int
                or yaml_version[0] != 1
                or yaml_version[1] < 0
                or yaml_version[1] > 999_999_999
            ):
                raise _not_source_error(
                    "unsupported_yaml_version",
                    {},
                    f"unsupported YAML directive version: {yaml_version!r}",
                )
        return Document(root=root, yaml_version=yaml_version, explicit_start=explicit_start)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Document):
            return NotImplemented
        return (
            self.yaml_version == other.yaml_version
            and self.explicit_start == other.explicit_start
            and _nodes_equal_iterative(self.root, other.root)
        )

    def __repr__(self) -> str:
        return (
            f"Document(root={_node_repr_bounded(self.root, 0)}, "
            f"yaml_version={self.yaml_version!r}, explicit_start={self.explicit_start!r})"
        )


class YamlprimsError(ValueError):
    """Structured diagnostic raised by `yamlprims` operations."""

    def __init__(
        self,
        kind: str,
        message: str,
        *,
        detail: dict[str, Any] | None = None,
        site: dict[str, Any] | None = None,
        related: Sequence[Related] = (),
        path: Sequence[PathSegment] = (),
    ) -> None:
        super().__init__(message)
        self.kind: str = kind
        self.message: str = message
        self.detail: dict[str, Any] = dict(detail) if detail is not None else {}
        self.site: dict[str, Any] = (
            dict(site)
            if site is not None
            else {"not_source": {"last_known": None}}
        )
        self.related: tuple[Related, ...] = tuple(related)
        self.path: tuple[PathSegment, ...] = tuple(path)
        src = self.site.get("source")
        self.byte_offset: int | None = src.byte_offset if isinstance(src, Location) else None
        self.line: int | None = src.line if isinstance(src, Location) else None
        self.column: int | None = src.column if isinstance(src, Location) else None

    def location(self) -> Location | None:
        src = self.site.get("source")
        return src if isinstance(src, Location) else None


@dataclass(frozen=True)
class WireEnvelope:
    """Decoded `yamlprims-value/1` wire envelope (`kind="documents"` or `kind="error"`)."""

    kind: str  # "documents" | "error"
    documents: tuple[Document, ...] | None = None
    error: YamlprimsError | None = None


def _not_source_error(
    kind: str,
    detail: dict[str, Any],
    message: str,
    last_known: Location | None = None,
) -> YamlprimsError:
    return YamlprimsError(
        kind=kind,
        message=message,
        detail=detail,
        site={"not_source": {"last_known": last_known}},
    )


def _invalid_options_error(message: str) -> YamlprimsError:
    return _not_source_error("invalid_options", {}, message)


@dataclass(frozen=True)
class Limits:
    """Resource budgets for YAML loading, wire decoding, and encoding (12 fields)."""

    max_input_bytes: int = _native.DEFAULT_MAX_INPUT_BYTES
    max_depth: int = _native.DEFAULT_MAX_DEPTH
    max_nodes: int = _native.DEFAULT_MAX_NODES
    max_expanded_nodes: int = _native.DEFAULT_MAX_EXPANDED_NODES
    max_alias_events: int = _native.DEFAULT_MAX_ALIAS_EVENTS
    max_owned_bytes: int = _native.DEFAULT_MAX_OWNED_BYTES
    max_scalar_bytes: int = _native.DEFAULT_MAX_SCALAR_BYTES
    max_key_bytes: int = _native.DEFAULT_MAX_KEY_BYTES
    max_mapping_entries: int = _native.DEFAULT_MAX_MAPPING_ENTRIES
    max_documents: int = _native.DEFAULT_MAX_DOCUMENTS
    max_output_bytes: int = _native.DEFAULT_MAX_OUTPUT_BYTES
    max_warnings: int = _native.DEFAULT_MAX_WARNINGS

    def validate(self) -> None:
        for name in _LIMIT_FIELD_NAMES:
            val = getattr(self, name)
            if type(val) is not int:
                raise _invalid_options_error(f"Limits.{name} must be an int, got {val!r}")
            if val < 1 or val > MAX_SAFE_METADATA_INT:
                raise _invalid_options_error(f"Limits.{name} must be in 1..={MAX_SAFE_METADATA_INT}")
        if self.max_depth > 255:
            raise _invalid_options_error("max_depth must be at most 255")
        if self.max_scalar_bytes > self.max_owned_bytes or self.max_key_bytes > self.max_owned_bytes:
            raise _invalid_options_error(
                "max_scalar_bytes and max_key_bytes must not exceed max_owned_bytes"
            )

    def _to_native_list(self) -> list[int]:
        self.validate()
        return [min(getattr(self, name), _native.MAX_LIMIT) for name in _LIMIT_FIELD_NAMES]


@dataclass(frozen=True)
class Options:
    """Loader and encoder options (`merge_keys`, `unknown_tags`, `limits`)."""

    merge_keys: str = "reject"
    unknown_tags: str = "preserve"
    limits: Limits = field(default_factory=Limits)

    def validate(self) -> Options:
        mk = "as_key" if self.merge_keys == "as-key" else self.merge_keys
        if mk not in ("reject", "apply", "as_key"):
            raise _invalid_options_error(f"invalid merge_keys: {self.merge_keys!r}")
        if self.unknown_tags not in ("preserve", "reject"):
            raise _invalid_options_error(f"invalid unknown_tags: {self.unknown_tags!r}")
        if not isinstance(self.limits, Limits):
            raise _invalid_options_error("Options.limits must be a Limits instance")
        self.limits.validate()
        if mk != self.merge_keys:
            return Options(merge_keys=mk, unknown_tags=self.unknown_tags, limits=self.limits)
        return self


def default_limits() -> Limits:
    return Limits()


def default_options() -> Options:
    return Options()


def validate_options(options: Options | TypingMapping[str, Any] | None = None) -> Options:
    if options is None:
        return Options()
    if isinstance(options, Options):
        return options.validate()
    if isinstance(options, TypingMapping):
        allowed = {"merge_keys", "unknown_tags", "limits"}
        extra = set(options.keys()) - allowed
        if extra:
            raise _invalid_options_error(f"unknown option keys: {sorted(extra)}")
        mk = options.get("merge_keys", "reject")
        ut = options.get("unknown_tags", "preserve")
        raw_lim = options.get("limits")
        if raw_lim is None:
            lim = Limits()
        elif isinstance(raw_lim, Limits):
            lim = raw_lim
        elif isinstance(raw_lim, TypingMapping):
            extra_lim = set(raw_lim.keys()) - set(_LIMIT_FIELD_NAMES)
            if extra_lim:
                raise _invalid_options_error(f"unknown limit keys: {sorted(extra_lim)}")
            kwargs: dict[str, int] = {}
            for k, v in raw_lim.items():
                if type(v) is not int:
                    raise _invalid_options_error(f"limit {k} must be an int")
                kwargs[k] = v
            lim = Limits(**kwargs)
        else:
            raise _invalid_options_error("limits must be a Limits or mapping")
        if not isinstance(mk, str) or not isinstance(ut, str):
            raise _invalid_options_error("merge_keys and unknown_tags must be strings")
        return Options(merge_keys=mk, unknown_tags=ut, limits=lim).validate()
    raise _invalid_options_error(f"options must be Options or dict, got {type(options).__name__}")


def default_ijson_options() -> Options:
    """Default options for RFC 7493 I-JSON projection (`unknown_tags="reject"`, `max_output_bytes=16 MiB`)."""
    return Options(
        merge_keys="reject",
        unknown_tags="reject",
        limits=Limits(max_output_bytes=DEFAULT_IJSON_MAX_OUTPUT_BYTES),
    )


_EMIT_LIMIT_FIELD_NAMES: Final[tuple[str, ...]] = (
    "max_depth",
    "max_nodes",
    "max_scalar_bytes",
    "max_key_bytes",
    "max_mapping_entries",
    "max_tag_bytes",
    "max_documents",
    "max_output_bytes",
)


@dataclass(frozen=True)
class EmitLimits:
    """Resource budgets for YAML 1.2.2 Core emission (8 fields, spec-v1.5 §10.2)."""

    max_depth: int = _native.DEFAULT_MAX_DEPTH
    max_nodes: int = _native.DEFAULT_MAX_NODES
    max_scalar_bytes: int = _native.DEFAULT_MAX_SCALAR_BYTES
    max_key_bytes: int = _native.DEFAULT_MAX_KEY_BYTES
    max_mapping_entries: int = _native.DEFAULT_MAX_MAPPING_ENTRIES
    max_tag_bytes: int = _native.DEFAULT_EMIT_MAX_TAG_BYTES
    max_documents: int = _native.DEFAULT_MAX_DOCUMENTS
    max_output_bytes: int = _native.DEFAULT_EMIT_MAX_OUTPUT_BYTES

    def validate(self) -> None:
        for name in _EMIT_LIMIT_FIELD_NAMES:
            val = getattr(self, name)
            if type(val) is not int:
                raise _invalid_options_error(f"EmitLimits.{name} must be an int, got {val!r}")
            if val < 1 or val > MAX_SAFE_METADATA_INT:
                raise _invalid_options_error(
                    f"EmitLimits.{name} must be in 1..={MAX_SAFE_METADATA_INT}"
                )
        if self.max_depth > 255:
            raise _invalid_options_error("max_depth must be at most 255")
        if (
            self.max_scalar_bytes > self.max_output_bytes
            or self.max_key_bytes > self.max_output_bytes
        ):
            raise _invalid_options_error(
                "max_scalar_bytes and max_key_bytes must not exceed max_output_bytes"
            )

    def _to_native_list(self) -> list[int]:
        self.validate()
        return [min(getattr(self, name), _native.MAX_LIMIT) for name in _EMIT_LIMIT_FIELD_NAMES]


@dataclass(frozen=True)
class EmitOptions:
    """YAML 1.2.2 Core emitter options (`compat_quoting`, `limits`, spec-v1.5 §10.2)."""

    compat_quoting: str = "yaml11_safe"
    limits: EmitLimits = field(default_factory=EmitLimits)

    def validate(self) -> EmitOptions:
        if not isinstance(self.compat_quoting, str):
            raise _invalid_options_error("compat_quoting must be a string")
        cq = (
            "yaml11_safe"
            if self.compat_quoting == "yaml11-safe"
            else ("core_only" if self.compat_quoting == "core-only" else self.compat_quoting)
        )
        if cq not in ("yaml11_safe", "core_only"):
            raise _invalid_options_error(f"invalid compat_quoting: {self.compat_quoting!r}")
        if not isinstance(self.limits, EmitLimits):
            raise _invalid_options_error("EmitOptions.limits must be an EmitLimits instance")
        self.limits.validate()
        if cq != self.compat_quoting:
            return EmitOptions(compat_quoting=cq, limits=self.limits)
        return self


def default_emit_limits() -> EmitLimits:
    return EmitLimits()


def default_emit_options() -> EmitOptions:
    return EmitOptions()


def validate_emit_options(
    options: EmitOptions | TypingMapping[str, Any] | None = None,
) -> EmitOptions:
    if options is None:
        return EmitOptions()
    if isinstance(options, EmitOptions):
        return options.validate()
    if isinstance(options, TypingMapping):
        allowed = {"compat_quoting", "limits"}
        extra = set(options.keys()) - allowed
        if extra:
            raise _invalid_options_error(f"unknown emit option keys: {sorted(extra)}")
        cq = options.get("compat_quoting", "yaml11_safe")
        raw_lim = options.get("limits")
        if raw_lim is None:
            lim = EmitLimits()
        elif isinstance(raw_lim, EmitLimits):
            lim = raw_lim
        elif isinstance(raw_lim, TypingMapping):
            extra_lim = set(raw_lim.keys()) - set(_EMIT_LIMIT_FIELD_NAMES)
            if extra_lim:
                raise _invalid_options_error(f"unknown emit limit keys: {sorted(extra_lim)}")
            kwargs: dict[str, int] = {}
            for k, v in raw_lim.items():
                if type(v) is not int:
                    raise _invalid_options_error(f"emit limit {k} must be an int")
                kwargs[k] = v
            lim = EmitLimits(**kwargs)
        else:
            raise _invalid_options_error("limits must be an EmitLimits or mapping")
        if not isinstance(cq, str):
            raise _invalid_options_error("compat_quoting must be a string")
        return EmitOptions(compat_quoting=cq, limits=lim).validate()
    raise _invalid_options_error(
        f"options must be EmitOptions or dict, got {type(options).__name__}"
    )


def _to_input_bytes(data: Input, require_bytes: bool = False) -> bytes:
    if isinstance(data, str):
        if require_bytes:
            raise TypeError("bytes-like object is required, not str")
        try:
            return data.encode("utf-8", errors="strict")
        except UnicodeEncodeError as exc:
            raise TypeError(f"input string contains unpaired surrogate: {exc}") from None
    try:
        view = memoryview(data)
    except TypeError:
        raise TypeError(f"input must be bytes-like or str, not {type(data).__name__}") from None
    with view:
        return view.tobytes()


# ---- Strict JSON Parser for yamlprims-value/1 ----------------------------


@dataclass(frozen=True)
class _JVal:
    t: str  # "null" | "bool" | "int" | "float" | "str" | "arr" | "obj"
    v: Any
    raw: str = ""


class _WireJsonParser:
    def __init__(self, text: str, max_depth: int, max_tokens: int) -> None:
        self.text = text
        self.n = len(text)
        self.pos = 0
        self.max_depth = max_depth
        self.tokens_left = max_tokens

    def parse_root(self) -> _JVal:
        stack: list[tuple[str, Any, str | None]] = []
        while True:
            self._skip_ws()
            if self.pos >= self.n:
                raise _not_source_error("internal", {}, "unexpected EOF in wire JSON")
            self._consume_token()
            ch = self.text[self.pos]
            val: _JVal | None = None

            if ch == "{":
                if len(stack) + 1 > self.max_depth:
                    raise _not_source_error("internal", {}, "wire JSON nesting exceeds max_depth")
                self.pos += 1
                self._skip_ws()
                if self.pos < self.n and self.text[self.pos] == "}":
                    self.pos += 1
                    val = _JVal("obj", {})
                else:
                    if self.pos >= self.n or self.text[self.pos] != '"':
                        raise _not_source_error("internal", {}, "expected string key in wire JSON object")
                    first_key = self._parse_string()
                    self._skip_ws()
                    if self.pos >= self.n or self.text[self.pos] != ":":
                        raise _not_source_error("internal", {}, "expected ':' after key in wire JSON object")
                    self.pos += 1
                    stack.append(("obj", {}, first_key))
                    continue
            elif ch == "[":
                if len(stack) + 1 > self.max_depth:
                    raise _not_source_error("internal", {}, "wire JSON nesting exceeds max_depth")
                self.pos += 1
                self._skip_ws()
                if self.pos < self.n and self.text[self.pos] == "]":
                    self.pos += 1
                    val = _JVal("arr", [])
                else:
                    stack.append(("arr", [], None))
                    continue
            elif ch == '"':
                val = _JVal("str", self._parse_string())
            elif ch == "t":
                if self.text.startswith("true", self.pos):
                    self.pos += 4
                    val = _JVal("bool", True)
                else:
                    raise _not_source_error("internal", {}, "invalid literal in wire JSON")
            elif ch == "f":
                if self.text.startswith("false", self.pos):
                    self.pos += 5
                    val = _JVal("bool", False)
                else:
                    raise _not_source_error("internal", {}, "invalid literal in wire JSON")
            elif ch == "n":
                if self.text.startswith("null", self.pos):
                    self.pos += 4
                    val = _JVal("null", None)
                else:
                    raise _not_source_error("internal", {}, "invalid literal in wire JSON")
            elif ch == "-" or ("0" <= ch <= "9"):
                val = self._parse_number()
            else:
                raise _not_source_error("internal", {}, f"unexpected character {ch!r} in wire JSON")

            while True:
                assert val is not None
                if not stack:
                    self._skip_ws()
                    if self.pos != self.n:
                        raise _not_source_error("internal", {}, "trailing characters after wire JSON value")
                    return val
                frame_kind, container, pending_key = stack[-1]
                if frame_kind == "arr":
                    container.append(val)
                    self._skip_ws()
                    if self.pos < self.n and self.text[self.pos] == ",":
                        self.pos += 1
                        break
                    if self.pos < self.n and self.text[self.pos] == "]":
                        self.pos += 1
                        stack.pop()
                        val = _JVal("arr", container)
                        continue
                    raise _not_source_error("internal", {}, "expected ',' or ']' in wire JSON array")
                else:
                    assert pending_key is not None
                    container[pending_key] = val
                    self._skip_ws()
                    if self.pos < self.n and self.text[self.pos] == ",":
                        self.pos += 1
                        self._skip_ws()
                        if self.pos >= self.n or self.text[self.pos] != '"':
                            raise _not_source_error("internal", {}, "expected string key in wire JSON object")
                        next_key = self._parse_string()
                        if next_key in container:
                            raise _not_source_error("internal", {}, f"duplicate JSON object key: {next_key}")
                        self._skip_ws()
                        if self.pos >= self.n or self.text[self.pos] != ":":
                            raise _not_source_error("internal", {}, "expected ':' after key in wire JSON object")
                        self.pos += 1
                        stack[-1] = ("obj", container, next_key)
                        break
                    if self.pos < self.n and self.text[self.pos] == "}":
                        self.pos += 1
                        stack.pop()
                        val = _JVal("obj", container)
                        continue
                    raise _not_source_error("internal", {}, "expected ',' or '}' in wire JSON object")

    def _consume_token(self) -> None:
        if self.tokens_left <= 0:
            raise _not_source_error("internal", {}, "wire JSON exceeds token budget")
        self.tokens_left -= 1

    def _skip_ws(self) -> None:
        t = self.text
        n = self.n
        p = self.pos
        while p < n and t[p] in (" ", "\t", "\n", "\r"):
            p += 1
        self.pos = p

    def _parse_string(self) -> str:
        if self.pos >= self.n or self.text[self.pos] != '"':
            raise _not_source_error("internal", {}, "expected '\"' in wire JSON")
        self.pos += 1
        out: list[str] = []
        run_start = self.pos
        while self.pos < self.n:
            c = self.text[self.pos]
            o = ord(c)
            if o < 0x20:
                raise _not_source_error("internal", {}, "unescaped control character in wire JSON string")
            if 0xD800 <= o <= 0xDFFF:
                raise _not_source_error("internal", {}, "unpaired surrogate in wire JSON string")
            if c == '"':
                if self.pos > run_start:
                    out.append(self.text[run_start : self.pos])
                self.pos += 1
                return "".join(out)
            if c == "\\":
                if self.pos > run_start:
                    out.append(self.text[run_start : self.pos])
                self.pos += 1
                if self.pos >= self.n:
                    raise _not_source_error("internal", {}, "unterminated escape in wire JSON string")
                esc = self.text[self.pos]
                self.pos += 1
                if esc == '"':
                    out.append('"')
                elif esc == "\\":
                    out.append("\\")
                elif esc == "/":
                    out.append("/")
                elif esc == "b":
                    out.append("\b")
                elif esc == "f":
                    out.append("\f")
                elif esc == "n":
                    out.append("\n")
                elif esc == "r":
                    out.append("\r")
                elif esc == "t":
                    out.append("\t")
                elif esc == "u":
                    u1 = self._read_hex4()
                    if 0xD800 <= u1 <= 0xDBFF:
                        if not self.text.startswith("\\u", self.pos):
                            raise _not_source_error("internal", {}, "unpaired high surrogate in wire JSON")
                        self.pos += 2
                        u2 = self._read_hex4()
                        if not (0xDC00 <= u2 <= 0xDFFF):
                            raise _not_source_error("internal", {}, "invalid low surrogate in wire JSON")
                        cp = 0x10000 + ((u1 - 0xD800) << 10) + (u2 - 0xDC00)
                        out.append(chr(cp))
                    elif 0xDC00 <= u1 <= 0xDFFF:
                        raise _not_source_error("internal", {}, "unpaired low surrogate in wire JSON")
                    else:
                        out.append(chr(u1))
                else:
                    raise _not_source_error("internal", {}, f"invalid escape \\{esc} in wire JSON")
                run_start = self.pos
                continue
            self.pos += 1
        raise _not_source_error("internal", {}, "unterminated string in wire JSON")

    def _read_hex4(self) -> int:
        if self.pos + 4 > self.n:
            raise _not_source_error("internal", {}, "truncated \\u escape in wire JSON")
        sub = self.text[self.pos : self.pos + 4]
        try:
            val = int(sub, 16)
        except ValueError:
            raise _not_source_error("internal", {}, "invalid hex in \\u escape in wire JSON") from None
        if any(c not in "0123456789abcdefABCDEF" for c in sub):
            raise _not_source_error("internal", {}, "invalid hex in \\u escape in wire JSON")
        self.pos += 4
        return val

    def _parse_number(self) -> _JVal:
        start = self.pos
        if self.text[self.pos] == "-":
            self.pos += 1
        if self.pos >= self.n or not ("0" <= self.text[self.pos] <= "9"):
            raise _not_source_error("internal", {}, "invalid number in wire JSON")
        if self.text[self.pos] == "0":
            self.pos += 1
            if self.pos < self.n and ("0" <= self.text[self.pos] <= "9"):
                raise _not_source_error("internal", {}, "leading zero in wire JSON number")
        else:
            while self.pos < self.n and ("0" <= self.text[self.pos] <= "9"):
                self.pos += 1
        is_float = False
        if self.pos < self.n and self.text[self.pos] == ".":
            is_float = True
            self.pos += 1
            if self.pos >= self.n or not ("0" <= self.text[self.pos] <= "9"):
                raise _not_source_error("internal", {}, "invalid fraction in wire JSON number")
            while self.pos < self.n and ("0" <= self.text[self.pos] <= "9"):
                self.pos += 1
        if self.pos < self.n and self.text[self.pos] in ("e", "E"):
            is_float = True
            self.pos += 1
            if self.pos < self.n and self.text[self.pos] in ("+", "-"):
                self.pos += 1
            if self.pos >= self.n or not ("0" <= self.text[self.pos] <= "9"):
                raise _not_source_error("internal", {}, "invalid exponent in wire JSON number")
            while self.pos < self.n and ("0" <= self.text[self.pos] <= "9"):
                self.pos += 1
        raw = self.text[start : self.pos]
        if not is_float and raw != "-0":
            ival = int(raw)
            if -MAX_SAFE_METADATA_INT <= ival <= MAX_SAFE_METADATA_INT:
                return _JVal("int", ival, raw)
        fval = float(raw)
        if not math.isfinite(fval):
            raise _not_source_error("internal", {}, "non-finite number literal in wire JSON")
        return _JVal("float", fval, raw)


def _expect_exact_keys(obj: dict[str, _JVal], required: Sequence[str], optional: Sequence[str] = ()) -> None:
    allowed = set(required) | set(optional)
    for k in obj:
        if k not in allowed:
            raise _not_source_error("internal", {}, f"unknown wire field: {k}")
    for r in required:
        if r not in obj:
            raise _not_source_error("internal", {}, f"missing required wire field: {r}")


def _expect_str(val: _JVal | None, label: str) -> str:
    if val is None or val.t != "str":
        raise _not_source_error("internal", {}, f"expected string for {label}")
    return val.v


def _expect_bool(val: _JVal | None, label: str) -> bool:
    if val is None or val.t != "bool":
        raise _not_source_error("internal", {}, f"expected boolean for {label}")
    return val.v


def _expect_nonneg_int(val: _JVal | None, label: str) -> int:
    if val is None or val.t != "int" or val.v < 0:
        raise _not_source_error("internal", {}, f"expected non-negative integer for {label}")
    return val.v


def _decode_loc(val: _JVal) -> Location:
    if val.t != "arr" or len(val.v) != 3:
        raise _not_source_error("internal", {}, "wire Location must be a 3-element array")
    b = _expect_nonneg_int(val.v[0], "Location.byte_offset")
    line = _expect_nonneg_int(val.v[1], "Location.line")
    col = _expect_nonneg_int(val.v[2], "Location.column")
    if line < 1 or col < 1:
        raise _not_source_error("internal", {}, "wire Location line and column must be >= 1")
    return Location(byte_offset=b, line=line, column=col)


def _decode_span(val: _JVal) -> Span:
    if val.t != "arr" or len(val.v) != 6:
        raise _not_source_error("internal", {}, "wire Span must be a 6-element array")
    sb = _expect_nonneg_int(val.v[0], "Span.start.byte_offset")
    sl = _expect_nonneg_int(val.v[1], "Span.start.line")
    sc = _expect_nonneg_int(val.v[2], "Span.start.column")
    eb = _expect_nonneg_int(val.v[3], "Span.end.byte_offset")
    el = _expect_nonneg_int(val.v[4], "Span.end.line")
    ec = _expect_nonneg_int(val.v[5], "Span.end.column")
    if sl < 1 or sc < 1 or el < 1 or ec < 1 or eb < sb:
        raise _not_source_error("internal", {}, "invalid wire Span coordinates")
    return Span(
        start=Location(byte_offset=sb, line=sl, column=sc),
        end=Location(byte_offset=eb, line=el, column=ec),
    )


def _decode_via(val: _JVal) -> Via | None:
    if val.t == "null":
        return None
    if val.t != "obj" or len(val.v) != 1:
        raise _not_source_error("internal", {}, "wire Via must be a single-key object")
    if "alias" in val.v:
        return Via(kind="alias", span=_decode_span(val.v["alias"]))
    if "merge" in val.v:
        return Via(kind="merge", span=_decode_span(val.v["merge"]))
    raise _not_source_error("internal", {}, "unknown wire Via variant")


def _parse_canonical_128_int(text: str) -> int:
    if not _INT_CANONICAL_RE.match(text):
        raise _not_source_error("internal", {}, f"non-canonical wire integer: {text}")
    val = int(text, 10)
    if val < I128_MIN or val > U128_MAX:
        raise _not_source_error("internal", {}, "wire integer outside 128-bit range")
    return val


def _wire_limit_error(limit: str, configured: int) -> YamlprimsError:
    return _not_source_error(
        "limit_exceeded",
        {"limit": limit, "configured": configured},
        f"wire payload exceeds {limit} ({configured})",
    )


def _subtree_weight_wire(root: Node) -> int:
    total = 0
    stack: list[Node] = [root]
    while stack:
        n = stack.pop()
        total += NODE_OVERHEAD
        if n.kind == "string":
            total += len(n.value.encode("utf-8"))
        elif n.kind == "tagged":
            t: Tagged = n.value
            total += (
                len(t.tag.uri.encode("utf-8"))
                + len(t.tag.handle.encode("utf-8"))
                + len(t.tag.suffix.encode("utf-8"))
            )
            stack.append(t.value)
        elif n.kind == "sequence":
            stack.extend(n.value)
        elif n.kind == "mapping":
            m: Mapping = n.value if isinstance(n.value, Mapping) else Mapping(n.value)
            total += len(m.entries) * ENTRY_OVERHEAD
            for k, v in m.entries:
                stack.append(v)
                stack.append(k)
    return total


@dataclass
class _DecodeCtx:
    opts: Options
    nodes_remaining: int
    owned_bytes_used: int = 0
    is_path_key: bool = False
    forbid_locations: bool = False


def _charge_owned_bytes(ctx: _DecodeCtx, extra_bytes: int) -> None:
    ctx.owned_bytes_used += extra_bytes
    if ctx.owned_bytes_used > ctx.opts.limits.max_owned_bytes:
        raise _wire_limit_error("max_owned_bytes", ctx.opts.limits.max_owned_bytes)


def _charge_node(ctx: _DecodeCtx, extra_bytes: int) -> None:
    if ctx.nodes_remaining <= 0:
        raise _wire_limit_error("max_nodes", ctx.opts.limits.max_nodes)
    ctx.nodes_remaining -= 1
    _charge_owned_bytes(ctx, NODE_OVERHEAD + extra_bytes)


def _decode_node(
    root_val: _JVal, ctx: _DecodeCtx, root_depth: int, root_in_tagged: bool = False
) -> Node:
    work: list[Any] = [("eval", root_val, root_depth, root_in_tagged)]
    results: list[Node] = []

    while work:
        frame = work.pop()
        op = frame[0]
        if op == "seq_step":
            _, raw_items, idx, next_depth, converted_items, span, via, tag_start = frame
            if idx > 0:
                converted_items.append(results.pop())
            if idx >= len(raw_items):
                results.append(
                    Node(
                        kind="sequence",
                        value=tuple(converted_items),
                        span=span,
                        via=via,
                        tag_start=tag_start,
                    )
                )
            else:
                if ctx.is_path_key and next_depth > ctx.opts.limits.max_depth:
                    raise _wire_limit_error("max_depth", ctx.opts.limits.max_depth)
                work.append(
                    (
                        "seq_step",
                        raw_items,
                        idx + 1,
                        next_depth,
                        converted_items,
                        span,
                        via,
                        tag_start,
                    )
                )
                work.append(("eval", raw_items[idx], next_depth, False))
            continue

        if op == "map_step":
            _, raw_entries, idx, next_depth, pairs, key_index, span, via, tag_start = frame
            if idx >= len(raw_entries):
                results.append(
                    Node(
                        kind="mapping",
                        value=Mapping._from_trusted_entries(pairs),
                        span=span,
                        via=via,
                        tag_start=tag_start,
                    )
                )
            else:
                if ctx.is_path_key and next_depth > ctx.opts.limits.max_depth:
                    raise _wire_limit_error("max_depth", ctx.opts.limits.max_depth)
                pair = raw_entries[idx]
                if pair.t != "obj":
                    raise _not_source_error("internal", {}, "wire mapping entry must be an object")
                _expect_exact_keys(pair.v, ("key", "value"))
                work.append(
                    (
                        "map_entry_done",
                        raw_entries,
                        idx + 1,
                        next_depth,
                        pairs,
                        key_index,
                        span,
                        via,
                        tag_start,
                    )
                )
                work.append(("eval", pair.v["value"], next_depth, False))
                work.append(("eval", pair.v["key"], next_depth, False))
            continue

        if op == "map_entry_done":
            _, raw_entries, next_idx, next_depth, pairs, key_index, span, via, tag_start = frame
            v_node = results.pop()
            k_node = results.pop()
            if _subtree_weight_wire(k_node) > ctx.opts.limits.max_key_bytes:
                raise _wire_limit_error("max_key_bytes", ctx.opts.limits.max_key_bytes)
            _charge_owned_bytes(ctx, ENTRY_OVERHEAD)
            if len(pairs) >= ctx.opts.limits.max_mapping_entries:
                raise _wire_limit_error("max_mapping_entries", ctx.opts.limits.max_mapping_entries)
            if _contains_nan(k_node):
                raise _not_source_error("internal", {}, "wire mapping key contains NaN")
            if not key_index.check_and_insert(k_node):
                raise _not_source_error("internal", {}, "wire mapping contains duplicate key")
            pairs.append((k_node, v_node))
            work.append(
                (
                    "map_step",
                    raw_entries,
                    next_idx,
                    next_depth,
                    pairs,
                    key_index,
                    span,
                    via,
                    tag_start,
                )
            )
            continue

        if op == "finish_tagged":
            _, tag_obj, uri, span, via, tag_start = frame
            inner = results.pop()
            valid_inner = (
                inner.kind == "string"
                if uri == "tag:yaml.org,2002:merge"
                else inner.kind in ("string", "sequence", "mapping")
            )
            if not valid_inner:
                raise _not_source_error(
                    "internal", {}, "invalid inner value kind for wire tagged node"
                )
            results.append(
                Node(
                    kind="tagged",
                    value=Tagged(tag=tag_obj, value=inner),
                    span=span,
                    via=via,
                    tag_start=tag_start,
                )
            )
            continue

        # op == "eval"
        _, val, depth, in_tagged = frame
        _charge_node(ctx, 0)
        if val.t != "obj":
            raise _not_source_error("internal", {}, "wire Node must be a JSON object")
        m: dict[str, _JVal] = val.v
        kind = _expect_str(m.get("kind"), "Node.kind")
        span_raw = m.get("span")
        via_raw = m.get("via")
        tag_start_raw = m.get("tag_start")
        if span_raw is None or via_raw is None or tag_start_raw is None:
            raise _not_source_error("internal", {}, "wire Node missing location fields")
        if ctx.forbid_locations:
            if span_raw.t != "null" or via_raw.t != "null" or tag_start_raw.t != "null":
                raise _not_source_error(
                    "internal", {}, "error PathSegment.key nodes must have null locations"
                )
        span = None if span_raw.t == "null" else _decode_span(span_raw)
        via = _decode_via(via_raw)
        tag_start = None if tag_start_raw.t == "null" else _decode_loc(tag_start_raw)

        if kind == "null":
            _expect_exact_keys(m, ("kind", "span", "via", "tag_start"))
            results.append(Node(kind="null", value=None, span=span, via=via, tag_start=tag_start))
            continue

        if kind == "bool":
            _expect_exact_keys(m, ("kind", "value", "span", "via", "tag_start"))
            b = _expect_bool(m.get("value"), "bool.value")
            results.append(Node(kind="bool", value=b, span=span, via=via, tag_start=tag_start))
            continue

        if kind == "int":
            _expect_exact_keys(m, ("kind", "value", "span", "via", "tag_start"))
            iv = _parse_canonical_128_int(_expect_str(m.get("value"), "int.value"))
            results.append(Node(kind="int", value=iv, span=span, via=via, tag_start=tag_start))
            continue

        if kind == "float":
            _expect_exact_keys(m, ("kind", "bits", "span", "via", "tag_start"))
            bits_hex = _expect_str(m.get("bits"), "float.bits")
            if not _HEX16_LOWER_RE.match(bits_hex):
                raise _not_source_error("internal", {}, f"invalid float bits: {bits_hex}")
            fval = _bits_hex_to_float(bits_hex)
            canonical_bits = CANONICAL_NAN_BITS if math.isnan(fval) else bits_hex
            results.append(
                Node(
                    kind="float",
                    value=fval,
                    bits=canonical_bits,
                    span=span,
                    via=via,
                    tag_start=tag_start,
                )
            )
            continue

        if kind == "string":
            _expect_exact_keys(m, ("kind", "value", "span", "via", "tag_start"))
            sval = _expect_str(m.get("value"), "string.value")
            sbytes = len(sval.encode("utf-8"))
            if sbytes > ctx.opts.limits.max_scalar_bytes:
                raise _wire_limit_error("max_scalar_bytes", ctx.opts.limits.max_scalar_bytes)
            _charge_owned_bytes(ctx, sbytes)
            results.append(
                Node(kind="string", value=sval, span=span, via=via, tag_start=tag_start)
            )
            continue

        if kind == "sequence":
            _expect_exact_keys(m, ("kind", "items", "span", "via", "tag_start"))
            items_raw = m["items"]
            if items_raw.t != "arr":
                raise _not_source_error("internal", {}, "wire sequence.items must be an array")
            if not ctx.is_path_key and depth + 1 > ctx.opts.limits.max_depth:
                raise _wire_limit_error("max_depth", ctx.opts.limits.max_depth)
            work.append(("seq_step", items_raw.v, 0, depth + 1, [], span, via, tag_start))
            continue

        if kind == "mapping":
            _expect_exact_keys(m, ("kind", "entries", "span", "via", "tag_start"))
            entries_raw = m["entries"]
            if entries_raw.t != "arr":
                raise _not_source_error("internal", {}, "wire mapping.entries must be an array")
            if not ctx.is_path_key and depth + 1 > ctx.opts.limits.max_depth:
                raise _wire_limit_error("max_depth", ctx.opts.limits.max_depth)
            work.append(
                (
                    "map_step",
                    entries_raw.v,
                    0,
                    depth + 1,
                    [],
                    _KeyIndex(),
                    span,
                    via,
                    tag_start,
                )
            )
            continue

        if kind == "tagged":
            if in_tagged:
                raise _not_source_error(
                    "internal", {}, "nested Tagged-in-Tagged is invalid in yamlprims-value/1"
                )
            _expect_exact_keys(m, ("kind", "tag", "value", "span", "via", "tag_start"))
            tag_raw = m["tag"]
            if tag_raw.t != "obj":
                raise _not_source_error("internal", {}, "wire tagged.tag must be an object")
            _expect_exact_keys(tag_raw.v, ("uri", "handle", "suffix"))
            uri = _expect_str(tag_raw.v.get("uri"), "tag.uri")
            handle = _expect_str(tag_raw.v.get("handle"), "tag.handle")
            suffix = _expect_str(tag_raw.v.get("suffix"), "tag.suffix")
            if not _is_valid_non_core_tag_uri(uri) or not _is_valid_tag_spelling(
                uri, handle, suffix
            ):
                raise _not_source_error(
                    "internal", {}, "wire tag object is invalid or uses a Core tag URI"
                )
            tag_bytes = (
                len(uri.encode("utf-8"))
                + len(handle.encode("utf-8"))
                + len(suffix.encode("utf-8"))
            )
            _charge_owned_bytes(ctx, tag_bytes)
            tag_obj = object.__new__(Tag)
            object.__setattr__(tag_obj, "uri", uri)
            object.__setattr__(tag_obj, "handle", handle)
            object.__setattr__(tag_obj, "suffix", suffix)
            work.append(("finish_tagged", tag_obj, uri, span, via, tag_start))
            work.append(("eval", m["value"], depth, True))
            continue

        raise _not_source_error("internal", {}, f"unknown wire Node kind: {kind}")

    return results.pop()


def _decode_error_envelope(err_obj: dict[str, _JVal], opts: Options) -> YamlprimsError:
    _expect_exact_keys(err_obj, ("kind", "detail", "site", "related", "path", "message"))
    kind_str = _expect_str(err_obj.get("kind"), "error.kind")
    detail_val = err_obj["detail"]
    if detail_val.t != "obj":
        raise _not_source_error("internal", {}, "error.detail must be an object")
    dm: dict[str, _JVal] = detail_val.v

    detail: dict[str, Any]
    if kind_str == "invalid_utf8":
        _expect_exact_keys(dm, ("invalid_len",))
        il = dm["invalid_len"]
        if il.t == "null":
            detail = {"invalid_len": None}
        elif il.t == "int" and il.v >= 0:
            detail = {"invalid_len": il.v}
        else:
            raise _not_source_error("internal", {}, "invalid invalid_utf8.invalid_len")
    elif kind_str == "syntax":
        _expect_exact_keys(dm, ("syntax_kind",))
        sk = _expect_str(dm.get("syntax_kind"), "detail.syntax_kind")
        if sk not in SYNTAX_KINDS:
            raise _not_source_error("internal", {}, f"unknown syntax_kind: {sk}")
        detail = {"syntax_kind": sk}
    elif kind_str == "limit_exceeded":
        _expect_exact_keys(dm, ("limit", "configured"))
        lim = _expect_str(dm.get("limit"), "detail.limit")
        if lim not in LIMIT_KINDS:
            raise _not_source_error("internal", {}, f"unknown limit kind: {lim}")
        conf = _expect_nonneg_int(dm.get("configured"), "detail.configured")
        if conf < 1:
            raise _not_source_error("internal", {}, "configured limit must be >= 1")
        detail = {"limit": lim, "configured": conf}
    elif kind_str == "type":
        _expect_exact_keys(dm, ("expected", "found"))
        detail = {
            "expected": _expect_str(dm.get("expected"), "detail.expected"),
            "found": _expect_str(dm.get("found"), "detail.found"),
        }
    elif kind_str in ("missing_field", "unknown_field"):
        _expect_exact_keys(dm, ("field",))
        detail = {"field": _expect_str(dm.get("field"), "detail.field")}
    elif kind_str == "output_limit_exceeded":
        _expect_exact_keys(dm, ("configured",))
        conf = _expect_nonneg_int(dm.get("configured"), "detail.configured")
        if conf < 1:
            raise _not_source_error("internal", {}, "configured output limit must be >= 1")
        detail = {"configured": conf}
    elif kind_str in _EMPTY_DETAIL_ERROR_KINDS:
        _expect_exact_keys(dm, ())
        detail = {}
    else:
        raise _not_source_error("internal", {}, f"unknown error kind: {kind_str}")

    site_val = err_obj["site"]
    if site_val.t != "obj" or len(site_val.v) != 1:
        raise _not_source_error("internal", {}, "invalid error.site")
    if "source" in site_val.v:
        site: dict[str, Any] = {"source": _decode_loc(site_val.v["source"])}
    elif "not_source" in site_val.v:
        ns = site_val.v["not_source"]
        if ns.t != "obj":
            raise _not_source_error("internal", {}, "invalid error.site.not_source")
        _expect_exact_keys(ns.v, ("last_known",))
        lk = ns.v["last_known"]
        site = {"not_source": {"last_known": None if lk.t == "null" else _decode_loc(lk)}}
    else:
        raise _not_source_error("internal", {}, "unknown error.site variant")

    rel_val = err_obj["related"]
    if rel_val.t != "arr":
        raise _not_source_error("internal", {}, "error.related must be an array")
    related: list[Related] = []
    for r in rel_val.v:
        if r.t != "obj":
            raise _not_source_error("internal", {}, "related entry must be an object")
        _expect_exact_keys(r.v, ("role", "span"))
        role = _expect_str(r.v.get("role"), "related.role")
        if role not in ("first_definition", "alias_use", "merge_directive", "anchor_definition"):
            raise _not_source_error("internal", {}, f"unknown related.role: {role}")
        related.append(Related(role=role, span=_decode_span(r.v["span"])))

    path_val = err_obj["path"]
    if path_val.t != "arr":
        raise _not_source_error("internal", {}, "error.path must be an array")
    if len(path_val.v) > opts.limits.max_depth:
        raise _wire_limit_error("max_depth", opts.limits.max_depth)
    path: list[PathSegment] = []
    key_ctx = _DecodeCtx(
        opts=opts,
        nodes_remaining=opts.limits.max_nodes,
        is_path_key=True,
        forbid_locations=True,
    )
    for seg in path_val.v:
        if seg.t != "obj" or len(seg.v) != 1:
            raise _not_source_error("internal", {}, "invalid error.path segment")
        if "index" in seg.v:
            path.append(PathSegment.from_index(_expect_nonneg_int(seg.v["index"], "path.index")))
        elif "key" in seg.v:
            k_node = _decode_node(seg.v["key"], key_ctx, 0, False)
            if _subtree_weight_wire(k_node) > opts.limits.max_key_bytes:
                raise _wire_limit_error("max_key_bytes", opts.limits.max_key_bytes)
            path.append(PathSegment.from_key(k_node))
        else:
            raise _not_source_error("internal", {}, "unknown error.path segment key")

    msg = _expect_str(err_obj.get("message"), "error.message")
    return YamlprimsError(
        kind=kind_str,
        message=msg,
        detail=detail,
        site=site,
        related=related,
        path=path,
    )


def decode_wire(wire_json: str, options: Options | TypingMapping[str, Any] | None = None) -> WireEnvelope:
    """Strictly parse and validate a `yamlprims-value/1` wire JSON envelope (spec-v1.5 §11.1)."""
    if not isinstance(wire_json, str):
        raise TypeError("wire_json must be a str")
    opts = validate_options(options)
    try:
        wire_utf8_len = len(wire_json.encode("utf-8", errors="strict"))
    except UnicodeEncodeError:
        raise _not_source_error("internal", {}, "wire JSON contains unpaired surrogate") from None
    if wire_utf8_len > opts.limits.max_input_bytes:
        raise _wire_limit_error("max_input_bytes", opts.limits.max_input_bytes)
    max_json_depth = min(2048, opts.limits.max_depth * 4 + 32)
    max_tokens = min(50_000_000, max(4096, opts.limits.max_nodes * 24 + 4096))
    root = _WireJsonParser(wire_json, max_json_depth, max_tokens).parse_root()
    if root.t != "obj":
        raise _not_source_error("internal", {}, "wire envelope must be a JSON object")
    rm: dict[str, _JVal] = root.v
    _expect_exact_keys(rm, ("format", "version"), ("documents", "error"))
    if _expect_str(rm.get("format"), "format") != "yamlprims-value":
        raise _not_source_error("internal", {}, "unsupported wire format")
    if _expect_nonneg_int(rm.get("version"), "version") != 1:
        raise _not_source_error("internal", {}, "unsupported wire version")

    has_docs = "documents" in rm
    has_err = "error" in rm
    if has_docs == has_err:
        raise _not_source_error("internal", {}, "wire envelope must have either documents or error")

    if has_err:
        err_val = rm["error"]
        if err_val.t != "obj":
            raise _not_source_error("internal", {}, "wire error must be a JSON object")
        return WireEnvelope(kind="error", error=_decode_error_envelope(err_val.v, opts))

    docs_val = rm["documents"]
    if docs_val.t != "arr":
        raise _not_source_error("internal", {}, "wire documents must be an array")

    ctx = _DecodeCtx(opts=opts, nodes_remaining=opts.limits.max_nodes)
    docs: list[Document] = []
    for dval in docs_val.v:
        if len(docs) + 1 > opts.limits.max_documents:
            raise _wire_limit_error("max_documents", opts.limits.max_documents)
        if dval.t != "obj":
            raise _not_source_error("internal", {}, "wire document must be an object")
        dm: dict[str, _JVal] = dval.v
        _expect_exact_keys(dm, ("yaml_version", "explicit_start", "root"))
        yv_raw = dm["yaml_version"]
        if yv_raw.t == "null":
            yaml_version = None
        elif yv_raw.t == "arr" and len(yv_raw.v) == 2:
            maj = _expect_nonneg_int(yv_raw.v[0], "yaml_version[0]")
            minor = _expect_nonneg_int(yv_raw.v[1], "yaml_version[1]")
            if maj > 0xFFFFFFFF or minor > 0xFFFFFFFF:
                raise _not_source_error("internal", {}, "yaml_version component exceeds u32::MAX")
            yaml_version = (maj, minor)
        else:
            raise _not_source_error("internal", {}, "invalid wire yaml_version")
        explicit_start = _expect_bool(dm.get("explicit_start"), "explicit_start")
        root_node = _decode_node(dm["root"], ctx, 0, False)
        docs.append(Document(root=root_node, yaml_version=yaml_version, explicit_start=explicit_start))

    return WireEnvelope(kind="documents", documents=tuple(docs))


# ---- Canonical yamlprims-value/1 Encoder ---------------------------------


class _BoundedWriter:
    def __init__(self, max_bytes: int) -> None:
        self.max_bytes = max_bytes
        self.byte_len = 0
        self.chunks: list[str] = []

    def _throw_over(self) -> None:
        raise _not_source_error(
            "output_limit_exceeded",
            {"configured": self.max_bytes},
            f"encoded output exceeds max_output_bytes ({self.max_bytes})",
        )

    def push_raw(self, ascii_str: str) -> None:
        n = len(ascii_str)
        if self.byte_len + n > self.max_bytes:
            self._throw_over()
        self.byte_len += n
        self.chunks.append(ascii_str)

    def push_meta_int(self, n: int, label: str) -> None:
        if type(n) is not int or n < 0 or n > MAX_SAFE_METADATA_INT:
            raise _not_source_error("internal", {}, f"metadata integer out of range for {label}")
        self.push_raw(str(n))

    def push_string(self, s: str) -> None:
        self._push_string_parts(s, "")

    def push_bounded_string(self, s: str, max_utf8_bytes: int) -> None:
        try:
            utf8 = s.encode("utf-8", errors="strict")
        except UnicodeEncodeError:
            raise _not_source_error("invalid_utf8", {"invalid_len": None}, "string contains invalid UTF-8") from None
        if len(utf8) <= max_utf8_bytes:
            self._push_string_parts(s, "")
            return
        cut = max(0, max_utf8_bytes - 3)
        while cut > 0 and (utf8[cut] & 0xC0) == 0x80:
            cut -= 1
        prefix = utf8[:cut].decode("utf-8", errors="strict")
        self._push_string_parts(prefix, "…")

    def _push_string_parts(self, a: str, b: str) -> None:
        try:
            a_bytes = len(a.encode("utf-8", errors="strict"))
            b_bytes = len(b.encode("utf-8", errors="strict"))
        except UnicodeEncodeError:
            raise _not_source_error("invalid_utf8", {"invalid_len": None}, "string contains invalid UTF-8") from None
        if self.byte_len + a_bytes + b_bytes + 2 > self.max_bytes:
            self._throw_over()
        self.push_raw('"')
        for part in (a, b):
            run = 0
            for i, ch in enumerate(part):
                c = ord(ch)
                esc: str | None = None
                if c == 0x22:
                    esc = '\\"'
                elif c == 0x5C:
                    esc = "\\\\"
                elif c == 0x08:
                    esc = "\\b"
                elif c == 0x0C:
                    esc = "\\f"
                elif c == 0x0A:
                    esc = "\\n"
                elif c == 0x0D:
                    esc = "\\r"
                elif c == 0x09:
                    esc = "\\t"
                elif c < 0x20:
                    esc = f"\\u{c:04x}"
                if esc is not None:
                    if i > run:
                        self._push_utf8_slice(part[run:i])
                    self.push_raw(esc)
                    run = i + 1
            if run < len(part):
                self._push_utf8_slice(part[run:])
        self.push_raw('"')

    def _push_utf8_slice(self, sub: str) -> None:
        n = len(sub.encode("utf-8"))
        if self.byte_len + n > self.max_bytes:
            self._throw_over()
        self.byte_len += n
        self.chunks.append(sub)

    def push_loc(self, loc: Location) -> None:
        if loc.line < 1 or loc.column < 1:
            raise _not_source_error("internal", {}, "Location line and column must be >= 1")
        self.push_raw("[")
        self.push_meta_int(loc.byte_offset, "Location.byte_offset")
        self.push_raw(",")
        self.push_meta_int(loc.line, "Location.line")
        self.push_raw(",")
        self.push_meta_int(loc.column, "Location.column")
        self.push_raw("]")

    def push_span(self, span: Span) -> None:
        if (
            span.start.line < 1
            or span.start.column < 1
            or span.end.line < 1
            or span.end.column < 1
            or span.end.byte_offset < span.start.byte_offset
        ):
            raise _not_source_error("internal", {}, "invalid Span coordinates")
        self.push_raw("[")
        self.push_meta_int(span.start.byte_offset, "Span.start.byte_offset")
        self.push_raw(",")
        self.push_meta_int(span.start.line, "Span.start.line")
        self.push_raw(",")
        self.push_meta_int(span.start.column, "Span.start.column")
        self.push_raw(",")
        self.push_meta_int(span.end.byte_offset, "Span.end.byte_offset")
        self.push_raw(",")
        self.push_meta_int(span.end.line, "Span.end.line")
        self.push_raw(",")
        self.push_meta_int(span.end.column, "Span.end.column")
        self.push_raw("]")

    def finish(self) -> str:
        return "".join(self.chunks)


@dataclass
class _EncodeState:
    opts: Options
    nodes_remaining: int
    active_ids: set[int] = field(default_factory=set)


_EXP_LEADING_ZERO_RE: Final[re.Pattern[str]] = re.compile(r"e([+-]?)0+(\d+)")


def _format_wire_float(f: float) -> str:
    if _is_neg_zero(f):
        return "-0.0"
    s = repr(f)
    if "." not in s and "e" not in s and "E" not in s:
        return s + ".0"
    return _EXP_LEADING_ZERO_RE.sub(
        lambda m: f"e{'-' if m.group(1) == '-' else ''}{m.group(2)}",
        s.replace("E", "e").replace("e+", "e"),
    )


def _finish_node_locations(w: _BoundedWriter, node: Node, with_spans: bool) -> None:
    w.push_raw(',"span":')
    if with_spans and node.span is not None:
        w.push_span(node.span)
    else:
        w.push_raw("null")
    w.push_raw(',"via":')
    if with_spans and node.via is not None:
        if node.via.kind == "alias":
            w.push_raw('{"alias":')
            w.push_span(node.via.span)
            w.push_raw("}")
        else:
            w.push_raw('{"merge":')
            w.push_span(node.via.span)
            w.push_raw("}")
    else:
        w.push_raw("null")
    w.push_raw(',"tag_start":')
    if with_spans and node.tag_start is not None:
        w.push_loc(node.tag_start)
    else:
        w.push_raw("null")
    w.push_raw("}")


def _run_write_node_frames(
    w: _BoundedWriter,
    root_node: Node,
    with_spans: bool,
    st: _EncodeState,
    root_depth: int,
    finish_root_locations: bool,
) -> None:
    stack: list[Any] = [("node", root_node, root_depth, finish_root_locations)]
    while stack:
        frame = stack.pop()
        op = frame[0]
        if op == "raw":
            w.push_raw(frame[1])
            continue
        if op == "finish_loc":
            _finish_node_locations(w, frame[1], with_spans)
            continue
        if op == "leave_id":
            st.active_ids.discard(frame[1])
            continue
        if op == "seq":
            _, seq_items, idx, next_depth, nid, seq_id = frame
            if idx >= len(seq_items):
                w.push_raw("]")
                st.active_ids.discard(seq_id)
                st.active_ids.discard(nid)
            else:
                if idx > 0:
                    w.push_raw(",")
                stack.append(("seq", seq_items, idx + 1, next_depth, nid, seq_id))
                stack.append(("node", seq_items[idx], next_depth, True))
            continue
        if op == "map":
            _, entries, idx, next_depth, nid, map_id, key_index = frame
            if idx >= len(entries):
                w.push_raw("]")
                st.active_ids.discard(map_id)
                st.active_ids.discard(nid)
            else:
                k, v = entries[idx]
                if _contains_nan(k):
                    st.active_ids.discard(map_id)
                    st.active_ids.discard(nid)
                    raise _not_source_error("invalid_key", {}, "mapping key contains NaN")
                if not key_index.check_and_insert(k):
                    st.active_ids.discard(map_id)
                    st.active_ids.discard(nid)
                    raise _not_source_error("duplicate_key", {}, "duplicate mapping key")
                if idx > 0:
                    w.push_raw(",")
                w.push_raw('{"key":')
                stack.append(
                    ("map", entries, idx + 1, next_depth, nid, map_id, key_index)
                )
                stack.append(("raw", "}"))
                stack.append(("node", v, next_depth, True))
                stack.append(("raw", ',"value":'))
                stack.append(("node", k, next_depth, True))
            continue

        # op == "node"
        _, node, depth, finish_loc = frame
        if st.nodes_remaining <= 0:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_nodes", "configured": st.opts.limits.max_nodes},
                "tree exceeds max_nodes",
            )
        st.nodes_remaining -= 1

        nid = id(node)
        if nid in st.active_ids:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_depth", "configured": st.opts.limits.max_depth},
                "cyclic node graph exceeds max_depth",
            )

        if finish_loc:
            stack.append(("finish_loc", node))

        w.push_raw('{"kind":')
        w.push_string(node.kind)

        if node.kind == "null":
            continue

        if node.kind == "bool":
            w.push_raw(',"value":true' if node.value else ',"value":false')
            continue

        if node.kind == "int":
            iv = node.value
            if type(iv) is not int or iv < I128_MIN or iv > U128_MAX:
                raise _not_source_error("integer_out_of_range", {}, "integer outside 128-bit range")
            w.push_raw(',"value":')
            w.push_string(str(iv))
            continue

        if node.kind == "float":
            fval = float(node.value)
            bits_hex = node.bits if node.bits is not None else _float_to_bits_hex(fval)
            if not _HEX16_LOWER_RE.match(bits_hex):
                raise _not_source_error("internal", {}, "invalid float bits on Node")
            bits_int = int(bits_hex, 16)
            is_nan_bits = (bits_int & 0x7FF0000000000000) == 0x7FF0000000000000 and (
                bits_int & 0x000FFFFFFFFFFFFF
            ) != 0
            canonical_bits = CANONICAL_NAN_BITS if (math.isnan(fval) or is_nan_bits) else bits_hex
            w.push_raw(f',"bits":"{canonical_bits}"')
            continue

        if node.kind == "string":
            w.push_raw(',"value":')
            w.push_string(node.value)
            continue

        if node.kind == "sequence":
            if depth + 1 > st.opts.limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": st.opts.limits.max_depth},
                    "sequence nesting exceeds max_depth",
                )
            seq_id = id(node.value)
            if seq_id in st.active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": st.opts.limits.max_depth},
                    "cyclic sequence exceeds max_depth",
                )
            st.active_ids.add(nid)
            st.active_ids.add(seq_id)
            w.push_raw(',"items":[')
            stack.append(("seq", node.value, 0, depth + 1, nid, seq_id))
            continue

        if node.kind == "mapping":
            if depth + 1 > st.opts.limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": st.opts.limits.max_depth},
                    "mapping nesting exceeds max_depth",
                )
            m: Mapping = node.value if isinstance(node.value, Mapping) else Mapping(node.value)
            if len(m.entries) > st.opts.limits.max_mapping_entries:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_mapping_entries", "configured": st.opts.limits.max_mapping_entries},
                    "mapping exceeds max_mapping_entries",
                )
            map_id = id(m)
            if map_id in st.active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": st.opts.limits.max_depth},
                    "cyclic mapping exceeds max_depth",
                )
            st.active_ids.add(nid)
            st.active_ids.add(map_id)
            w.push_raw(',"entries":[')
            stack.append(("map", m.entries, 0, depth + 1, nid, map_id, _KeyIndex()))
            continue

        if node.kind == "tagged":
            t: Tagged = node.value
            if not isinstance(t, Tagged) or not t.tag.uri:
                raise _not_source_error("internal", {}, "tagged node must carry a non-empty Tag.uri")
            if t.value.kind == "tagged":
                raise _not_source_error(
                    "internal", {}, "nested Tagged-in-Tagged is invalid in yamlprims-value/1"
                )
            st.active_ids.add(nid)
            w.push_raw(',"tag":{"uri":')
            w.push_string(t.tag.uri)
            w.push_raw(',"handle":')
            w.push_string(t.tag.handle)
            w.push_raw(',"suffix":')
            w.push_string(t.tag.suffix)
            w.push_raw('},"value":')
            stack.append(("leave_id", nid))
            stack.append(("node", t.value, depth, True))
            continue

        raise _not_source_error("internal", {}, f"unknown Node kind: {node.kind}")


def _write_node_value(
    w: _BoundedWriter,
    node: Node,
    with_spans: bool,
    st: _EncodeState,
    depth: int,
) -> None:
    _run_write_node_frames(w, node, with_spans, st, depth, False)


def _write_node(
    w: _BoundedWriter,
    node: Node,
    with_spans: bool,
    st: _EncodeState,
    depth: int,
) -> None:
    _run_write_node_frames(w, node, with_spans, st, depth, True)


def encode(docs: Sequence[Document], options: Options | TypingMapping[str, Any] | None = None) -> str:
    """Serialize a sequence of `Document` values to the `yamlprims-value/1` wire format."""
    opts = validate_options(options)
    if len(docs) > opts.limits.max_documents:
        raise _not_source_error(
            "limit_exceeded",
            {"limit": "max_documents", "configured": opts.limits.max_documents},
            f"document count exceeds max_documents ({opts.limits.max_documents})",
        )
    w = _BoundedWriter(opts.limits.max_output_bytes)
    st = _EncodeState(opts=opts, nodes_remaining=opts.limits.max_nodes)
    w.push_raw('{"format":"yamlprims-value","version":1,"documents":[')
    for i, doc in enumerate(docs):
        if i > 0:
            w.push_raw(",")
        w.push_raw('{"yaml_version":')
        if doc.yaml_version is not None:
            maj, minor = doc.yaml_version
            if (
                type(maj) is not int
                or type(minor) is not int
                or maj < 0
                or maj > 0xFFFFFFFF
                or minor < 0
                or minor > 0xFFFFFFFF
            ):
                raise _not_source_error("internal", {}, "invalid Document.yaml_version")
            w.push_raw(f"[{maj},{minor}]")
        else:
            w.push_raw("null")
        w.push_raw(',"explicit_start":true,"root":' if doc.explicit_start else ',"explicit_start":false,"root":')
        _write_node(w, doc.root, True, st, 0)
        w.push_raw("}")
    w.push_raw("]}")
    return w.finish()


def _preflight_key_node(root_key: Node, limits: Limits) -> None:
    max_key = limits.max_key_bytes
    weight = NODE_OVERHEAD
    if weight > max_key:
        raise _not_source_error(
            "limit_exceeded",
            {"limit": "max_key_bytes", "configured": limits.max_key_bytes},
            "error path key exceeds max_key_bytes",
        )
    stack: list[tuple[Node, int]] = [(root_key, 0)]
    while stack:
        node, depth = stack.pop()
        more = 0
        children = 0
        if node.kind == "string":
            more = len(node.value.encode("utf-8"))
        elif node.kind == "tagged":
            t: Tagged = node.value
            more = (
                len(t.tag.uri.encode("utf-8"))
                + len(t.tag.handle.encode("utf-8"))
                + len(t.tag.suffix.encode("utf-8"))
                + NODE_OVERHEAD
            )
            children = 1
        elif node.kind == "sequence":
            more = NODE_OVERHEAD * len(node.value)
            children = len(node.value)
        elif node.kind == "mapping":
            m: Mapping = node.value if isinstance(node.value, Mapping) else Mapping(node.value)
            more = (2 * NODE_OVERHEAD + ENTRY_OVERHEAD) * len(m.entries)
            children = 2 * len(m.entries)
        weight += more
        if weight > max_key:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_key_bytes", "configured": limits.max_key_bytes},
                "error path key exceeds max_key_bytes",
            )
        if children > 0 and node.kind != "tagged" and depth + 1 > limits.max_depth:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_depth", "configured": limits.max_depth},
                "error path key nests deeper than max_depth",
            )
        if node.kind == "tagged":
            stack.append((node.value.value, depth))
        elif node.kind == "sequence":
            for it in node.value:
                stack.append((it, depth + 1))
        elif node.kind == "mapping":
            m = node.value if isinstance(node.value, Mapping) else Mapping(node.value)
            for k, v in m.entries:
                stack.append((k, depth + 1))
                stack.append((v, depth + 1))


def encode_error(
    err: YamlprimsError,
    options: Options | TypingMapping[str, Any] | None = None,
) -> str:
    """Serialize a `YamlprimsError` to the `yamlprims-value/1` wire error envelope."""
    opts = validate_options(options)
    if len(err.path) > opts.limits.max_depth:
        raise _not_source_error(
            "limit_exceeded",
            {"limit": "max_depth", "configured": opts.limits.max_depth},
            f"error path has {len(err.path)} segments, more than max_depth",
        )
    for seg in err.path:
        if seg.kind == "key" and seg.key is not None:
            _preflight_key_node(seg.key, opts.limits)

    w = _BoundedWriter(opts.limits.max_output_bytes)
    w.push_raw('{"format":"yamlprims-value","version":1,"error":{"kind":')
    w.push_string(err.kind)
    w.push_raw(',"detail":')

    d = err.detail
    if err.kind == "invalid_utf8":
        if set(d.keys()) != {"invalid_len"}:
            raise _not_source_error("internal", {}, "invalid invalid_utf8 detail")
        il = d["invalid_len"]
        if il is None:
            w.push_raw('{"invalid_len":null}')
        elif type(il) is int:
            w.push_raw('{"invalid_len":')
            w.push_meta_int(il, "detail.invalid_len")
            w.push_raw("}")
        else:
            raise _not_source_error("internal", {}, "invalid invalid_utf8.invalid_len")
    elif err.kind == "syntax":
        if set(d.keys()) != {"syntax_kind"} or not isinstance(d["syntax_kind"], str):
            raise _not_source_error("internal", {}, "invalid syntax detail")
        sk = d["syntax_kind"]
        if sk not in SYNTAX_KINDS:
            raise _not_source_error("internal", {}, f"unknown syntax_kind: {sk}")
        w.push_raw('{"syntax_kind":')
        w.push_string(sk)
        w.push_raw("}")
    elif err.kind == "limit_exceeded":
        if (
            set(d.keys()) != {"limit", "configured"}
            or not isinstance(d["limit"], str)
            or d["limit"] not in LIMIT_KINDS
            or type(d["configured"]) is not int
        ):
            raise _not_source_error("internal", {}, "invalid limit_exceeded detail")
        w.push_raw('{"limit":')
        w.push_string(d["limit"])
        w.push_raw(',"configured":')
        w.push_meta_int(d["configured"], "detail.configured")
        w.push_raw("}")
    elif err.kind == "type":
        if (
            set(d.keys()) != {"expected", "found"}
            or not isinstance(d["expected"], str)
            or not isinstance(d["found"], str)
        ):
            raise _not_source_error("internal", {}, "invalid type detail")
        w.push_raw('{"expected":')
        w.push_bounded_string(d["expected"], MAX_FIELD_BYTES)
        w.push_raw(',"found":')
        w.push_bounded_string(d["found"], MAX_FIELD_BYTES)
        w.push_raw("}")
    elif err.kind in ("missing_field", "unknown_field"):
        if set(d.keys()) != {"field"} or not isinstance(d["field"], str):
            raise _not_source_error("internal", {}, f"invalid {err.kind} detail")
        w.push_raw('{"field":')
        w.push_bounded_string(d["field"], MAX_FIELD_BYTES)
        w.push_raw("}")
    elif err.kind == "output_limit_exceeded":
        if set(d.keys()) != {"configured"} or type(d["configured"]) is not int:
            raise _not_source_error("internal", {}, "invalid output_limit_exceeded detail")
        w.push_raw('{"configured":')
        w.push_meta_int(d["configured"], "detail.configured")
        w.push_raw("}")
    elif err.kind in _EMPTY_DETAIL_ERROR_KINDS:
        if d:
            raise _not_source_error("internal", {}, f"non-empty detail for {err.kind}")
        w.push_raw("{}")
    else:
        raise _not_source_error("internal", {}, f"unknown error kind: {err.kind}")

    w.push_raw(',"site":')
    if "source" in err.site and err.site["source"] is not None:
        w.push_raw('{"source":')
        w.push_loc(err.site["source"])
        w.push_raw("}")
    else:
        ns = err.site.get("not_source", {})
        lk = ns.get("last_known") if isinstance(ns, dict) else None
        w.push_raw('{"not_source":{"last_known":')
        if lk is not None:
            w.push_loc(lk)
        else:
            w.push_raw("null")
        w.push_raw("}}")

    w.push_raw(',"related":[')
    for i, r in enumerate(err.related):
        if i > 0:
            w.push_raw(",")
        w.push_raw('{"role":')
        w.push_string(r.role)
        w.push_raw(',"span":')
        w.push_span(r.span)
        w.push_raw("}")

    path_opts = Options(
        merge_keys=opts.merge_keys,
        unknown_tags=opts.unknown_tags,
        limits=Limits(max_depth=255, max_nodes=MAX_SAFE_METADATA_INT),
    )
    st = _EncodeState(opts=path_opts, nodes_remaining=MAX_SAFE_METADATA_INT)
    w.push_raw('],"path":[')
    for i, p in enumerate(err.path):
        if i > 0:
            w.push_raw(",")
        if p.kind == "index" and p.index is not None:
            w.push_raw('{"index":')
            w.push_meta_int(p.index, "path.index")
            w.push_raw("}")
        elif p.kind == "key" and p.key is not None:
            w.push_raw('{"key":')
            _write_node_value(w, p.key, False, st, 0)
            w.push_raw(',"span":null,"via":null,"tag_start":null}}')
        else:
            raise _not_source_error("internal", {}, "invalid PathSegment")

    w.push_raw('],"message":')
    w.push_bounded_string(err.message, MAX_MESSAGE_BYTES)
    w.push_raw("}}")
    return w.finish()


# ---- Plain JSON & to_plain projection (spec-v1.4 §11.3) ------------------


def _loss_error(node: Node, what: str) -> YamlprimsError:
    site = (
        {"source": node.span.start}
        if node.span is not None
        else {"not_source": {"last_known": None}}
    )
    return YamlprimsError(
        kind="type",
        message=f"cannot write {what} as plain JSON without loss",
        detail={"expected": "plain JSON value", "found": what},
        site=site,
    )


def to_plain(
    node_or_doc: Node | Document,
    options: Options | TypingMapping[str, Any] | None = None,
) -> Any:
    """Project a `Node` or `Document` to plain Python values (`dict`, `list`, `str`, `int`, `float`, `bool`, `None`).

    Fails with `YamlprimsError(kind="type")` on:
    - non-`string` mapping keys (preventing Python `True == 1 == 1.0` dict key collisions),
    - `Tagged` values.
    Enforces `max_depth`, `max_nodes`, and cyclic graph detection via `id(obj)`.
    """
    opts = validate_options(options)
    root = node_or_doc.root if isinstance(node_or_doc, Document) else node_or_doc
    nodes_remaining = opts.limits.max_nodes
    active_ids: set[int] = set()
    work: list[Any] = [("eval", root, 0)]
    results: list[Any] = []

    while work:
        frame = work.pop()
        op = frame[0]
        if op == "seq_step":
            _, seq_items, idx, next_depth, nid, seq_id, out_list = frame
            if idx > 0:
                out_list.append(results.pop())
            if idx >= len(seq_items):
                active_ids.discard(seq_id)
                active_ids.discard(nid)
                results.append(out_list)
            else:
                work.append(("seq_step", seq_items, idx + 1, next_depth, nid, seq_id, out_list))
                work.append(("eval", seq_items[idx], next_depth))
            continue

        if op == "map_step":
            _, entries, idx, next_depth, nid, map_id, out_dict, pending_key = frame
            if pending_key is not None:
                out_dict[pending_key] = results.pop()
            if idx >= len(entries):
                active_ids.discard(map_id)
                active_ids.discard(nid)
                results.append(out_dict)
            else:
                k, v = entries[idx]
                if k.kind != "string":
                    active_ids.discard(map_id)
                    active_ids.discard(nid)
                    raise _loss_error(k, "non-string key")
                try:
                    k.value.encode("utf-8", errors="strict")
                except UnicodeEncodeError:
                    active_ids.discard(map_id)
                    active_ids.discard(nid)
                    raise _not_source_error(
                        "invalid_utf8", {"invalid_len": None}, "invalid UTF-8"
                    ) from None
                if k.value in out_dict:
                    active_ids.discard(map_id)
                    active_ids.discard(nid)
                    raise _not_source_error("duplicate_key", {}, "duplicate mapping key")
                work.append(
                    ("map_step", entries, idx + 1, next_depth, nid, map_id, out_dict, k.value)
                )
                work.append(("eval", v, next_depth))
            continue

        _, n, depth = frame
        if nodes_remaining <= 0:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_nodes", "configured": opts.limits.max_nodes},
                "tree exceeds max_nodes",
            )
        nodes_remaining -= 1
        nid = id(n)
        if nid in active_ids:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_depth", "configured": opts.limits.max_depth},
                "cyclic node graph exceeds max_depth",
            )
        if n.kind == "null":
            results.append(None)
            continue
        if n.kind == "bool":
            results.append(bool(n.value))
            continue
        if n.kind == "int":
            results.append(int(n.value))
            continue
        if n.kind == "float":
            results.append(float(n.value))
            continue
        if n.kind == "string":
            try:
                n.value.encode("utf-8", errors="strict")
            except UnicodeEncodeError:
                raise _not_source_error(
                    "invalid_utf8", {"invalid_len": None}, "invalid UTF-8"
                ) from None
            results.append(n.value)
            continue
        if n.kind == "sequence":
            if depth + 1 > opts.limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": opts.limits.max_depth},
                    "sequence nesting exceeds max_depth",
                )
            seq_id = id(n.value)
            if seq_id in active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": opts.limits.max_depth},
                    "cyclic sequence exceeds max_depth",
                )
            active_ids.add(nid)
            active_ids.add(seq_id)
            work.append(("seq_step", n.value, 0, depth + 1, nid, seq_id, []))
            continue
        if n.kind == "mapping":
            if depth + 1 > opts.limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": opts.limits.max_depth},
                    "mapping nesting exceeds max_depth",
                )
            m: Mapping = n.value if isinstance(n.value, Mapping) else Mapping(n.value)
            map_id = id(m)
            if map_id in active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": opts.limits.max_depth},
                    "cyclic mapping exceeds max_depth",
                )
            active_ids.add(nid)
            active_ids.add(map_id)
            work.append(("map_step", m.entries, 0, depth + 1, nid, map_id, {}, None))
            continue
        if n.kind == "tagged":
            raise _loss_error(n, "tagged value")
        raise _not_source_error("internal", {}, f"unknown Node kind: {n.kind}")

    return results.pop()


def encode_plain(
    node_or_doc: Node | Document,
    options: Options | TypingMapping[str, Any] | None = None,
) -> str:
    """Encode a `Node` or `Document` root as plain JSON text (`spec-v1.4 §11.3`)."""
    opts = validate_options(options)
    root = node_or_doc.root if isinstance(node_or_doc, Document) else node_or_doc
    w = _BoundedWriter(opts.limits.max_output_bytes)
    nodes_remaining = opts.limits.max_nodes
    active_ids: set[int] = set()
    stack: list[Any] = [("eval", root, 0)]

    while stack:
        frame = stack.pop()
        op = frame[0]
        if op == "seq_step":
            _, seq_items, idx, next_depth, nid, seq_id = frame
            if idx >= len(seq_items):
                w.push_raw("]")
                active_ids.discard(seq_id)
                active_ids.discard(nid)
            else:
                if idx > 0:
                    w.push_raw(",")
                stack.append(("seq_step", seq_items, idx + 1, next_depth, nid, seq_id))
                stack.append(("eval", seq_items[idx], next_depth))
            continue

        if op == "map_step":
            _, entries, idx, next_depth, nid, map_id = frame
            if idx >= len(entries):
                w.push_raw("}")
                active_ids.discard(map_id)
                active_ids.discard(nid)
            else:
                k, v = entries[idx]
                if k.kind != "string":
                    active_ids.discard(map_id)
                    active_ids.discard(nid)
                    raise _loss_error(k, "non-string key")
                if idx > 0:
                    w.push_raw(",")
                w.push_string(k.value)
                w.push_raw(":")
                stack.append(("map_step", entries, idx + 1, next_depth, nid, map_id))
                stack.append(("eval", v, next_depth))
            continue

        _, n, depth = frame
        if nodes_remaining <= 0:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_nodes", "configured": opts.limits.max_nodes},
                "tree exceeds max_nodes",
            )
        nodes_remaining -= 1
        nid = id(n)
        if nid in active_ids:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_depth", "configured": opts.limits.max_depth},
                "cyclic node graph exceeds max_depth",
            )
        if n.kind == "null":
            w.push_raw("null")
            continue
        if n.kind == "bool":
            w.push_raw("true" if n.value else "false")
            continue
        if n.kind == "int":
            w.push_raw(str(int(n.value)))
            continue
        if n.kind == "float":
            fval = float(n.value)
            if not math.isfinite(fval):
                raise _loss_error(n, "non-finite float")
            w.push_raw(_format_wire_float(fval))
            continue
        if n.kind == "string":
            w.push_string(n.value)
            continue
        if n.kind == "sequence":
            if depth + 1 > opts.limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": opts.limits.max_depth},
                    "sequence nesting exceeds max_depth",
                )
            seq_id = id(n.value)
            if seq_id in active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": opts.limits.max_depth},
                    "cyclic sequence exceeds max_depth",
                )
            active_ids.add(nid)
            active_ids.add(seq_id)
            w.push_raw("[")
            stack.append(("seq_step", n.value, 0, depth + 1, nid, seq_id))
            continue
        if n.kind == "mapping":
            if depth + 1 > opts.limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": opts.limits.max_depth},
                    "mapping nesting exceeds max_depth",
                )
            m: Mapping = n.value if isinstance(n.value, Mapping) else Mapping(n.value)
            map_id = id(m)
            if map_id in active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": opts.limits.max_depth},
                    "cyclic mapping exceeds max_depth",
                )
            active_ids.add(nid)
            active_ids.add(map_id)
            w.push_raw("{")
            stack.append(("map_step", m.entries, 0, depth + 1, nid, map_id))
            continue
        if n.kind == "tagged":
            raise _loss_error(n, "tagged value")
        raise _not_source_error("internal", {}, f"unknown Node kind: {n.kind}")

    return w.finish()


# ---- Primary entrypoints -------------------------------------------------


def _handle_native_status(status: int, payload: str, opts: Options) -> WireEnvelope:
    if status == 2:
        configured = int(payload) if payload.isdigit() else opts.limits.max_output_bytes
        raise _not_source_error(
            "output_limit_exceeded",
            {"configured": configured},
            f"encoded output exceeds max_output_bytes ({configured})",
        )
    if status == 3:
        raise _not_source_error("internal", {}, payload or "internal native error")
    payload_len = len(payload.encode("utf-8"))
    lim = opts.limits
    bridge_nodes = max(lim.max_nodes, lim.max_expanded_nodes, 250_000)
    bridge_owned = max(
        lim.max_owned_bytes,
        min(MAX_SAFE_METADATA_INT, payload_len * 4 + 65_536),
    )
    bridge_scalar = min(bridge_owned, max(lim.max_scalar_bytes, payload_len))
    bridge_key = min(bridge_owned, max(lim.max_key_bytes, payload_len))
    bridge_limits = Limits(
        max_input_bytes=max(lim.max_input_bytes, lim.max_output_bytes, payload_len, 1),
        max_depth=255,
        max_nodes=bridge_nodes,
        max_expanded_nodes=max(lim.max_expanded_nodes, bridge_nodes),
        max_alias_events=lim.max_alias_events,
        max_owned_bytes=bridge_owned,
        max_scalar_bytes=bridge_scalar,
        max_key_bytes=bridge_key,
        max_mapping_entries=max(lim.max_mapping_entries, 250_000),
        max_documents=max(lim.max_documents, 1_024),
        max_output_bytes=max(lim.max_output_bytes, payload_len, 1),
        max_warnings=lim.max_warnings,
    )
    return decode_wire(
        payload,
        Options(
            merge_keys=opts.merge_keys,
            unknown_tags=opts.unknown_tags,
            limits=bridge_limits,
        ),
    )


def load_all(
    data: Input,
    options: Options | TypingMapping[str, Any] | None = None,
) -> list[Document]:
    """Load all YAML 1.2.2 documents from a `str` or bytes-like input."""
    opts = validate_options(options)
    raw = _to_input_bytes(data, require_bytes=False)
    status, payload = _native.load_wire(
        raw,
        False,
        opts.merge_keys,
        opts.unknown_tags,
        opts.limits._to_native_list(),
    )
    env = _handle_native_status(status, payload, opts)
    if env.kind == "error":
        assert env.error is not None
        raise env.error
    assert env.documents is not None
    return list(env.documents)


def load(
    data: Input,
    options: Options | TypingMapping[str, Any] | None = None,
) -> Document:
    """Load a single YAML 1.2.2 document from a `str` or bytes-like input."""
    opts = validate_options(options)
    raw = _to_input_bytes(data, require_bytes=False)
    status, payload = _native.load_wire(
        raw,
        True,
        opts.merge_keys,
        opts.unknown_tags,
        opts.limits._to_native_list(),
    )
    env = _handle_native_status(status, payload, opts)
    if env.kind == "error":
        assert env.error is not None
        raise env.error
    if not env.documents or len(env.documents) != 1:
        raise _not_source_error("internal", {}, "expected single document from load_wire")
    return env.documents[0]


def load_bytes(
    data: bytes | bytearray | memoryview,
    options: Options | TypingMapping[str, Any] | None = None,
) -> Document:
    """Load a single YAML 1.2.2 document from a bytes-like input."""
    raw = _to_input_bytes(data, require_bytes=True)
    return load(raw, options)


def load_all_bytes(
    data: bytes | bytearray | memoryview,
    options: Options | TypingMapping[str, Any] | None = None,
) -> list[Document]:
    """Load all YAML 1.2.2 documents from a bytes-like input."""
    raw = _to_input_bytes(data, require_bytes=True)
    return load_all(raw, options)


def load_plain(
    data: Input,
    options: Options | TypingMapping[str, Any] | None = None,
) -> str:
    """Load a single YAML 1.2.2 document and return its plain JSON text."""
    opts = validate_options(options)
    raw = _to_input_bytes(data, require_bytes=False)
    status, payload = _native.load_plain(
        raw,
        opts.merge_keys,
        opts.unknown_tags,
        opts.limits._to_native_list(),
    )
    if status == 0:
        return payload
    env = _handle_native_status(status, payload, opts)
    if env.kind == "error" and env.error is not None:
        raise env.error
    raise _not_source_error("internal", {}, "expected error envelope from load_plain")


def _validate_ijson_options(
    options: Options | TypingMapping[str, Any] | None = None,
    **kwargs: Any,
) -> Options:
    if kwargs:
        if options is None:
            options = dict(kwargs)
        elif isinstance(options, TypingMapping):
            merged = dict(options)
            merged.update(kwargs)
            options = merged
        else:
            raise _invalid_options_error("cannot combine Options instance with keyword arguments")
    if options is None:
        return default_ijson_options()
    if isinstance(options, Options):
        return options.validate()
    if isinstance(options, TypingMapping):
        allowed = {"merge_keys", "unknown_tags", "limits"}
        extra = set(options.keys()) - allowed
        if extra:
            raise _invalid_options_error(f"unknown option keys: {sorted(extra)}")
        mk = options.get("merge_keys", "reject")
        ut = options.get("unknown_tags", "reject")
        raw_lim = options.get("limits")
        if raw_lim is None:
            lim = Limits(max_output_bytes=DEFAULT_IJSON_MAX_OUTPUT_BYTES)
        elif isinstance(raw_lim, Limits):
            lim = raw_lim
        elif isinstance(raw_lim, TypingMapping):
            extra_lim = set(raw_lim.keys()) - set(_LIMIT_FIELD_NAMES)
            if extra_lim:
                raise _invalid_options_error(f"unknown limit keys: {sorted(extra_lim)}")
            lim_kwargs: dict[str, int] = {"max_output_bytes": DEFAULT_IJSON_MAX_OUTPUT_BYTES}
            for k, v in raw_lim.items():
                if type(v) is not int:
                    raise _invalid_options_error(f"limit {k} must be an int")
                lim_kwargs[k] = v
            lim = Limits(**lim_kwargs)
        else:
            raise _invalid_options_error("limits must be a Limits or mapping")
        if not isinstance(mk, str) or not isinstance(ut, str):
            raise _invalid_options_error("merge_keys and unknown_tags must be strings")
        return Options(merge_keys=mk, unknown_tags=ut, limits=lim).validate()
    raise _invalid_options_error(f"options must be Options or dict, got {type(options).__name__}")


def load_ijson(
    data: Input,
    options: Options | TypingMapping[str, Any] | None = None,
    **kwargs: Any,
) -> str:
    """Load a single YAML 1.2.2 document and project it to compact RFC 7493 I-JSON text (`spec-v1.5 §11.6`)."""
    opts = _validate_ijson_options(options, **kwargs)
    raw = _to_input_bytes(data, require_bytes=False)
    status, payload = _native.load_ijson(
        raw,
        opts.merge_keys,
        opts.unknown_tags,
        opts.limits._to_native_list(),
    )
    if status == 0:
        return payload
    env = _handle_native_status(status, payload, opts)
    if env.kind == "error" and env.error is not None:
        raise env.error
    raise _not_source_error("internal", {}, "expected error envelope from load_ijson")


# ---- Host emission & from_plain (`spec-v1.5 §10.2`, `§11.3`, `§11.5`) -----


def _validate_utf8_str(s: str) -> bytes:
    if not isinstance(s, str):
        raise _not_source_error(
            "type",
            {"expected": "str", "found": type(s).__name__},
            f"expected str, got {type(s).__name__}",
        )
    try:
        return s.encode("utf-8", errors="strict")
    except UnicodeEncodeError:
        raise _not_source_error(
            "invalid_utf8",
            {"invalid_len": None},
            "invalid UTF-8 (unpaired surrogate in string)",
        ) from None


def _rendered_tag_min_bytes(uri_utf8: bytes, uri: str) -> int:
    n = len(uri_utf8)
    if uri.startswith("tag:yaml.org,2002:") and n > 18:
        return 2 + (n - 18)
    return n


def _check_emit_key_weight(raw_key: Any, limits: EmitLimits) -> None:
    max_key_bytes = limits.max_key_bytes
    total = 0
    stack: list[Any] = [("eval", raw_key, set())]
    while stack:
        frame = stack.pop()
        op = frame[0]
        if op == "seq_step":
            _, seq_items, idx, ancestors = frame
            if idx < len(seq_items):
                stack.append(("seq_step", seq_items, idx + 1, ancestors))
                stack.append(("eval", seq_items[idx], ancestors))
            continue
        if op == "map_step":
            _, entries, idx, ancestors = frame
            if idx < len(entries):
                k, v = entries[idx]
                stack.append(("map_step", entries, idx + 1, ancestors))
                stack.append(("eval", v, ancestors))
                stack.append(("eval", k, ancestors))
            continue
        if op == "dict_step":
            _, it, ancestors = frame
            pair = next(it, None)
            if pair is not None:
                k, v = pair
                stack.append(("dict_step", it, ancestors))
                stack.append(("eval", v, ancestors))
                stack.append(("eval", k, ancestors))
            continue

        _, cur, ancestors = frame
        if isinstance(cur, Node):
            kind = cur.kind
            val = cur.value
            if kind == "string":
                s_bytes = _validate_utf8_str(val)
                total += NODE_OVERHEAD + len(s_bytes)
                if total > max_key_bytes:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_key_bytes", "configured": max_key_bytes},
                        f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                    )
                continue
            if kind == "tagged":
                if not isinstance(val, Tagged) or not isinstance(val.tag, Tag):
                    raise _invalid_options_error("Tagged node must hold a Tagged value with Tag")
                uri_bytes = _validate_utf8_str(val.tag.uri)
                total += NODE_OVERHEAD + 2 * len(uri_bytes)
                if total > max_key_bytes:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_key_bytes", "configured": max_key_bytes},
                        f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                    )
                cid = id(cur)
                if cid in ancestors:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_depth", "configured": limits.max_depth},
                        f"cyclic mapping key exceeds max_depth ({limits.max_depth})",
                    )
                stack.append(("eval", val.value, ancestors | {cid}))
                continue
            if kind in ("null", "bool", "int", "float"):
                total += NODE_OVERHEAD
                if total > max_key_bytes:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_key_bytes", "configured": max_key_bytes},
                        f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                    )
                continue
            if kind == "sequence":
                total += NODE_OVERHEAD
                if total > max_key_bytes:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_key_bytes", "configured": max_key_bytes},
                        f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                    )
                cid = id(cur)
                vid = id(val)
                if cid in ancestors or vid in ancestors:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_depth", "configured": limits.max_depth},
                        f"cyclic mapping key exceeds max_depth ({limits.max_depth})",
                    )
                next_anc = ancestors | {cid, vid}
                stack.append(("seq_step", val, 0, next_anc))
                continue
            if kind == "mapping":
                entries = val.entries if isinstance(val, Mapping) else val
                total += NODE_OVERHEAD + len(entries) * ENTRY_OVERHEAD
                if total > max_key_bytes:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_key_bytes", "configured": max_key_bytes},
                        f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                    )
                cid = id(cur)
                vid = id(val)
                if cid in ancestors or vid in ancestors:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_depth", "configured": limits.max_depth},
                        f"cyclic mapping key exceeds max_depth ({limits.max_depth})",
                    )
                next_anc = ancestors | {cid, vid}
                stack.append(("map_step", entries, 0, next_anc))
                continue
            raise _not_source_error("internal", {}, f"unknown Node kind: {kind}")

        if isinstance(cur, Tagged):
            if not isinstance(cur.tag, Tag):
                raise _invalid_options_error("Tagged must hold a Tag")
            uri_bytes = _validate_utf8_str(cur.tag.uri)
            total += NODE_OVERHEAD + 2 * len(uri_bytes)
            if total > max_key_bytes:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_key_bytes", "configured": max_key_bytes},
                    f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                )
            cid = id(cur)
            if cid in ancestors:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"cyclic mapping key exceeds max_depth ({limits.max_depth})",
                )
            stack.append(("eval", cur.value, ancestors | {cid}))
            continue

        if type(cur) is str:
            s_bytes = _validate_utf8_str(cur)
            total += NODE_OVERHEAD + len(s_bytes)
            if total > max_key_bytes:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_key_bytes", "configured": max_key_bytes},
                    f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                )
            continue

        if cur is None or type(cur) in (bool, int, float):
            total += NODE_OVERHEAD
            if total > max_key_bytes:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_key_bytes", "configured": max_key_bytes},
                    f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                )
            continue

        if isinstance(cur, (list, tuple)):
            total += NODE_OVERHEAD
            if total > max_key_bytes:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_key_bytes", "configured": max_key_bytes},
                    f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                )
            cid = id(cur)
            if cid in ancestors:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"cyclic mapping key exceeds max_depth ({limits.max_depth})",
                )
            next_anc = ancestors | {cid}
            stack.append(("seq_step", cur, 0, next_anc))
            continue

        if isinstance(cur, Mapping):
            raw_entries = cur.entries
            total += NODE_OVERHEAD + len(raw_entries) * ENTRY_OVERHEAD
            if total > max_key_bytes:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_key_bytes", "configured": max_key_bytes},
                    f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                )
            cid = id(cur)
            if cid in ancestors:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"cyclic mapping key exceeds max_depth ({limits.max_depth})",
                )
            next_anc = ancestors | {cid}
            stack.append(("map_step", raw_entries, 0, next_anc))
            continue

        if isinstance(cur, dict):
            total += NODE_OVERHEAD + len(cur) * ENTRY_OVERHEAD
            if total > max_key_bytes:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_key_bytes", "configured": max_key_bytes},
                    f"mapping key exceeds max_key_bytes ({max_key_bytes})",
                )
            cid = id(cur)
            if cid in ancestors:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"cyclic mapping key exceeds max_depth ({limits.max_depth})",
                )
            next_anc = ancestors | {cid}
            stack.append(("dict_step", iter(cur.items()), next_anc))
            continue

        raise _not_source_error(
            "type",
            {"expected": "YAML-serializable value", "found": type(cur).__name__},
            f"unsupported type for YAML emission: {type(cur).__name__}",
        )


@dataclass
class _EmitPreflightState:
    opts: EmitOptions
    nodes_visited: int = 0
    min_output_bytes: int = 0


def _convert_and_preflight_node(
    root_raw: Any,
    opts: EmitOptions,
    state: _EmitPreflightState,
) -> Node:
    limits = opts.limits
    active_ids: set[int] = set()
    work: list[Any] = [("eval", root_raw, 0, False, False)]
    results: list[Node] = []

    while work:
        frame = work.pop()
        op = frame[0]
        if op == "finish_tagged":
            _, clean_tag, nid = frame
            active_ids.discard(nid)
            clean_inner = results.pop()
            results.append(Node(kind="tagged", value=Tagged(tag=clean_tag, value=clean_inner)))
            continue
        if op == "seq_step":
            _, seq_raw, idx, next_depth, in_key, nid, seq_id, converted_items = frame
            if idx > 0:
                converted_items.append(results.pop())
            if idx >= len(seq_raw):
                active_ids.discard(seq_id)
                if nid is not None:
                    active_ids.discard(nid)
                results.append(Node.sequence(converted_items))
            else:
                work.append(
                    (
                        "seq_step",
                        seq_raw,
                        idx + 1,
                        next_depth,
                        in_key,
                        nid,
                        seq_id,
                        converted_items,
                    )
                )
                work.append(("eval", seq_raw[idx], next_depth, in_key, False))
            continue
        if op == "map_iter":
            (
                _,
                entries_raw,
                idx,
                next_depth,
                in_key,
                nid,
                map_id,
                converted_entries,
                key_index,
            ) = frame
            if idx >= len(entries_raw):
                active_ids.discard(map_id)
                if nid is not None:
                    active_ids.discard(nid)
                results.append(
                    Node(kind="mapping", value=Mapping._from_trusted_entries(converted_entries))
                )
            else:
                raw_k, raw_v = entries_raw[idx]
                work.append(
                    (
                        "map_entry_done",
                        entries_raw,
                        idx + 1,
                        next_depth,
                        in_key,
                        nid,
                        map_id,
                        converted_entries,
                        key_index,
                    )
                )
                work.append(("eval", raw_v, next_depth, in_key, False))
                work.append(("eval", raw_k, next_depth, True, True))
            continue
        if op == "map_entry_done":
            (
                _,
                entries_raw,
                next_idx,
                next_depth,
                in_key,
                nid,
                map_id,
                converted_entries,
                key_index,
            ) = frame
            v_node = results.pop()
            k_node = results.pop()
            if not key_index.check_and_insert(k_node):
                raise _not_source_error("duplicate_key", {}, "duplicate mapping key")
            converted_entries.append((k_node, v_node))
            work.append(
                (
                    "map_iter",
                    entries_raw,
                    next_idx,
                    next_depth,
                    in_key,
                    nid,
                    map_id,
                    converted_entries,
                    key_index,
                )
            )
            continue
        if op == "dict_iter":
            (
                _,
                dict_it,
                next_depth,
                in_key,
                map_id,
                converted_entries,
                key_index,
            ) = frame
            pair = next(dict_it, None)
            if pair is None:
                active_ids.discard(map_id)
                results.append(
                    Node(kind="mapping", value=Mapping._from_trusted_entries(converted_entries))
                )
            else:
                raw_k, raw_v = pair
                work.append(
                    (
                        "dict_entry_done",
                        dict_it,
                        next_depth,
                        in_key,
                        map_id,
                        converted_entries,
                        key_index,
                    )
                )
                work.append(("eval", raw_v, next_depth, in_key, False))
                work.append(("eval", raw_k, next_depth, True, True))
            continue
        if op == "dict_entry_done":
            (
                _,
                dict_it,
                next_depth,
                in_key,
                map_id,
                converted_entries,
                key_index,
            ) = frame
            v_node = results.pop()
            k_node = results.pop()
            if not key_index.check_and_insert(k_node):
                raise _not_source_error("duplicate_key", {}, "duplicate mapping key")
            converted_entries.append((k_node, v_node))
            work.append(
                (
                    "dict_iter",
                    dict_it,
                    next_depth,
                    in_key,
                    map_id,
                    converted_entries,
                    key_index,
                )
            )
            continue

        # op == "eval"
        _, raw, depth, in_key, is_key_root = frame
        if isinstance(raw, Tagged):
            raw = Node(kind="tagged", value=raw)

        state.nodes_visited += 1
        if state.nodes_visited > limits.max_nodes:
            raise _not_source_error(
                "limit_exceeded",
                {"limit": "max_nodes", "configured": limits.max_nodes},
                f"node count exceeds max_nodes ({limits.max_nodes})",
            )

        if is_key_root:
            _check_emit_key_weight(raw, limits)

        if isinstance(raw, Node):
            nid = id(raw)
            if nid in active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"cyclic node graph exceeds max_depth ({limits.max_depth})",
                )
            kind = raw.kind
            val = raw.value
            if kind == "null":
                results.append(Node.null())
                continue
            if kind == "bool":
                if type(val) is not bool:
                    raise _not_source_error("type", {"expected": "bool", "found": type(val).__name__}, "expected bool")
                results.append(Node.boolean(val))
                continue
            if kind == "int":
                if type(val) is not int:
                    raise _not_source_error("type", {"expected": "int", "found": type(val).__name__}, "expected int")
                if val < I128_MIN or val > U128_MAX:
                    raise _not_source_error("integer_out_of_range", {}, "integer outside 128-bit range")
                results.append(Node.integer(val))
                continue
            if kind == "float":
                if type(val) not in (float, int) or type(val) is bool:
                    raise _not_source_error("type", {"expected": "float", "found": type(val).__name__}, "expected float")
                fval = float(val)
                bits_hex = raw.bits if raw.bits is not None else _float_to_bits_hex(fval)
                if not isinstance(bits_hex, str) or not _HEX16_LOWER_RE.match(bits_hex):
                    raise _not_source_error("internal", {}, "invalid float bits on Node")
                bits_int = int(bits_hex, 16)
                is_nan_bits = (bits_int & 0x7FF0000000000000) == 0x7FF0000000000000 and (
                    bits_int & 0x000FFFFFFFFFFFFF
                ) != 0
                if math.isnan(fval) or is_nan_bits:
                    if in_key:
                        raise _not_source_error("invalid_key", {}, "NaN float is not valid inside a mapping key")
                    results.append(Node.floating(float("nan"), bits=CANONICAL_NAN_BITS))
                else:
                    results.append(Node.floating(_bits_hex_to_float(bits_hex), bits=bits_hex))
                continue
            if kind == "string":
                s_bytes = _validate_utf8_str(val)
                if len(s_bytes) > limits.max_scalar_bytes:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_scalar_bytes", "configured": limits.max_scalar_bytes},
                        f"scalar byte length exceeds max_scalar_bytes ({limits.max_scalar_bytes})",
                    )
                state.min_output_bytes += len(s_bytes)
                results.append(Node.string(val))
                continue
            if kind == "tagged":
                if not isinstance(val, Tagged) or not isinstance(val.tag, Tag):
                    raise _invalid_options_error("Tagged node must hold a Tagged value with Tag")
                uri_bytes = _validate_utf8_str(val.tag.uri)
                _validate_utf8_str(val.tag.handle)
                _validate_utf8_str(val.tag.suffix)
                if len(uri_bytes) > limits.max_tag_bytes or not _is_valid_non_core_tag_uri(val.tag.uri):
                    raise _invalid_options_error("Tagged node contains invalid or oversized Tag.uri")
                inner_raw = val.value
                inner_kind = inner_raw.kind if isinstance(inner_raw, Node) else None
                if val.tag.uri == "tag:yaml.org,2002:merge":
                    if in_key or inner_kind != "string":
                        raise _not_source_error(
                            "tag_kind_mismatch",
                            {},
                            "tag:yaml.org,2002:merge may only wrap a non-key String node",
                        )
                elif inner_kind not in ("string", "sequence", "mapping"):
                    raise _not_source_error(
                        "tag_kind_mismatch",
                        {},
                        "Tagged node must wrap String, Sequence, or Mapping",
                    )
                state.min_output_bytes += _rendered_tag_min_bytes(uri_bytes, val.tag.uri)
                active_ids.add(nid)
                clean_tag = Tag(uri=val.tag.uri, handle="", suffix=val.tag.uri)
                work.append(("finish_tagged", clean_tag, nid))
                work.append(("eval", inner_raw, depth, in_key, False))
                continue
            if kind == "sequence":
                next_depth = depth + 1
                if next_depth > limits.max_depth:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_depth", "configured": limits.max_depth},
                        f"collection nesting exceeds max_depth ({limits.max_depth})",
                    )
                seq_id = id(val)
                if seq_id in active_ids:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_depth", "configured": limits.max_depth},
                        f"cyclic sequence exceeds max_depth ({limits.max_depth})",
                    )
                active_ids.add(nid)
                active_ids.add(seq_id)
                work.append(("seq_step", val, 0, next_depth, in_key, nid, seq_id, []))
                continue
            if kind == "mapping":
                next_depth = depth + 1
                if next_depth > limits.max_depth:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_depth", "configured": limits.max_depth},
                        f"collection nesting exceeds max_depth ({limits.max_depth})",
                    )
                entries_raw = val.entries if isinstance(val, Mapping) else val
                if len(entries_raw) > limits.max_mapping_entries:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_mapping_entries", "configured": limits.max_mapping_entries},
                        f"mapping entry count exceeds max_mapping_entries ({limits.max_mapping_entries})",
                    )
                map_id = id(val)
                if map_id in active_ids:
                    raise _not_source_error(
                        "limit_exceeded",
                        {"limit": "max_depth", "configured": limits.max_depth},
                        f"cyclic mapping exceeds max_depth ({limits.max_depth})",
                    )
                active_ids.add(nid)
                active_ids.add(map_id)
                work.append(
                    ("map_iter", entries_raw, 0, next_depth, in_key, nid, map_id, [], _KeyIndex())
                )
                continue
            raise _not_source_error("internal", {}, f"unknown Node kind: {kind}")

        if raw is None:
            results.append(Node.null())
            continue
        if type(raw) is bool:
            results.append(Node.boolean(raw))
            continue
        if type(raw) is int:
            if raw < I128_MIN or raw > U128_MAX:
                raise _not_source_error("integer_out_of_range", {}, "integer outside 128-bit range")
            results.append(Node.integer(raw))
            continue
        if type(raw) is float:
            if math.isnan(raw):
                if in_key:
                    raise _not_source_error("invalid_key", {}, "NaN float is not valid inside a mapping key")
                results.append(Node.floating(float("nan"), bits=CANONICAL_NAN_BITS))
            else:
                results.append(Node.floating(raw))
            continue
        if type(raw) is str:
            s_bytes = _validate_utf8_str(raw)
            if len(s_bytes) > limits.max_scalar_bytes:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_scalar_bytes", "configured": limits.max_scalar_bytes},
                    f"scalar byte length exceeds max_scalar_bytes ({limits.max_scalar_bytes})",
                )
            state.min_output_bytes += len(s_bytes)
            results.append(Node.string(raw))
            continue
        if isinstance(raw, (list, tuple)):
            next_depth = depth + 1
            if next_depth > limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"collection nesting exceeds max_depth ({limits.max_depth})",
                )
            seq_id = id(raw)
            if seq_id in active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"cyclic sequence exceeds max_depth ({limits.max_depth})",
                )
            active_ids.add(seq_id)
            work.append(("seq_step", raw, 0, next_depth, in_key, None, seq_id, []))
            continue
        if isinstance(raw, Mapping):
            next_depth = depth + 1
            if next_depth > limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"collection nesting exceeds max_depth ({limits.max_depth})",
                )
            entries_raw = raw.entries
            if len(entries_raw) > limits.max_mapping_entries:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_mapping_entries", "configured": limits.max_mapping_entries},
                    f"mapping entry count exceeds max_mapping_entries ({limits.max_mapping_entries})",
                )
            map_id = id(raw)
            if map_id in active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"cyclic mapping exceeds max_depth ({limits.max_depth})",
                )
            active_ids.add(map_id)
            work.append(
                ("map_iter", entries_raw, 0, next_depth, in_key, None, map_id, [], _KeyIndex())
            )
            continue
        if isinstance(raw, dict):
            next_depth = depth + 1
            if next_depth > limits.max_depth:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"collection nesting exceeds max_depth ({limits.max_depth})",
                )
            if len(raw) > limits.max_mapping_entries:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_mapping_entries", "configured": limits.max_mapping_entries},
                    f"mapping entry count exceeds max_mapping_entries ({limits.max_mapping_entries})",
                )
            map_id = id(raw)
            if map_id in active_ids:
                raise _not_source_error(
                    "limit_exceeded",
                    {"limit": "max_depth", "configured": limits.max_depth},
                    f"cyclic mapping exceeds max_depth ({limits.max_depth})",
                )
            active_ids.add(map_id)
            work.append(
                ("dict_iter", iter(raw.items()), next_depth, in_key, map_id, [], _KeyIndex())
            )
            continue

        raise _not_source_error(
            "type",
            {"expected": "YAML-serializable value", "found": type(raw).__name__},
            f"unsupported type for YAML emission: {type(raw).__name__}",
        )

    return results.pop()


def _merge_emit_options(
    options: EmitOptions | TypingMapping[str, Any] | None = None,
    **kwargs: Any,
) -> EmitOptions:
    if kwargs:
        if options is None:
            options = dict(kwargs)
        elif isinstance(options, TypingMapping):
            merged = dict(options)
            merged.update(kwargs)
            options = merged
        else:
            raise _invalid_options_error("cannot combine EmitOptions instance with keyword arguments")
    return validate_emit_options(options)


def from_plain(
    value: Any,
    options: EmitOptions | TypingMapping[str, Any] | None = None,
    **kwargs: Any,
) -> Node:
    """Convert a plain Python value (or validate/clone a `Node` / `Tagged`) into a synthetic `Node` (`spec-v1.5 §11.3`)."""
    opts = _merge_emit_options(options, **kwargs)
    state = _EmitPreflightState(opts=opts)
    return _convert_and_preflight_node(value, opts, state)


def _encode_emit_wire(docs: Sequence[Document], opts: EmitOptions) -> str:
    lim = opts.limits
    wire_cap = min(
        MAX_SAFE_METADATA_INT,
        65_536 + lim.max_nodes * 512 + lim.max_output_bytes * 18,
    )
    owned_cap = min(
        MAX_SAFE_METADATA_INT,
        65_536 + lim.max_nodes * 128 + lim.max_output_bytes * 4,
    )
    enc_opts = Options(
        merge_keys="reject",
        unknown_tags="preserve",
        limits=Limits(
            max_input_bytes=wire_cap,
            max_depth=lim.max_depth,
            max_nodes=lim.max_nodes,
            max_expanded_nodes=lim.max_nodes,
            max_owned_bytes=max(owned_cap, lim.max_scalar_bytes, lim.max_key_bytes),
            max_scalar_bytes=lim.max_scalar_bytes,
            max_key_bytes=lim.max_key_bytes,
            max_mapping_entries=lim.max_mapping_entries,
            max_documents=lim.max_documents,
            max_output_bytes=wire_cap,
        ),
    )
    w = _BoundedWriter(wire_cap)
    st = _EncodeState(opts=enc_opts, nodes_remaining=lim.max_nodes)
    w.push_raw('{"format":"yamlprims-value","version":1,"documents":[')
    for i, doc in enumerate(docs):
        if i > 0:
            w.push_raw(",")
        w.push_raw('{"yaml_version":')
        if doc.yaml_version is not None:
            maj, minor = doc.yaml_version
            w.push_raw(f"[{maj},{minor}]")
        else:
            w.push_raw("null")
        w.push_raw(
            ',"explicit_start":true,"root":'
            if doc.explicit_start
            else ',"explicit_start":false,"root":'
        )
        _write_node(w, doc.root, False, st, 0)
        w.push_raw("}")
    w.push_raw("]}")
    return w.finish()


def dump_all(
    documents: Sequence[Any],
    options: EmitOptions | TypingMapping[str, Any] | None = None,
    **kwargs: Any,
) -> str:
    """Emit a sequence of `Document`, `Node`, or plain Python values as a YAML 1.2.2 Core stream (`spec-v1.5 §10.2`, `§11.5`)."""
    opts = _merge_emit_options(options, **kwargs)
    if not isinstance(documents, (list, tuple)):
        if isinstance(documents, (str, bytes, bytearray, memoryview, dict, Mapping, Node, Document)):
            raise _not_source_error(
                "type",
                {"expected": "sequence of documents", "found": type(documents).__name__},
                "dump_all expects a sequence of documents",
            )
        try:
            doc_iter = iter(documents)
        except TypeError:
            raise _not_source_error(
                "type",
                {"expected": "sequence of documents", "found": type(documents).__name__},
                "dump_all expects a sequence of documents",
            ) from None
        documents = list(itertools.islice(doc_iter, opts.limits.max_documents + 1))
    if len(documents) > opts.limits.max_documents:
        raise _not_source_error(
            "limit_exceeded",
            {"limit": "max_documents", "configured": opts.limits.max_documents},
            f"document count exceeds max_documents ({opts.limits.max_documents})",
        )
    if len(documents) == 0:
        return ""

    state = _EmitPreflightState(opts=opts)
    clean_docs: list[Document] = []
    for item in documents:
        if isinstance(item, Document):
            if item.yaml_version is not None:
                if (
                    not isinstance(item.yaml_version, tuple)
                    or len(item.yaml_version) != 2
                    or type(item.yaml_version[0]) is not int
                    or type(item.yaml_version[1]) is not int
                    or item.yaml_version[0] != 1
                    or item.yaml_version[1] < 0
                    or item.yaml_version[1] > 999_999_999
                ):
                    raise _not_source_error(
                        "unsupported_yaml_version",
                        {},
                        f"unsupported YAML directive version: {item.yaml_version!r}",
                    )
            if type(item.explicit_start) is not bool:
                raise _invalid_options_error("Document.explicit_start must be a bool")
            root = _convert_and_preflight_node(item.root, opts, state)
            clean_docs.append(
                Document(
                    yaml_version=item.yaml_version,
                    explicit_start=item.explicit_start,
                    root=root,
                )
            )
        else:
            root = _convert_and_preflight_node(item, opts, state)
            clean_docs.append(Document(yaml_version=None, explicit_start=False, root=root))

    if state.min_output_bytes > opts.limits.max_output_bytes:
        raise _not_source_error(
            "output_limit_exceeded",
            {"configured": opts.limits.max_output_bytes},
            f"encoded output exceeds max_output_bytes ({opts.limits.max_output_bytes})",
        )

    wire_json = _encode_emit_wire(clean_docs, opts)
    status, payload = _native.emit_wire(
        wire_json,
        opts.compat_quoting,
        opts.limits._to_native_list(),
    )
    if status == 0:
        return payload
    err_opts = Options(
        limits=Limits(max_output_bytes=opts.limits.max_output_bytes),
    )
    env = _handle_native_status(status, payload, err_opts)
    if env.kind == "error" and env.error is not None:
        raise env.error
    raise _not_source_error("internal", {}, "expected error envelope from emit_wire")


def dump(
    value: Any,
    options: EmitOptions | TypingMapping[str, Any] | None = None,
    **kwargs: Any,
) -> str:
    """Emit a single `Document`, `Node`, or plain Python value as a YAML 1.2.2 Core document (`spec-v1.5 §10.2`, `§11.5`)."""
    return dump_all([value], options, **kwargs)
