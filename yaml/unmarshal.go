package yaml

import (
	"fmt"
	"io"
	"math"
	"math/big"
	"reflect"
	"strings"
	"unicode/utf8"
)

var (
	nodeType     = reflect.TypeOf(Node{})
	valueType    = reflect.TypeOf(Value{})
	documentType = reflect.TypeOf(Document{})
	taggedType   = reflect.TypeOf(Tagged{})
	mappingType  = reflect.TypeOf(Mapping{})
	integerType  = reflect.TypeOf(Integer{})
	bigIntType   = reflect.TypeOf(big.Int{})
)

// Unmarshal parses and resolves a single YAML 1.2 Core Schema document from
// data and unmarshals its root into the non-nil pointer v (spec-v1.6 §11.2, §11.3).
func Unmarshal(data []byte, v any, opts ...Options) *Error {
	return UnmarshalWith(data, v, resolveOpts(opts))
}

// UnmarshalWith parses and resolves a single YAML 1.2 Core Schema document from
// data under opts and unmarshals its root into the non-nil pointer v (spec-v1.6 §11.2, §11.3).
func UnmarshalWith(data []byte, v any, opts Options) *Error {
	doc, err := LoadBytes(data, opts)
	if err != nil {
		return err
	}
	targetRV, err := validateUnmarshalTarget(v)
	if err != nil {
		return err
	}
	ctx := newUnmarshalCtx(&doc.root, opts)
	return unmarshalInto(ctx, targetRV, &doc)
}

// UnmarshalReader reads up to MaxInputBytes+1 bytes from r and unmarshals a
// single YAML 1.2 Core Schema document into the non-nil pointer v using DefaultOptions() (spec-v1.6 §10.1, §11.2, §11.3).
func UnmarshalReader(r io.Reader, v any, opts ...Options) *Error {
	return UnmarshalReaderWith(r, v, resolveOpts(opts))
}

// UnmarshalReaderWith reads up to opts.Limits.MaxInputBytes+1 bytes from r and
// unmarshals a single YAML 1.2 Core Schema document into the non-nil pointer v under opts (spec-v1.6 §10.1, §11.2, §11.3).
func UnmarshalReaderWith(r io.Reader, v any, opts Options) *Error {
	buf, err := readBounded(r, opts)
	if err != nil {
		return err
	}
	return UnmarshalWith(buf, v, opts)
}

// UnmarshalNode unmarshals a resolved Node into the non-nil pointer v using DefaultOptions() (spec-v1.6 §11.2, §11.3).
func UnmarshalNode(node Node, v any, opts ...Options) *Error {
	return UnmarshalNodeWith(node, v, resolveOpts(opts))
}

// UnmarshalNodeWith preflights node against opts and unmarshals it into the non-nil pointer v (spec-v1.6 §11.2, §11.3).
func UnmarshalNodeWith(node Node, v any, opts Options) *Error {
	if err := preflightNode(&node, opts); err != nil {
		return err
	}
	targetRV, err := validateUnmarshalTarget(v)
	if err != nil {
		return err
	}
	ctx := newUnmarshalCtx(&node, opts)
	return unmarshalInto(ctx, targetRV, nil)
}

func validateUnmarshalTarget(v any) (reflect.Value, *Error) {
	if v == nil {
		return reflect.Value{}, errTypeAtSite(
			SiteNotSource(nil),
			"non-nil pointer",
			"nil",
			"unmarshal target must be a non-nil pointer",
		)
	}
	rv := reflect.ValueOf(v)
	if rv.Kind() != reflect.Pointer {
		return reflect.Value{}, errTypeAtSite(
			SiteNotSource(nil),
			"non-nil pointer",
			rv.Type().String(),
			"unmarshal target must be a non-nil pointer",
		)
	}
	if rv.IsNil() {
		return reflect.Value{}, errTypeAtSite(
			SiteNotSource(nil),
			"non-nil pointer",
			"nil "+rv.Type().String(),
			"unmarshal target must be a non-nil pointer",
		)
	}
	return rv.Elem(), nil
}

type unmarshalCtx struct {
	node      *Node
	opts      Options
	depth     int
	lastKnown *Location
	origin    nodeOrigin
}

func newUnmarshalCtx(node *Node, opts Options) unmarshalCtx {
	var lk *Location
	if s, ok := node.Span(); ok {
		l := s.Start
		lk = &l
	}
	return unmarshalCtx{
		node:      node,
		opts:      opts,
		depth:     0,
		lastKnown: lk,
		origin:    originOf(node, nodeOrigin{}),
	}
}

func (c unmarshalCtx) child(n *Node) unmarshalCtx {
	lk := c.lastKnown
	if s, ok := n.Span(); ok {
		l := s.Start
		lk = &l
	}
	return unmarshalCtx{
		node:      n,
		opts:      c.opts,
		depth:     c.depth + 1,
		lastKnown: lk,
		origin:    originOf(n, c.origin),
	}
}

func (c unmarshalCtx) site() Site {
	if s, ok := c.node.Span(); ok {
		return SiteSource(s.Start)
	}
	return SiteNotSource(c.lastKnown)
}

func (c unmarshalCtx) attachOrigin(err *Error) *Error {
	if err == nil {
		return nil
	}
	if c.origin.has && !hasRelated(err, c.origin.role, c.origin.span) {
		err.Related = append(err.Related, Related{Role: c.origin.role, Span: c.origin.span})
	}
	return err
}

func hasRelated(err *Error, role RelatedRole, span Span) bool {
	for _, r := range err.Related {
		if r.Role == role && r.Span == span {
			return true
		}
	}
	return false
}

func (c unmarshalCtx) foundKindDesc() string {
	if c.node.value.kind == ValueKindTagged {
		return "tagged value"
	}
	return c.node.value.KindName()
}

func (c unmarshalCtx) typeErr(expected string) *Error {
	found := c.foundKindDesc()
	err := errTypeAtSite(
		c.site(),
		expected,
		found,
		fmt.Sprintf("expected %s, found %s", expected, found),
	)
	return c.attachOrigin(err)
}

func (c unmarshalCtx) simpleErr(kind ErrorKind, msg string) *Error {
	err := &Error{
		Kind:    kind,
		Site:    c.site(),
		Message: boundedString(msg, MaxMessageBytes),
	}
	return c.attachOrigin(err)
}

func (c unmarshalCtx) outOfRangeErr(target string) *Error {
	return c.simpleErr(ErrorKindIntegerOutOfRange, fmt.Sprintf("integer out of range for %s", target))
}

func (c unmarshalCtx) invalidUtf8Err() *Error {
	err := &Error{
		Kind:    ErrorKindInvalidUtf8,
		Detail:  ErrorDetail{HasInvalidUtf8: true},
		Site:    c.site(),
		Message: "string is not valid UTF-8",
	}
	return c.attachOrigin(err)
}

func (c unmarshalCtx) unknownFieldErr(field string) *Error {
	err := &Error{
		Kind: ErrorKindUnknownField,
		Detail: ErrorDetail{
			Field:    boundedString(field, MaxFieldBytes),
			HasField: true,
		},
		Site:    c.site(),
		Message: boundedString(fmt.Sprintf("unknown field `%s`", field), MaxMessageBytes),
	}
	return c.attachOrigin(err)
}

func (c unmarshalCtx) depthGuard() *Error {
	if c.depth > c.opts.Limits.MaxDepth {
		err := &Error{
			Kind: ErrorKindLimitExceeded,
			Detail: ErrorDetail{
				Limit:         LimitMaxDepth,
				Configured:    uint64(c.opts.Limits.MaxDepth),
				HasConfigured: true,
			},
			Site:    c.site(),
			Message: "max_depth exceeded during conversion",
		}
		return c.attachOrigin(err)
	}
	return nil
}

func prependPath(err *Error, seg PathSegment) *Error {
	err.Path = append([]PathSegment{seg}, err.Path...)
	return err
}

func isSpecialUnmarshalType(rt reflect.Type) bool {
	switch rt {
	case nodeType, valueType, documentType, taggedType, mappingType, integerType, bigIntType:
		return true
	default:
		return false
	}
}

func hasPointerTypeCycle(rt reflect.Type) bool {
	slow := rt
	fast := rt
	for fast.Kind() == reflect.Pointer && !isSpecialUnmarshalType(fast) {
		fast = fast.Elem()
		if fast.Kind() != reflect.Pointer || isSpecialUnmarshalType(fast) {
			return false
		}
		fast = fast.Elem()
		slow = slow.Elem()
		if slow == fast {
			return true
		}
	}
	return false
}

func unmarshalInto(c unmarshalCtx, rv reflect.Value, rootDoc *Document) *Error {
	if err := c.depthGuard(); err != nil {
		return err
	}

	rt := rv.Type()

	// Pointer target (*T): Null sets *v = nil; non-Null checks for cyclic pointer types
	// before allocating and unwraps finite pointer chains iteratively without charging collection depth.
	if rv.Kind() == reflect.Pointer && !isSpecialUnmarshalType(rt) {
		if c.node.value.kind == ValueKindNull {
			rv.Set(reflect.Zero(rt))
			return nil
		}
		if hasPointerTypeCycle(rt) {
			return c.typeErr(rt.String())
		}
		for rv.Kind() == reflect.Pointer && !isSpecialUnmarshalType(rv.Type()) {
			elemType := rv.Type().Elem()
			if rv.IsNil() {
				rv.Set(reflect.New(elemType))
			}
			rv = rv.Elem()
		}
		rt = rv.Type()
	}

	switch rt {
	case nodeType:
		rv.Set(reflect.ValueOf(c.node.Clone()))
		return nil
	case valueType:
		rv.Set(reflect.ValueOf(c.node.value.Clone()))
		return nil
	case documentType:
		cp := c.node.Clone()
		if rootDoc != nil {
			rv.Set(reflect.ValueOf(NewDocument(cp, rootDoc.yamlVersion, rootDoc.explicitStart)))
		} else {
			rv.Set(reflect.ValueOf(NewSimpleDocument(cp)))
		}
		return nil
	case taggedType:
		if c.node.value.kind != ValueKindTagged || c.node.value.tagged == nil {
			return c.typeErr("tagged value")
		}
		cp := c.node.Clone()
		rv.Set(reflect.ValueOf(*cp.value.tagged))
		return nil
	case mappingType:
		if c.node.value.kind != ValueKindMapping {
			return c.typeErr("mapping")
		}
		cp := c.node.value.m.Clone()
		rv.Set(reflect.ValueOf(*cp))
		return nil
	case integerType:
		if c.node.value.kind != ValueKindInt {
			return c.typeErr("int")
		}
		rv.Set(reflect.ValueOf(c.node.value.i))
		return nil
	case bigIntType:
		if c.node.value.kind != ValueKindInt {
			return c.typeErr("big.Int")
		}
		rv.Set(reflect.ValueOf(*c.node.value.i.BigInt()))
		return nil
	}

	// Empty interface (*any / *interface{}): delegates to ToPlain semantics while preserving Path/Related provenance.
	if rv.Kind() == reflect.Interface {
		if rv.NumMethod() != 0 {
			return c.typeErr(rt.String())
		}
		val, err := c.unmarshalPlain()
		if err != nil {
			return err
		}
		if val == nil {
			rv.Set(reflect.Zero(rt))
		} else {
			rv.Set(reflect.ValueOf(val))
		}
		return nil
	}

	// All remaining target types reject ValueKindTagged closed with ErrorKindType (found: "tagged value").
	if c.node.value.kind == ValueKindTagged {
		return c.typeErr(expectedNameForType(rt))
	}

	switch rv.Kind() {
	case reflect.Bool:
		if c.node.value.kind != ValueKindBool {
			return c.typeErr("bool")
		}
		rv.SetBool(c.node.value.b)
		return nil

	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		if c.node.value.kind != ValueKindInt {
			return c.typeErr(expectedNameForType(rt))
		}
		v, ok := c.node.value.i.Int64()
		if !ok || rv.OverflowInt(v) {
			return c.outOfRangeErr(expectedNameForType(rt))
		}
		rv.SetInt(v)
		return nil

	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		if c.node.value.kind != ValueKindInt {
			return c.typeErr(expectedNameForType(rt))
		}
		u, ok := c.node.value.i.Uint64()
		if !ok || rv.OverflowUint(u) {
			return c.outOfRangeErr(expectedNameForType(rt))
		}
		rv.SetUint(u)
		return nil

	case reflect.Float32:
		switch c.node.value.kind {
		case ValueKindFloat:
			f := c.node.value.f
			if math.IsNaN(f) {
				rv.SetFloat(float64(math.Float32frombits(0x7fc00000)))
				return nil
			}
			r := float32(f)
			if math.IsInf(float64(r), 0) && !math.IsInf(f, 0) {
				return c.simpleErr(ErrorKindFloatOutOfRange, "float out of range for float32")
			}
			rv.SetFloat(float64(r))
			return nil
		case ValueKindInt:
			f, ok := c.node.value.i.Float64Exact()
			if !ok {
				return c.outOfRangeErr("float32")
			}
			r := float32(f)
			if math.IsInf(float64(r), 0) || math.IsNaN(float64(r)) || float64(r) != f {
				return c.outOfRangeErr("float32")
			}
			rv.SetFloat(float64(r))
			return nil
		default:
			return c.typeErr("float32")
		}

	case reflect.Float64:
		switch c.node.value.kind {
		case ValueKindFloat:
			rv.SetFloat(c.node.value.f)
			return nil
		case ValueKindInt:
			f, ok := c.node.value.i.Float64Exact()
			if !ok {
				return c.outOfRangeErr("float64")
			}
			rv.SetFloat(f)
			return nil
		default:
			return c.typeErr("float64")
		}

	case reflect.String:
		if c.node.value.kind != ValueKindString {
			return c.typeErr("string")
		}
		if !utf8.ValidString(c.node.value.s) {
			return c.invalidUtf8Err()
		}
		rv.SetString(c.node.value.s)
		return nil

	case reflect.Slice:
		if c.node.value.kind == ValueKindNull {
			rv.Set(reflect.Zero(rt))
			return nil
		}
		if c.node.value.kind != ValueKindSequence {
			return c.typeErr("sequence")
		}
		items := c.node.value.seq
		outSlice := reflect.MakeSlice(rt, len(items), len(items))
		for i := range items {
			childCtx := c.child(&items[i])
			if err := unmarshalInto(childCtx, outSlice.Index(i), nil); err != nil {
				return prependPath(err, PathIndex(i))
			}
		}
		rv.Set(outSlice)
		return nil

	case reflect.Array:
		wantLen := rt.Len()
		if c.node.value.kind != ValueKindSequence || len(c.node.value.seq) != wantLen {
			return c.typeErr(fmt.Sprintf("sequence of length %d", wantLen))
		}
		items := c.node.value.seq
		outArr := reflect.New(rt).Elem()
		for i := range items {
			childCtx := c.child(&items[i])
			if err := unmarshalInto(childCtx, outArr.Index(i), nil); err != nil {
				return prependPath(err, PathIndex(i))
			}
		}
		rv.Set(outArr)
		return nil

	case reflect.Map:
		if c.node.value.kind == ValueKindNull {
			rv.Set(reflect.Zero(rt))
			return nil
		}
		if c.node.value.kind != ValueKindMapping {
			return c.typeErr("mapping")
		}
		var entries []MappingEntry
		if c.node.value.m != nil {
			entries = c.node.value.m.entries
		}
		outMap := reflect.MakeMapWithSize(rt, len(entries))
		seenKeys := make(map[any]Node, len(entries))
		keyType := rt.Key()
		valType := rt.Elem()
		for i := range entries {
			entry := &entries[i]
			keyCtx := c.child(&entry.Key)
			kElem := reflect.New(keyType).Elem()
			if err := unmarshalInto(keyCtx, kElem, nil); err != nil {
				return prependPath(err, PathKey(entry.Key.value))
			}
			kIface := kElem.Interface()
			if !reflect.ValueOf(kIface).IsValid() && keyType.Kind() != reflect.Interface {
				return prependPath(keyCtx.simpleErr(ErrorKindInvalidKey, "invalid nil map key"), PathKey(entry.Key.value))
			}
			if kIface != nil && !reflect.TypeOf(kIface).Comparable() {
				return prependPath(keyCtx.simpleErr(ErrorKindInvalidKey, "map key is not comparable in Go"), PathKey(entry.Key.value))
			}
			if prevKeyNode, exists := seenKeys[kIface]; exists {
				dupErr := &Error{
					Kind:    ErrorKindDuplicateKey,
					Site:    keyCtx.site(),
					Message: "duplicate mapping key",
				}
				if firstSpan, ok := prevKeyNode.Span(); ok {
					dupErr.Related = append(dupErr.Related, Related{
						Role: RelatedRoleFirstDefinition,
						Span: firstSpan,
					})
				}
				return keyCtx.attachOrigin(dupErr)
			}
			seenKeys[kIface] = entry.Key

			valCtx := c.child(&entry.Value)
			vElem := reflect.New(valType).Elem()
			if err := unmarshalInto(valCtx, vElem, nil); err != nil {
				return prependPath(err, PathKey(entry.Key.value))
			}
			outMap.SetMapIndex(kElem, vElem)
		}
		rv.Set(outMap)
		return nil

	case reflect.Struct:
		if c.node.value.kind != ValueKindMapping {
			return c.typeErr("mapping")
		}
		fieldMap := structFieldIndices(rt)
		var entries []MappingEntry
		if c.node.value.m != nil {
			entries = c.node.value.m.entries
		}
		outStruct := reflect.New(rt).Elem()
		for i := range entries {
			entry := &entries[i]
			keyCtx := c.child(&entry.Key)
			if entry.Key.value.kind != ValueKindString {
				return prependPath(keyCtx.typeErr("string"), PathKey(entry.Key.value))
			}
			keyStr := entry.Key.value.s
			if !utf8.ValidString(keyStr) {
				return prependPath(keyCtx.invalidUtf8Err(), PathKey(entry.Key.value))
			}
			fieldIdx, ok := fieldMap[keyStr]
			if !ok {
				return keyCtx.unknownFieldErr(keyStr)
			}
			valCtx := c.child(&entry.Value)
			if err := unmarshalInto(valCtx, outStruct.Field(fieldIdx), nil); err != nil {
				return prependPath(err, PathKey(entry.Key.value))
			}
		}
		rv.Set(outStruct)
		return nil

	default:
		return c.typeErr(rt.String())
	}
}

func expectedNameForType(rt reflect.Type) string {
	switch rt.Kind() {
	case reflect.Bool:
		return "bool"
	case reflect.Int:
		return "int"
	case reflect.Int8:
		return "int8"
	case reflect.Int16:
		return "int16"
	case reflect.Int32:
		return "int32"
	case reflect.Int64:
		return "int64"
	case reflect.Uint:
		return "uint"
	case reflect.Uint8:
		return "uint8"
	case reflect.Uint16:
		return "uint16"
	case reflect.Uint32:
		return "uint32"
	case reflect.Uint64:
		return "uint64"
	case reflect.Uintptr:
		return "uintptr"
	case reflect.Float32:
		return "float32"
	case reflect.Float64:
		return "float64"
	case reflect.String:
		return "string"
	case reflect.Slice:
		return "sequence"
	case reflect.Array:
		return fmt.Sprintf("sequence of length %d", rt.Len())
	case reflect.Map, reflect.Struct:
		return "mapping"
	default:
		return rt.String()
	}
}

func structFieldIndices(rt reflect.Type) map[string]int {
	fields := make(map[string]int, rt.NumField())
	for i := 0; i < rt.NumField(); i++ {
		sf := rt.Field(i)
		if sf.PkgPath != "" {
			continue // unexported
		}
		name := sf.Name
		if tag, ok := sf.Tag.Lookup("yaml"); ok {
			if tag == "-" {
				continue
			}
			parts := strings.Split(tag, ",")
			if parts[0] != "" {
				name = parts[0]
			}
		}
		if _, exists := fields[name]; !exists {
			fields[name] = i
		}
	}
	return fields
}

func (c unmarshalCtx) unmarshalPlain() (any, *Error) {
	if err := c.depthGuard(); err != nil {
		return nil, err
	}
	switch c.node.value.kind {
	case ValueKindNull:
		return nil, nil
	case ValueKindBool:
		return c.node.value.b, nil
	case ValueKindInt:
		v, ok := c.node.value.i.Int64()
		if !ok {
			err := toPlainLoss(c.node, c.lastKnown, "integer outside int64")
			return nil, c.attachOrigin(err)
		}
		return v, nil
	case ValueKindFloat:
		return c.node.value.f, nil
	case ValueKindString:
		if !utf8.ValidString(c.node.value.s) {
			return nil, c.invalidUtf8Err()
		}
		return c.node.value.s, nil
	case ValueKindSequence:
		items := c.node.value.seq
		out := make([]any, len(items))
		for i := range items {
			childCtx := c.child(&items[i])
			val, err := childCtx.unmarshalPlain()
			if err != nil {
				return nil, prependPath(err, PathIndex(i))
			}
			out[i] = val
		}
		return out, nil
	case ValueKindMapping:
		var entries []MappingEntry
		if c.node.value.m != nil {
			entries = c.node.value.m.entries
		}
		out := make(map[string]any, len(entries))
		for i := range entries {
			entry := &entries[i]
			keyCtx := c.child(&entry.Key)
			if entry.Key.value.kind != ValueKindString {
				err := toPlainLoss(&entry.Key, keyCtx.lastKnown, "non-string key")
				return nil, prependPath(keyCtx.attachOrigin(err), PathKey(entry.Key.value))
			}
			if !utf8.ValidString(entry.Key.value.s) {
				return nil, prependPath(keyCtx.invalidUtf8Err(), PathKey(entry.Key.value))
			}
			valCtx := c.child(&entry.Value)
			val, err := valCtx.unmarshalPlain()
			if err != nil {
				return nil, prependPath(err, PathKey(entry.Key.value))
			}
			out[entry.Key.value.s] = val
		}
		return out, nil
	case ValueKindTagged:
		err := toPlainLoss(c.node, c.lastKnown, "tagged value")
		return nil, c.attachOrigin(err)
	default:
		return nil, errInternal("unknown ValueKind")
	}
}
