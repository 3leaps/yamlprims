package yaml

import (
	"bytes"
	"fmt"
	"math"
	"reflect"
	"sort"
	"strings"
	"unicode/utf8"
)

func resolveEmitOpts(opts []EmitOptions) EmitOptions {
	if len(opts) > 0 {
		return opts[0]
	}
	return DefaultEmitOptions()
}

// FromPlain converts a Go host value into a synthetic Node under EmitOptions (spec-v1.5 §11.3).
func FromPlain(v any, opts ...EmitOptions) (Node, *Error) {
	o := resolveEmitOpts(opts)
	if err := o.Validate(); err != nil {
		return Node{}, err
	}
	nodesUsed := 0
	return hostToNode(v, o, &nodesUsed, 0, false)
}

// Marshal converts a Go host value into canonical YAML 1.2.2 Core text under EmitOptions (spec-v1.5 §11.3).
func Marshal(v any, opts ...EmitOptions) (string, *Error) {
	o := resolveEmitOpts(opts)
	node, err := FromPlain(v, o)
	if err != nil {
		return "", err
	}
	return EmitWith(node, o)
}

// MarshalWith converts a Go host value into canonical YAML 1.2.2 Core text under opts (spec-v1.5 §11.3).
func MarshalWith(v any, opts EmitOptions) (string, *Error) {
	return Marshal(v, opts)
}

func chargeMarshalNode(nodesUsed *int, opts EmitOptions) *Error {
	*nodesUsed++
	if *nodesUsed > opts.Limits.MaxNodes {
		return emitLimitErr(LimitMaxNodes, opts.Limits.MaxNodes, "node count exceeds max_nodes")
	}
	return nil
}

func copyExistingNodeForMarshal(root Node, opts EmitOptions, nodesUsed *int, startDepth int, startInKey bool) (Node, *Error) {
	const (
		copyOpVisit uint8 = iota
		copyOpSeqStep
		copyOpMapValStep
		copyOpMapInsertStep
		copyOpTaggedDone
	)
	type copyFrame struct {
		op         uint8
		node       *Node
		depth      int
		inKey      bool
		isKeyRoot  bool
		srcSeq     []Node
		outSeq     []Node
		srcEntries []MappingEntry
		outMap     *Mapping
		idx        int
		tag        Tag
	}

	stack := []copyFrame{{
		op:        copyOpVisit,
		node:      &root,
		depth:     startDepth,
		inKey:     startInKey,
		isKeyRoot: false,
	}}
	var results []Node

	cappedCap := func(n int) int {
		rem := opts.Limits.MaxNodes - *nodesUsed
		if rem < 0 {
			rem = 0
		}
		if n > rem {
			n = rem
		}
		if n > 1024 {
			n = 1024
		}
		return n
	}

	for len(stack) > 0 {
		f := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		switch f.op {
		case copyOpVisit:
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			if f.isKeyRoot {
				if err := checkEmitKeyWeight(f.node, opts.Limits.MaxKeyBytes); err != nil {
					return Node{}, err
				}
			}
			v := &f.node.value
			switch v.kind {
			case ValueKindNull:
				results = append(results, NewSyntheticNode(NullValue()))
			case ValueKindBool:
				results = append(results, NewSyntheticNode(BoolValue(v.b)))
			case ValueKindInt:
				results = append(results, NewSyntheticNode(IntValue(v.i)))
			case ValueKindFloat:
				if f.inKey && math.IsNaN(v.f) {
					return Node{}, emitSimpleErr(ErrorKindInvalidKey, "NaN float is not valid inside a mapping key")
				}
				results = append(results, NewSyntheticNode(FloatValue(v.f)))
			case ValueKindString:
				if !utf8.ValidString(v.s) {
					return Node{}, emitInvalidUtf8Err("string value is not valid UTF-8")
				}
				if len(v.s) > opts.Limits.MaxScalarBytes {
					return Node{}, emitLimitErr(LimitMaxScalarBytes, opts.Limits.MaxScalarBytes, "scalar byte length exceeds max_scalar_bytes")
				}
				results = append(results, NewSyntheticNode(StringValue(v.s)))
			case ValueKindTagged:
				if v.tagged == nil {
					return Node{}, emitSimpleErr(ErrorKindInvalidOptions, "nil Tagged payload")
				}
				t := v.tagged
				if !utf8.ValidString(t.tag.uri) || !utf8.ValidString(t.tag.handle) || !utf8.ValidString(t.tag.suffix) {
					return Node{}, emitInvalidUtf8Err("Tag contains invalid UTF-8")
				}
				tag := t.tag
				if tag.handle == "" && tag.suffix == "" && tag.uri != "" {
					tag.suffix = tag.uri
				}
				if len(tag.uri) > opts.Limits.MaxTagBytes || !isValidNonCoreTagURI(tag.uri) || !isValidTagSpelling(tag.uri, tag.handle, tag.suffix) {
					return Node{}, emitSimpleErr(ErrorKindInvalidOptions, "Tagged node contains invalid or oversized Tag.URI")
				}
				innerKind := t.value.value.kind
				if tag.uri == "tag:yaml.org,2002:merge" {
					if f.inKey || innerKind != ValueKindString {
						return Node{}, emitSimpleErr(ErrorKindTagKindMismatch, "tag:yaml.org,2002:merge may only wrap a non-key String node")
					}
				} else if innerKind != ValueKindString && innerKind != ValueKindSequence && innerKind != ValueKindMapping {
					return Node{}, emitSimpleErr(ErrorKindTagKindMismatch, "Tagged node must wrap String, Sequence, or Mapping")
				}
				stack = append(stack,
					copyFrame{op: copyOpTaggedDone, tag: tag},
					copyFrame{
						op:        copyOpVisit,
						node:      &t.value,
						depth:     f.depth,
						inKey:     f.inKey,
						isKeyRoot: false,
					},
				)
			case ValueKindSequence:
				nextDepth := f.depth + 1
				if nextDepth > opts.Limits.MaxDepth {
					return Node{}, emitLimitErr(LimitMaxDepth, opts.Limits.MaxDepth, "collection nesting exceeds max_depth")
				}
				if len(v.seq) == 0 {
					results = append(results, NewSyntheticNode(SequenceValue(nil)))
				} else {
					stack = append(stack,
						copyFrame{
							op:     copyOpSeqStep,
							srcSeq: v.seq,
							outSeq: make([]Node, 0, cappedCap(len(v.seq))),
							idx:    0,
							depth:  nextDepth,
							inKey:  f.inKey,
						},
						copyFrame{
							op:        copyOpVisit,
							node:      &v.seq[0],
							depth:     nextDepth,
							inKey:     f.inKey,
							isKeyRoot: false,
						},
					)
				}
			case ValueKindMapping:
				nextDepth := f.depth + 1
				if nextDepth > opts.Limits.MaxDepth {
					return Node{}, emitLimitErr(LimitMaxDepth, opts.Limits.MaxDepth, "collection nesting exceeds max_depth")
				}
				if v.m == nil || len(v.m.entries) == 0 {
					results = append(results, NewSyntheticNode(MappingValue(NewMapping())))
				} else {
					if len(v.m.entries) > opts.Limits.MaxMappingEntries {
						return Node{}, emitLimitErr(LimitMaxMappingEntries, opts.Limits.MaxMappingEntries, "mapping entry count exceeds max_mapping_entries")
					}
					stack = append(stack,
						copyFrame{
							op:         copyOpMapValStep,
							srcEntries: v.m.entries,
							outMap:     NewMappingWithCapacity(cappedCap(len(v.m.entries))),
							idx:        0,
							depth:      nextDepth,
							inKey:      f.inKey,
						},
						copyFrame{
							op:        copyOpVisit,
							node:      &v.m.entries[0].Key,
							depth:     nextDepth,
							inKey:     true,
							isKeyRoot: true,
						},
					)
				}
			}
		case copyOpSeqStep:
			child := results[len(results)-1]
			results = results[:len(results)-1]
			f.outSeq = append(f.outSeq, child)
			f.idx++
			if f.idx < len(f.srcSeq) {
				stack = append(stack,
					f,
					copyFrame{
						op:        copyOpVisit,
						node:      &f.srcSeq[f.idx],
						depth:     f.depth,
						inKey:     f.inKey,
						isKeyRoot: false,
					},
				)
			} else {
				results = append(results, NewSyntheticNode(Value{kind: ValueKindSequence, seq: f.outSeq}))
			}
		case copyOpMapValStep:
			f.op = copyOpMapInsertStep
			stack = append(stack,
				f,
				copyFrame{
					op:        copyOpVisit,
					node:      &f.srcEntries[f.idx].Value,
					depth:     f.depth,
					inKey:     f.inKey,
					isKeyRoot: false,
				},
			)
		case copyOpMapInsertStep:
			valNode := results[len(results)-1]
			keyNode := results[len(results)-2]
			results = results[:len(results)-2]
			if insErr := f.outMap.InsertNew(keyNode, valNode); insErr != nil {
				if insErr.Kind == InsertErrorInvalidKey {
					return Node{}, emitSimpleErr(ErrorKindInvalidKey, "NaN float is not valid inside a mapping key")
				}
				return Node{}, emitSimpleErr(ErrorKindDuplicateKey, "duplicate mapping key")
			}
			f.idx++
			if f.idx < len(f.srcEntries) {
				f.op = copyOpMapValStep
				stack = append(stack,
					f,
					copyFrame{
						op:        copyOpVisit,
						node:      &f.srcEntries[f.idx].Key,
						depth:     f.depth,
						inKey:     true,
						isKeyRoot: true,
					},
				)
			} else {
				results = append(results, NewSyntheticNode(MappingValue(f.outMap)))
			}
		case copyOpTaggedDone:
			inner := results[len(results)-1]
			results = results[:len(results)-1]
			results = append(results, NewSyntheticNode(TaggedValue(NewTagged(f.tag, inner))))
		}
	}
	return results[0], nil
}

func isZeroReflectValue(rv reflect.Value) bool {
	if !rv.IsValid() {
		return true
	}
	switch rv.Kind() {
	case reflect.Bool:
		return !rv.Bool()
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return rv.Int() == 0
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		return rv.Uint() == 0
	case reflect.Float32, reflect.Float64:
		return rv.Float() == 0
	case reflect.String, reflect.Slice, reflect.Map, reflect.Array:
		return rv.Len() == 0
	case reflect.Pointer, reflect.Interface:
		return rv.IsNil()
	case reflect.Struct:
		return rv.IsZero()
	}
	return false
}

func hostToNode(v any, opts EmitOptions, nodesUsed *int, depth int, inKey bool) (Node, *Error) {
	if v == nil {
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		return NewSyntheticNode(NullValue()), nil
	}

	switch tv := v.(type) {
	case Node:
		return copyExistingNodeForMarshal(tv, opts, nodesUsed, depth, inKey)
	case *Node:
		if tv == nil {
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			return NewSyntheticNode(NullValue()), nil
		}
		return copyExistingNodeForMarshal(*tv, opts, nodesUsed, depth, inKey)
	case Value:
		return copyExistingNodeForMarshal(NewSyntheticNode(tv), opts, nodesUsed, depth, inKey)
	case *Value:
		if tv == nil {
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			return NewSyntheticNode(NullValue()), nil
		}
		return copyExistingNodeForMarshal(NewSyntheticNode(*tv), opts, nodesUsed, depth, inKey)
	case Document:
		if tv.yamlVersion != nil {
			if tv.yamlVersion[0] != 1 || tv.yamlVersion[1] > 999_999_999 {
				return Node{}, emitSimpleErr(ErrorKindUnsupportedYamlVersion, "unsupported YAML version in Document")
			}
		}
		return copyExistingNodeForMarshal(tv.root, opts, nodesUsed, depth, inKey)
	case *Document:
		if tv == nil {
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			return NewSyntheticNode(NullValue()), nil
		}
		return hostToNode(*tv, opts, nodesUsed, depth, inKey)
	case Tagged:
		return copyExistingNodeForMarshal(NewSyntheticNode(TaggedValue(&tv)), opts, nodesUsed, depth, inKey)
	case *Tagged:
		if tv == nil {
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			return NewSyntheticNode(NullValue()), nil
		}
		return copyExistingNodeForMarshal(NewSyntheticNode(TaggedValue(tv)), opts, nodesUsed, depth, inKey)
	case Mapping:
		return copyExistingNodeForMarshal(NewSyntheticNode(Value{kind: ValueKindMapping, m: &tv}), opts, nodesUsed, depth, inKey)
	case *Mapping:
		if tv == nil {
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			return NewSyntheticNode(NullValue()), nil
		}
		return copyExistingNodeForMarshal(NewSyntheticNode(Value{kind: ValueKindMapping, m: tv}), opts, nodesUsed, depth, inKey)
	case Integer:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		return NewSyntheticNode(IntValue(tv)), nil
	case *Integer:
		if tv == nil {
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			return NewSyntheticNode(NullValue()), nil
		}
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		return NewSyntheticNode(IntValue(*tv)), nil
	}

	rv := reflect.ValueOf(v)
	return reflectToNode(rv, opts, nodesUsed, depth, inKey)
}

func reflectToNode(rv reflect.Value, opts EmitOptions, nodesUsed *int, depth int, inKey bool) (Node, *Error) {
	hops := 0
	for rv.IsValid() && (rv.Kind() == reflect.Pointer || rv.Kind() == reflect.Interface) {
		hops++
		if depth+hops > opts.Limits.MaxDepth+16 {
			return Node{}, emitLimitErr(LimitMaxDepth, opts.Limits.MaxDepth, "pointer/interface indirection exceeds max_depth")
		}
		if rv.IsNil() {
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			return NewSyntheticNode(NullValue()), nil
		}
		if rv.CanInterface() {
			switch rv.Interface().(type) {
			case Node, *Node, Value, *Value, Document, *Document, Tagged, *Tagged, Mapping, *Mapping, Integer, *Integer:
				return hostToNode(rv.Interface(), opts, nodesUsed, depth, inKey)
			}
		}
		rv = rv.Elem()
	}
	if !rv.IsValid() {
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		return NewSyntheticNode(NullValue()), nil
	}
	if rv.CanInterface() {
		switch rv.Interface().(type) {
		case Node, Value, Document, Tagged, Mapping, Integer:
			return hostToNode(rv.Interface(), opts, nodesUsed, depth, inKey)
		}
	}

	switch rv.Kind() {
	case reflect.Bool:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		return NewSyntheticNode(BoolValue(rv.Bool())), nil
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		return NewSyntheticNode(IntValue(IntegerFromInt64(rv.Int()))), nil
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		return NewSyntheticNode(IntValue(IntegerFromUint64(rv.Uint()))), nil
	case reflect.Float32, reflect.Float64:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		f := rv.Float()
		if inKey && math.IsNaN(f) {
			return Node{}, emitSimpleErr(ErrorKindInvalidKey, "NaN float is not valid inside a mapping key")
		}
		return NewSyntheticNode(FloatValue(f)), nil
	case reflect.String:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		s := rv.String()
		if !utf8.ValidString(s) {
			return Node{}, emitInvalidUtf8Err("string value is not valid UTF-8")
		}
		if len(s) > opts.Limits.MaxScalarBytes {
			return Node{}, emitLimitErr(LimitMaxScalarBytes, opts.Limits.MaxScalarBytes, "scalar byte length exceeds max_scalar_bytes")
		}
		return NewSyntheticNode(StringValue(s)), nil
	case reflect.Slice, reflect.Array:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		nextDepth := depth + 1
		if nextDepth > opts.Limits.MaxDepth {
			return Node{}, emitLimitErr(LimitMaxDepth, opts.Limits.MaxDepth, "collection nesting exceeds max_depth")
		}
		n := rv.Len()
		capHint := n
		remNodes := opts.Limits.MaxNodes - *nodesUsed
		if remNodes < 0 {
			remNodes = 0
		}
		if capHint > remNodes {
			capHint = remNodes
		}
		if capHint > 1024 {
			capHint = 1024
		}
		items := make([]Node, 0, capHint)
		for i := 0; i < n; i++ {
			elem, err := reflectToNode(rv.Index(i), opts, nodesUsed, nextDepth, inKey)
			if err != nil {
				return Node{}, err
			}
			items = append(items, elem)
		}
		return NewSyntheticNode(SequenceValue(items)), nil
	case reflect.Map:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		nextDepth := depth + 1
		if nextDepth > opts.Limits.MaxDepth {
			return Node{}, emitLimitErr(LimitMaxDepth, opts.Limits.MaxDepth, "collection nesting exceeds max_depth")
		}
		mapLen := rv.Len()
		if mapLen > opts.Limits.MaxMappingEntries {
			return Node{}, emitLimitErr(LimitMaxMappingEntries, opts.Limits.MaxMappingEntries, "mapping entry count exceeds max_mapping_entries")
		}
		type mapPair struct {
			keyNode Node
			valRV   reflect.Value
			sortKey string
		}
		capHint := mapLen
		remNodes := (opts.Limits.MaxNodes - *nodesUsed) / 2
		if remNodes < 0 {
			remNodes = 0
		}
		if capHint > remNodes {
			capHint = remNodes
		}
		if capHint > 1024 {
			capHint = 1024
		}
		pairs := make([]mapPair, 0, capHint)
		sortOpts := opts
		sortOpts.CompatQuoting = CompatQuotingCoreOnly
		sortOpts.Limits.MaxOutputBytes = int(^uint(0) >> 1)
		iter := rv.MapRange()
		for iter.Next() {
			kRV := iter.Key()
			kNode, err := reflectToNode(kRV, opts, nodesUsed, nextDepth, true)
			if err != nil {
				return Node{}, err
			}
			if err := checkEmitKeyWeight(&kNode, opts.Limits.MaxKeyBytes); err != nil {
				return Node{}, err
			}
			out := &outBuf{max: sortOpts.Limits.MaxOutputBytes}
			if err := emitRootNode(out, &kNode, CompatQuotingCoreOnly); err != nil {
				return Node{}, err
			}
			pairs = append(pairs, mapPair{
				keyNode: kNode,
				valRV:   iter.Value(),
				sortKey: string(out.buf),
			})
		}
		sort.Slice(pairs, func(i, j int) bool {
			return bytes.Compare([]byte(pairs[i].sortKey), []byte(pairs[j].sortKey)) < 0
		})
		m := NewMappingWithCapacity(len(pairs))
		for _, p := range pairs {
			vNode, err := reflectToNode(p.valRV, opts, nodesUsed, nextDepth, inKey)
			if err != nil {
				return Node{}, err
			}
			if insErr := m.InsertNew(p.keyNode, vNode); insErr != nil {
				if insErr.Kind == InsertErrorInvalidKey {
					return Node{}, emitSimpleErr(ErrorKindInvalidKey, "NaN float is not valid inside a mapping key")
				}
				return Node{}, emitSimpleErr(ErrorKindDuplicateKey, "duplicate mapping key")
			}
		}
		return NewSyntheticNode(MappingValue(m)), nil
	case reflect.Struct:
		if err := chargeMarshalNode(nodesUsed, opts); err != nil {
			return Node{}, err
		}
		nextDepth := depth + 1
		if nextDepth > opts.Limits.MaxDepth {
			return Node{}, emitLimitErr(LimitMaxDepth, opts.Limits.MaxDepth, "collection nesting exceeds max_depth")
		}
		rt := rv.Type()
		m := NewMapping()
		for i := 0; i < rt.NumField(); i++ {
			sf := rt.Field(i)
			if sf.PkgPath != "" {
				continue // unexported
			}
			name := sf.Name
			omitEmpty := false
			if tag, ok := sf.Tag.Lookup("yaml"); ok {
				if tag == "-" {
					continue
				}
				parts := strings.Split(tag, ",")
				if parts[0] != "" {
					name = parts[0]
				}
				for _, opt := range parts[1:] {
					if opt == "omitempty" {
						omitEmpty = true
					}
				}
			}
			fv := rv.Field(i)
			if omitEmpty && isZeroReflectValue(fv) {
				continue
			}
			if m.Len() >= opts.Limits.MaxMappingEntries {
				return Node{}, emitLimitErr(LimitMaxMappingEntries, opts.Limits.MaxMappingEntries, "mapping entry count exceeds max_mapping_entries")
			}
			if err := chargeMarshalNode(nodesUsed, opts); err != nil {
				return Node{}, err
			}
			if !utf8.ValidString(name) {
				return Node{}, emitInvalidUtf8Err("struct field name is not valid UTF-8")
			}
			if len(name) > opts.Limits.MaxScalarBytes {
				return Node{}, emitLimitErr(LimitMaxScalarBytes, opts.Limits.MaxScalarBytes, "scalar byte length exceeds max_scalar_bytes")
			}
			kNode := NewSyntheticNode(StringValue(name))
			if err := checkEmitKeyWeight(&kNode, opts.Limits.MaxKeyBytes); err != nil {
				return Node{}, err
			}
			vNode, err := reflectToNode(fv, opts, nodesUsed, nextDepth, inKey)
			if err != nil {
				return Node{}, err
			}
			if insErr := m.InsertNew(kNode, vNode); insErr != nil {
				return Node{}, emitSimpleErr(ErrorKindDuplicateKey, "duplicate struct field key")
			}
		}
		return NewSyntheticNode(MappingValue(m)), nil
	default:
		found := rv.Kind().String()
		return Node{}, errTypeAtSite(
			SiteNotSource(nil),
			"YAML value",
			found,
			fmt.Sprintf("unsupported Go type %s for YAML conversion", rv.Type().String()),
		)
	}
}
