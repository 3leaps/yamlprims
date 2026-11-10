package yaml

import (
	"fmt"
	"math"
	"strconv"
	"strings"
	"unicode/utf8"

	"github.com/3leaps/yamlprims/yaml/internal/engine"
)

// DefaultIJSONMaxOutputBytes is the 16 MiB default output cap for convenience
// I-JSON entrypoints (EncodeIJSON, LoadIJSON) per spec-v1.5 §11.6.
const DefaultIJSONMaxOutputBytes = 16 * 1024 * 1024

// DefaultIJSONOptions returns DefaultOptions() with UnknownTagsReject and
// Limits.MaxOutputBytes set to DefaultIJSONMaxOutputBytes (16 MiB) per spec-v1.5 §11.6.
func DefaultIJSONOptions() Options {
	opts := DefaultOptions()
	opts.UnknownTags = UnknownTagsReject
	opts.Limits.MaxOutputBytes = DefaultIJSONMaxOutputBytes
	return opts
}

const maxSafeIJSONMag uint64 = 9_007_199_254_740_991

// isUnicodeNoncharacter reports whether r is one of the 66 Unicode noncharacters
// prohibited by RFC 7493 §2.1: U+FDD0..=U+FDEF and U+..FFFE / U+..FFFF on planes 0..=16.
func isUnicodeNoncharacter(r rune) bool {
	cp := uint32(r)
	return (cp >= 0xFDD0 && cp <= 0xFDEF) || ((cp&0xFFFE) == 0xFFFE && cp <= 0x10FFFF)
}

// decomposeShortestFloat decomposes a finite positive float64 into its shortest
// round-trip decimal digits (without trailing zeros) and decimal exponent k such
// that abs = 0.<digits> * 10^k (sciExp = k - 1).
func decomposeShortestFloat(abs float64) (string, int) {
	sci := strconv.FormatFloat(abs, 'e', -1, 64)
	eIdx := strings.IndexByte(sci, 'e')
	mant := sci[:eIdx]
	expStr := sci[eIdx+1:]
	sciExp, _ := strconv.Atoi(expStr)
	var b strings.Builder
	b.Grow(len(mant))
	for i := 0; i < len(mant); i++ {
		if mant[i] != '.' {
			b.WriteByte(mant[i])
		}
	}
	digits := b.String()
	for len(digits) > 1 && digits[len(digits)-1] == '0' {
		digits = digits[:len(digits)-1]
	}
	return digits, sciExp + 1
}

// formatIJSONFloat formats a finite float64 per RFC 8785 §3.2.2.3 (ECMAScript Number::toString).
// -0.0 formats as "0".
func formatIJSONFloat(f float64) string {
	if f == 0 {
		return "0"
	}
	var out strings.Builder
	out.Grow(24)
	if math.Signbit(f) {
		out.WriteByte('-')
	}
	digits, k := decomposeShortestFloat(math.Abs(f))
	n := len(digits)
	if k > 0 && k <= 21 {
		if n <= k {
			out.WriteString(digits)
			for i := 0; i < k-n; i++ {
				out.WriteByte('0')
			}
		} else {
			out.WriteString(digits[:k])
			out.WriteByte('.')
			out.WriteString(digits[k:])
		}
	} else if k >= -5 && k <= 0 {
		out.WriteString("0.")
		for i := 0; i < -k; i++ {
			out.WriteByte('0')
		}
		out.WriteString(digits)
	} else {
		out.WriteByte(digits[0])
		if len(digits) > 1 {
			out.WriteByte('.')
			out.WriteString(digits[1:])
		}
		sciExp := k - 1
		if sciExp >= 0 {
			fmt.Fprintf(&out, "e+%d", sciExp)
		} else {
			fmt.Fprintf(&out, "e%d", sciExp)
		}
	}
	return out.String()
}

func ijsonNodeSite(n *Node) Site {
	if s, ok := n.Span(); ok {
		return SiteSource(s.Start)
	}
	return SiteNotSource(nil)
}

func withIJSONNodeOrigin(n *Node, err *Error) *Error {
	if n.via != nil {
		switch n.via.Kind {
		case ViaKindAlias:
			err.Related = append(err.Related, Related{Role: RelatedRoleAliasUse, Span: n.via.Span})
		case ViaKindMerge:
			err.Related = append(err.Related, Related{Role: RelatedRoleMergeDirective, Span: n.via.Span})
		}
	}
	return err
}

func ijsonNodeSimpleErr(n *Node, kind ErrorKind, msg string) *Error {
	return withIJSONNodeOrigin(n, &Error{
		Kind:    kind,
		Site:    ijsonNodeSite(n),
		Message: boundedString(msg, MaxMessageBytes),
	})
}

func ijsonNodeLimitErr(n *Node, limit LimitKind, configured int, msg string) *Error {
	return withIJSONNodeOrigin(n, &Error{
		Kind: ErrorKindLimitExceeded,
		Detail: ErrorDetail{
			Limit:         limit,
			Configured:    uint64(configured),
			HasConfigured: true,
		},
		Site:    ijsonNodeSite(n),
		Message: boundedString(msg, MaxMessageBytes),
	})
}

func ijsonNodeTypeErr(n *Node, expected, found, msg string) *Error {
	return withIJSONNodeOrigin(n, errTypeAtSite(ijsonNodeSite(n), expected, found, msg))
}

func ijsonNodeInvalidUtf8Err(n *Node) *Error {
	return withIJSONNodeOrigin(n, &Error{
		Kind:    ErrorKindInvalidUtf8,
		Detail:  ErrorDetail{HasInvalidUtf8: true},
		Site:    ijsonNodeSite(n),
		Message: "string is not valid UTF-8",
	})
}

type ijsonFrameKind uint8

const (
	ijsonFrameNode ijsonFrameKind = iota
	ijsonFrameLiteral
	ijsonFrameSeq
	ijsonFrameMap
)

type ijsonFrame struct {
	kind    ijsonFrameKind
	node    *Node
	depth   int
	isKey   bool
	literal string
	items   []Node
	entries []MappingEntry
	idx     int
}

// EncodeIJSON projects node to compact UTF-8 RFC 7493 I-JSON text with DefaultIJSONOptions()
// (including the 16 MiB default MaxOutputBytes ceiling; spec-v1.5 §11.6).
func EncodeIJSON(node Node) (string, *Error) {
	return EncodeIJSONWith(node, DefaultIJSONOptions())
}

// EncodeIJSONWith projects node to compact UTF-8 RFC 7493 I-JSON text governed by opts
// (spec-v1.5 §11.6).
func EncodeIJSONWith(node Node, opts Options) (string, *Error) {
	if err := opts.Validate(); err != nil {
		return "", err
	}
	out := newOutBuf(opts.Limits.MaxOutputBytes)
	nodesSeen := 0
	stack := []ijsonFrame{{
		kind:  ijsonFrameNode,
		node:  &node,
		depth: 0,
		isKey: false,
	}}

	for len(stack) > 0 {
		f := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		switch f.kind {
		case ijsonFrameLiteral:
			if err := out.push(f.literal); err != nil {
				return "", err
			}

		case ijsonFrameSeq:
			if f.idx >= len(f.items) {
				if err := out.push("]"); err != nil {
					return "", err
				}
			} else {
				if f.idx > 0 {
					if err := out.push(","); err != nil {
						return "", err
					}
				}
				stack = append(stack, ijsonFrame{
					kind:  ijsonFrameSeq,
					items: f.items,
					idx:   f.idx + 1,
					depth: f.depth,
				})
				stack = append(stack, ijsonFrame{
					kind:  ijsonFrameNode,
					node:  &f.items[f.idx],
					depth: f.depth,
					isKey: false,
				})
			}

		case ijsonFrameMap:
			if f.idx >= len(f.entries) {
				if err := out.push("}"); err != nil {
					return "", err
				}
			} else {
				if f.idx > 0 {
					if err := out.push(","); err != nil {
						return "", err
					}
				}
				entry := &f.entries[f.idx]
				stack = append(stack, ijsonFrame{
					kind:    ijsonFrameMap,
					entries: f.entries,
					idx:     f.idx + 1,
					depth:   f.depth,
				})
				stack = append(stack, ijsonFrame{
					kind:  ijsonFrameNode,
					node:  &entry.Value,
					depth: f.depth,
					isKey: false,
				})
				stack = append(stack, ijsonFrame{
					kind:    ijsonFrameLiteral,
					literal: ":",
				})
				stack = append(stack, ijsonFrame{
					kind:  ijsonFrameNode,
					node:  &entry.Key,
					depth: f.depth,
					isKey: true,
				})
			}

		case ijsonFrameNode:
			n := f.node
			nodesSeen++
			if nodesSeen > opts.Limits.MaxNodes {
				return "", ijsonNodeLimitErr(n, LimitMaxNodes, opts.Limits.MaxNodes, "I-JSON node count exceeds max_nodes")
			}
			if n.value.kind == ValueKindTagged {
				site := ijsonNodeSite(n)
				if n.tagStart != nil {
					site = SiteSource(*n.tagStart)
				}
				return "", withIJSONNodeOrigin(n, &Error{
					Kind:    ErrorKindUnknownTag,
					Site:    site,
					Message: "tagged values are rejected in RFC 7493 I-JSON projection",
				})
			}
			if f.isKey {
				if n.value.kind != ValueKindString {
					return "", ijsonNodeTypeErr(
						n,
						"I-JSON string key",
						"non_string_key",
						"mapping key must be a String in RFC 7493 I-JSON projection",
					)
				}
				if !utf8.ValidString(n.value.s) {
					return "", ijsonNodeInvalidUtf8Err(n)
				}
				keyWeight := NodeOverhead + uint64(len(n.value.s))
				if keyWeight > uint64(opts.Limits.MaxKeyBytes) {
					return "", ijsonNodeLimitErr(n, LimitMaxKeyBytes, opts.Limits.MaxKeyBytes, "mapping key exceeds max_key_bytes")
				}
			}

			switch n.value.kind {
			case ValueKindNull:
				if err := out.push("null"); err != nil {
					return "", err
				}
			case ValueKindBool:
				if n.value.b {
					if err := out.push("true"); err != nil {
						return "", err
					}
				} else {
					if err := out.push("false"); err != nil {
						return "", err
					}
				}
			case ValueKindInt:
				if n.value.i.hi != 0 || n.value.i.lo > maxSafeIJSONMag {
					return "", ijsonNodeSimpleErr(
						n,
						ErrorKindIntegerOutOfRange,
						"integer exceeds RFC 7493 I-JSON safe range ±(2^53 - 1)",
					)
				}
				if err := out.push(n.value.i.String()); err != nil {
					return "", err
				}
			case ValueKindFloat:
				if math.IsNaN(n.value.f) || math.IsInf(n.value.f, 0) {
					return "", ijsonNodeSimpleErr(
						n,
						ErrorKindFloatOutOfRange,
						"non-finite float cannot be represented in RFC 7493 I-JSON",
					)
				}
				if err := out.push(formatIJSONFloat(n.value.f)); err != nil {
					return "", err
				}
			case ValueKindString:
				s := n.value.s
				if !utf8.ValidString(s) {
					return "", ijsonNodeInvalidUtf8Err(n)
				}
				if len(s) > opts.Limits.MaxScalarBytes {
					return "", ijsonNodeLimitErr(n, LimitMaxScalarBytes, opts.Limits.MaxScalarBytes, "string exceeds max_scalar_bytes")
				}
				for _, r := range s {
					if isUnicodeNoncharacter(r) {
						return "", ijsonNodeTypeErr(
							n,
							"RFC 7493 I-JSON string",
							"noncharacter",
							"string contains a Unicode noncharacter prohibited by RFC 7493",
						)
					}
				}
				if err := out.string(s); err != nil {
					return "", err
				}
			case ValueKindSequence:
				if f.depth+1 > opts.Limits.MaxDepth {
					return "", ijsonNodeLimitErr(n, LimitMaxDepth, opts.Limits.MaxDepth, "sequence nesting exceeds max_depth")
				}
				if err := out.push("["); err != nil {
					return "", err
				}
				stack = append(stack, ijsonFrame{
					kind:  ijsonFrameSeq,
					items: n.value.seq,
					idx:   0,
					depth: f.depth + 1,
				})
			case ValueKindMapping:
				if f.depth+1 > opts.Limits.MaxDepth {
					return "", ijsonNodeLimitErr(n, LimitMaxDepth, opts.Limits.MaxDepth, "mapping nesting exceeds max_depth")
				}
				var entries []MappingEntry
				if n.value.m != nil {
					entries = n.value.m.entries
				}
				if len(entries) > opts.Limits.MaxMappingEntries {
					return "", ijsonNodeLimitErr(n, LimitMaxMappingEntries, opts.Limits.MaxMappingEntries, "mapping exceeds max_mapping_entries")
				}
				if err := out.push("{"); err != nil {
					return "", err
				}
				stack = append(stack, ijsonFrame{
					kind:    ijsonFrameMap,
					entries: entries,
					idx:     0,
					depth:   f.depth + 1,
				})
			default:
				return "", errInternal("unknown ValueKind")
			}
		}
	}
	return string(out.buf), nil
}

func prepareIJSONLoadOptions(opts Options) (Options, *Error) {
	if err := opts.Validate(); err != nil {
		return Options{}, err
	}
	if opts.MergeKeys == MergeKeysAsKey {
		return Options{}, newNotSourceError(
			ErrorKindInvalidOptions,
			ErrorDetail{},
			nil,
			"MergeKeys::AsKey is not permitted in RFC 7493 I-JSON projection",
		)
	}
	eff := opts
	eff.UnknownTags = UnknownTagsReject
	return eff, nil
}

// LoadIJSON parses a single non-empty YAML document with DefaultIJSONOptions() (16 MiB output cap)
// and projects it to compact UTF-8 RFC 7493 I-JSON text (spec-v1.5 §11.6).
func LoadIJSON(input string) (string, *Error) {
	return LoadIJSONWith(input, DefaultIJSONOptions())
}

// LoadIJSONWith parses a single non-empty YAML document with opts (UnknownTagsReject enforced)
// and projects it to compact UTF-8 RFC 7493 I-JSON text (spec-v1.5 §11.6).
func LoadIJSONWith(input string, opts Options) (string, *Error) {
	return LoadIJSONBytes([]byte(input), opts)
}

// LoadIJSONBytes parses a single non-empty YAML document from raw UTF-8 bytes with opts
// (UnknownTagsReject enforced) and projects it to compact UTF-8 RFC 7493 I-JSON text.
func LoadIJSONBytes(input []byte, opts Options) (string, *Error) {
	eff, err := prepareIJSONLoadOptions(opts)
	if err != nil {
		return "", err
	}
	eOpts := engine.DefaultOptions()
	eOpts.Limits.MaxInputBytes = eff.Limits.MaxInputBytes
	eOpts.Limits.MaxDepth = eff.Limits.MaxDepth
	s, bom, ee := engine.GateBytes(input, eOpts)
	if ee != nil {
		return "", fromEngineError(ee)
	}
	doc, err := newLoader(s, bom, eff).loadSingleNonEmpty()
	if err != nil {
		return "", err
	}
	return EncodeIJSONWith(doc.root, eff)
}
