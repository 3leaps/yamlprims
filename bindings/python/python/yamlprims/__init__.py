# SPDX-License-Identifier: MIT OR Apache-2.0
"""YAML 1.2.2 Core Schema value loader, strict wire decoder and canonical encoder."""

from __future__ import annotations

from dataclasses import dataclass, field
import math
import re
import struct
from typing import Any, Final, Iterator, Mapping as TypingMapping, Sequence, Union

from . import _yamlprims as _native

__all__ = [
    "ABI_VERSION",
    "Document",
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
    "default_limits",
    "default_options",
    "encode",
    "encode_error",
    "encode_plain",
    "key_eq",
    "load",
    "load_all",
    "load_all_bytes",
    "load_bytes",
    "load_plain",
    "to_plain",
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


@dataclass(frozen=True)
class Tag:
    """Retained YAML tag (`uri`, `handle`, `suffix`)."""

    uri: str
    handle: str = ""
    suffix: str = ""


@dataclass(frozen=True)
class Tagged:
    """A tagged YAML node (`tag` + inner `Node`)."""

    tag: Tag
    value: Node


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
            if (
                not isinstance(self.value.tag, Tag)
                or not isinstance(self.value.tag.uri, str)
                or not self.value.tag.uri
            ):
                raise _not_source_error("internal", {}, "Tag.uri must not be empty")
            if not isinstance(self.value.value, Node) or self.value.value.kind == "tagged":
                raise _not_source_error(
                    "internal", {}, "nested Tagged-in-Tagged is invalid in yamlprims-value/1"
                )
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
        if not isinstance(tag, Tag) or not tag.uri:
            raise _not_source_error("internal", {}, "Tag.uri must not be empty")
        if not isinstance(inner, Node) or inner.kind == "tagged":
            raise _not_source_error("internal", {}, "nested Tagged-in-Tagged is invalid in yamlprims-value/1")
        return Node(
            kind="tagged",
            value=Tagged(tag=tag, value=inner),
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


def key_eq(a: Node, b: Node) -> bool:
    """YAML 1.2.2 Core Schema structural key equality (`spec-v1.4 §5.1`).

    - `Bool(True) != Int(1) != Float(1.0)` (distinct kinds never collide).
    - `Float`: `NaN` is never equal to anything (including `NaN`); `-0.0` and `+0.0` are equal.
    - `Mapping`: order-independent over `(key, value)` pairs.
    - `Sequence` / `Tagged`: recursive structural equality; spans/via/tag_start are ignored.
    """
    stack: list[tuple[Node, Node]] = [(a, b)]
    while stack:
        x, y = stack.pop()
        if x.kind != y.kind:
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
            xf = float(x.value)
            yf = float(y.value)
            if math.isnan(xf) or math.isnan(yf):
                return False
            if xf == 0.0 and yf == 0.0:
                continue
            xb = x.bits if x.bits is not None else _float_to_bits_hex(xf)
            yb = y.bits if y.bits is not None else _float_to_bits_hex(yf)
            if xb != yb:
                return False
        elif kind == "string":
            if x.value != y.value:
                return False
        elif kind == "tagged":
            xt: Tagged = x.value
            yt: Tagged = y.value
            if xt.tag.uri != yt.tag.uri:
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
            used = [False] * len(ym.entries)
            for xk, xv in xm.entries:
                matched = False
                for j, (yk, yv) in enumerate(ym.entries):
                    if used[j]:
                        continue
                    if key_eq(xk, yk):
                        if not key_eq(xv, yv):
                            return False
                        used[j] = True
                        matched = True
                        break
                if not matched:
                    return False
        else:
            return False
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
            for ek, _ in validated:
                if key_eq(ek, k):
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
        return self._entries == other._entries

    def __repr__(self) -> str:
        return f"Mapping({list(self._entries)!r})"


@dataclass(frozen=True)
class Document:
    """A loaded YAML 1.2.2 document."""

    root: Node
    yaml_version: tuple[int, int] | None = None
    explicit_start: bool = False


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


@dataclass
class _DecodeCtx:
    opts: Options
    nodes_remaining: int
    owned_bytes_used: int = 0
    in_key: bool = False
    forbid_locations: bool = False


def _charge_node(ctx: _DecodeCtx, extra_bytes: int) -> None:
    if ctx.nodes_remaining <= 0:
        raise _not_source_error("internal", {}, "wire tree exceeds max_nodes")
    ctx.nodes_remaining -= 1
    ctx.owned_bytes_used += NODE_OVERHEAD + extra_bytes
    if ctx.owned_bytes_used > ctx.opts.limits.max_owned_bytes:
        raise _not_source_error("internal", {}, "wire tree exceeds max_owned_bytes")


def _decode_node(val: _JVal, ctx: _DecodeCtx, depth: int) -> Node:
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
            raise _not_source_error("internal", {}, "error PathSegment.key nodes must have null locations")
    span = None if span_raw.t == "null" else _decode_span(span_raw)
    via = _decode_via(via_raw)
    tag_start = None if tag_start_raw.t == "null" else _decode_loc(tag_start_raw)

    if kind == "null":
        _expect_exact_keys(m, ("kind", "span", "via", "tag_start"))
        _charge_node(ctx, 0)
        return Node(kind="null", value=None, span=span, via=via, tag_start=tag_start)

    if kind == "bool":
        _expect_exact_keys(m, ("kind", "value", "span", "via", "tag_start"))
        b = _expect_bool(m.get("value"), "bool.value")
        _charge_node(ctx, 0)
        return Node(kind="bool", value=b, span=span, via=via, tag_start=tag_start)

    if kind == "int":
        _expect_exact_keys(m, ("kind", "value", "span", "via", "tag_start"))
        iv = _parse_canonical_128_int(_expect_str(m.get("value"), "int.value"))
        _charge_node(ctx, 0)
        return Node(kind="int", value=iv, span=span, via=via, tag_start=tag_start)

    if kind == "float":
        _expect_exact_keys(m, ("kind", "bits", "span", "via", "tag_start"))
        bits_hex = _expect_str(m.get("bits"), "float.bits")
        if not _HEX16_LOWER_RE.match(bits_hex):
            raise _not_source_error("internal", {}, f"invalid float bits: {bits_hex}")
        fval = _bits_hex_to_float(bits_hex)
        canonical_bits = CANONICAL_NAN_BITS if math.isnan(fval) else bits_hex
        _charge_node(ctx, 0)
        return Node(kind="float", value=fval, bits=canonical_bits, span=span, via=via, tag_start=tag_start)

    if kind == "string":
        _expect_exact_keys(m, ("kind", "value", "span", "via", "tag_start"))
        sval = _expect_str(m.get("value"), "string.value")
        sbytes = len(sval.encode("utf-8"))
        if sbytes > ctx.opts.limits.max_scalar_bytes:
            raise _not_source_error("internal", {}, "wire string exceeds max_scalar_bytes")
        if ctx.in_key and sbytes > ctx.opts.limits.max_key_bytes:
            raise _not_source_error("internal", {}, "wire key string exceeds max_key_bytes")
        _charge_node(ctx, sbytes)
        return Node(kind="string", value=sval, span=span, via=via, tag_start=tag_start)

    if kind == "sequence":
        _expect_exact_keys(m, ("kind", "items", "span", "via", "tag_start"))
        if depth + 1 > ctx.opts.limits.max_depth:
            raise _not_source_error("internal", {}, "wire sequence depth exceeds max_depth")
        items_raw = m["items"]
        if items_raw.t != "arr":
            raise _not_source_error("internal", {}, "wire sequence.items must be an array")
        _charge_node(ctx, 0)
        items = tuple(_decode_node(it, ctx, depth + 1) for it in items_raw.v)
        return Node(kind="sequence", value=items, span=span, via=via, tag_start=tag_start)

    if kind == "mapping":
        _expect_exact_keys(m, ("kind", "entries", "span", "via", "tag_start"))
        if depth + 1 > ctx.opts.limits.max_depth:
            raise _not_source_error("internal", {}, "wire mapping depth exceeds max_depth")
        entries_raw = m["entries"]
        if entries_raw.t != "arr":
            raise _not_source_error("internal", {}, "wire mapping.entries must be an array")
        if len(entries_raw.v) > ctx.opts.limits.max_mapping_entries:
            raise _not_source_error("internal", {}, "wire mapping exceeds max_mapping_entries")
        _charge_node(ctx, len(entries_raw.v) * ENTRY_OVERHEAD)
        pairs: list[tuple[Node, Node]] = []
        for pair in entries_raw.v:
            if pair.t != "obj":
                raise _not_source_error("internal", {}, "wire mapping entry must be an object")
            _expect_exact_keys(pair.v, ("key", "value"))
            prev_in_key = ctx.in_key
            ctx.in_key = True
            k_node = _decode_node(pair.v["key"], ctx, depth + 1)
            ctx.in_key = prev_in_key
            if _contains_nan(k_node):
                raise _not_source_error("internal", {}, "wire mapping key contains NaN")
            for ek, _ in pairs:
                if key_eq(ek, k_node):
                    raise _not_source_error("internal", {}, "wire mapping contains duplicate key")
            v_node = _decode_node(pair.v["value"], ctx, depth + 1)
            pairs.append((k_node, v_node))
        return Node(
            kind="mapping",
            value=Mapping._from_trusted_entries(pairs),
            span=span,
            via=via,
            tag_start=tag_start,
        )

    if kind == "tagged":
        _expect_exact_keys(m, ("kind", "tag", "value", "span", "via", "tag_start"))
        tag_raw = m["tag"]
        if tag_raw.t != "obj":
            raise _not_source_error("internal", {}, "wire tagged.tag must be an object")
        _expect_exact_keys(tag_raw.v, ("uri", "handle", "suffix"))
        uri = _expect_str(tag_raw.v.get("uri"), "tag.uri")
        handle = _expect_str(tag_raw.v.get("handle"), "tag.handle")
        suffix = _expect_str(tag_raw.v.get("suffix"), "tag.suffix")
        if not uri:
            raise _not_source_error("internal", {}, "wire tag.uri must not be empty")
        tag_bytes = len(uri.encode("utf-8")) + len(handle.encode("utf-8")) + len(suffix.encode("utf-8"))
        _charge_node(ctx, tag_bytes)
        inner = _decode_node(m["value"], ctx, depth)
        if inner.kind == "tagged":
            raise _not_source_error("internal", {}, "nested Tagged-in-Tagged is invalid in yamlprims-value/1")
        return Node(
            kind="tagged",
            value=Tagged(tag=Tag(uri=uri, handle=handle, suffix=suffix), value=inner),
            span=span,
            via=via,
            tag_start=tag_start,
        )

    raise _not_source_error("internal", {}, f"unknown wire Node kind: {kind}")


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
        detail = {"configured": _expect_nonneg_int(dm.get("configured"), "detail.configured")}
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
        raise _not_source_error("internal", {}, "error.path length exceeds max_depth")
    path: list[PathSegment] = []
    for seg in path_val.v:
        if seg.t != "obj" or len(seg.v) != 1:
            raise _not_source_error("internal", {}, "invalid error.path segment")
        if "index" in seg.v:
            path.append(PathSegment.from_index(_expect_nonneg_int(seg.v["index"], "path.index")))
        elif "key" in seg.v:
            key_ctx = _DecodeCtx(
                opts=opts,
                nodes_remaining=opts.limits.max_nodes,
                in_key=True,
                forbid_locations=True,
            )
            k_node = _decode_node(seg.v["key"], key_ctx, 0)
            if key_ctx.owned_bytes_used > opts.limits.max_key_bytes:
                raise _not_source_error("internal", {}, "error path key exceeds max_key_bytes")
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
    """Strictly parse and validate a `yamlprims-value/1` wire JSON envelope."""
    if not isinstance(wire_json, str):
        raise TypeError("wire_json must be a str")
    opts = validate_options(options)
    try:
        wire_utf8_len = len(wire_json.encode("utf-8", errors="strict"))
    except UnicodeEncodeError:
        raise _not_source_error("internal", {}, "wire JSON contains unpaired surrogate") from None
    if wire_utf8_len > opts.limits.max_output_bytes:
        raise _not_source_error(
            "output_limit_exceeded",
            {"configured": opts.limits.max_output_bytes},
            f"wire JSON exceeds max_output_bytes ({opts.limits.max_output_bytes})",
        )
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
    if len(docs_val.v) > opts.limits.max_documents:
        raise _not_source_error("internal", {}, "wire documents exceed max_documents")

    ctx = _DecodeCtx(opts=opts, nodes_remaining=opts.limits.max_nodes)
    docs: list[Document] = []
    for dval in docs_val.v:
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
        root_node = _decode_node(dm["root"], ctx, 0)
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


def _write_node(
    w: _BoundedWriter,
    node: Node,
    with_spans: bool,
    st: _EncodeState,
    depth: int,
) -> None:
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

    w.push_raw('{"kind":')
    w.push_string(node.kind)

    if node.kind == "null":
        pass
    elif node.kind == "bool":
        w.push_raw(',"value":true' if node.value else ',"value":false')
    elif node.kind == "int":
        iv = node.value
        if type(iv) is not int or iv < I128_MIN or iv > U128_MAX:
            raise _not_source_error("integer_out_of_range", {}, "integer outside 128-bit range")
        w.push_raw(',"value":')
        w.push_string(str(iv))
    elif node.kind == "float":
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
    elif node.kind == "string":
        w.push_raw(',"value":')
        w.push_string(node.value)
    elif node.kind == "sequence":
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
        for i, item in enumerate(node.value):
            if i > 0:
                w.push_raw(",")
            _write_node(w, item, with_spans, st, depth + 1)
        w.push_raw("]")
        st.active_ids.remove(seq_id)
        st.active_ids.remove(nid)
    elif node.kind == "mapping":
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
        seen_keys: list[Node] = []
        for i, (k, v) in enumerate(m.entries):
            if _contains_nan(k):
                st.active_ids.remove(map_id)
                st.active_ids.remove(nid)
                raise _not_source_error("invalid_key", {}, "mapping key contains NaN")
            for ek in seen_keys:
                if key_eq(ek, k):
                    st.active_ids.remove(map_id)
                    st.active_ids.remove(nid)
                    raise _not_source_error("duplicate_key", {}, "duplicate mapping key")
            seen_keys.append(k)
            if i > 0:
                w.push_raw(",")
            w.push_raw('{"key":')
            _write_node(w, k, with_spans, st, depth + 1)
            w.push_raw(',"value":')
            _write_node(w, v, with_spans, st, depth + 1)
            w.push_raw("}")
        w.push_raw("]")
        st.active_ids.remove(map_id)
        st.active_ids.remove(nid)
    elif node.kind == "tagged":
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
        _write_node(w, t.value, with_spans, st, depth)
        st.active_ids.remove(nid)
    else:
        raise _not_source_error("internal", {}, f"unknown Node kind: {node.kind}")

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

    st = _EncodeState(opts=opts, nodes_remaining=opts.limits.max_nodes)
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
            _write_node(w, p.key, False, st, 0)
            w.push_raw("}")
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

    def walk(n: Node, depth: int) -> Any:
        nonlocal nodes_remaining
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
            return None
        if n.kind == "bool":
            return bool(n.value)
        if n.kind == "int":
            return int(n.value)
        if n.kind == "float":
            return float(n.value)
        if n.kind == "string":
            try:
                n.value.encode("utf-8", errors="strict")
            except UnicodeEncodeError:
                raise _not_source_error("invalid_utf8", {"invalid_len": None}, "invalid UTF-8") from None
            return n.value
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
            out = [walk(it, depth + 1) for it in n.value]
            active_ids.remove(seq_id)
            active_ids.remove(nid)
            return out
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
            out_dict: dict[str, Any] = {}
            for k, v in m.entries:
                if k.kind != "string":
                    active_ids.remove(map_id)
                    active_ids.remove(nid)
                    raise _loss_error(k, "non-string key")
                try:
                    k.value.encode("utf-8", errors="strict")
                except UnicodeEncodeError:
                    active_ids.remove(map_id)
                    active_ids.remove(nid)
                    raise _not_source_error("invalid_utf8", {"invalid_len": None}, "invalid UTF-8") from None
                if k.value in out_dict:
                    active_ids.remove(map_id)
                    active_ids.remove(nid)
                    raise _not_source_error("duplicate_key", {}, "duplicate mapping key")
                out_dict[k.value] = walk(v, depth + 1)
            active_ids.remove(map_id)
            active_ids.remove(nid)
            return out_dict
        if n.kind == "tagged":
            raise _loss_error(n, "tagged value")
        raise _not_source_error("internal", {}, f"unknown Node kind: {n.kind}")

    return walk(root, 0)


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

    def walk(n: Node, depth: int) -> None:
        nonlocal nodes_remaining
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
            return
        if n.kind == "bool":
            w.push_raw("true" if n.value else "false")
            return
        if n.kind == "int":
            w.push_raw(str(int(n.value)))
            return
        if n.kind == "float":
            fval = float(n.value)
            if not math.isfinite(fval):
                raise _loss_error(n, "non-finite float")
            w.push_raw(_format_wire_float(fval))
            return
        if n.kind == "string":
            w.push_string(n.value)
            return
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
            for i, it in enumerate(n.value):
                if i > 0:
                    w.push_raw(",")
                walk(it, depth + 1)
            w.push_raw("]")
            active_ids.remove(seq_id)
            active_ids.remove(nid)
            return
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
            for i, (k, v) in enumerate(m.entries):
                if k.kind != "string":
                    active_ids.remove(map_id)
                    active_ids.remove(nid)
                    raise _loss_error(k, "non-string key")
                if i > 0:
                    w.push_raw(",")
                w.push_string(k.value)
                w.push_raw(":")
                walk(v, depth + 1)
            w.push_raw("}")
            active_ids.remove(map_id)
            active_ids.remove(nid)
            return
        if n.kind == "tagged":
            raise _loss_error(n, "tagged value")
        raise _not_source_error("internal", {}, f"unknown Node kind: {n.kind}")

    walk(root, 0)
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
    return decode_wire(payload, opts)


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
