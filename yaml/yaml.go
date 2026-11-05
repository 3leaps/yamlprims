package yaml

import (
	"fmt"
	"math"
	"unicode/utf8"

	"github.com/3leaps/yamlprims/yaml/internal/engine"
)

// Load parses and resolves a single YAML 1.2 Core Schema document from input
// using DefaultOptions().
//
// If the stream contains more than one document, Load fails with
// ErrorKindMultipleDocuments. An empty or comment-only stream produces a
// single document whose root is a zero-width Null node at the stream end.
func Load(input string) (Document, *Error) {
	return LoadWith(input, DefaultOptions())
}

// LoadWith parses and resolves a single YAML 1.2 Core Schema document from
// input using opts.
func LoadWith(input string, opts Options) (Document, *Error) {
	return LoadBytes([]byte(input), opts)
}

// LoadBytes parses and resolves a single YAML 1.2 Core Schema document from
// raw bytes using opts.
func LoadBytes(input []byte, opts Options) (Document, *Error) {
	docs, err := loadDocuments(input, opts, true)
	if err != nil {
		return Document{}, err
	}
	return docs[0], nil
}

// LoadAll parses and resolves all YAML 1.2 Core Schema documents in input
// using DefaultOptions().
func LoadAll(input string) ([]Document, *Error) {
	return LoadAllWith(input, DefaultOptions())
}

// LoadAllWith parses and resolves all YAML 1.2 Core Schema documents in input
// using opts.
func LoadAllWith(input string, opts Options) ([]Document, *Error) {
	return LoadAllBytes([]byte(input), opts)
}

// LoadAllBytes parses and resolves all YAML 1.2 Core Schema documents from
// raw bytes using opts.
func LoadAllBytes(input []byte, opts Options) ([]Document, *Error) {
	return loadDocuments(input, opts, false)
}

func loadDocuments(input []byte, opts Options, single bool) ([]Document, *Error) {
	if err := opts.Validate(); err != nil {
		return nil, err
	}
	eOpts := engine.DefaultOptions()
	eOpts.Limits.MaxInputBytes = opts.Limits.MaxInputBytes
	eOpts.Limits.MaxDepth = opts.Limits.MaxDepth
	text, bom, ee := engine.GateBytes(input, eOpts)
	if ee != nil {
		return nil, fromEngineError(ee)
	}
	return newLoader(text, bom, opts).loadAll(single)
}

func resolveOpts(opts []Options) Options {
	if len(opts) > 0 {
		return opts[0]
	}
	return DefaultOptions()
}

type nodeOrigin struct {
	has  bool
	role RelatedRole
	span Span
}

func originOf(n *Node, inherited nodeOrigin) nodeOrigin {
	if n.via != nil {
		switch n.via.Kind {
		case ViaKindAlias:
			return nodeOrigin{has: true, role: RelatedRoleAliasUse, span: n.via.Span}
		case ViaKindMerge:
			return nodeOrigin{has: true, role: RelatedRoleMergeDirective, span: n.via.Span}
		}
	}
	return inherited
}

func nodeSite(n *Node, lastKnown *Location) Site {
	if s, ok := n.Span(); ok {
		return SiteSource(s.Start)
	}
	return SiteNotSource(lastKnown)
}

func withOrigin(err *Error, origin nodeOrigin) *Error {
	if origin.has {
		return err.WithRelated(origin.role, origin.span)
	}
	return err
}

type preflightFrame struct {
	node      *Node
	depth     int
	lastKnown *Location
	origin    nodeOrigin
}

func preflightNode(node *Node, opts Options) *Error {
	if err := opts.Validate(); err != nil {
		return err
	}
	return countNodes(node, opts)
}

// countNodes walks a caller-supplied Node iteratively before any conversion,
// enforcing MaxNodes and MaxDepth. Tagged wrappers consume a node slot without
// adding container depth, matching crates/yamlprims/src/lib.rs:count_nodes.
func countNodes(root *Node, opts Options) *Error {
	budget := opts.Limits.MaxNodes
	maxDepth := opts.Limits.MaxDepth

	limitErr := func(limit LimitKind, site Site, origin nodeOrigin) *Error {
		configured := opts.Limits.MaxNodes
		if limit == LimitMaxDepth {
			configured = opts.Limits.MaxDepth
		}
		err := &Error{
			Kind: ErrorKindLimitExceeded,
			Detail: ErrorDetail{
				Limit:         limit,
				Configured:    uint64(configured),
				HasConfigured: true,
			},
			Site:    site,
			Message: "tree exceeds limits",
		}
		return withOrigin(err, origin)
	}

	stack := []preflightFrame{{
		node:   root,
		depth:  0,
		origin: originOf(root, nodeOrigin{}),
	}}
	for len(stack) > 0 {
		frame := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		n := frame.node
		lastKnown := frame.lastKnown
		origin := frame.origin

		for {
			if s, ok := n.Span(); ok {
				loc := s.Start
				lastKnown = &loc
			}
			origin = originOf(n, origin)
			if budget <= 0 {
				return limitErr(LimitMaxNodes, nodeSite(n, lastKnown), origin)
			}
			budget--
			if n.value.kind == ValueKindTagged && n.value.tagged != nil {
				n = &n.value.tagged.value
				continue
			}
			break
		}

		switch n.value.kind {
		case ValueKindSequence:
			if frame.depth+1 > maxDepth {
				return limitErr(LimitMaxDepth, nodeSite(n, lastKnown), origin)
			}
			for i := len(n.value.seq) - 1; i >= 0; i-- {
				stack = append(stack, preflightFrame{
					node:      &n.value.seq[i],
					depth:     frame.depth + 1,
					lastKnown: lastKnown,
					origin:    origin,
				})
			}
		case ValueKindMapping:
			if frame.depth+1 > maxDepth {
				return limitErr(LimitMaxDepth, nodeSite(n, lastKnown), origin)
			}
			if n.value.m != nil {
				for i := len(n.value.m.entries) - 1; i >= 0; i-- {
					entry := &n.value.m.entries[i]
					stack = append(stack, preflightFrame{
						node:      &entry.Value,
						depth:     frame.depth + 1,
						lastKnown: lastKnown,
						origin:    origin,
					})
					stack = append(stack, preflightFrame{
						node:      &entry.Key,
						depth:     frame.depth + 1,
						lastKnown: lastKnown,
						origin:    origin,
					})
				}
			}
		}
	}
	return nil
}

func foundDesc(v Value) string {
	if v.kind == ValueKindTagged && v.tagged != nil {
		return "tagged " + boundedString(v.tagged.tag.uri, 256)
	}
	return v.KindName()
}

func nodeTypeErr(node *Node, expected string) *Error {
	found := foundDesc(node.value)
	site := nodeSite(node, nil)
	err := errTypeAtSite(site, expected, found, fmt.Sprintf("expected %s, found %s", expected, found))
	return withOrigin(err, originOf(node, nodeOrigin{}))
}

func nodeSimpleErr(node *Node, kind ErrorKind, msg string) *Error {
	site := nodeSite(node, nil)
	err := &Error{
		Kind:    kind,
		Site:    site,
		Message: boundedString(msg, MaxMessageBytes),
	}
	return withOrigin(err, originOf(node, nodeOrigin{}))
}

func nodeOutOfRangeErr(node *Node, target string) *Error {
	return nodeSimpleErr(node, ErrorKindIntegerOutOfRange, fmt.Sprintf("integer out of range for %s", target))
}

// NodeToBool converts a resolved Node to a bool under spec-v1.4 §8.
// Any non-Bool (or Tagged) node fails with ErrorKindType.
func NodeToBool(node Node, opts ...Options) (bool, *Error) {
	o := resolveOpts(opts)
	if err := preflightNode(&node, o); err != nil {
		return false, err
	}
	if node.value.kind != ValueKindBool {
		return false, nodeTypeErr(&node, "bool")
	}
	return node.value.b, nil
}

// NodeToOptionalBool converts a resolved Node to *bool under spec-v1.4 §8.
// Null returns (nil, nil); Bool returns (&b, nil); any other kind (including
// Tagged) fails with ErrorKindType.
func NodeToOptionalBool(node Node, opts ...Options) (*bool, *Error) {
	o := resolveOpts(opts)
	if err := preflightNode(&node, o); err != nil {
		return nil, err
	}
	switch node.value.kind {
	case ValueKindNull:
		return nil, nil
	case ValueKindBool:
		b := node.value.b
		return &b, nil
	default:
		return nil, nodeTypeErr(&node, "bool")
	}
}

// NodeToString converts a resolved Node to a string under spec-v1.4 §8.
// Any non-String (or Tagged) node fails with ErrorKindType; ill-formed UTF-8 fails with ErrorKindInvalidUtf8.
func NodeToString(node Node, opts ...Options) (string, *Error) {
	o := resolveOpts(opts)
	if err := preflightNode(&node, o); err != nil {
		return "", err
	}
	if node.value.kind != ValueKindString {
		return "", nodeTypeErr(&node, "string")
	}
	if !utf8.ValidString(node.value.s) {
		site := nodeSite(&node, nil)
		err := &Error{
			Kind:    ErrorKindInvalidUtf8,
			Detail:  ErrorDetail{HasInvalidUtf8: true},
			Site:    site,
			Message: "string is not valid UTF-8",
		}
		return "", withOrigin(err, originOf(&node, nodeOrigin{}))
	}
	return node.value.s, nil
}

// NodeToUint8 converts a resolved Node to uint8 under spec-v1.4 §8.
func NodeToUint8(node Node, opts ...Options) (uint8, *Error) {
	u, err := nodeToUintRange(node, "u8", math.MaxUint8, resolveOpts(opts))
	return uint8(u), err
}

// NodeToUint16 converts a resolved Node to uint16 under spec-v1.4 §8.
func NodeToUint16(node Node, opts ...Options) (uint16, *Error) {
	u, err := nodeToUintRange(node, "u16", math.MaxUint16, resolveOpts(opts))
	return uint16(u), err
}

// NodeToUint32 converts a resolved Node to uint32 under spec-v1.4 §8.
func NodeToUint32(node Node, opts ...Options) (uint32, *Error) {
	u, err := nodeToUintRange(node, "u32", math.MaxUint32, resolveOpts(opts))
	return uint32(u), err
}

// NodeToUint64 converts a resolved Node to uint64 under spec-v1.4 §8.
func NodeToUint64(node Node, opts ...Options) (uint64, *Error) {
	return nodeToUintRange(node, "u64", math.MaxUint64, resolveOpts(opts))
}

// NodeToUint128 validates that node is an untagged Int in the unsigned 128-bit
// range [0, 2^128 - 1] under spec-v1.4 §8.
// Non-Int or Tagged nodes fail with ErrorKindType; negative integers fail with
// ErrorKindIntegerOutOfRange.
func NodeToUint128(node Node, opts ...Options) (Integer, *Error) {
	o := resolveOpts(opts)
	if err := preflightNode(&node, o); err != nil {
		return Integer{}, err
	}
	if node.value.kind != ValueKindInt {
		return Integer{}, nodeTypeErr(&node, "u128")
	}
	if node.value.i.IsNegative() {
		return Integer{}, nodeOutOfRangeErr(&node, "u128")
	}
	return node.value.i, nil
}

func nodeToUintRange(node Node, target string, max uint64, opts Options) (uint64, *Error) {
	if err := preflightNode(&node, opts); err != nil {
		return 0, err
	}
	if node.value.kind != ValueKindInt {
		return 0, nodeTypeErr(&node, target)
	}
	u, ok := node.value.i.Uint64()
	if !ok || u > max {
		return 0, nodeOutOfRangeErr(&node, target)
	}
	return u, nil
}

// NodeToInt8 converts a resolved Node to int8 under spec-v1.4 §8.
func NodeToInt8(node Node, opts ...Options) (int8, *Error) {
	v, err := nodeToIntRange(node, "i8", math.MinInt8, math.MaxInt8, resolveOpts(opts))
	return int8(v), err
}

// NodeToInt16 converts a resolved Node to int16 under spec-v1.4 §8.
func NodeToInt16(node Node, opts ...Options) (int16, *Error) {
	v, err := nodeToIntRange(node, "i16", math.MinInt16, math.MaxInt16, resolveOpts(opts))
	return int16(v), err
}

// NodeToInt32 converts a resolved Node to int32 under spec-v1.4 §8.
func NodeToInt32(node Node, opts ...Options) (int32, *Error) {
	v, err := nodeToIntRange(node, "i32", math.MinInt32, math.MaxInt32, resolveOpts(opts))
	return int32(v), err
}

// NodeToInt64 converts a resolved Node to int64 under spec-v1.4 §8.
func NodeToInt64(node Node, opts ...Options) (int64, *Error) {
	return nodeToIntRange(node, "i64", math.MinInt64, math.MaxInt64, resolveOpts(opts))
}

// NodeToInt128 validates that node is an untagged Int in the signed 128-bit
// range [-2^127, 2^127 - 1] under spec-v1.4 §8.
// Non-Int or Tagged nodes fail with ErrorKindType; integers >= 2^127 fail with
// ErrorKindIntegerOutOfRange.
func NodeToInt128(node Node, opts ...Options) (Integer, *Error) {
	o := resolveOpts(opts)
	if err := preflightNode(&node, o); err != nil {
		return Integer{}, err
	}
	if node.value.kind != ValueKindInt {
		return Integer{}, nodeTypeErr(&node, "i128")
	}
	if !node.value.i.IsNegative() && node.value.i.hi >= i128MinMagHi {
		return Integer{}, nodeOutOfRangeErr(&node, "i128")
	}
	return node.value.i, nil
}

func nodeToIntRange(node Node, target string, min, max int64, opts Options) (int64, *Error) {
	if err := preflightNode(&node, opts); err != nil {
		return 0, err
	}
	if node.value.kind != ValueKindInt {
		return 0, nodeTypeErr(&node, target)
	}
	v, ok := node.value.i.Int64()
	if !ok || v < min || v > max {
		return 0, nodeOutOfRangeErr(&node, target)
	}
	return v, nil
}

// NodeToFloat32 converts a resolved Node to float32 under spec-v1.4 §8.
// Finite binary64 values that overflow binary32 after IEEE 754 roundTiesToEven
// fail with ErrorKindFloatOutOfRange. Int nodes convert only if exactly
// representable in binary32, else fail with ErrorKindType.
func NodeToFloat32(node Node, opts ...Options) (float32, *Error) {
	o := resolveOpts(opts)
	if err := preflightNode(&node, o); err != nil {
		return 0, err
	}
	switch node.value.kind {
	case ValueKindFloat:
		f := node.value.f
		if math.IsNaN(f) {
			return math.Float32frombits(0x7fc00000), nil
		}
		r := float32(f)
		if math.IsInf(float64(r), 0) && !math.IsInf(f, 0) {
			return 0, nodeSimpleErr(&node, ErrorKindFloatOutOfRange, "float out of range for f32")
		}
		return r, nil
	case ValueKindInt:
		f, ok := node.value.i.Float64Exact()
		if !ok {
			return 0, nodeTypeErr(&node, "f32 (integer not exactly representable)")
		}
		r := float32(f)
		if math.IsInf(float64(r), 0) || math.IsNaN(float64(r)) || float64(r) != f {
			return 0, nodeTypeErr(&node, "f32 (integer not exactly representable)")
		}
		return r, nil
	default:
		return 0, nodeTypeErr(&node, "f32")
	}
}

// NodeToFloat64 converts a resolved Node to float64 under spec-v1.4 §8.
// Int nodes convert only if exactly representable in binary64, else fail with
// ErrorKindType.
func NodeToFloat64(node Node, opts ...Options) (float64, *Error) {
	o := resolveOpts(opts)
	if err := preflightNode(&node, o); err != nil {
		return 0, err
	}
	switch node.value.kind {
	case ValueKindFloat:
		return node.value.f, nil
	case ValueKindInt:
		f, ok := node.value.i.Float64Exact()
		if !ok {
			return 0, nodeTypeErr(&node, "f64 (integer not exactly representable)")
		}
		return f, nil
	default:
		return 0, nodeTypeErr(&node, "f64")
	}
}

// ConvertMappingField looks up a string key in a Mapping node and converts its
// value node with fn, recording PathKey(StringValue(key)) on any conversion error.
func ConvertMappingField[T any](node Node, key string, fn func(Node, ...Options) (T, *Error), opts ...Options) (T, *Error) {
	var zero T
	o := resolveOpts(opts)
	if err := preflightNode(&node, o); err != nil {
		return zero, err
	}
	if node.value.kind != ValueKindMapping {
		return zero, nodeTypeErr(&node, "mapping")
	}
	valPtr, ok := node.value.m.Get(key)
	if !ok {
		site := nodeSite(&node, nil)
		err := &Error{
			Kind: ErrorKindMissingField,
			Detail: ErrorDetail{
				Field:    boundedString(key, MaxFieldBytes),
				HasField: true,
			},
			Site:    site,
			Message: boundedString(fmt.Sprintf("missing field `%s`", key), MaxMessageBytes),
		}
		return zero, withOrigin(err, originOf(&node, nodeOrigin{}))
	}
	valNode := *valPtr
	if valNode.via == nil && node.via != nil {
		viaCopy := *node.via
		valNode.via = &viaCopy
	}
	out, err := fn(valNode, o)
	if err != nil {
		if err.Site.Kind == SiteKindNotSource && err.Site.LastKnown == nil {
			if s, ok := node.Span(); ok {
				loc := s.Start
				err.Site.LastKnown = &loc
			}
		}
		err.Path = append([]PathSegment{PathKey(StringValue(key))}, err.Path...)
		return zero, err
	}
	return out, nil
}
