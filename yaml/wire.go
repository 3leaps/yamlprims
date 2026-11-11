package yaml

import (
	"fmt"
	"math"
	"strconv"
	"strings"
	"unicode/utf8"
)

// MaxWireInt is the maximum unsigned integer allowed in wire metadata fields (2^53 - 1).
const MaxWireInt uint64 = (uint64(1) << 53) - 1

type outBuf struct {
	buf []byte
	max int
}

func newOutBuf(max int) outBuf {
	return outBuf{max: max}
}

func (o *outBuf) pushOver() *Error {
	return errOutputLimit(o.max)
}

func (o *outBuf) push(t string) *Error {
	if len(t) > o.max-len(o.buf) {
		return o.pushOver()
	}
	o.buf = append(o.buf, t...)
	return nil
}

func (o *outBuf) pushUint(n uint64) *Error {
	if n > MaxWireInt {
		return errInternal("wire metadata integer exceeds 2^53 - 1")
	}
	return o.push(strconv.FormatUint(n, 10))
}

func (o *outBuf) pushInt(n int) *Error {
	if n < 0 || uint64(n) > MaxWireInt {
		return errInternal("wire metadata integer out of range")
	}
	return o.push(strconv.Itoa(n))
}

func (o *outBuf) string(v string) *Error {
	if !utf8.ValidString(v) {
		return newNotSourceError(ErrorKindInvalidUtf8, ErrorDetail{HasInvalidUtf8: true}, nil, "string is not valid UTF-8")
	}
	return o.stringParts(v, "")
}

func (o *outBuf) boundedString(v string, max int) *Error {
	if !utf8.ValidString(v) {
		return newNotSourceError(ErrorKindInvalidUtf8, ErrorDetail{HasInvalidUtf8: true}, nil, "string is not valid UTF-8")
	}
	if len(v) <= max {
		return o.stringParts(v, "")
	}
	i := max - len(truncatedMark)
	if i < 0 {
		i = 0
	}
	for i > 0 && !utf8.RuneStart(v[i]) {
		i--
	}
	return o.stringParts(v[:i], truncatedMark)
}

func (o *outBuf) stringParts(a, b string) *Error {
	minNeed := len(a) + len(b) + 2
	if minNeed > o.max-len(o.buf) {
		return o.pushOver()
	}
	if err := o.push("\""); err != nil {
		return err
	}
	parts := [2]string{a, b}
	var hexEsc [6]byte
	hexEsc[0] = '\\'
	hexEsc[1] = 'u'
	hexEsc[2] = '0'
	hexEsc[3] = '0'
	const hexDigits = "0123456789abcdef"
	for _, v := range parts {
		run := 0
		for i := 0; i < len(v); {
			c := v[i]
			if c >= 0x20 && c != '"' && c != '\\' {
				i++
				continue
			}
			if err := o.push(v[run:i]); err != nil {
				return err
			}
			switch c {
			case '"':
				if err := o.push(`\"`); err != nil {
					return err
				}
			case '\\':
				if err := o.push(`\\`); err != nil {
					return err
				}
			case '\b':
				if err := o.push(`\b`); err != nil {
					return err
				}
			case '\f':
				if err := o.push(`\f`); err != nil {
					return err
				}
			case '\n':
				if err := o.push(`\n`); err != nil {
					return err
				}
			case '\r':
				if err := o.push(`\r`); err != nil {
					return err
				}
			case '\t':
				if err := o.push(`\t`); err != nil {
					return err
				}
			default:
				hexEsc[4] = hexDigits[c>>4]
				hexEsc[5] = hexDigits[c&0x0f]
				if err := o.push(string(hexEsc[:])); err != nil {
					return err
				}
			}
			i++
			run = i
		}
		if err := o.push(v[run:]); err != nil {
			return err
		}
	}
	return o.push("\"")
}

func (o *outBuf) loc(l Location) *Error {
	if err := o.push("["); err != nil {
		return err
	}
	if err := o.pushInt(l.ByteOffset); err != nil {
		return err
	}
	if err := o.push(","); err != nil {
		return err
	}
	if err := o.pushInt(l.Line); err != nil {
		return err
	}
	if err := o.push(","); err != nil {
		return err
	}
	if err := o.pushInt(l.Column); err != nil {
		return err
	}
	return o.push("]")
}

func (o *outBuf) span(s Span) *Error {
	if err := o.push("["); err != nil {
		return err
	}
	if err := o.pushInt(s.Start.ByteOffset); err != nil {
		return err
	}
	if err := o.push(","); err != nil {
		return err
	}
	if err := o.pushInt(s.Start.Line); err != nil {
		return err
	}
	if err := o.push(","); err != nil {
		return err
	}
	if err := o.pushInt(s.Start.Column); err != nil {
		return err
	}
	if err := o.push(","); err != nil {
		return err
	}
	if err := o.pushInt(s.End.ByteOffset); err != nil {
		return err
	}
	if err := o.push(","); err != nil {
		return err
	}
	if err := o.pushInt(s.End.Line); err != nil {
		return err
	}
	if err := o.push(","); err != nil {
		return err
	}
	if err := o.pushInt(s.End.Column); err != nil {
		return err
	}
	return o.push("]")
}

func floatBitsHex(f float64) string {
	b := math.Float64bits(f)
	if math.IsNaN(f) {
		b = canonicalQuietNaNBits
	}
	return fmt.Sprintf("%016x", b)
}

const (
	wireWriteOpNode uint8 = iota
	wireWriteOpValue
	wireWriteOpFinishNode
	wireWriteOpSeqStep
	wireWriteOpMapStep
	wireWriteOpAfterKey
	wireWriteOpAfterVal
)

type wireWriteFrame struct {
	op      uint8
	node    *Node
	val     *Value
	seq     []Node
	entries []MappingEntry
	idx     int
}

func writeWireNodeMeta(o *outBuf, n *Node, withSpans bool) *Error {
	if err := o.push(`,"span":`); err != nil {
		return err
	}
	if withSpans && n.span.Kind == NodeSpanSource {
		if err := o.span(n.span.Source); err != nil {
			return err
		}
	} else {
		if err := o.push("null"); err != nil {
			return err
		}
	}
	if err := o.push(`,"via":`); err != nil {
		return err
	}
	if withSpans && n.via != nil {
		switch n.via.Kind {
		case ViaKindAlias:
			if err := o.push(`{"alias":`); err != nil {
				return err
			}
			if err := o.span(n.via.Span); err != nil {
				return err
			}
			if err := o.push("}"); err != nil {
				return err
			}
		case ViaKindMerge:
			if err := o.push(`{"merge":`); err != nil {
				return err
			}
			if err := o.span(n.via.Span); err != nil {
				return err
			}
			if err := o.push("}"); err != nil {
				return err
			}
		default:
			if err := o.push("null"); err != nil {
				return err
			}
		}
	} else {
		if err := o.push("null"); err != nil {
			return err
		}
	}
	if err := o.push(`,"tag_start":`); err != nil {
		return err
	}
	if withSpans && n.tagStart != nil {
		if err := o.loc(*n.tagStart); err != nil {
			return err
		}
	} else {
		if err := o.push("null"); err != nil {
			return err
		}
	}
	return o.push("}")
}

func runWireStack(o *outBuf, stack []wireWriteFrame, withSpans bool) *Error {
	for len(stack) > 0 {
		f := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		switch f.op {
		case wireWriteOpNode:
			stack = append(stack,
				wireWriteFrame{op: wireWriteOpFinishNode, node: f.node},
				wireWriteFrame{op: wireWriteOpValue, val: &f.node.value},
			)
		case wireWriteOpFinishNode:
			if err := writeWireNodeMeta(o, f.node, withSpans); err != nil {
				return err
			}
		case wireWriteOpValue:
			v := f.val
			if err := o.push(`{"kind":`); err != nil {
				return err
			}
			if err := o.string(v.KindName()); err != nil {
				return err
			}
			switch v.kind {
			case ValueKindNull:
			case ValueKindBool:
				if v.b {
					if err := o.push(`,"value":true`); err != nil {
						return err
					}
				} else {
					if err := o.push(`,"value":false`); err != nil {
						return err
					}
				}
			case ValueKindInt:
				if err := o.push(`,"value":`); err != nil {
					return err
				}
				if err := o.string(v.i.String()); err != nil {
					return err
				}
			case ValueKindFloat:
				if err := o.push(`,"bits":`); err != nil {
					return err
				}
				if err := o.string(floatBitsHex(v.f)); err != nil {
					return err
				}
			case ValueKindString:
				if err := o.push(`,"value":`); err != nil {
					return err
				}
				if err := o.string(v.s); err != nil {
					return err
				}
			case ValueKindSequence:
				if err := o.push(`,"items":[`); err != nil {
					return err
				}
				stack = append(stack, wireWriteFrame{op: wireWriteOpSeqStep, seq: v.seq, idx: 0})
			case ValueKindMapping:
				if err := o.push(`,"entries":[`); err != nil {
					return err
				}
				var entries []MappingEntry
				if v.m != nil {
					entries = v.m.entries
				}
				stack = append(stack, wireWriteFrame{op: wireWriteOpMapStep, entries: entries, idx: 0})
			case ValueKindTagged:
				if v.tagged == nil {
					return errInternal("nil Tagged pointer")
				}
				if err := o.push(`,"tag":{"uri":`); err != nil {
					return err
				}
				if err := o.string(v.tagged.tag.uri); err != nil {
					return err
				}
				if err := o.push(`,"handle":`); err != nil {
					return err
				}
				if err := o.string(v.tagged.tag.handle); err != nil {
					return err
				}
				if err := o.push(`,"suffix":`); err != nil {
					return err
				}
				if err := o.string(v.tagged.tag.suffix); err != nil {
					return err
				}
				if err := o.push(`},"value":`); err != nil {
					return err
				}
				stack = append(stack, wireWriteFrame{op: wireWriteOpNode, node: &v.tagged.value})
			default:
				return errInternal("unknown ValueKind")
			}
		case wireWriteOpSeqStep:
			if f.idx >= len(f.seq) {
				if err := o.push("]"); err != nil {
					return err
				}
			} else {
				if f.idx > 0 {
					if err := o.push(","); err != nil {
						return err
					}
				}
				item := &f.seq[f.idx]
				f.idx++
				stack = append(stack, f, wireWriteFrame{op: wireWriteOpNode, node: item})
			}
		case wireWriteOpMapStep:
			if f.idx >= len(f.entries) {
				if err := o.push("]"); err != nil {
					return err
				}
			} else {
				if f.idx > 0 {
					if err := o.push(","); err != nil {
						return err
					}
				}
				if err := o.push(`{"key":`); err != nil {
					return err
				}
				e := &f.entries[f.idx]
				stack = append(stack,
					wireWriteFrame{op: wireWriteOpAfterVal, entries: f.entries, idx: f.idx + 1},
					wireWriteFrame{op: wireWriteOpNode, node: &e.Value},
					wireWriteFrame{op: wireWriteOpAfterKey},
					wireWriteFrame{op: wireWriteOpNode, node: &e.Key},
				)
			}
		case wireWriteOpAfterKey:
			if err := o.push(`,"value":`); err != nil {
				return err
			}
		case wireWriteOpAfterVal:
			if err := o.push("}"); err != nil {
				return err
			}
			stack = append(stack, wireWriteFrame{op: wireWriteOpMapStep, entries: f.entries, idx: f.idx})
		}
	}
	return nil
}

func writeWireNode(o *outBuf, n *Node, withSpans bool) *Error {
	return runWireStack(o, []wireWriteFrame{{op: wireWriteOpNode, node: n}}, withSpans)
}

func writeWireValue(o *outBuf, v *Value, withSpans bool) *Error {
	return runWireStack(o, []wireWriteFrame{{op: wireWriteOpValue, val: v}}, withSpans)
}

// Encode serializes docs into a canonical yamlprims-value/1 documents envelope,
// bounded by opts.Limits.MaxOutputBytes (spec-v1.4 §11.1).
func Encode(docs []Document, opts Options) (string, *Error) {
	if err := opts.Validate(); err != nil {
		return "", err
	}
	o := newOutBuf(opts.Limits.MaxOutputBytes)
	if err := o.push(`{"format":"yamlprims-value","version":1,"documents":[`); err != nil {
		return "", err
	}
	for i := range docs {
		if i > 0 {
			if err := o.push(","); err != nil {
				return "", err
			}
		}
		if err := o.push(`{"yaml_version":`); err != nil {
			return "", err
		}
		if docs[i].yamlVersion != nil {
			if err := o.push("["); err != nil {
				return "", err
			}
			if err := o.pushUint(uint64(docs[i].yamlVersion[0])); err != nil {
				return "", err
			}
			if err := o.push(","); err != nil {
				return "", err
			}
			if err := o.pushUint(uint64(docs[i].yamlVersion[1])); err != nil {
				return "", err
			}
			if err := o.push("]"); err != nil {
				return "", err
			}
		} else {
			if err := o.push("null"); err != nil {
				return "", err
			}
		}
		if docs[i].explicitStart {
			if err := o.push(`,"explicit_start":true,"root":`); err != nil {
				return "", err
			}
		} else {
			if err := o.push(`,"explicit_start":false,"root":`); err != nil {
				return "", err
			}
		}
		if err := writeWireNode(&o, &docs[i].root, true); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	}
	if err := o.push("]}"); err != nil {
		return "", err
	}
	return string(o.buf), nil
}

func validateAndResolveSyntaxCode(d ErrorDetail) (string, bool) {
	if d.InvalidUtf8Len != nil || d.HasInvalidUtf8 || d.Limit != "" || d.HasConfigured || d.Configured != 0 ||
		d.HasType || d.Expected != "" || d.Found != "" || d.HasField || d.Field != "" {
		return "", false
	}
	code := d.SyntaxKind.AsStr()
	if code == "" {
		return "", false
	}
	return code, true
}

func validateErrorPair(e *Error) (string, *Error) {
	if e == nil || !e.Kind.IsValid() {
		return "", errInternal("invalid error kind")
	}
	if !utf8.ValidString(e.Message) {
		return "", newNotSourceError(ErrorKindInvalidUtf8, ErrorDetail{HasInvalidUtf8: true}, nil, "string is not valid UTF-8")
	}
	d := e.Detail
	switch e.Kind {
	case ErrorKindInvalidUtf8:
		if d.SyntaxKind != 0 || d.Limit != "" || d.HasConfigured || d.Configured != 0 ||
			d.HasType || d.Expected != "" || d.Found != "" || d.HasField || d.Field != "" {
			return "", errInternal("stale detail on invalid_utf8 error")
		}
		if d.InvalidUtf8Len != nil && (*d.InvalidUtf8Len < 0 || uint64(*d.InvalidUtf8Len) > MaxWireInt) {
			return "", errInternal("invalid_len out of wire range")
		}
		return "", nil

	case ErrorKindSyntax:
		code, ok := validateAndResolveSyntaxCode(d)
		if !ok {
			return "", errInternal("invalid syntax_kind detail")
		}
		return code, nil

	case ErrorKindLimitExceeded:
		if d.InvalidUtf8Len != nil || d.HasInvalidUtf8 || d.SyntaxKind != 0 ||
			d.HasType || d.Expected != "" || d.Found != "" || d.HasField || d.Field != "" {
			return "", errInternal("stale detail on limit_exceeded error")
		}
		if !d.Limit.IsValid() || d.Configured == 0 || d.Configured > MaxWireInt {
			return "", errInternal("invalid limit_exceeded detail")
		}
		return "", nil

	case ErrorKindType:
		if d.InvalidUtf8Len != nil || d.HasInvalidUtf8 || d.SyntaxKind != 0 ||
			d.Limit != "" || d.HasConfigured || d.Configured != 0 || d.HasField || d.Field != "" {
			return "", errInternal("stale detail on type error")
		}
		if !utf8.ValidString(d.Expected) || !utf8.ValidString(d.Found) {
			return "", newNotSourceError(ErrorKindInvalidUtf8, ErrorDetail{HasInvalidUtf8: true}, nil, "string is not valid UTF-8")
		}
		return "", nil

	case ErrorKindMissingField, ErrorKindUnknownField:
		if d.InvalidUtf8Len != nil || d.HasInvalidUtf8 || d.SyntaxKind != 0 ||
			d.Limit != "" || d.HasConfigured || d.Configured != 0 || d.HasType || d.Expected != "" || d.Found != "" {
			return "", errInternal("stale detail on field error")
		}
		if !utf8.ValidString(d.Field) {
			return "", newNotSourceError(ErrorKindInvalidUtf8, ErrorDetail{HasInvalidUtf8: true}, nil, "string is not valid UTF-8")
		}
		return "", nil

	case ErrorKindOutputLimitExceeded:
		if d.InvalidUtf8Len != nil || d.HasInvalidUtf8 || d.SyntaxKind != 0 ||
			d.Limit != "" || d.HasType || d.Expected != "" || d.Found != "" || d.HasField || d.Field != "" {
			return "", errInternal("stale detail on output_limit_exceeded error")
		}
		if d.Configured == 0 || d.Configured > MaxWireInt {
			return "", errInternal("invalid output_limit_exceeded detail")
		}
		return "", nil

	default:
		if d.InvalidUtf8Len != nil || d.HasInvalidUtf8 || d.SyntaxKind != 0 ||
			d.Limit != "" || d.HasConfigured || d.Configured != 0 ||
			d.HasType || d.Expected != "" || d.Found != "" || d.HasField || d.Field != "" {
			return "", errInternal("unexpected detail on unit-detail error kind")
		}
		return "", nil
	}
}

func notSourceLimit(limit LimitKind, configured int, msg string) *Error {
	return newNotSourceError(
		ErrorKindLimitExceeded,
		ErrorDetail{Limit: limit, Configured: uint64(configured), HasConfigured: true},
		nil,
		msg,
	)
}

func preflightError(e *Error, l Limits) *Error {
	if len(e.Path) > l.MaxDepth {
		return notSourceLimit(
			LimitMaxDepth,
			l.MaxDepth,
			fmt.Sprintf("error path has %d segments, more than max_depth", len(e.Path)),
		)
	}
	type stackItem struct {
		val   *Value
		depth int
	}
	for i := range e.Path {
		seg := &e.Path[i]
		if seg.Kind == PathSegmentIndex {
			if seg.Index < 0 || uint64(seg.Index) > MaxWireInt {
				return errInternal("path index out of wire range")
			}
			continue
		}
		weight := NodeOverhead
		if weight > uint64(l.MaxKeyBytes) {
			return notSourceLimit(LimitMaxKeyBytes, l.MaxKeyBytes, "error path key exceeds max_key_bytes")
		}
		stack := []stackItem{{val: &seg.Key, depth: 0}}
		for len(stack) > 0 {
			top := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			v := top.val
			depth := top.depth

			var more uint64
			var children int
			switch v.kind {
			case ValueKindString:
				if !utf8.ValidString(v.s) {
					return newNotSourceError(ErrorKindInvalidUtf8, ErrorDetail{HasInvalidUtf8: true}, nil, "string is not valid UTF-8")
				}
				more = uint64(len(v.s))
			case ValueKindTagged:
				if v.tagged != nil {
					if !utf8.ValidString(v.tagged.tag.uri) || !utf8.ValidString(v.tagged.tag.handle) || !utf8.ValidString(v.tagged.tag.suffix) {
						return newNotSourceError(ErrorKindInvalidUtf8, ErrorDetail{HasInvalidUtf8: true}, nil, "string is not valid UTF-8")
					}
					more = satAdd64(tagBytes(v.tagged.tag), NodeOverhead)
					children = 1
				}
			case ValueKindSequence:
				more = satMul64(NodeOverhead, uint64(len(v.seq)))
				children = len(v.seq)
			case ValueKindMapping:
				n := v.m.Len()
				more = satMul64(2*NodeOverhead+EntryOverhead, uint64(n))
				children = 2 * n
			}
			weight = satAdd64(weight, more)
			if weight > uint64(l.MaxKeyBytes) {
				return notSourceLimit(LimitMaxKeyBytes, l.MaxKeyBytes, "error path key exceeds max_key_bytes")
			}
			if children > 0 && v.kind != ValueKindTagged && depth+1 > l.MaxDepth {
				return notSourceLimit(LimitMaxDepth, l.MaxDepth, "error path key nests deeper than max_depth")
			}
			switch v.kind {
			case ValueKindTagged:
				if v.tagged != nil {
					stack = append(stack, stackItem{val: &v.tagged.value.value, depth: depth})
				}
			case ValueKindSequence:
				for j := range v.seq {
					stack = append(stack, stackItem{val: &v.seq[j].value, depth: depth + 1})
				}
			case ValueKindMapping:
				if v.m != nil {
					for j := range v.m.entries {
						stack = append(stack, stackItem{val: &v.m.entries[j].Key.value, depth: depth + 1})
						stack = append(stack, stackItem{val: &v.m.entries[j].Value.value, depth: depth + 1})
					}
				}
			}
		}
	}
	return nil
}

// EncodeError serializes err into a canonical yamlprims-value/1 error envelope,
// validating the kind/detail pair and preflighting structured path keys before writing (spec-v1.4 §11.1).
func EncodeError(e *Error, opts Options) (string, *Error) {
	if err := opts.Validate(); err != nil {
		return "", err
	}
	syntaxCode, err := validateErrorPair(e)
	if err != nil {
		return "", err
	}
	for i := range e.Related {
		if !e.Related[i].Role.IsValid() {
			return "", errInternal("invalid related role")
		}
	}
	if err := preflightError(e, opts.Limits); err != nil {
		return "", err
	}

	l := opts.Limits
	o := newOutBuf(l.MaxOutputBytes)
	if err := o.push(`{"format":"yamlprims-value","version":1,"error":{"kind":`); err != nil {
		return "", err
	}
	if err := o.string(e.Kind.Name()); err != nil {
		return "", err
	}
	if err := o.push(`,"detail":`); err != nil {
		return "", err
	}
	switch e.Kind {
	case ErrorKindInvalidUtf8:
		if e.Detail.InvalidUtf8Len != nil {
			if err := o.push(`{"invalid_len":`); err != nil {
				return "", err
			}
			if err := o.pushInt(*e.Detail.InvalidUtf8Len); err != nil {
				return "", err
			}
			if err := o.push("}"); err != nil {
				return "", err
			}
		} else {
			if err := o.push(`{"invalid_len":null}`); err != nil {
				return "", err
			}
		}
	case ErrorKindSyntax:
		if err := o.push(`{"syntax_kind":`); err != nil {
			return "", err
		}
		if err := o.string(syntaxCode); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	case ErrorKindLimitExceeded:
		if err := o.push(`{"limit":`); err != nil {
			return "", err
		}
		if err := o.string(e.Detail.Limit.FieldName()); err != nil {
			return "", err
		}
		if err := o.push(`,"configured":`); err != nil {
			return "", err
		}
		if err := o.pushUint(e.Detail.Configured); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	case ErrorKindType:
		if err := o.push(`{"expected":`); err != nil {
			return "", err
		}
		if err := o.boundedString(e.Detail.Expected, MaxFieldBytes); err != nil {
			return "", err
		}
		if err := o.push(`,"found":`); err != nil {
			return "", err
		}
		if err := o.boundedString(e.Detail.Found, MaxFieldBytes); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	case ErrorKindMissingField, ErrorKindUnknownField:
		if err := o.push(`{"field":`); err != nil {
			return "", err
		}
		if err := o.boundedString(e.Detail.Field, MaxFieldBytes); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	case ErrorKindOutputLimitExceeded:
		if err := o.push(`{"configured":`); err != nil {
			return "", err
		}
		if err := o.pushUint(e.Detail.Configured); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	default:
		if err := o.push("{}"); err != nil {
			return "", err
		}
	}

	if err := o.push(`,"site":`); err != nil {
		return "", err
	}
	if e.Site.Kind == SiteKindSource {
		if err := o.push(`{"source":`); err != nil {
			return "", err
		}
		if err := o.loc(e.Site.Source); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	} else {
		if err := o.push(`{"not_source":{"last_known":`); err != nil {
			return "", err
		}
		if e.Site.LastKnown != nil {
			if err := o.loc(*e.Site.LastKnown); err != nil {
				return "", err
			}
		} else {
			if err := o.push("null"); err != nil {
				return "", err
			}
		}
		if err := o.push("}}"); err != nil {
			return "", err
		}
	}

	if err := o.push(`,"related":[`); err != nil {
		return "", err
	}
	for i := range e.Related {
		if i > 0 {
			if err := o.push(","); err != nil {
				return "", err
			}
		}
		if err := o.push(`{"role":`); err != nil {
			return "", err
		}
		if err := o.string(e.Related[i].Role.Name()); err != nil {
			return "", err
		}
		if err := o.push(`,"span":`); err != nil {
			return "", err
		}
		if err := o.span(e.Related[i].Span); err != nil {
			return "", err
		}
		if err := o.push("}"); err != nil {
			return "", err
		}
	}

	if err := o.push(`],"path":[`); err != nil {
		return "", err
	}
	for i := range e.Path {
		if i > 0 {
			if err := o.push(","); err != nil {
				return "", err
			}
		}
		seg := &e.Path[i]
		if seg.Kind == PathSegmentIndex {
			if err := o.push(`{"index":`); err != nil {
				return "", err
			}
			if err := o.pushInt(seg.Index); err != nil {
				return "", err
			}
			if err := o.push("}"); err != nil {
				return "", err
			}
		} else {
			if err := o.push(`{"key":`); err != nil {
				return "", err
			}
			if err := writeWireValue(&o, &seg.Key, false); err != nil {
				return "", err
			}
			if err := o.push(`,"span":null,"via":null,"tag_start":null}}`); err != nil {
				return "", err
			}
		}
	}

	if err := o.push(`],"message":`); err != nil {
		return "", err
	}
	if err := o.boundedString(e.Message, MaxMessageBytes); err != nil {
		return "", err
	}
	if err := o.push("}}"); err != nil {
		return "", err
	}
	return string(o.buf), nil
}

func plainLossError(n *Node, what string) *Error {
	var loc *Location
	if s, ok := n.Span(); ok {
		l := s.Start
		loc = &l
	}
	return newNotSourceError(
		ErrorKindType,
		ErrorDetail{
			Expected: "plain JSON value",
			Found:    what,
			HasType:  true,
		},
		loc,
		fmt.Sprintf("cannot write %s as plain JSON without loss", what),
	)
}

func formatPlainFloat(f float64) string {
	if f == 0 {
		if math.Signbit(f) {
			return "-0.0"
		}
		return "0.0"
	}
	prefix := ""
	if math.Signbit(f) {
		prefix = "-"
	}
	abs := math.Abs(f)
	s := strconv.FormatFloat(abs, 'e', -1, 64)
	eIdx := strings.IndexByte(s, 'e')
	mant := s[:eIdx]
	exp, _ := strconv.Atoi(s[eIdx+1:])
	digits := strings.ReplaceAll(mant, ".", "")
	if exp >= -4 && exp < 16 {
		if exp >= 0 {
			intDigits := exp + 1
			if len(digits) <= intDigits {
				return prefix + digits + strings.Repeat("0", intDigits-len(digits)) + ".0"
			}
			return prefix + digits[:intDigits] + "." + digits[intDigits:]
		}
		return prefix + "0." + strings.Repeat("0", -exp-1) + digits
	}
	return prefix + mant + "e" + strconv.Itoa(exp)
}

func writePlainNode(o *outBuf, root *Node) *Error {
	const (
		plainOpNode uint8 = iota
		plainOpSeqStep
		plainOpMapStep
	)
	type plainFrame struct {
		op      uint8
		node    *Node
		seq     []Node
		entries []MappingEntry
		idx     int
	}
	stack := []plainFrame{{op: plainOpNode, node: root}}
	for len(stack) > 0 {
		f := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		switch f.op {
		case plainOpNode:
			n := f.node
			switch n.value.kind {
			case ValueKindNull:
				if err := o.push("null"); err != nil {
					return err
				}
			case ValueKindBool:
				if n.value.b {
					if err := o.push("true"); err != nil {
						return err
					}
				} else {
					if err := o.push("false"); err != nil {
						return err
					}
				}
			case ValueKindInt:
				if err := o.push(n.value.i.String()); err != nil {
					return err
				}
			case ValueKindFloat:
				if math.IsNaN(n.value.f) || math.IsInf(n.value.f, 0) {
					return plainLossError(n, "non-finite float")
				}
				if err := o.push(formatPlainFloat(n.value.f)); err != nil {
					return err
				}
			case ValueKindString:
				if err := o.string(n.value.s); err != nil {
					return err
				}
			case ValueKindSequence:
				if err := o.push("["); err != nil {
					return err
				}
				stack = append(stack, plainFrame{op: plainOpSeqStep, seq: n.value.seq, idx: 0})
			case ValueKindMapping:
				if err := o.push("{"); err != nil {
					return err
				}
				var entries []MappingEntry
				if n.value.m != nil {
					entries = n.value.m.entries
				}
				stack = append(stack, plainFrame{op: plainOpMapStep, entries: entries, idx: 0})
			case ValueKindTagged:
				return plainLossError(n, "tagged value")
			default:
				return errInternal("unknown ValueKind")
			}
		case plainOpSeqStep:
			if f.idx >= len(f.seq) {
				if err := o.push("]"); err != nil {
					return err
				}
			} else {
				if f.idx > 0 {
					if err := o.push(","); err != nil {
						return err
					}
				}
				item := &f.seq[f.idx]
				f.idx++
				stack = append(stack, f, plainFrame{op: plainOpNode, node: item})
			}
		case plainOpMapStep:
			if f.idx >= len(f.entries) {
				if err := o.push("}"); err != nil {
					return err
				}
			} else {
				e := &f.entries[f.idx]
				if e.Key.value.kind != ValueKindString {
					return plainLossError(&e.Key, "non-string key")
				}
				if f.idx > 0 {
					if err := o.push(","); err != nil {
						return err
					}
				}
				if err := o.string(e.Key.value.s); err != nil {
					return err
				}
				if err := o.push(":"); err != nil {
					return err
				}
				f.idx++
				stack = append(stack, f, plainFrame{op: plainOpNode, node: &e.Value})
			}
		}
	}
	return nil
}

// EncodePlain serializes node as plain JSON text bounded by opts.Limits.MaxOutputBytes,
// failing with ErrorKindType on non-finite floats, non-String mapping keys, or Tagged values (spec-v1.4 §11.3).
func EncodePlain(node Node, opts Options) (string, *Error) {
	if err := opts.Validate(); err != nil {
		return "", err
	}
	o := newOutBuf(opts.Limits.MaxOutputBytes)
	if err := writePlainNode(&o, &node); err != nil {
		if err.Site.Kind == SiteKindNotSource && err.Site.LastKnown != nil {
			err.Site = SiteSource(*err.Site.LastKnown)
		}
		return "", err
	}
	return string(o.buf), nil
}

// ToPlain converts node to plain Go values (nil, bool, int64, float64, string, []any, map[string]any)
// per the spec-v1.4 §11.3 loss matrix:
//   - Int converts to int64 if it fits, else returns ErrorKindType
//   - Float converts to float64 (including -0.0, NaN, and ±Inf in memory)
//   - String-keyed Mapping converts to map[string]any; non-String keys return ErrorKindType
//   - Tagged values return ErrorKindType.
func ToPlain(node Node, opts ...Options) (any, *Error) {
	o := DefaultOptions()
	if len(opts) > 0 {
		o = opts[0]
		if err := o.Validate(); err != nil {
			return nil, err
		}
	}
	if err := preflightNode(&node, o); err != nil {
		return nil, err
	}
	return toPlainNode(&node, nil)
}

func toPlainLoss(n *Node, lastKnown *Location, what string) *Error {
	site := SiteNotSource(lastKnown)
	if s, ok := n.Span(); ok {
		site = SiteSource(s.Start)
	}
	return errTypeAtSite(
		site,
		"plain value",
		what,
		fmt.Sprintf("cannot convert %s to plain Go value without loss", what),
	)
}

func toPlainNode(n *Node, lastKnown *Location) (any, *Error) {
	if s, ok := n.Span(); ok {
		l := s.Start
		lastKnown = &l
	}
	switch n.value.kind {
	case ValueKindNull:
		return nil, nil
	case ValueKindBool:
		return n.value.b, nil
	case ValueKindInt:
		v, ok := n.value.i.Int64()
		if !ok {
			return nil, toPlainLoss(n, lastKnown, "integer outside int64")
		}
		return v, nil
	case ValueKindFloat:
		return n.value.f, nil
	case ValueKindString:
		if !utf8.ValidString(n.value.s) {
			site := SiteNotSource(lastKnown)
			if s, ok := n.Span(); ok {
				site = SiteSource(s.Start)
			}
			return nil, &Error{
				Kind:    ErrorKindInvalidUtf8,
				Detail:  ErrorDetail{HasInvalidUtf8: true},
				Site:    site,
				Message: "string is not valid UTF-8",
			}
		}
		return n.value.s, nil
	case ValueKindSequence:
		out := make([]any, len(n.value.seq))
		for i := range n.value.seq {
			item, err := toPlainNode(&n.value.seq[i], lastKnown)
			if err != nil {
				return nil, err
			}
			out[i] = item
		}
		return out, nil
	case ValueKindMapping:
		out := make(map[string]any, n.value.m.Len())
		if n.value.m != nil {
			for i := range n.value.m.entries {
				k := &n.value.m.entries[i].Key
				v := &n.value.m.entries[i].Value
				if k.value.kind != ValueKindString {
					return nil, toPlainLoss(k, lastKnown, "non-string key")
				}
				if !utf8.ValidString(k.value.s) {
					site := SiteNotSource(lastKnown)
					if s, ok := k.Span(); ok {
						site = SiteSource(s.Start)
					}
					return nil, &Error{
						Kind:    ErrorKindInvalidUtf8,
						Detail:  ErrorDetail{HasInvalidUtf8: true},
						Site:    site,
						Message: "string is not valid UTF-8",
					}
				}
				val, err := toPlainNode(v, lastKnown)
				if err != nil {
					return nil, err
				}
				out[k.value.s] = val
			}
		}
		return out, nil
	case ValueKindTagged:
		return nil, toPlainLoss(n, lastKnown, "tagged value")
	default:
		return nil, errInternal("unknown ValueKind")
	}
}

// WireEnvelope holds the decoded payload of a yamlprims-value/1 wire message.
// Exactly one of HasDocuments or Error != nil is true on a valid WireEnvelope.
type WireEnvelope struct {
	HasDocuments bool
	Documents    []Document
	Error        *Error
}

// DecodeWire strictly validates and decodes a yamlprims-value/1 JSON wire payload
// (spec-v1.4 §11.1, §11.4), rejecting duplicate object members, unknown fields,
// unknown version/kind, non-canonical integer strings, and float bits not matching
// 16 lowercase hex digits.
func DecodeWire(text string, opts ...Options) (WireEnvelope, *Error) {
	o := DefaultOptions()
	if len(opts) > 0 {
		o = opts[0]
		if err := o.Validate(); err != nil {
			return WireEnvelope{}, err
		}
	}
	if len(text) > o.Limits.MaxInputBytes {
		return WireEnvelope{}, notSourceLimit(LimitMaxInputBytes, o.Limits.MaxInputBytes, "wire input exceeds max_input_bytes")
	}
	if !utf8.ValidString(text) {
		return WireEnvelope{}, errInvalidUtf8(Location{ByteOffset: 0, Line: 1, Column: 1}, nil)
	}
	p := wireParser{
		src:    text,
		limits: o.Limits,
	}
	env, err := p.parseEnvelope()
	if err != nil {
		return WireEnvelope{}, err
	}
	return env, nil
}

type wireParser struct {
	src        string
	pos        int
	limits     Limits
	nodesUsed  int
	ownedBytes uint64
}

func (p *wireParser) fail(msg string) *Error {
	return errInternal("wire decode error: " + msg)
}

func (p *wireParser) limitErr(kind LimitKind, configured int) *Error {
	return notSourceLimit(kind, configured, "wire decode exceeded resource limit")
}

func (p *wireParser) chargeOwnedBytes(add uint64) *Error {
	p.ownedBytes = satAdd64(p.ownedBytes, add)
	if p.ownedBytes > uint64(p.limits.MaxOwnedBytes) {
		return p.limitErr(LimitMaxOwnedBytes, p.limits.MaxOwnedBytes)
	}
	return nil
}

func (p *wireParser) chargeNode(extraBytes uint64) *Error {
	p.nodesUsed++
	if p.nodesUsed > p.limits.MaxNodes {
		return p.limitErr(LimitMaxNodes, p.limits.MaxNodes)
	}
	return p.chargeOwnedBytes(satAdd64(NodeOverhead, extraBytes))
}

func (p *wireParser) skipWS() {
	for p.pos < len(p.src) {
		switch p.src[p.pos] {
		case ' ', '\t', '\r', '\n':
			p.pos++
		default:
			return
		}
	}
}

func (p *wireParser) expectByte(b byte) *Error {
	p.skipWS()
	if p.pos >= len(p.src) || p.src[p.pos] != b {
		return p.fail(fmt.Sprintf("expected %q", b))
	}
	p.pos++
	return nil
}

func (p *wireParser) peekByte() (byte, bool) {
	p.skipWS()
	if p.pos >= len(p.src) {
		return 0, false
	}
	return p.src[p.pos], true
}

func (p *wireParser) consumeLiteral(lit string) bool {
	p.skipWS()
	if strings.HasPrefix(p.src[p.pos:], lit) {
		p.pos += len(lit)
		return true
	}
	return false
}

func (p *wireParser) parseString() (string, *Error) {
	if err := p.expectByte('"'); err != nil {
		return "", err
	}
	var sb strings.Builder
	for p.pos < len(p.src) {
		b := p.src[p.pos]
		if b == '"' {
			p.pos++
			return sb.String(), nil
		}
		if b < 0x20 {
			return "", p.fail("unescaped control character in JSON string")
		}
		if b != '\\' {
			r, sz := utf8.DecodeRuneInString(p.src[p.pos:])
			if r == utf8.RuneError && sz == 1 {
				return "", p.fail("invalid UTF-8 in JSON string")
			}
			sb.WriteString(p.src[p.pos : p.pos+sz])
			p.pos += sz
			continue
		}
		p.pos++
		if p.pos >= len(p.src) {
			return "", p.fail("unexpected EOF in string escape")
		}
		esc := p.src[p.pos]
		p.pos++
		switch esc {
		case '"', '\\', '/':
			sb.WriteByte(esc)
		case 'b':
			sb.WriteByte('\b')
		case 'f':
			sb.WriteByte('\f')
		case 'n':
			sb.WriteByte('\n')
		case 'r':
			sb.WriteByte('\r')
		case 't':
			sb.WriteByte('\t')
		case 'u':
			u, err := p.parseHex4()
			if err != nil {
				return "", err
			}
			if u >= 0xD800 && u <= 0xDBFF {
				if !strings.HasPrefix(p.src[p.pos:], `\u`) {
					return "", p.fail("unpaired high surrogate in JSON string")
				}
				p.pos += 2
				low, err := p.parseHex4()
				if err != nil {
					return "", err
				}
				if low < 0xDC00 || low > 0xDFFF {
					return "", p.fail("invalid low surrogate in JSON string")
				}
				cp := 0x10000 + ((rune(u)-0xD800)<<10 | (rune(low) - 0xDC00))
				sb.WriteRune(cp)
			} else if u >= 0xDC00 && u <= 0xDFFF {
				return "", p.fail("unpaired low surrogate in JSON string")
			} else {
				sb.WriteRune(rune(u))
			}
		default:
			return "", p.fail("invalid escape sequence in JSON string")
		}
	}
	return "", p.fail("unterminated JSON string")
}

func (p *wireParser) parseHex4() (uint16, *Error) {
	if p.pos+4 > len(p.src) {
		return 0, p.fail("short \\uXXXX escape")
	}
	s := p.src[p.pos : p.pos+4]
	if !allHexDigits(s) {
		return 0, p.fail("invalid hex in \\uXXXX escape")
	}
	v, err := strconv.ParseUint(s, 16, 16)
	if err != nil {
		return 0, p.fail("invalid \\uXXXX escape")
	}
	p.pos += 4
	return uint16(v), nil
}

func (p *wireParser) parseMetaUint() (uint64, *Error) {
	p.skipWS()
	start := p.pos
	for p.pos < len(p.src) && p.src[p.pos] >= '0' && p.src[p.pos] <= '9' {
		p.pos++
	}
	digits := p.src[start:p.pos]
	if len(digits) == 0 {
		return 0, p.fail("expected unsigned integer")
	}
	if len(digits) > 1 && digits[0] == '0' {
		return 0, p.fail("non-canonical leading zero on metadata integer")
	}
	if p.pos < len(p.src) && (p.src[p.pos] == '.' || p.src[p.pos] == 'e' || p.src[p.pos] == 'E') {
		return 0, p.fail("non-integer metadata number")
	}
	v, err := strconv.ParseUint(digits, 10, 64)
	if err != nil || v > MaxWireInt {
		return 0, p.fail("metadata integer exceeds 2^53 - 1")
	}
	return v, nil
}

func (p *wireParser) parseMetaInt() (int, *Error) {
	v, err := p.parseMetaUint()
	if err != nil {
		return 0, err
	}
	return int(v), nil
}

func (p *wireParser) parseBool() (bool, *Error) {
	if p.consumeLiteral("true") {
		return true, nil
	}
	if p.consumeLiteral("false") {
		return false, nil
	}
	return false, p.fail("expected boolean")
}

func (p *wireParser) parseLoc() (Location, *Error) {
	if err := p.expectByte('['); err != nil {
		return Location{}, err
	}
	bOff, err := p.parseMetaInt()
	if err != nil {
		return Location{}, err
	}
	if err := p.expectByte(','); err != nil {
		return Location{}, err
	}
	line, err := p.parseMetaInt()
	if err != nil {
		return Location{}, err
	}
	if err := p.expectByte(','); err != nil {
		return Location{}, err
	}
	col, err := p.parseMetaInt()
	if err != nil {
		return Location{}, err
	}
	if err := p.expectByte(']'); err != nil {
		return Location{}, err
	}
	if line < 1 || col < 1 {
		return Location{}, p.fail("location line and column must be >= 1")
	}
	return Location{ByteOffset: bOff, Line: line, Column: col}, nil
}

func (p *wireParser) parseSpan() (Span, *Error) {
	if err := p.expectByte('['); err != nil {
		return Span{}, err
	}
	sb, err := p.parseMetaInt()
	if err != nil {
		return Span{}, err
	}
	if err := p.expectByte(','); err != nil {
		return Span{}, err
	}
	sl, err := p.parseMetaInt()
	if err != nil {
		return Span{}, err
	}
	if err := p.expectByte(','); err != nil {
		return Span{}, err
	}
	sc, err := p.parseMetaInt()
	if err != nil {
		return Span{}, err
	}
	if err := p.expectByte(','); err != nil {
		return Span{}, err
	}
	eb, err := p.parseMetaInt()
	if err != nil {
		return Span{}, err
	}
	if err := p.expectByte(','); err != nil {
		return Span{}, err
	}
	el, err := p.parseMetaInt()
	if err != nil {
		return Span{}, err
	}
	if err := p.expectByte(','); err != nil {
		return Span{}, err
	}
	ec, err := p.parseMetaInt()
	if err != nil {
		return Span{}, err
	}
	if err := p.expectByte(']'); err != nil {
		return Span{}, err
	}
	if sl < 1 || sc < 1 || el < 1 || ec < 1 || sb > eb {
		return Span{}, p.fail("invalid span coordinates")
	}
	return Span{
		Start: Location{ByteOffset: sb, Line: sl, Column: sc},
		End:   Location{ByteOffset: eb, Line: el, Column: ec},
	}, nil
}

func (p *wireParser) parseEnvelope() (WireEnvelope, *Error) {
	if err := p.expectByte('{'); err != nil {
		return WireEnvelope{}, err
	}
	seen := make(map[string]bool, 3)
	var hasFormat, hasVersion, hasDocs, hasErr bool
	var docs []Document
	var wireErr *Error

	first := true
	for {
		b, ok := p.peekByte()
		if !ok {
			return WireEnvelope{}, p.fail("unexpected EOF in envelope")
		}
		if b == '}' {
			p.pos++
			break
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return WireEnvelope{}, err
			}
		}
		first = false
		key, err := p.parseString()
		if err != nil {
			return WireEnvelope{}, err
		}
		if seen[key] {
			return WireEnvelope{}, p.fail("duplicate envelope member: " + key)
		}
		seen[key] = true
		if err := p.expectByte(':'); err != nil {
			return WireEnvelope{}, err
		}
		switch key {
		case "format":
			f, err := p.parseString()
			if err != nil {
				return WireEnvelope{}, err
			}
			if f != "yamlprims-value" {
				return WireEnvelope{}, p.fail("unsupported wire format: " + f)
			}
			hasFormat = true
		case "version":
			v, err := p.parseMetaUint()
			if err != nil {
				return WireEnvelope{}, err
			}
			if v != 1 {
				return WireEnvelope{}, p.fail(fmt.Sprintf("unsupported wire version: %d", v))
			}
			hasVersion = true
		case "documents":
			if hasErr {
				return WireEnvelope{}, p.fail("envelope cannot have both documents and error")
			}
			d, err := p.parseDocuments()
			if err != nil {
				return WireEnvelope{}, err
			}
			docs = d
			hasDocs = true
		case "error":
			if hasDocs {
				return WireEnvelope{}, p.fail("envelope cannot have both documents and error")
			}
			e, err := p.parseWireError()
			if err != nil {
				return WireEnvelope{}, err
			}
			wireErr = e
			hasErr = true
		default:
			return WireEnvelope{}, p.fail("unknown envelope field: " + key)
		}
	}
	p.skipWS()
	if p.pos != len(p.src) {
		return WireEnvelope{}, p.fail("trailing content after wire envelope")
	}
	if !hasFormat || !hasVersion || (!hasDocs && !hasErr) {
		return WireEnvelope{}, p.fail("incomplete wire envelope")
	}
	return WireEnvelope{
		HasDocuments: hasDocs,
		Documents:    docs,
		Error:        wireErr,
	}, nil
}

func (p *wireParser) parseDocuments() ([]Document, *Error) {
	if err := p.expectByte('['); err != nil {
		return nil, err
	}
	var docs []Document
	first := true
	for {
		b, ok := p.peekByte()
		if !ok {
			return nil, p.fail("unexpected EOF in documents array")
		}
		if b == ']' {
			p.pos++
			return docs, nil
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return nil, err
			}
		}
		first = false
		if len(docs)+1 > p.limits.MaxDocuments {
			return nil, p.limitErr(LimitMaxDocuments, p.limits.MaxDocuments)
		}
		doc, err := p.parseDocument()
		if err != nil {
			return nil, err
		}
		docs = append(docs, doc)
	}
}

func (p *wireParser) parseDocument() (Document, *Error) {
	if err := p.expectByte('{'); err != nil {
		return Document{}, err
	}
	seen := make(map[string]bool, 3)
	var ver *[2]uint32
	var explicit bool
	var root Node
	first := true

	for {
		b, ok := p.peekByte()
		if !ok {
			return Document{}, p.fail("unexpected EOF in document")
		}
		if b == '}' {
			p.pos++
			break
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return Document{}, err
			}
		}
		first = false
		key, err := p.parseString()
		if err != nil {
			return Document{}, err
		}
		if seen[key] {
			return Document{}, p.fail("duplicate document field: " + key)
		}
		seen[key] = true
		if err := p.expectByte(':'); err != nil {
			return Document{}, err
		}
		switch key {
		case "yaml_version":
			if p.consumeLiteral("null") {
				ver = nil
			} else {
				if err := p.expectByte('['); err != nil {
					return Document{}, err
				}
				maj, err := p.parseMetaUint()
				if err != nil {
					return Document{}, err
				}
				if err := p.expectByte(','); err != nil {
					return Document{}, err
				}
				min, err := p.parseMetaUint()
				if err != nil {
					return Document{}, err
				}
				if err := p.expectByte(']'); err != nil {
					return Document{}, err
				}
				if maj > math.MaxUint32 || min > math.MaxUint32 {
					return Document{}, p.fail("yaml_version component exceeds uint32")
				}
				v := [2]uint32{uint32(maj), uint32(min)}
				ver = &v
			}
		case "explicit_start":
			b, err := p.parseBool()
			if err != nil {
				return Document{}, err
			}
			explicit = b
		case "root":
			n, err := p.parseNode(0, false, false)
			if err != nil {
				return Document{}, err
			}
			root = n
		default:
			return Document{}, p.fail("unknown document field: " + key)
		}
	}
	if !seen["yaml_version"] || !seen["explicit_start"] || !seen["root"] {
		return Document{}, p.fail("missing required document field")
	}
	return newDocumentOwned(root, ver, explicit), nil
}

func isCanonicalWireInt(s string) (Integer, bool) {
	if s == "" {
		return Integer{}, false
	}
	if s == "0" {
		return Integer{}, true
	}
	digits := s
	neg := false
	if strings.HasPrefix(s, "-") {
		neg = true
		digits = s[1:]
	}
	if len(digits) == 0 || digits[0] < '1' || digits[0] > '9' || !allDecDigits(digits) {
		return Integer{}, false
	}
	res := parseRadix128(neg, digits, 10)
	if res.kind != intParseValue {
		return Integer{}, false
	}
	return res.val, true
}

func isLowerHex16(s string) (float64, bool) {
	if len(s) != 16 {
		return 0, false
	}
	for i := 0; i < 16; i++ {
		b := s[i]
		if !((b >= '0' && b <= '9') || (b >= 'a' && b <= 'f')) {
			return 0, false
		}
	}
	bits, err := strconv.ParseUint(s, 16, 64)
	if err != nil {
		return 0, false
	}
	f := math.Float64frombits(bits)
	if math.IsNaN(f) {
		f = canonicalNaN()
	}
	return f, true
}

func (p *wireParser) parseNode(depth int, isPathKey bool, inTagged bool) (Node, *Error) {
	const (
		payloadNone uint8 = iota
		payloadBool
		payloadStrOrInt
		payloadTaggedInner
		payloadFloat
		payloadSequence
		payloadMapping
	)
	if err := p.chargeNode(0); err != nil {
		return Node{}, err
	}

	if err := p.expectByte('{'); err != nil {
		return Node{}, err
	}
	seen := make(map[string]bool, 6)
	var kindStr string
	var payloadKind uint8 = payloadNone
	var payBool bool
	var payStr string
	var payStrCharged bool
	var payInner Node
	var payFloat float64
	var paySeq []Node
	var payMap *Mapping
	var tagVal Tag
	var hasTag bool
	var nodeSpan NodeSpan = SyntheticNodeSpan()
	var via *Via
	var tagStart *Location

	first := true
	for {
		b, ok := p.peekByte()
		if !ok {
			return Node{}, p.fail("unexpected EOF in node")
		}
		if b == '}' {
			p.pos++
			break
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return Node{}, err
			}
		}
		first = false
		key, err := p.parseString()
		if err != nil {
			return Node{}, err
		}
		if seen[key] {
			return Node{}, p.fail("duplicate node member: " + key)
		}
		seen[key] = true
		if err := p.expectByte(':'); err != nil {
			return Node{}, err
		}
		switch key {
		case "kind":
			k, err := p.parseString()
			if err != nil {
				return Node{}, err
			}
			switch k {
			case "null", "bool", "int", "float", "string", "sequence", "mapping":
			case "tagged":
				if inTagged {
					return Node{}, p.fail("nested tagged wrapper in wire node")
				}
			default:
				return Node{}, p.fail("unknown node kind: " + k)
			}
			kindStr = k

		case "value":
			if payloadKind != payloadNone {
				return Node{}, p.fail("conflicting node payload members")
			}
			vb, ok := p.peekByte()
			if !ok {
				return Node{}, p.fail("unexpected EOF in node value")
			}
			switch vb {
			case 't', 'f':
				if seen["kind"] && kindStr != "bool" {
					return Node{}, p.fail("unexpected bool value for kind " + kindStr)
				}
				bv, err := p.parseBool()
				if err != nil {
					return Node{}, err
				}
				payBool = bv
				payloadKind = payloadBool
			case '"':
				if seen["kind"] && kindStr != "int" && kindStr != "string" {
					return Node{}, p.fail("unexpected string value for kind " + kindStr)
				}
				s, err := p.parseString()
				if err != nil {
					return Node{}, err
				}
				if kindStr == "string" || len(s) > 64 {
					if len(s) > p.limits.MaxScalarBytes {
						return Node{}, p.limitErr(LimitMaxScalarBytes, p.limits.MaxScalarBytes)
					}
					if err := p.chargeOwnedBytes(uint64(len(s))); err != nil {
						return Node{}, err
					}
					payStrCharged = true
				}
				payStr = s
				payloadKind = payloadStrOrInt
			case '{':
				if inTagged {
					return Node{}, p.fail("nested tagged wrapper in wire node")
				}
				if seen["kind"] && kindStr != "tagged" {
					return Node{}, p.fail("unexpected object value for kind " + kindStr)
				}
				inner, err := p.parseNode(depth, isPathKey, true)
				if err != nil {
					return Node{}, err
				}
				payInner = inner
				payloadKind = payloadTaggedInner
			default:
				return Node{}, p.fail("invalid value token in wire node")
			}

		case "bits":
			if payloadKind != payloadNone || (seen["kind"] && kindStr != "float") {
				return Node{}, p.fail("unexpected bits member for kind " + kindStr)
			}
			s, err := p.parseString()
			if err != nil {
				return Node{}, err
			}
			fv, ok := isLowerHex16(s)
			if !ok {
				return Node{}, p.fail("invalid float bits: " + s)
			}
			payFloat = fv
			payloadKind = payloadFloat

		case "items":
			if payloadKind != payloadNone || (seen["kind"] && kindStr != "sequence") {
				return Node{}, p.fail("unexpected items member for kind " + kindStr)
			}
			if !isPathKey && depth+1 > p.limits.MaxDepth {
				return Node{}, p.limitErr(LimitMaxDepth, p.limits.MaxDepth)
			}
			if err := p.expectByte('['); err != nil {
				return Node{}, err
			}
			var items []Node
			fItem := true
			for {
				ib, ok := p.peekByte()
				if !ok {
					return Node{}, p.fail("unexpected EOF in sequence items")
				}
				if ib == ']' {
					p.pos++
					break
				}
				if !fItem {
					if err := p.expectByte(','); err != nil {
						return Node{}, err
					}
				}
				fItem = false
				if isPathKey && depth+1 > p.limits.MaxDepth {
					return Node{}, p.limitErr(LimitMaxDepth, p.limits.MaxDepth)
				}
				it, err := p.parseNode(depth+1, isPathKey, false)
				if err != nil {
					return Node{}, err
				}
				items = append(items, it)
			}
			paySeq = items
			payloadKind = payloadSequence

		case "entries":
			if payloadKind != payloadNone || (seen["kind"] && kindStr != "mapping") {
				return Node{}, p.fail("unexpected entries member for kind " + kindStr)
			}
			if !isPathKey && depth+1 > p.limits.MaxDepth {
				return Node{}, p.limitErr(LimitMaxDepth, p.limits.MaxDepth)
			}
			if err := p.expectByte('['); err != nil {
				return Node{}, err
			}
			m := NewMapping()
			fEntry := true
			for {
				eb, ok := p.peekByte()
				if !ok {
					return Node{}, p.fail("unexpected EOF in mapping entries")
				}
				if eb == ']' {
					p.pos++
					break
				}
				if !fEntry {
					if err := p.expectByte(','); err != nil {
						return Node{}, err
					}
				}
				fEntry = false
				if isPathKey && depth+1 > p.limits.MaxDepth {
					return Node{}, p.limitErr(LimitMaxDepth, p.limits.MaxDepth)
				}
				kn, vn, err := p.parseMappingEntry(depth+1, isPathKey)
				if err != nil {
					return Node{}, err
				}
				_, kw, _ := metrics(kn)
				if kw > uint64(p.limits.MaxKeyBytes) {
					return Node{}, p.limitErr(LimitMaxKeyBytes, p.limits.MaxKeyBytes)
				}
				if err := p.chargeOwnedBytes(EntryOverhead); err != nil {
					return Node{}, err
				}
				if m.Len() >= p.limits.MaxMappingEntries {
					return Node{}, p.limitErr(LimitMaxMappingEntries, p.limits.MaxMappingEntries)
				}
				if insErr := m.insertNewOwned(kn, vn); insErr != nil {
					return Node{}, p.fail("invalid or duplicate key in wire mapping")
				}
			}
			payMap = m
			payloadKind = payloadMapping

		case "tag":
			if seen["kind"] && kindStr != "tagged" {
				return Node{}, p.fail("unexpected tag member for kind " + kindStr)
			}
			t, err := p.parseTagObject()
			if err != nil {
				return Node{}, err
			}
			if err := p.chargeOwnedBytes(tagBytes(t)); err != nil {
				return Node{}, err
			}
			tagVal = t
			hasTag = true

		case "span":
			if p.consumeLiteral("null") {
				nodeSpan = SyntheticNodeSpan()
			} else {
				if isPathKey {
					return Node{}, p.fail("path key span must be null")
				}
				sp, err := p.parseSpan()
				if err != nil {
					return Node{}, err
				}
				nodeSpan = SourceNodeSpan(sp)
			}

		case "via":
			if p.consumeLiteral("null") {
				via = nil
			} else {
				if isPathKey {
					return Node{}, p.fail("path key via must be null")
				}
				v, err := p.parseViaObject()
				if err != nil {
					return Node{}, err
				}
				via = &v
			}

		case "tag_start":
			if p.consumeLiteral("null") {
				tagStart = nil
			} else {
				if isPathKey {
					return Node{}, p.fail("path key tag_start must be null")
				}
				loc, err := p.parseLoc()
				if err != nil {
					return Node{}, err
				}
				tagStart = &loc
			}

		default:
			return Node{}, p.fail("unknown node member: " + key)
		}
	}

	if !seen["kind"] || !seen["span"] || !seen["via"] || !seen["tag_start"] {
		return Node{}, p.fail("incomplete wire node")
	}
	if hasTag && kindStr != "tagged" {
		return Node{}, p.fail("unexpected tag member for kind " + kindStr)
	}
	var val Value
	switch kindStr {
	case "null":
		if payloadKind != payloadNone {
			return Node{}, p.fail("unexpected payload for null node")
		}
		val = NullValue()
	case "bool":
		if payloadKind != payloadBool {
			return Node{}, p.fail("incomplete or mismatched bool node")
		}
		val = BoolValue(payBool)
	case "int":
		if payloadKind != payloadStrOrInt || payStrCharged {
			return Node{}, p.fail("non-canonical wire integer")
		}
		iv, ok := isCanonicalWireInt(payStr)
		if !ok {
			return Node{}, p.fail("non-canonical wire integer: " + payStr)
		}
		val = IntValue(iv)
	case "string":
		if payloadKind != payloadStrOrInt {
			return Node{}, p.fail("incomplete or mismatched string node")
		}
		if !payStrCharged {
			if len(payStr) > p.limits.MaxScalarBytes {
				return Node{}, p.limitErr(LimitMaxScalarBytes, p.limits.MaxScalarBytes)
			}
			if err := p.chargeOwnedBytes(uint64(len(payStr))); err != nil {
				return Node{}, err
			}
		}
		val = StringValue(payStr)
	case "float":
		if payloadKind != payloadFloat {
			return Node{}, p.fail("incomplete or mismatched float node")
		}
		val = FloatValue(payFloat)
	case "sequence":
		if payloadKind != payloadSequence {
			return Node{}, p.fail("incomplete or mismatched sequence node")
		}
		val = sequenceValueOwned(paySeq)
	case "mapping":
		if payloadKind != payloadMapping {
			return Node{}, p.fail("incomplete or mismatched mapping node")
		}
		val = mappingValueOwned(payMap)
	case "tagged":
		if !hasTag || payloadKind != payloadTaggedInner {
			return Node{}, p.fail("incomplete or mismatched tagged node")
		}
		validInner := false
		if tagVal.uri == "tag:yaml.org,2002:merge" {
			validInner = payInner.value.kind == ValueKindString
		} else {
			validInner = payInner.value.kind == ValueKindString ||
				payInner.value.kind == ValueKindSequence ||
				payInner.value.kind == ValueKindMapping
		}
		if !validInner {
			return Node{}, p.fail("invalid inner value kind for tagged node")
		}
		val = taggedValueOwned(newTaggedWithPlain(tagVal, payInner, false))
	default:
		return Node{}, p.fail("incomplete wire node")
	}
	return Node{
		value:    val,
		span:     nodeSpan,
		via:      via,
		tagStart: tagStart,
	}, nil
}

func (p *wireParser) parseMappingEntry(depth int, isPathKey bool) (Node, Node, *Error) {
	if err := p.expectByte('{'); err != nil {
		return Node{}, Node{}, err
	}
	seen := make(map[string]bool, 2)
	var kNode, vNode Node
	first := true
	for {
		b, ok := p.peekByte()
		if !ok {
			return Node{}, Node{}, p.fail("unexpected EOF in mapping entry")
		}
		if b == '}' {
			p.pos++
			break
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return Node{}, Node{}, err
			}
		}
		first = false
		k, err := p.parseString()
		if err != nil {
			return Node{}, Node{}, err
		}
		if seen[k] {
			return Node{}, Node{}, p.fail("duplicate mapping entry member: " + k)
		}
		seen[k] = true
		if err := p.expectByte(':'); err != nil {
			return Node{}, Node{}, err
		}
		switch k {
		case "key":
			n, err := p.parseNode(depth, isPathKey, false)
			if err != nil {
				return Node{}, Node{}, err
			}
			kNode = n
		case "value":
			n, err := p.parseNode(depth, isPathKey, false)
			if err != nil {
				return Node{}, Node{}, err
			}
			vNode = n
		default:
			return Node{}, Node{}, p.fail("unknown mapping entry member: " + k)
		}
	}
	if !seen["key"] || !seen["value"] {
		return Node{}, Node{}, p.fail("incomplete mapping entry")
	}
	return kNode, vNode, nil
}

func (p *wireParser) parseTagObject() (Tag, *Error) {
	if err := p.expectByte('{'); err != nil {
		return Tag{}, err
	}
	seen := make(map[string]bool, 3)
	var uri, handle, suffix string
	first := true
	for {
		b, ok := p.peekByte()
		if !ok {
			return Tag{}, p.fail("unexpected EOF in tag")
		}
		if b == '}' {
			p.pos++
			break
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return Tag{}, err
			}
		}
		first = false
		k, err := p.parseString()
		if err != nil {
			return Tag{}, err
		}
		if seen[k] {
			return Tag{}, p.fail("duplicate tag member: " + k)
		}
		seen[k] = true
		if err := p.expectByte(':'); err != nil {
			return Tag{}, err
		}
		v, err := p.parseString()
		if err != nil {
			return Tag{}, err
		}
		switch k {
		case "uri":
			uri = v
		case "handle":
			handle = v
		case "suffix":
			suffix = v
		default:
			return Tag{}, p.fail("unknown tag member: " + k)
		}
	}
	if !seen["uri"] || !seen["handle"] || !seen["suffix"] {
		return Tag{}, p.fail("incomplete tag object")
	}
	if !isValidNonCoreTagURI(uri) || !isValidTagSpelling(uri, handle, suffix) {
		return Tag{}, p.fail("invalid or empty tag object")
	}
	return NewTag(uri, handle, suffix), nil
}

func (p *wireParser) parseViaObject() (Via, *Error) {
	if err := p.expectByte('{'); err != nil {
		return Via{}, err
	}
	k, err := p.parseString()
	if err != nil {
		return Via{}, err
	}
	if err := p.expectByte(':'); err != nil {
		return Via{}, err
	}
	sp, err := p.parseSpan()
	if err != nil {
		return Via{}, err
	}
	if err := p.expectByte('}'); err != nil {
		return Via{}, err
	}
	switch k {
	case "alias":
		return ViaAlias(sp), nil
	case "merge":
		return ViaMerge(sp), nil
	default:
		return Via{}, p.fail("unknown via member: " + k)
	}
}

type rawErrorDetail struct {
	seen           map[string]bool
	invalidUtf8Len *int
	hasInvalidUtf8 bool
	syntaxKind     SyntaxKind
	limit          LimitKind
	configured     uint64
	hasConfigured  bool
	expected       string
	found          string
	field          string
}

func (p *wireParser) parseWireError() (*Error, *Error) {
	if err := p.expectByte('{'); err != nil {
		return nil, err
	}
	seen := make(map[string]bool, 6)
	out := &Error{}
	var rawDetail rawErrorDetail
	first := true

	for {
		b, ok := p.peekByte()
		if !ok {
			return nil, p.fail("unexpected EOF in error")
		}
		if b == '}' {
			p.pos++
			break
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return nil, err
			}
		}
		first = false
		k, err := p.parseString()
		if err != nil {
			return nil, err
		}
		if seen[k] {
			return nil, p.fail("duplicate error member: " + k)
		}
		seen[k] = true
		if err := p.expectByte(':'); err != nil {
			return nil, err
		}
		switch k {
		case "kind":
			ks, err := p.parseString()
			if err != nil {
				return nil, err
			}
			ek := ErrorKind(ks)
			if !ek.IsValid() {
				return nil, p.fail("unknown error kind: " + ks)
			}
			out.Kind = ek

		case "detail":
			d, err := p.parseRawErrorDetail()
			if err != nil {
				return nil, err
			}
			rawDetail = d

		case "site":
			site, err := p.parseErrorSite()
			if err != nil {
				return nil, err
			}
			out.Site = site

		case "related":
			rel, err := p.parseErrorRelated()
			if err != nil {
				return nil, err
			}
			out.Related = rel

		case "path":
			path, err := p.parseErrorPath()
			if err != nil {
				return nil, err
			}
			out.Path = path

		case "message":
			msg, err := p.parseString()
			if err != nil {
				return nil, err
			}
			out.Message = msg

		default:
			return nil, p.fail("unknown error member: " + k)
		}
	}
	if !seen["kind"] || !seen["detail"] || !seen["site"] || !seen["related"] || !seen["path"] || !seen["message"] {
		return nil, p.fail("incomplete wire error object")
	}
	detail, err := p.validateErrorDetail(out.Kind, rawDetail)
	if err != nil {
		return nil, err
	}
	out.Detail = detail
	return out, nil
}

func (p *wireParser) parseRawErrorDetail() (rawErrorDetail, *Error) {
	if err := p.expectByte('{'); err != nil {
		return rawErrorDetail{}, err
	}
	d := rawErrorDetail{
		seen: make(map[string]bool, 2),
	}
	first := true
	for {
		b, ok := p.peekByte()
		if !ok {
			return rawErrorDetail{}, p.fail("unexpected EOF in error detail")
		}
		if b == '}' {
			p.pos++
			break
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return rawErrorDetail{}, err
			}
		}
		first = false
		k, err := p.parseString()
		if err != nil {
			return rawErrorDetail{}, err
		}
		if d.seen[k] {
			return rawErrorDetail{}, p.fail("duplicate error detail member: " + k)
		}
		d.seen[k] = true
		if err := p.expectByte(':'); err != nil {
			return rawErrorDetail{}, err
		}
		switch k {
		case "invalid_len":
			if !p.consumeLiteral("null") {
				n, err := p.parseMetaInt()
				if err != nil {
					return rawErrorDetail{}, err
				}
				d.invalidUtf8Len = &n
			}
			d.hasInvalidUtf8 = true
		case "syntax_kind":
			code, err := p.parseString()
			if err != nil {
				return rawErrorDetail{}, err
			}
			sk, ok := SyntaxKindFromCode(code)
			if !ok {
				return rawErrorDetail{}, p.fail("unknown syntax_kind code: " + code)
			}
			d.syntaxKind = sk
		case "limit":
			ls, err := p.parseString()
			if err != nil {
				return rawErrorDetail{}, err
			}
			lk := LimitKind(ls)
			if !lk.IsValid() {
				return rawErrorDetail{}, p.fail("unknown limit kind: " + ls)
			}
			d.limit = lk
		case "configured":
			c, err := p.parseMetaUint()
			if err != nil || c == 0 {
				return rawErrorDetail{}, p.fail("invalid configured limit")
			}
			d.configured = c
			d.hasConfigured = true
		case "expected":
			v, err := p.parseString()
			if err != nil {
				return rawErrorDetail{}, err
			}
			d.expected = v
		case "found":
			v, err := p.parseString()
			if err != nil {
				return rawErrorDetail{}, err
			}
			d.found = v
		case "field":
			v, err := p.parseString()
			if err != nil {
				return rawErrorDetail{}, err
			}
			d.field = v
		default:
			return rawErrorDetail{}, p.fail("unknown error detail member: " + k)
		}
	}
	return d, nil
}

func (p *wireParser) validateErrorDetail(kind ErrorKind, raw rawErrorDetail) (ErrorDetail, *Error) {
	var d ErrorDetail
	switch kind {
	case ErrorKindInvalidUtf8:
		if len(raw.seen) != 1 || !raw.seen["invalid_len"] {
			return ErrorDetail{}, p.fail("expected invalid_len in invalid_utf8 detail")
		}
		d.InvalidUtf8Len = raw.invalidUtf8Len
		d.HasInvalidUtf8 = true
	case ErrorKindSyntax:
		if len(raw.seen) != 1 || !raw.seen["syntax_kind"] {
			return ErrorDetail{}, p.fail("expected syntax_kind in syntax detail")
		}
		d.SyntaxKind = raw.syntaxKind
	case ErrorKindLimitExceeded:
		if len(raw.seen) != 2 || !raw.seen["limit"] || !raw.seen["configured"] {
			return ErrorDetail{}, p.fail("incomplete limit_exceeded detail")
		}
		d.Limit = raw.limit
		d.Configured = raw.configured
		d.HasConfigured = true
	case ErrorKindType:
		if len(raw.seen) != 2 || !raw.seen["expected"] || !raw.seen["found"] {
			return ErrorDetail{}, p.fail("incomplete type detail")
		}
		d.Expected = raw.expected
		d.Found = raw.found
		d.HasType = true
	case ErrorKindMissingField, ErrorKindUnknownField:
		if len(raw.seen) != 1 || !raw.seen["field"] {
			return ErrorDetail{}, p.fail("expected field in detail")
		}
		d.Field = raw.field
		d.HasField = true
	case ErrorKindOutputLimitExceeded:
		if len(raw.seen) != 1 || !raw.seen["configured"] {
			return ErrorDetail{}, p.fail("expected configured in output_limit_exceeded detail")
		}
		d.Configured = raw.configured
		d.HasConfigured = true
	default:
		if len(raw.seen) != 0 {
			return ErrorDetail{}, p.fail("expected empty detail object")
		}
	}
	return d, nil
}

func (p *wireParser) parseErrorSite() (Site, *Error) {
	if err := p.expectByte('{'); err != nil {
		return Site{}, err
	}
	k, err := p.parseString()
	if err != nil {
		return Site{}, err
	}
	if err := p.expectByte(':'); err != nil {
		return Site{}, err
	}
	var site Site
	switch k {
	case "source":
		loc, err := p.parseLoc()
		if err != nil {
			return Site{}, err
		}
		site = SiteSource(loc)
	case "not_source":
		if err := p.expectByte('{'); err != nil {
			return Site{}, err
		}
		lkKey, err := p.parseString()
		if err != nil {
			return Site{}, err
		}
		if lkKey != "last_known" {
			return Site{}, p.fail("expected last_known in not_source")
		}
		if err := p.expectByte(':'); err != nil {
			return Site{}, err
		}
		if p.consumeLiteral("null") {
			site = SiteNotSource(nil)
		} else {
			loc, err := p.parseLoc()
			if err != nil {
				return Site{}, err
			}
			site = SiteNotSource(&loc)
		}
		if err := p.expectByte('}'); err != nil {
			return Site{}, err
		}
	default:
		return Site{}, p.fail("unknown site member: " + k)
	}
	if err := p.expectByte('}'); err != nil {
		return Site{}, err
	}
	return site, nil
}

func (p *wireParser) parseErrorRelated() ([]Related, *Error) {
	if err := p.expectByte('['); err != nil {
		return nil, err
	}
	var rel []Related
	first := true
	for {
		b, ok := p.peekByte()
		if !ok {
			return nil, p.fail("unexpected EOF in related")
		}
		if b == ']' {
			p.pos++
			return rel, nil
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return nil, err
			}
		}
		first = false
		if err := p.expectByte('{'); err != nil {
			return nil, err
		}
		seen := make(map[string]bool, 2)
		var r Related
		fRel := true
		for {
			rb, ok := p.peekByte()
			if !ok {
				return nil, p.fail("unexpected EOF in related item")
			}
			if rb == '}' {
				p.pos++
				break
			}
			if !fRel {
				if err := p.expectByte(','); err != nil {
					return nil, err
				}
			}
			fRel = false
			k, err := p.parseString()
			if err != nil {
				return nil, err
			}
			if seen[k] {
				return nil, p.fail("duplicate related member: " + k)
			}
			seen[k] = true
			if err := p.expectByte(':'); err != nil {
				return nil, err
			}
			switch k {
			case "role":
				rs, err := p.parseString()
				if err != nil {
					return nil, err
				}
				role := RelatedRole(rs)
				if !role.IsValid() {
					return nil, p.fail("unknown related role: " + rs)
				}
				r.Role = role
			case "span":
				sp, err := p.parseSpan()
				if err != nil {
					return nil, err
				}
				r.Span = sp
			default:
				return nil, p.fail("unknown related member: " + k)
			}
		}
		if !seen["role"] || !seen["span"] {
			return nil, p.fail("incomplete related item")
		}
		rel = append(rel, r)
	}
}

func (p *wireParser) parseErrorPath() ([]PathSegment, *Error) {
	if err := p.expectByte('['); err != nil {
		return nil, err
	}
	var path []PathSegment
	first := true
	for {
		b, ok := p.peekByte()
		if !ok {
			return nil, p.fail("unexpected EOF in path")
		}
		if b == ']' {
			p.pos++
			return path, nil
		}
		if !first {
			if err := p.expectByte(','); err != nil {
				return nil, err
			}
		}
		first = false
		if len(path)+1 > p.limits.MaxDepth {
			return nil, p.limitErr(LimitMaxDepth, p.limits.MaxDepth)
		}
		if err := p.expectByte('{'); err != nil {
			return nil, err
		}
		k, err := p.parseString()
		if err != nil {
			return nil, err
		}
		if err := p.expectByte(':'); err != nil {
			return nil, err
		}
		switch k {
		case "index":
			idx, err := p.parseMetaInt()
			if err != nil {
				return nil, err
			}
			path = append(path, PathIndex(idx))
		case "key":
			n, err := p.parseNode(0, true, false)
			if err != nil {
				return nil, err
			}
			_, kw, _ := metrics(n)
			if kw > uint64(p.limits.MaxKeyBytes) {
				return nil, p.limitErr(LimitMaxKeyBytes, p.limits.MaxKeyBytes)
			}
			path = append(path, PathKey(n.value))
		default:
			return nil, p.fail("unknown path segment member: " + k)
		}
		if err := p.expectByte('}'); err != nil {
			return nil, err
		}
	}
}
