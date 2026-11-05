package yaml

import (
	"fmt"
	"strings"
	"unicode/utf8"

	"github.com/3leaps/yamlprims/yaml/internal/engine"
)

// Test instrumentation hooks for registration_admission_tests and merge_admission_tests.
var (
	registrationCopyHook   func()
	mergeCopyStartedHook   func(stored, capacity int)
	mergeInsertStartedHook func(stored, capacity int)
)

func placeEmptyRoot(root *Node, at Location) {
	if root.span.Kind != NodeSpanSource {
		return
	}
	old := root.span.Source
	isCollection := func(v Value) bool {
		return v.kind == ValueKindSequence || v.kind == ValueKindMapping
	}
	innerCollection := root.value.kind == ValueKindTagged &&
		root.value.tagged != nil &&
		isCollection(root.value.tagged.value.value)
	if old.Start != old.End || root.via != nil || isCollection(root.value) || innerCollection {
		return
	}
	root.span = SourceNodeSpan(Span{Start: at, End: at})
	if root.value.kind == ValueKindTagged && root.value.tagged != nil {
		if root.value.tagged.value.span == SourceNodeSpan(old) {
			root.value.tagged.value.span = SourceNodeSpan(Span{Start: at, End: at})
		}
	}
}

type built struct {
	node           Node
	count          uint64
	weight         uint64
	height         int
	mergeDirective bool
}

type started struct {
	isOpen bool
	leaf   built
	open   openFrame
}

type openKind uint8

const (
	openSeq openKind = iota
	openMap
)

type mergeDirectiveSlot struct {
	slot   int
	dspan  Span
	source Node
}

type pendingKey struct {
	node        Node
	count       uint64
	weight      uint64
	height      int
	keyEnd      Location
	kspan       Span
	keyUse      *Span
	isDirective bool
}

type openFrame struct {
	kind      openKind
	seqItems  []Node
	mapVal    *Mapping
	directive *mergeDirectiveSlot
	pending   *pendingKey

	anchor    int
	class     tagClass
	flow      bool
	span      Span
	tagStart  *Location
	count     uint64
	weight    uint64
	height    int
	lastStart Location
	lastEnd   Location
	floor     Location
}

type anchored struct {
	node   Node
	count  uint64
	weight uint64
	height int
}

type loader struct {
	input       string
	bom         int
	opts        Options
	eng         *engine.Engine
	hasPeeked   bool
	peekedEv    engine.Event
	peekedSpan  engine.EventSpan
	budget      budget
	anchors     map[int]anchored
	openAnchors map[int]struct{}
	path        []PathSegment
}

func newLoader(input string, bom int, opts Options) *loader {
	eOpts := engine.DefaultOptions()
	eOpts.Limits.MaxInputBytes = opts.Limits.MaxInputBytes
	eOpts.Limits.MaxDepth = opts.Limits.MaxDepth
	return &loader{
		input:       input,
		bom:         bom,
		opts:        opts,
		eng:         engine.New(input, bom, eOpts),
		budget:      newBudget(opts.Limits),
		anchors:     make(map[int]anchored),
		openAnchors: make(map[int]struct{}),
	}
}

func (l *loader) trimBreaks(end, floor Location) Location {
	floorIdx := floor.ByteOffset - l.bom
	j := end.ByteOffset - l.bom
	line := end.Line
	for j > floorIdx && (l.input[j-1] == '\n' || l.input[j-1] == '\r') {
		if l.input[j-1] == '\n' && j-1 > floorIdx && l.input[j-2] == '\r' {
			j -= 2
		} else {
			j--
		}
		line--
	}
	if j == end.ByteOffset-l.bom {
		return end
	}
	lineStart := 0
	if idx := strings.LastIndexAny(l.input[:j], "\n\r"); idx >= 0 {
		lineStart = idx + 1
	}
	column := utf8.RuneCountInString(l.input[lineStart:j]) + 1
	return Location{
		ByteOffset: j + l.bom,
		Line:       line,
		Column:     column,
	}
}

func (l *loader) skipIndentation(at, floor Location) Location {
	j := at.ByteOffset - l.bom
	floorIdx := floor.ByteOffset - l.bom
	k := j
	for k > floorIdx && l.input[k-1] == ' ' {
		k--
	}
	if k == j || k == floorIdx || (l.input[k-1] != '\n' && l.input[k-1] != '\r') {
		return at
	}
	return Location{
		ByteOffset: k + l.bom,
		Line:       at.Line,
		Column:     1,
	}
}

func (l *loader) blockScalarEnd(span Span) Location {
	s := span.Start.ByteOffset - l.bom
	e := span.End.ByteOffset - l.bom
	k := e
	for k > s && (l.input[k-1] == ' ' || l.input[k-1] == '\t' || l.input[k-1] == '\n' || l.input[k-1] == '\r') {
		k--
	}
	for k < e && l.input[k] != '\n' && l.input[k] != '\r' {
		k++
	}
	if k == e {
		return span.End
	}
	line := span.Start.Line
	column := span.Start.Column
	sub := l.input[s:k]
	for i := 0; i < len(sub); {
		b := sub[i]
		if b == '\n' {
			line++
			column = 1
			i++
			continue
		}
		if b == '\r' {
			if i+1 < len(sub) && sub[i+1] == '\n' {
				i += 2
			} else {
				i++
			}
			line++
			column = 1
			continue
		}
		_, sz := utf8.DecodeRuneInString(sub[i:])
		i += sz
		column++
	}
	return Location{
		ByteOffset: k + l.bom,
		Line:       line,
		Column:     column,
	}
}

func (l *loader) settleEmpty(n *Node, floor Location) {
	if n.span.Kind != NodeSpanSource {
		return
	}
	s := n.span.Source
	if s.Start != s.End || n.via != nil || s.Start.ByteOffset <= floor.ByteOffset {
		return
	}
	isCollection := func(v Value) bool {
		return v.kind == ValueKindMapping || v.kind == ValueKindSequence
	}
	if isCollection(n.value) ||
		(n.value.kind == ValueKindTagged && n.value.tagged != nil && isCollection(n.value.tagged.value.value)) {
		return
	}
	at := l.trimBreaks(l.skipIndentation(s.Start, floor), floor)
	if at == s.Start {
		return
	}
	n.span = SourceNodeSpan(Span{Start: at, End: at})
	if n.value.kind == ValueKindTagged && n.value.tagged != nil {
		if n.value.tagged.value.span == SourceNodeSpan(s) {
			n.value.tagged.value.span = SourceNodeSpan(Span{Start: at, End: at})
		}
	}
}

func (l *loader) next() (engine.Event, engine.EventSpan, *Error) {
	if l.hasPeeked {
		l.hasPeeked = false
		return l.peekedEv, l.peekedSpan, nil
	}
	ev, span, ok, ee := l.eng.Next()
	if ee != nil {
		return engine.Event{}, engine.EventSpan{}, fromEngineError(ee)
	}
	if !ok {
		return engine.Event{}, engine.EventSpan{}, errInternal("event stream ended early")
	}
	return ev, span, nil
}

func (l *loader) peek() (*engine.Event, *engine.EventSpan, *Error) {
	if !l.hasPeeked {
		ev, span, err := l.next()
		if err != nil {
			return nil, nil, err
		}
		l.peekedEv = ev
		l.peekedSpan = span
		l.hasPeeked = true
	}
	return &l.peekedEv, &l.peekedSpan, nil
}

func (l *loader) eof(span engine.EventSpan) Location {
	end := fromEngineLoc(span.Span.End)
	if end.ByteOffset < len(l.input)+l.bom {
		return fromEngineLoc(engine.ScanLocation([]byte(l.input), l.bom))
	}
	return end
}

func (l *loader) clonePath() []PathSegment {
	if len(l.path) == 0 {
		return nil
	}
	cp := make([]PathSegment, len(l.path))
	copy(cp, l.path)
	return cp
}

func (l *loader) errAt(kind ErrorKind, at Location, msg string) *Error {
	e := errSourceSimple(kind, at, msg)
	e.Path = l.clonePath()
	return e
}

func (l *loader) errLimitAt(k LimitKind, configured int, at Location) *Error {
	e := errLimit(k, configured, at)
	e.Path = l.clonePath()
	return e
}

func (l *loader) charge(count, weight uint64, at Location, expanded bool) *Error {
	if kind, configured, ok := l.budget.charge(count, weight, expanded); !ok {
		return l.errLimitAt(kind, configured, at)
	}
	return nil
}

func (l *loader) loadAll(single bool) ([]Document, *Error) {
	maxDocs := l.opts.Limits.MaxDocuments
	if single {
		maxDocs = 1
	}
	if ee := l.eng.SyntaxPass(); ee != nil {
		return nil, fromEngineError(ee)
	}
	ev, _, err := l.next()
	if err != nil {
		return nil, err
	}
	if ev.Kind != engine.EventStreamStart {
		return nil, errInternal("missing stream start")
	}
	var docs []Document
	for {
		ev, evSpan, err := l.next()
		if err != nil {
			return nil, err
		}
		switch ev.Kind {
		case engine.EventStreamEnd:
			if len(docs) == 0 && single {
				end := l.eof(evSpan)
				docs = append(docs, newDocumentOwned(
					newNodeOwned(NullValue(), Span{Start: end, End: end}),
					nil,
					false,
				))
			}
			return docs, nil

		case engine.EventDocumentStart:
			at := fromEngineLoc(evSpan.Span.Start)
			if len(docs) >= maxDocs {
				if single {
					return nil, errSourceSimple(ErrorKindMultipleDocuments, at, "more than one document")
				}
				return nil, errLimit(LimitMaxDocuments, maxDocs, at)
			}
			if ev.Version != nil && ev.Version.Major != 1 {
				return nil, errSourceSimple(ErrorKindUnsupportedYamlVersion, at, "unsupported YAML version")
			}
			clear(l.anchors)
			clear(l.openAnchors)
			rootBuilt, err := l.node(0)
			if err != nil {
				return nil, err
			}
			root := rootBuilt.node
			endEv, endSpan, err := l.next()
			if err != nil {
				return nil, err
			}
			if endEv.Kind != engine.EventDocumentEnd {
				return nil, errInternal("missing document end")
			}
			var endMarker *Location
			if endSpan.Span.Start.ByteOffset < endSpan.Span.End.ByteOffset {
				loc := fromEngineLoc(endSpan.Span.Start)
				endMarker = &loc
			}
			if ev.Explicit {
				var boundary Location
				if endMarker != nil {
					boundary = *endMarker
				} else {
					pEv, pSpan, err := l.peek()
					if err != nil {
						return nil, err
					}
					if pEv.Kind == engine.EventStreamEnd {
						boundary = l.eof(*pSpan)
					} else {
						boundary = fromEngineLoc(pSpan.Span.Start)
					}
				}
				placeEmptyRoot(&root, boundary)
			}
			var ver *[2]uint32
			if ev.Version != nil {
				v := [2]uint32{ev.Version.Major, ev.Version.Minor}
				ver = &v
			}
			docs = append(docs, NewDocument(root, ver, ev.Explicit))

		default:
			return nil, errInternal("unexpected event between documents")
		}
	}
}

func (l *loader) node(base int) (built, *Error) {
	var stack []openFrame
	var curBuilt *built
	for {
		if curBuilt != nil {
			b := *curBuilt
			curBuilt = nil
			if len(stack) == 0 {
				return b, nil
			}
			if err := l.accept(&stack[len(stack)-1], b); err != nil {
				return built{}, err
			}
		}
		if len(stack) > 0 {
			top := &stack[len(stack)-1]
			ends := false
			switch top.kind {
			case openSeq:
				pEv, _, err := l.peek()
				if err != nil {
					return built{}, err
				}
				ends = pEv.Kind == engine.EventSequenceEnd
			case openMap:
				if top.pending == nil {
					pEv, _, err := l.peek()
					if err != nil {
						return built{}, err
					}
					ends = pEv.Kind == engine.EventMappingEnd
				}
			}
			if ends {
				o := stack[len(stack)-1]
				stack = stack[:len(stack)-1]
				closed, err := l.close(o)
				if err != nil {
					return built{}, err
				}
				curBuilt = &closed
				continue
			}
			if top.kind == openSeq {
				l.path = append(l.path, PathIndex(len(top.seqItems)))
			}
		}
		st, err := l.start(base + len(stack))
		if err != nil {
			return built{}, err
		}
		if st.isOpen {
			stack = append(stack, st.open)
		} else {
			b := st.leaf
			curBuilt = &b
		}
	}
}

func (l *loader) start(depth int) (started, *Error) {
	ev, gspan, err := l.next()
	if err != nil {
		return started{}, err
	}
	span := fromEngineSpan(gspan.Span)
	if ev.Kind == engine.EventScalar && (ev.ScalarStyle == engine.ScalarStyleLiteral || ev.ScalarStyle == engine.ScalarStyleFolded) {
		span.End = l.blockScalarEnd(span)
	}
	var tagStart *Location
	if gspan.TagStart != nil {
		loc := fromEngineLoc(*gspan.TagStart)
		tagStart = &loc
	}
	seq := ev.Kind == engine.EventSequenceStart

	switch ev.Kind {
	case engine.EventScalar:
		text := ev.Value
		style := ev.ScalarStyle
		anchor := ev.AnchorID
		if len(text) > l.opts.Limits.MaxScalarBytes {
			return started{}, l.errLimitAt(LimitMaxScalarBytes, l.opts.Limits.MaxScalarBytes, span.Start)
		}
		class := classifyEngineTag(ev.Tag)
		tagLoc := span.Start
		if tagStart != nil {
			tagLoc = *tagStart
		}
		if class.kind == tagClassOther && l.opts.UnknownTags == UnknownTagsReject {
			return started{}, l.errAt(ErrorKindUnknownTag, tagLoc, "unknown tag")
		}
		mergeDirective := false
		switch class.kind {
		case tagClassNone:
			mergeDirective = style == engine.ScalarStylePlain && text == "<<"
		case tagClassMerge:
			mergeDirective = text == "<<"
		}
		mergeTaggedOther := class.kind == tagClassMerge && text != "<<"
		resolved, ek := resolveScalarValue(text, style, class)
		if ek != "" {
			msg := fmt.Sprintf("cannot resolve scalar: %s", ek.Name())
			return started{}, l.errAt(ek, tagLoc, msg)
		}
		var stringBytes uint64
		if resolved.value.kind == ValueKindString {
			stringBytes = uint64(len(resolved.value.s))
		}
		var val Value
		var tBytes uint64
		var nodes uint64
		if !resolved.isTagged {
			val = resolved.value
			tBytes = 0
			nodes = 1
		} else {
			tb := tagBytes(resolved.tag)
			inner := newNodeOwned(resolved.value, span)
			innerPlain := style == engine.ScalarStylePlain
			val = taggedValueOwned(newTaggedWithPlain(resolved.tag, inner, innerPlain))
			tBytes = tb + NodeOverhead
			nodes = 2
		}
		weight := NodeOverhead + stringBytes + tBytes
		if err := l.charge(nodes, weight, span.Start, false); err != nil {
			return started{}, err
		}
		n := newNodeOwned(val, span)
		if tagStart != nil {
			n = n.WithTagStart(*tagStart)
		}
		b := built{
			node:           n,
			count:          nodes,
			weight:         weight,
			height:         0,
			mergeDirective: mergeDirective,
		}
		if mergeTaggedOther {
			b.mergeDirective = false
			b.node = b.node.WithTagStart(tagLoc)
		}
		if err := l.register(anchor, &b); err != nil {
			return started{}, err
		}
		return started{leaf: b}, nil

	case engine.EventAlias:
		id := ev.AnchorID
		if kind, configured, ok := l.budget.alias(); !ok {
			return started{}, l.errLimitAt(kind, configured, span.Start)
		}
		if _, open := l.openAnchors[id]; open {
			return started{}, l.errAt(ErrorKindRecursiveAlias, span.Start, "alias refers to an enclosing node")
		}
		a, ok := l.anchors[id]
		if !ok {
			return started{}, l.errAt(ErrorKindUnknownAnchor, span.Start, "unknown anchor")
		}
		if depth+a.height > l.opts.Limits.MaxDepth {
			defSpan := span
			if s, ok := a.node.Span(); ok {
				defSpan = s
			}
			return started{}, l.errLimitAt(LimitMaxDepth, l.opts.Limits.MaxDepth, span.Start).
				WithRelated(RelatedRoleAnchorDefinition, defSpan)
		}
		if err := l.charge(a.count, a.weight, span.Start, true); err != nil {
			return started{}, err
		}
		cloned := a.node.Clone().WithVia(ViaAlias(span))
		return started{
			leaf: built{
				node:   cloned,
				count:  a.count,
				weight: a.weight,
				height: a.height,
			},
		}, nil

	case engine.EventSequenceStart, engine.EventMappingStart:
		if err := l.collectionGuard(depth, span.Start); err != nil {
			return started{}, err
		}
		anchor := ev.AnchorID
		if anchor != 0 {
			l.openAnchors[anchor] = struct{}{}
		}
		class := classifyEngineTag(ev.Tag)
		kindName := "map"
		if seq {
			kindName = "seq"
		}
		tagLoc := span.Start
		if tagStart != nil {
			tagLoc = *tagStart
		}
		wrapCount, wrapWeight, err := l.collectionTag(class, kindName, tagLoc)
		if err != nil {
			return started{}, err
		}
		count := 1 + wrapCount
		weight := NodeOverhead + wrapWeight
		if err := l.charge(count, weight, span.Start, false); err != nil {
			return started{}, err
		}
		of := openFrame{
			anchor:    anchor,
			class:     class,
			flow:      ev.StructureStyle == engine.StructureStyleFlow,
			span:      span,
			tagStart:  tagStart,
			count:     count,
			weight:    weight,
			height:    0,
			lastStart: span.Start,
			lastEnd:   span.End,
			floor:     span.Start,
		}
		if seq {
			of.kind = openSeq
		} else {
			of.kind = openMap
			of.mapVal = NewMapping()
		}
		return started{isOpen: true, open: of}, nil

	default:
		return started{}, errInternal("unexpected event in node position")
	}
}

func (l *loader) accept(o *openFrame, b built) *Error {
	span := o.span
	switch o.kind {
	case openSeq:
		if len(l.path) > 0 {
			l.path = l.path[:len(l.path)-1]
		}
		l.settleEmpty(&b.node, o.floor)
		if s, ok := b.node.positionSpan(); ok {
			o.lastStart = s.Start
			o.lastEnd = s.End
			o.floor = s.End
		}
		o.count = satAdd64(o.count, b.count)
		o.weight = satAdd64(o.weight, b.weight)
		if b.height > o.height {
			o.height = b.height
		}
		o.seqItems = append(o.seqItems, b.node)
		return nil

	case openMap:
		if o.pending == nil {
			k := b
			l.settleEmpty(&k.node, o.floor)
			keyEnd := o.floor
			if s, ok := k.node.positionSpan(); ok {
				keyEnd = s.End
			}
			kspan := span
			if s, ok := k.node.Span(); ok {
				kspan = s
			}
			keyUse := aliasUse(&k.node)
			if k.weight > uint64(l.opts.Limits.MaxKeyBytes) {
				return withAliasUse(
					l.errLimitAt(LimitMaxKeyBytes, l.opts.Limits.MaxKeyBytes, kspan.Start),
					keyUse,
				)
			}
			if containsNaN(k.node.value) {
				return withAliasUse(
					l.errAt(ErrorKindInvalidKey, kspan.Start, "NaN inside a mapping key"),
					keyUse,
				)
			}
			if !k.mergeDirective && k.node.value.kind == ValueKindTagged && k.node.value.tagged != nil {
				if k.node.value.tagged.tag.uri == yamlNamespacePrefix+"merge" {
					at := kspan.Start
					if k.node.tagStart != nil {
						at = *k.node.tagStart
					}
					return withAliasUse(
						l.errAt(ErrorKindTagValueMismatch, at, "!!merge key must be <<"),
						keyUse,
					)
				}
			}
			isDirective := k.mergeDirective
			if isDirective && l.opts.MergeKeys == MergeKeysReject {
				return withAliasUse(
					l.errAt(ErrorKindMergeRejected, kspan.Start, "merge keys are rejected (merge_keys = Reject)"),
					keyUse,
				)
			}
			keyNode := k.node
			if isDirective {
				keyNode.value = StringValue("<<")
			}
			l.path = append(l.path, PathKey(keyNode.value.Clone()))
			o.pending = &pendingKey{
				node:        keyNode,
				count:       k.count,
				weight:      k.weight,
				height:      k.height,
				keyEnd:      keyEnd,
				kspan:       kspan,
				keyUse:      keyUse,
				isDirective: isDirective,
			}
			return nil
		}

		k := *o.pending
		o.pending = nil
		v := b
		if len(l.path) > 0 {
			l.path = l.path[:len(l.path)-1]
		}
		l.settleEmpty(&v.node, k.keyEnd)
		if s, ok := v.node.positionSpan(); ok {
			o.lastStart = s.Start
			o.lastEnd = s.End
			o.floor = s.End
		}
		if err := l.charge(0, EntryOverhead, k.kspan.Start, false); err != nil {
			return err
		}
		if k.isDirective && l.opts.MergeKeys == MergeKeysApply {
			if o.directive != nil {
				return withAliasUse(
					l.errAt(ErrorKindDuplicateKey, k.kspan.Start, "second merge directive in one mapping").
						WithRelated(RelatedRoleFirstDefinition, o.directive.dspan),
					k.keyUse,
				)
			}
			o.directive = &mergeDirectiveSlot{
				slot:   o.mapVal.Len(),
				dspan:  k.kspan,
				source: v.node,
			}
			return nil
		}
		o.count = satAdd64(o.count, satAdd64(k.count, v.count))
		o.weight = satAdd64(o.weight, satAdd64(satAdd64(k.weight, v.weight), EntryOverhead))
		if k.height > o.height {
			o.height = k.height
		}
		if v.height > o.height {
			o.height = v.height
		}
		if o.mapVal.Len() >= l.opts.Limits.MaxMappingEntries {
			return withAliasUse(
				l.errLimitAt(LimitMaxMappingEntries, l.opts.Limits.MaxMappingEntries, k.kspan.Start),
				k.keyUse,
			)
		}
		if insErr := o.mapVal.insertNewOwned(k.node, v.node); insErr != nil {
			if insErr.Kind != InsertErrorDuplicate {
				return withAliasUse(
					l.errAt(ErrorKindInvalidKey, k.kspan.Start, "NaN inside a mapping key"),
					k.keyUse,
				)
			}
			e := withAliasUse(
				l.errAt(ErrorKindDuplicateKey, k.kspan.Start, "duplicate mapping key"),
				k.keyUse,
			)
			if firstSpan, ok := insErr.Existing.Span(); ok {
				e = e.WithRelated(RelatedRoleFirstDefinition, firstSpan)
			}
			return e
		}
		return nil
	}
	return nil
}

func (l *loader) close(o openFrame) (built, *Error) {
	_, endSpan, err := l.next()
	if err != nil {
		return built{}, err
	}
	var end Location
	if o.flow {
		end = fromEngineLoc(endSpan.Span.End)
	} else {
		end = l.trimBreaks(o.lastEnd, o.lastStart)
	}
	full := Span{Start: o.span.Start, End: end}
	count := o.count
	weight := o.weight
	height := o.height

	var val Value
	switch o.kind {
	case openSeq:
		val = sequenceValueOwned(o.seqItems)
	case openMap:
		m := o.mapVal
		if o.directive != nil {
			merged, addCount, addWeight, addHeight, err := l.applyMerge(m, o.directive.slot, o.directive.dspan, o.directive.source)
			if err != nil {
				return built{}, err
			}
			m = merged
			count = satAdd64(count, addCount)
			weight = satAdd64(weight, addWeight)
			if addHeight > height {
				height = addHeight
			}
		}
		val = mappingValueOwned(m)
	}

	val = wrapCollection(val, o.class, full)
	n := newNodeOwned(val, full)
	if o.tagStart != nil {
		n = n.WithTagStart(*o.tagStart)
	}
	b := built{
		node:           n,
		count:          count,
		weight:         weight,
		height:         height + 1,
		mergeDirective: false,
	}
	if o.anchor != 0 {
		delete(l.openAnchors, o.anchor)
	}
	if err := l.register(o.anchor, &b); err != nil {
		return built{}, err
	}
	return b, nil
}

func (l *loader) collectionGuard(depth int, at Location) *Error {
	if depth+1 > l.opts.Limits.MaxDepth {
		return l.errLimitAt(LimitMaxDepth, l.opts.Limits.MaxDepth, at)
	}
	return nil
}

func (l *loader) register(anchor int, b *built) *Error {
	if anchor != 0 {
		at := Location{ByteOffset: 0, Line: 1, Column: 1}
		if s, ok := b.node.Span(); ok {
			at = s.Start
		}
		if err := l.charge(0, b.weight, at, false); err != nil {
			return err
		}
		if registrationCopyHook != nil {
			registrationCopyHook()
		}
		l.anchors[anchor] = anchored{
			node:   b.node.Clone(),
			count:  b.count,
			weight: b.weight,
			height: b.height,
		}
	}
	return nil
}

func (l *loader) collectionTag(class tagClass, kind string, at Location) (uint64, uint64, *Error) {
	switch class.kind {
	case tagClassNone, tagClassNonSpecific:
		return 0, 0, nil
	case tagClassCore:
		if class.coreName == kind {
			return 0, 0, nil
		}
		return 0, 0, l.errAt(ErrorKindTagKindMismatch, at, "core tag does not match node kind")
	case tagClassMerge:
		return 0, 0, l.errAt(ErrorKindTagKindMismatch, at, "!!merge on a collection")
	case tagClassOther:
		if l.opts.UnknownTags == UnknownTagsReject {
			return 0, 0, l.errAt(ErrorKindUnknownTag, at, "unknown tag")
		}
		return 1, NodeOverhead + tagBytes(class.other), nil
	default:
		return 0, 0, errInternal("unexpected tag class")
	}
}

func (l *loader) applyMerge(
	explicit *Mapping,
	slot int,
	dspan Span,
	source Node,
) (*Mapping, uint64, uint64, int, *Error) {
	srcSpan := dspan
	if s, ok := source.Span(); ok {
		srcSpan = s
	}
	srcUse := aliasUse(&source)
	var sources []*Mapping
	switch source.value.kind {
	case ValueKindMapping:
		sources = []*Mapping{source.value.m}
	case ValueKindSequence:
		items := source.value.seq
		sources = make([]*Mapping, 0, len(items))
		for i := range items {
			it := &items[i]
			if it.value.kind == ValueKindMapping {
				sources = append(sources, it.value.m)
			} else {
				at := srcSpan.Start
				if s, ok := it.Span(); ok {
					at = s.Start
				}
				e := l.errAt(ErrorKindMergeSource, at, "merge source must be a mapping")
				return nil, 0, 0, 0, withAliasUse(withAliasUse(e, aliasUse(it)), srcUse).
					WithRelated(RelatedRoleMergeDirective, dspan)
			}
		}
	default:
		e := l.errAt(ErrorKindMergeSource, srcSpan.Start, "merge source must be a mapping or a sequence of mappings")
		return nil, 0, 0, 0, withAliasUse(e, srcUse).WithRelated(RelatedRoleMergeDirective, dspan)
	}

	merged := NewMapping()
	capacity := l.opts.Limits.MaxMappingEntries - explicit.Len()
	if capacity < 0 {
		capacity = 0
	}
	var count, weight uint64
	var height int
	for _, m := range sources {
		if m == nil {
			continue
		}
		for i := range m.entries {
			k := m.entries[i].Key
			v := m.entries[i].Value
			if explicit.ContainsValue(k.value) || merged.ContainsValue(k.value) {
				continue
			}
			if merged.Len() >= capacity {
				return nil, 0, 0, 0, l.errLimitAt(LimitMaxMappingEntries, l.opts.Limits.MaxMappingEntries, dspan.Start).
					WithRelated(RelatedRoleMergeDirective, dspan)
			}
			if mergeCopyStartedHook != nil {
				mergeCopyStartedHook(merged.Len(), capacity)
			}
			kc, kw, kh := metrics(k)
			vc, vw, vh := metrics(v)
			if err := l.charge(satAdd64(kc, vc), satAdd64(satAdd64(kw, vw), EntryOverhead), dspan.Start, true); err != nil {
				return nil, 0, 0, 0, err.WithRelated(RelatedRoleMergeDirective, dspan)
			}
			count = satAdd64(count, satAdd64(kc, vc))
			weight = satAdd64(weight, satAdd64(satAdd64(kw, vw), EntryOverhead))
			if kh > height {
				height = kh
			}
			if vh > height {
				height = vh
			}
			via := ViaMerge(dspan)
			k = k.Clone().WithVia(via)
			v = v.Clone().WithVia(via)
			if mergeInsertStartedHook != nil {
				mergeInsertStartedHook(merged.Len(), capacity)
			}
			if insErr := merged.insertNewOwned(k, v); insErr != nil {
				return nil, 0, 0, 0, errInternal("merge dup")
			}
		}
	}

	out := NewMappingWithCapacity(explicit.Len() + merged.Len())
	if slot > explicit.Len() {
		return nil, 0, 0, 0, errInternal("merge slot")
	}
	for i := 0; i < slot; i++ {
		if insErr := out.insertNewOwned(explicit.entries[i].Key, explicit.entries[i].Value); insErr != nil {
			return nil, 0, 0, 0, errInternal("merge explicit dup")
		}
	}
	for i := range merged.entries {
		if insErr := out.insertNewOwned(merged.entries[i].Key, merged.entries[i].Value); insErr != nil {
			return nil, 0, 0, 0, errInternal("merge dup")
		}
	}
	for i := slot; i < len(explicit.entries); i++ {
		if insErr := out.insertNewOwned(explicit.entries[i].Key, explicit.entries[i].Value); insErr != nil {
			return nil, 0, 0, 0, errInternal("merge explicit dup")
		}
	}
	return out, count, weight, height, nil
}

func wrapCollection(v Value, class tagClass, span Span) Value {
	if class.kind == tagClassOther {
		inner := newNodeOwned(v, span)
		return taggedValueOwned(newTaggedWithPlain(class.other, inner, false))
	}
	return v
}

func aliasUse(n *Node) *Span {
	if n.via != nil && n.via.Kind == ViaKindAlias {
		s := n.via.Span
		return &s
	}
	return nil
}

func withAliasUse(e *Error, useSite *Span) *Error {
	if useSite != nil {
		return e.WithRelated(RelatedRoleAliasUse, *useSite)
	}
	return e
}
