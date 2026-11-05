package engine

import (
	"fmt"
	"math"
)

// EventKind identifies the type of a parser Event.
type EventKind uint8

const (
	EventStreamStart EventKind = iota + 1
	EventStreamEnd
	EventDocumentStart
	EventDocumentEnd
	EventAlias
	EventScalar
	EventSequenceStart
	EventSequenceEnd
	EventMappingStart
	EventMappingEnd
)

// Event is a YAML 1.2.2 parser event.
type Event struct {
	Kind           EventKind
	Explicit       bool
	Version        *Version
	AnchorID       int
	Value          string
	ScalarStyle    ScalarStyle
	StructureStyle StructureStyle
	Tag            *Tag
}

func (e Event) isNode() bool {
	switch e.Kind {
	case EventAlias, EventScalar, EventSequenceStart, EventMappingStart:
		return true
	default:
		return false
	}
}

func emptyScalar() Event {
	return Event{
		Kind:        EventScalar,
		Value:       "~",
		ScalarStyle: ScalarStylePlain,
	}
}

func emptyScalarWithAnchor(anchor int, tag *Tag) Event {
	val := "~"
	if tag != nil {
		val = ""
	}
	return Event{
		Kind:        EventScalar,
		Value:       val,
		ScalarStyle: ScalarStylePlain,
		AnchorID:    anchor,
		Tag:         tag,
	}
}

type parserState uint8

const (
	stateStreamStart parserState = iota
	stateImplicitDocumentStart
	stateDocumentStart
	stateDocumentContent
	stateDocumentEnd
	stateBlockNode
	stateBlockNodeOrIndentlessSequence
	stateFlowNode
	stateBlockSequenceFirstEntry
	stateBlockSequenceEntry
	stateIndentlessSequenceEntry
	stateIndentlessSequenceEntryNode
	stateBlockMappingFirstKey
	stateBlockMappingKey
	stateBlockMappingValue
	stateFlowSequenceFirstEntry
	stateFlowSequenceEntry
	stateFlowSequenceEntryMappingKey
	stateFlowSequenceEntryMappingValue
	stateFlowSequenceEntryMappingEnd
	stateFlowMappingFirstKey
	stateFlowMappingKey
	stateFlowMappingValue
	stateFlowMappingEmptyValue
	stateEnd
)

type parser struct {
	scanner                *scanner
	blockNestingLimit      int
	blockLevel             int
	states                 []parserState
	state                  parserState
	token                  *queuedToken
	currentEvent           *Event
	currentSpan            rawSpan
	currentError           *scanError
	pendingKeyIndent       int
	hasPendingKeyIndent    bool
	pendingEmptyScalarSpan *rawSpan
	lastNonCommentEventEnd *Marker
	anchors                map[string]int
	anchorIDCount          int
	tags                   map[string]string
	streamEndEmitted       bool
	keepTags               bool
}

func newParser(src string, nativeDepth int) *parser {
	return &parser{
		scanner:           newScanner(src, nativeDepth),
		blockNestingLimit: nativeDepth,
		states:            make([]parserState, 0, 16),
		state:             stateStreamStart,
		anchors:           make(map[string]int),
		anchorIDCount:     1,
		tags:              make(map[string]string),
	}
}

func (p *parser) nextEvent() (Event, rawSpan, bool, *scanError) {
	if p.currentError != nil {
		err := p.currentError
		p.currentError = nil
		p.streamEndEmitted = true
		return Event{}, rawSpan{}, true, err
	}
	if p.streamEndEmitted {
		return Event{}, rawSpan{}, false, nil
	}
	ev, span, err := p.nextEventImpl()
	if err != nil {
		p.streamEndEmitted = true
		return Event{}, rawSpan{}, true, err
	}
	if ev.Kind == EventStreamEnd {
		p.streamEndEmitted = true
	}
	return ev, span, true, nil
}

func (p *parser) nextEventImpl() (Event, rawSpan, *scanError) {
	if p.currentEvent != nil {
		ev := *p.currentEvent
		span := p.currentSpan
		p.currentEvent = nil
		ev, span = p.rememberNonCommentEventEnd(ev, span)
		return ev, span, nil
	}
	ev, span, err := p.parse()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	ev, span = p.rememberNonCommentEventEnd(ev, span)
	return ev, span, nil
}

func (p *parser) applyPendingKeyIndent(ev Event, span rawSpan) (Event, rawSpan) {
	if ev.isNode() && p.hasPendingKeyIndent {
		indent := p.pendingKeyIndent
		p.hasPendingKeyIndent = false
		return ev, span.withIndent(indent, true)
	}
	return ev, span
}

func (p *parser) rememberNonCommentEventEnd(ev Event, span rawSpan) (Event, rawSpan) {
	end := span.end
	p.lastNonCommentEventEnd = &end
	return ev, span
}

func (p *parser) peekToken() (*queuedToken, *scanError) {
	if p.token == nil {
		tok, err := p.scanNextToken()
		if err != nil {
			return nil, err
		}
		p.token = tok
	}
	return p.token, nil
}

func (p *parser) scanNextToken() (*queuedToken, *scanError) {
	tok, ok, err := p.scanner.nextQueuedToken()
	if err != nil {
		return nil, err
	}
	if !ok {
		panic("scanner ended before parser consumed stream-end token")
	}
	return tok, nil
}

func (p *parser) fetchToken() queuedToken {
	if p.token == nil {
		panic("fetchToken needs to be preceded by peekToken")
	}
	tok := *p.token
	p.token = nil
	return tok
}

func (p *parser) skip() {
	p.token = nil
}

func (p *parser) popState() {
	n := len(p.states)
	p.state = p.states[n-1]
	p.states = p.states[:n-1]
}

func (p *parser) pushState(state parserState) {
	p.states = append(p.states, state)
}

func (p *parser) startBlockCollection(mark Marker) *scanError {
	if p.blockLevel >= p.blockNestingLimit {
		return newScanError(mark, scanErrRecursionLimitExceeded)
	}
	p.blockLevel++
	return nil
}

func (p *parser) endBlockCollection() {
	p.blockLevel--
}

func (p *parser) deferParseNode(nodeState, returnState parserState, block, indentlessSequence bool) (Event, rawSpan, *scanError) {
	p.pushState(returnState)
	p.state = nodeState
	return p.parseNode(block, indentlessSequence)
}

func (p *parser) parse() (Event, rawSpan, *scanError) {
	if p.state == stateEnd {
		return Event{Kind: EventStreamEnd}, emptyRawSpan(p.scanner.mark), nil
	}
	ev, span, err := p.stateMachine()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	ev, span = p.applyPendingKeyIndent(ev, span)
	return ev, span, nil
}

func (p *parser) stateMachine() (Event, rawSpan, *scanError) {
	switch p.state {
	case stateStreamStart:
		return p.streamStart()
	case stateImplicitDocumentStart:
		return p.documentStart(true)
	case stateDocumentStart:
		return p.documentStart(false)
	case stateDocumentContent:
		return p.documentContent()
	case stateDocumentEnd:
		return p.documentEnd()
	case stateBlockNode:
		return p.parseNode(true, false)
	case stateBlockNodeOrIndentlessSequence:
		return p.parseNode(true, true)
	case stateFlowNode:
		return p.parseNode(false, false)
	case stateBlockMappingFirstKey:
		return p.blockMappingKey(true)
	case stateBlockMappingKey:
		return p.blockMappingKey(false)
	case stateBlockMappingValue:
		return p.blockMappingValue()
	case stateBlockSequenceFirstEntry:
		return p.blockSequenceEntry(true)
	case stateBlockSequenceEntry:
		return p.blockSequenceEntry(false)
	case stateFlowSequenceFirstEntry:
		return p.flowSequenceEntry(true)
	case stateFlowSequenceEntry:
		return p.flowSequenceEntry(false)
	case stateFlowMappingFirstKey:
		return p.flowMappingKey(true)
	case stateFlowMappingKey:
		return p.flowMappingKey(false)
	case stateFlowMappingValue:
		return p.flowMappingValue(false)
	case stateIndentlessSequenceEntry:
		return p.indentlessSequenceEntry()
	case stateIndentlessSequenceEntryNode:
		return p.indentlessSequenceEntryNode()
	case stateFlowSequenceEntryMappingKey:
		return p.flowSequenceEntryMappingKey()
	case stateFlowSequenceEntryMappingValue:
		return p.flowSequenceEntryMappingValue()
	case stateFlowSequenceEntryMappingEnd:
		return p.flowSequenceEntryMappingEnd()
	case stateFlowMappingEmptyValue:
		return p.flowMappingValue(true)
	default:
		panic("end state is handled before state-machine dispatch")
	}
}

func (p *parser) streamStart() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenStreamStart {
		span := tok.span
		p.state = stateImplicitDocumentStart
		p.skip()
		return Event{Kind: EventStreamStart}, span, nil
	}
	return Event{}, rawSpan{}, newScanError(tok.span.start, scanErrExpectedStreamStart)
}

func (p *parser) documentStart(implicit bool) (Event, rawSpan, *scanError) {
	for {
		tok, err := p.peekToken()
		if err != nil {
			return Event{}, rawSpan{}, err
		}
		if tok.kind != tokenDocumentEnd {
			break
		}
		p.skip()
	}
	clear(p.anchors)

	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch tok.kind {
	case tokenStreamEnd:
		span := tok.span
		p.state = stateEnd
		p.skip()
		return Event{Kind: EventStreamEnd}, span, nil
	case tokenVersionDirective, tokenTagDirective, tokenReservedDirective, tokenDocumentStart:
		return p.explicitDocumentStart()
	default:
		if implicit {
			span := tok.span
			if _, err := p.parserProcessDirectives(nil, false, make(map[string]struct{})); err != nil {
				return Event{}, rawSpan{}, err
			}
			p.pushState(stateDocumentEnd)
			p.state = stateBlockNode
			return Event{Kind: EventDocumentStart, Explicit: false, Version: nil}, span, nil
		}
		return p.explicitDocumentStart()
	}
}

func (p *parser) parserProcessDirectives(
	version *Version,
	continuing bool,
	documentTagHandles map[string]struct{},
) (*Version, *scanError) {
	var tags map[string]string
	if continuing || p.keepTags {
		tags = p.tags
		p.tags = make(map[string]string)
	} else {
		tags = make(map[string]string)
	}

	for {
		tok, err := p.peekToken()
		if err != nil {
			return nil, err
		}
		if tok.kind != tokenVersionDirective && tok.kind != tokenTagDirective && tok.kind != tokenReservedDirective {
			break
		}
		qt := p.fetchToken()
		switch qt.kind {
		case tokenVersionDirective:
			if version != nil {
				return nil, newScanError(qt.span.start, scanErrDuplicateVersionDirective)
			}
			if qt.major != 1 {
				return nil, newScanError(qt.span.start, scanErrUnsupportedYamlMajorVersion)
			}
			v := Version{Major: qt.major, Minor: qt.minor}
			version = &v
		case tokenTagDirective:
			if _, exists := documentTagHandles[qt.text1]; exists {
				return nil, newScanError(qt.span.start, scanErrDuplicateTagDirective)
			}
			documentTagHandles[qt.text1] = struct{}{}
			tags[qt.text1] = qt.text2
		case tokenReservedDirective:
			// Reserved directives are ignored per YAML 1.2.2.
		}
	}
	p.tags = tags
	return version, nil
}

func (p *parser) explicitDocumentStart() (Event, rawSpan, *scanError) {
	version, err := p.parserProcessDirectives(nil, false, make(map[string]struct{}))
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenDocumentStart {
		mark := tok.span
		p.pushState(stateDocumentEnd)
		p.state = stateDocumentContent
		p.skip()
		return Event{Kind: EventDocumentStart, Explicit: true, Version: version}, mark, nil
	}
	return Event{}, rawSpan{}, newScanError(tok.span.start, scanErrExpectedDocumentStart)
}

func (p *parser) documentContent() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch tok.kind {
	case tokenVersionDirective, tokenTagDirective, tokenReservedDirective,
		tokenDocumentStart, tokenDocumentEnd, tokenStreamEnd:
		p.popState()
		span := emptyRawSpan(tok.span.start)
		if p.lastNonCommentEventEnd != nil {
			span = emptyRawSpan(*p.lastNonCommentEventEnd)
		}
		return emptyScalar(), span, nil
	default:
		p.state = stateBlockNode
		return p.parseNode(true, false)
	}
}

func (p *parser) documentEnd() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	explicitEnd := false
	var span rawSpan
	if tok.kind == tokenDocumentEnd {
		explicitEnd = true
		span = tok.span
		p.skip()
	} else if p.lastNonCommentEventEnd != nil {
		span = emptyRawSpan(*p.lastNonCommentEventEnd)
	} else {
		span = emptyRawSpan(tok.span.start)
	}

	if p.keepTags {
		delete(p.tags, "!!")
		delete(p.tags, "!")
	} else {
		clear(p.tags)
	}

	if explicitEnd {
		p.state = stateImplicitDocumentStart
	} else {
		nextTok, err := p.peekToken()
		if err != nil {
			return Event{}, rawSpan{}, err
		}
		if nextTok.kind == tokenVersionDirective || nextTok.kind == tokenTagDirective || nextTok.kind == tokenReservedDirective {
			return Event{}, rawSpan{}, newScanError(nextTok.span.start, scanErrMissingDocumentEndBeforeDirective)
		}
		p.state = stateDocumentStart
	}
	return Event{Kind: EventDocumentEnd}, span, nil
}

func (p *parser) registerAnchor(name string, mark rawSpan) (int, *scanError) {
	newID := p.anchorIDCount
	if p.anchorIDCount == math.MaxInt {
		return 0, newScanError(mark.start, scanErrAnchorCountOverflow)
	}
	p.anchorIDCount++
	p.anchors[name] = newID
	return newID, nil
}

func (p *parser) parseNode(block, indentlessSequence bool) (Event, rawSpan, *scanError) {
	anchorID := 0
	var tag *Tag
	var tagStart *Marker
	var propertyEnd *Marker

	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch {
	case tok.kind == tokenAlias && anchorID == 0 && tag == nil:
		p.popState()
		qt := p.fetchToken()
		id, ok := p.anchors[qt.text1]
		if !ok {
			return Event{}, rawSpan{}, newScanError(qt.span.start, scanErrUnknownAnchor)
		}
		return Event{Kind: EventAlias, AnchorID: id}, qt.span, nil

	case tok.kind == tokenAnchor && anchorID == 0:
		qt := p.fetchToken()
		id, err := p.registerAnchor(qt.text1, qt.span)
		if err != nil {
			return Event{}, rawSpan{}, err
		}
		anchorID = id
		end := qt.span.end
		propertyEnd = &end

		nextTok, err := p.peekToken()
		if err != nil {
			return Event{}, rawSpan{}, err
		}
		if tag == nil && nextTok.kind == tokenTag {
			tagTok := p.fetchToken()
			ts := tagTok.span.start
			tagStart = &ts
			resolved, err := p.resolveTag(tagTok.span, tagTok.text1, tagTok.text2)
			if err != nil {
				return Event{}, rawSpan{}, err
			}
			tag = resolved
			te := tagTok.span.end
			propertyEnd = &te
		}

	case tok.kind == tokenTag && tag == nil:
		tagTok := p.fetchToken()
		ts := tagTok.span.start
		tagStart = &ts
		te := tagTok.span.end
		propertyEnd = &te
		resolved, err := p.resolveTag(tagTok.span, tagTok.text1, tagTok.text2)
		if err != nil {
			return Event{}, rawSpan{}, err
		}
		tag = resolved

		nextTok, err := p.peekToken()
		if err != nil {
			return Event{}, rawSpan{}, err
		}
		if anchorID == 0 && nextTok.kind == tokenAnchor {
			anchTok := p.fetchToken()
			id, err := p.registerAnchor(anchTok.text1, anchTok.span)
			if err != nil {
				return Event{}, rawSpan{}, err
			}
			anchorID = id
			ae := anchTok.span.end
			propertyEnd = &ae
		}
	}

	tok, err = p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch {
	case tok.kind == tokenBlockEntry && indentlessSequence:
		mark := tok.span
		if err := p.startBlockCollection(mark.start); err != nil {
			return Event{}, rawSpan{}, err
		}
		p.skip()
		cp := mark
		p.pendingEmptyScalarSpan = &cp
		p.state = stateIndentlessSequenceEntryNode
		return Event{
			Kind:           EventSequenceStart,
			StructureStyle: StructureStyleBlock,
			AnchorID:       anchorID,
			Tag:            tag,
		}, mark.withTagStart(tagStart), nil

	case tok.kind == tokenScalar:
		p.popState()
		qt := p.fetchToken()
		return Event{
			Kind:        EventScalar,
			Value:       qt.text1,
			ScalarStyle: qt.style,
			AnchorID:    anchorID,
			Tag:         tag,
		}, qt.span.withTagStart(tagStart), nil

	case tok.kind == tokenFlowSequenceStart:
		mark := tok.span
		p.state = stateFlowSequenceFirstEntry
		p.skip()
		return Event{
			Kind:           EventSequenceStart,
			StructureStyle: StructureStyleFlow,
			AnchorID:       anchorID,
			Tag:            tag,
		}, mark.withTagStart(tagStart), nil

	case tok.kind == tokenFlowMappingStart:
		mark := tok.span
		p.state = stateFlowMappingFirstKey
		p.skip()
		return Event{
			Kind:           EventMappingStart,
			StructureStyle: StructureStyleFlow,
			AnchorID:       anchorID,
			Tag:            tag,
		}, mark.withTagStart(tagStart), nil

	case tok.kind == tokenBlockSequenceStart && block:
		mark := tok.span
		if err := p.startBlockCollection(mark.start); err != nil {
			return Event{}, rawSpan{}, err
		}
		p.state = stateBlockSequenceFirstEntry
		p.skip()
		return Event{
			Kind:           EventSequenceStart,
			StructureStyle: StructureStyleBlock,
			AnchorID:       anchorID,
			Tag:            tag,
		}, mark.withTagStart(tagStart), nil

	case tok.kind == tokenBlockMappingStart && block:
		mark := tok.span
		if err := p.startBlockCollection(mark.start); err != nil {
			return Event{}, rawSpan{}, err
		}
		p.state = stateBlockMappingFirstKey
		p.skip()
		return Event{
			Kind:           EventMappingStart,
			StructureStyle: StructureStyleBlock,
			AnchorID:       anchorID,
			Tag:            tag,
		}, mark.withTagStart(tagStart), nil

	case tag != nil || anchorID > 0:
		p.popState()
		span := emptyRawSpan(tok.span.start)
		if propertyEnd != nil {
			span = emptyRawSpan(*propertyEnd)
		}
		return emptyScalarWithAnchor(anchorID, tag), span.withTagStart(tagStart), nil

	default:
		var kind scanErrKind
		switch p.state {
		case stateFlowSequenceFirstEntry, stateFlowSequenceEntry:
			kind = scanErrUnexpectedEofFlowSequence
		case stateFlowMappingFirstKey, stateFlowMappingKey, stateFlowMappingValue, stateFlowMappingEmptyValue:
			kind = scanErrUnexpectedEofFlowMapping
		case stateFlowSequenceEntryMappingKey, stateFlowSequenceEntryMappingValue, stateFlowSequenceEntryMappingEnd, stateFlowNode:
			kind = scanErrUnexpectedEofImplicitFlowMapping
		case stateBlockSequenceFirstEntry, stateBlockSequenceEntry, stateBlockNode:
			kind = scanErrUnexpectedEofBlockSequence
		case stateBlockMappingFirstKey, stateBlockMappingKey, stateBlockMappingValue, stateBlockNodeOrIndentlessSequence:
			kind = scanErrUnexpectedEofBlockMapping
		default:
			kind = scanErrExpectedNodeContent
		}
		return Event{}, rawSpan{}, newScanError(tok.span.start, kind)
	}
}

func (p *parser) blockMappingKey(_ bool) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch tok.kind {
	case tokenKey:
		p.pendingKeyIndent = tok.span.start.col
		p.hasPendingKeyIndent = true
		p.skip()
		return p.blockMappingKeyNode()
	case tokenValue:
		mark := tok.span
		p.state = stateBlockMappingValue
		return emptyScalar(), emptyRawSpan(mark.start), nil
	case tokenBlockEnd:
		mark := tok.span
		p.endBlockCollection()
		p.popState()
		p.skip()
		return Event{Kind: EventMappingEnd}, mark, nil
	default:
		return Event{}, rawSpan{}, newScanError(tok.span.start, scanErrExpectedBlockMappingKey)
	}
}

func (p *parser) blockMappingKeyNode() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenKey || tok.kind == tokenValue || tok.kind == tokenBlockEnd {
		p.state = stateBlockMappingValue
		return emptyScalar(), emptyRawSpan(tok.span.start), nil
	}
	return p.deferParseNode(stateBlockNodeOrIndentlessSequence, stateBlockMappingValue, true, true)
}

func (p *parser) blockMappingValue() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenValue {
		mark := tok.span
		p.skip()
		return p.blockMappingValueNodeWithEmptySpan(mark)
	}
	p.state = stateBlockMappingKey
	return emptyScalar(), emptyRawSpan(tok.span.start), nil
}

func (p *parser) blockMappingValueNodeWithEmptySpan(mark rawSpan) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenKey || tok.kind == tokenValue || tok.kind == tokenBlockEnd {
		p.state = stateBlockMappingKey
		return emptyScalar(), mark, nil
	}
	return p.deferParseNode(stateBlockNodeOrIndentlessSequence, stateBlockMappingKey, true, true)
}

func (p *parser) flowMappingKey(first bool) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	var span rawSpan
	if tok.kind == tokenFlowMappingEnd {
		span = tok.span
	} else {
		if !first {
			if tok.kind == tokenFlowEntry {
				p.skip()
			} else {
				return Event{}, rawSpan{}, newScanError(tok.span.start, scanErrExpectedFlowMappingSeparator)
			}
		}
		tok, err = p.peekToken()
		if err != nil {
			return Event{}, rawSpan{}, err
		}
		switch tok.kind {
		case tokenKey:
			p.skip()
			return p.flowMappingKeyNode()
		case tokenValue:
			marker := tok.span
			p.state = stateFlowMappingValue
			return emptyScalar(), emptyRawSpan(marker.start), nil
		case tokenFlowMappingEnd:
			span = tok.span
		default:
			return p.deferParseNode(stateFlowNode, stateFlowMappingEmptyValue, false, false)
		}
	}
	p.popState()
	p.skip()
	return Event{Kind: EventMappingEnd}, span, nil
}

func (p *parser) flowMappingKeyNode() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenValue || tok.kind == tokenFlowEntry || tok.kind == tokenFlowMappingEnd {
		p.state = stateFlowMappingValue
		return emptyScalar(), emptyRawSpan(tok.span.start), nil
	}
	return p.deferParseNode(stateFlowNode, stateFlowMappingValue, false, false)
}

func (p *parser) flowMappingValue(empty bool) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if empty {
		p.state = stateFlowMappingKey
		return emptyScalar(), emptyRawSpan(tok.span.start), nil
	}
	if tok.kind == tokenValue {
		span := tok.span
		p.skip()
		return p.flowMappingValueNodeWithEmptySpan(span)
	}
	p.state = stateFlowMappingKey
	return emptyScalar(), emptyRawSpan(tok.span.start), nil
}

func (p *parser) flowMappingValueNodeWithEmptySpan(mark rawSpan) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenFlowEntry || tok.kind == tokenFlowMappingEnd {
		p.state = stateFlowMappingKey
		return emptyScalar(), mark, nil
	}
	return p.deferParseNode(stateFlowNode, stateFlowMappingKey, false, false)
}

func (p *parser) flowSequenceEntry(first bool) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch {
	case tok.kind == tokenFlowSequenceEnd:
		mark := tok.span
		p.popState()
		p.skip()
		return Event{Kind: EventSequenceEnd}, mark, nil
	case tok.kind == tokenFlowEntry && !first:
		p.skip()
	case !first:
		return Event{}, rawSpan{}, newScanError(tok.span.start, scanErrExpectedFlowSequenceSeparator)
	}

	tok, err = p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch tok.kind {
	case tokenFlowSequenceEnd:
		mark := tok.span
		p.popState()
		p.skip()
		return Event{Kind: EventSequenceEnd}, mark, nil
	case tokenKey:
		mark := tok.span
		p.state = stateFlowSequenceEntryMappingKey
		p.skip()
		return Event{
			Kind:           EventMappingStart,
			StructureStyle: StructureStyleFlow,
		}, mark, nil
	default:
		return p.deferParseNode(stateFlowNode, stateFlowSequenceEntry, false, false)
	}
}

func (p *parser) indentlessSequenceEntry() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenBlockEntry {
		mark := tok.span
		p.skip()
		return p.indentlessSequenceEntryNodeWithEmptySpan(mark)
	}
	p.endBlockCollection()
	p.popState()
	return Event{Kind: EventSequenceEnd}, emptyRawSpan(tok.span.start), nil
}

func (p *parser) indentlessSequenceEntryNode() (Event, rawSpan, *scanError) {
	mark := *p.pendingEmptyScalarSpan
	p.pendingEmptyScalarSpan = nil
	return p.indentlessSequenceEntryNodeWithEmptySpan(mark)
}

func (p *parser) indentlessSequenceEntryNodeWithEmptySpan(mark rawSpan) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch tok.kind {
	case tokenBlockEntry, tokenKey, tokenValue, tokenBlockEnd:
		p.state = stateIndentlessSequenceEntry
		return emptyScalar(), mark, nil
	default:
		return p.deferParseNode(stateBlockNode, stateIndentlessSequenceEntry, true, false)
	}
}

func (p *parser) blockSequenceEntry(_ bool) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	switch tok.kind {
	case tokenBlockEnd:
		mark := tok.span
		p.endBlockCollection()
		p.popState()
		p.skip()
		return Event{Kind: EventSequenceEnd}, mark, nil
	case tokenBlockEntry:
		mark := tok.span
		p.skip()
		return p.blockSequenceEntryNodeWithEmptySpan(mark)
	default:
		return Event{}, rawSpan{}, newScanError(tok.span.start, scanErrExpectedBlockSequenceEntry)
	}
}

func (p *parser) blockSequenceEntryNodeWithEmptySpan(mark rawSpan) (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenBlockEntry || tok.kind == tokenBlockEnd {
		p.state = stateBlockSequenceEntry
		return emptyScalar(), mark, nil
	}
	return p.deferParseNode(stateBlockNode, stateBlockSequenceEntry, true, false)
}

func (p *parser) flowSequenceEntryMappingKey() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenFlowEntry || tok.kind == tokenFlowSequenceEnd {
		p.state = stateFlowSequenceEntryMappingValue
		return emptyScalar(), emptyRawSpan(tok.span.start), nil
	}
	return p.deferParseNode(stateFlowNode, stateFlowSequenceEntryMappingValue, false, false)
}

func (p *parser) flowSequenceEntryMappingValue() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenValue {
		p.skip()
		return p.flowSequenceEntryMappingValueNode()
	}
	p.state = stateFlowSequenceEntryMappingEnd
	return emptyScalar(), emptyRawSpan(tok.span.start), nil
}

func (p *parser) flowSequenceEntryMappingValueNode() (Event, rawSpan, *scanError) {
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	if tok.kind == tokenFlowEntry || tok.kind == tokenFlowSequenceEnd {
		p.state = stateFlowSequenceEntryMappingEnd
		return emptyScalar(), emptyRawSpan(tok.span.start), nil
	}
	return p.deferParseNode(stateFlowNode, stateFlowSequenceEntryMappingEnd, false, false)
}

func (p *parser) flowSequenceEntryMappingEnd() (Event, rawSpan, *scanError) {
	p.state = stateFlowSequenceEntry
	tok, err := p.peekToken()
	if err != nil {
		return Event{}, rawSpan{}, err
	}
	return Event{Kind: EventMappingEnd}, emptyRawSpan(tok.span.start), nil
}

func (p *parser) resolveTag(span rawSpan, handle, suffix string) (*Tag, *scanError) {
	originalHandle := handle
	if handle == "!!" {
		prefix, ok := p.tags["!!"]
		if !ok {
			prefix = "tag:yaml.org,2002:"
		}
		return &Tag{
			Handle:         prefix,
			Suffix:         suffix,
			OriginalHandle: originalHandle,
		}, nil
	}
	if handle == "" && suffix == "!" {
		return &Tag{
			Handle:         "",
			Suffix:         suffix,
			OriginalHandle: originalHandle,
		}, nil
	}
	if prefix, ok := p.tags[handle]; ok {
		return &Tag{
			Handle:         prefix,
			Suffix:         suffix,
			OriginalHandle: originalHandle,
		}, nil
	}
	if len(handle) >= 2 && handle[0] == '!' && handle[len(handle)-1] == '!' {
		return nil, newScanError(span.start, scanErrUndeclaredTagHandle)
	}
	return &Tag{
		Handle:         handle,
		Suffix:         suffix,
		OriginalHandle: originalHandle,
	}, nil
}

// Engine wraps the YAML 1.2.2 event parser with original-byte coordinate
// translation and the bounded source-validation pass (spec §7.1, §7.2).
type Engine struct {
	parser      *parser
	input       string
	bom         int
	maxDepth    int
	nativeDepth int
}

// New constructs an Engine over a BOM-stripped UTF-8 string and its BOM byte
// length (0 or 3).
func New(input string, bom int, opts Options) *Engine {
	nativeDepth := opts.Limits.MaxDepth + simpleKeyMaxLookahead + 2
	return &Engine{
		parser:      newParser(input, nativeDepth),
		input:       input,
		bom:         bom,
		maxDepth:    opts.Limits.MaxDepth,
		nativeDepth: nativeDepth,
	}
}

// SyntaxPass executes the bounded source-validation pass (spec §7.1 phase 4),
// checking combined block and flow collection depth against MaxDepth in
// yielded event/error order before node construction without consuming the
// primary event iterator.
func (e *Engine) SyntaxPass() *Error {
	p := newParser(e.input, e.nativeDepth)
	depth := 0
	for {
		ev, raw, ok, sErr := p.nextEvent()
		if sErr != nil {
			return e.scanErr(sErr)
		}
		if !ok {
			return nil
		}
		switch ev.Kind {
		case EventSequenceStart, EventMappingStart:
			depth++
			if depth > e.maxDepth {
				start, err := e.loc(raw.start)
				if err != nil {
					return err
				}
				return newLimitError(LimitMaxDepth, uint64(e.maxDepth), start)
			}
		case EventSequenceEnd, EventMappingEnd:
			if depth == 0 {
				return newInternalError("source depth underflow")
			}
			depth--
		case EventDocumentStart, EventDocumentEnd, EventStreamEnd:
			if depth != 0 {
				return newInternalError("unbalanced source document")
			}
		}
	}
}

// Next yields the next parser Event and translated EventSpan. When the stream
// is exhausted, ok is false and err is nil.
func (e *Engine) Next() (Event, EventSpan, bool, *Error) {
	ev, raw, ok, sErr := e.parser.nextEvent()
	if sErr != nil {
		return Event{}, EventSpan{}, false, e.scanErr(sErr)
	}
	if !ok {
		return Event{}, EventSpan{}, false, nil
	}
	span, err := e.span(raw)
	if err != nil {
		return Event{}, EventSpan{}, false, err
	}
	return ev, span, true, nil
}

func (e *Engine) loc(m Marker) (Location, *Error) {
	if !m.hasByteOffset {
		return Location{}, newInternalError("parser emitted a marker without a byte offset on slice input")
	}
	return Location{
		ByteOffset: m.byteOffset + e.bom,
		Line:       m.line,
		Column:     m.col + 1,
	}, nil
}

func (e *Engine) span(s rawSpan) (EventSpan, *Error) {
	start, err := e.loc(s.start)
	if err != nil {
		return EventSpan{}, err
	}
	end, err := e.loc(s.end)
	if err != nil {
		return EventSpan{}, err
	}
	var tagStart *Location
	if s.tagStart != nil {
		ts, err := e.loc(*s.tagStart)
		if err != nil {
			return EventSpan{}, err
		}
		tagStart = &ts
	}
	return EventSpan{
		Span:      Span{Start: start, End: end},
		TagStart:  tagStart,
		Indent:    s.indent,
		HasIndent: s.hasIndent,
	}, nil
}

func (e *Engine) scanErr(err *scanError) *Error {
	loc, locErr := e.loc(err.mark)
	if locErr != nil {
		return locErr
	}
	detail := err.info()
	syntax := func(sk SyntaxKind) *Error {
		out := newSourceError(ErrorKindSyntax, loc, fmt.Sprintf("syntax error: %s", detail))
		out.Syntax = sk
		out.EngineDetail = detail
		return out
	}

	switch err.kind {
	case scanErrUnknownAnchor:
		out := newSourceError(ErrorKindUnknownAnchor, loc, "unknown anchor")
		out.EngineDetail = detail
		return out
	case scanErrUnsupportedYamlMajorVersion:
		out := newSourceError(ErrorKindUnsupportedYamlVersion, loc, "unsupported YAML version")
		out.EngineDetail = detail
		return out
	case scanErrRecursionLimitExceeded:
		out := newLimitError(LimitMaxDepth, uint64(e.maxDepth), loc)
		out.EngineDetail = detail
		return out
	case scanErrInputOffsetsWithoutSlice:
		return newInternalError("parser input offsets had no backing slice")

	case scanErrUnexpectedEofFlowSequence:
		return syntax(SyntaxUnexpectedEofFlowSequence)
	case scanErrUnexpectedEofFlowMapping:
		return syntax(SyntaxUnexpectedEofFlowMapping)
	case scanErrUnexpectedEofImplicitFlowMapping:
		return syntax(SyntaxUnexpectedEofImplicitFlowMapping)
	case scanErrUnexpectedEofBlockSequence:
		return syntax(SyntaxUnexpectedEofBlockSequence)
	case scanErrUnexpectedEofBlockMapping:
		return syntax(SyntaxUnexpectedEofBlockMapping)
	case scanErrUnexpectedEof:
		return syntax(SyntaxUnexpectedEof)
	case scanErrExpectedStreamStart:
		return syntax(SyntaxExpectedStreamStart)
	case scanErrDuplicateVersionDirective:
		return syntax(SyntaxDuplicateVersionDirective)
	case scanErrDuplicateTagDirective:
		return syntax(SyntaxDuplicateTagDirective)
	case scanErrExpectedDocumentStart:
		return syntax(SyntaxExpectedDocumentStart)
	case scanErrMissingDocumentEndBeforeDirective:
		return syntax(SyntaxMissingDocumentEndBeforeDirective)
	case scanErrAnchorCountOverflow:
		return syntax(SyntaxAnchorCountOverflow)
	case scanErrExpectedNodeContent:
		return syntax(SyntaxExpectedNodeContent)
	case scanErrExpectedBlockMappingKey:
		return syntax(SyntaxExpectedBlockMappingKey)
	case scanErrExpectedFlowMappingSeparator:
		return syntax(SyntaxExpectedFlowMappingSeparator)
	case scanErrExpectedFlowSequenceSeparator:
		return syntax(SyntaxExpectedFlowSequenceSeparator)
	case scanErrExpectedBlockSequenceEntry:
		return syntax(SyntaxExpectedBlockSequenceEntry)
	case scanErrUndeclaredTagHandle:
		return syntax(SyntaxUndeclaredTagHandle)
	case scanErrExpectedTagBang:
		return syntax(SyntaxExpectedTagBang)
	case scanErrExpectedTagDirectiveBang:
		return syntax(SyntaxExpectedTagDirectiveBang)
	case scanErrInvalidGlobalTagCharacter:
		return syntax(SyntaxInvalidGlobalTagCharacter)
	case scanErrSimpleKeyExpected:
		return syntax(SyntaxSimpleKeyExpected)
	case scanErrInvalidSimpleKey:
		return syntax(SyntaxInvalidSimpleKey)
	case scanErrInvalidDocumentEnd:
		return syntax(SyntaxInvalidDocumentEnd)
	case scanErrInvalidIndentation:
		return syntax(SyntaxInvalidIndentation)
	case scanErrBomInsideDocument:
		return syntax(SyntaxBomInsideDocument)
	case scanErrUnexpectedCharacter:
		return syntax(SyntaxUnexpectedCharacter)
	case scanErrTabNotAllowed:
		return syntax(SyntaxTabNotAllowed)
	case scanErrTabInBlockIndentation:
		return syntax(SyntaxTabInBlockIndentation)
	case scanErrCommentInterceptedScalar:
		return syntax(SyntaxCommentInterceptedScalar)
	case scanErrExpectedWhitespace:
		return syntax(SyntaxExpectedWhitespace)
	case scanErrCommentNotSeparated:
		return syntax(SyntaxCommentNotSeparated)
	case scanErrInvalidDirectiveTerminator:
		return syntax(SyntaxInvalidDirectiveTerminator)
	case scanErrMissingYamlVersionSeparator:
		return syntax(SyntaxMissingYamlVersionSeparator)
	case scanErrMissingDirectiveName:
		return syntax(SyntaxMissingDirectiveName)
	case scanErrInvalidDirectiveName:
		return syntax(SyntaxInvalidDirectiveName)
	case scanErrDirectiveByteLimitExceeded:
		return syntax(SyntaxDirectiveByteLimitExceeded)
	case scanErrTooManyReservedDirectiveParams:
		return syntax(SyntaxTooManyReservedDirectiveParams)
	case scanErrYamlVersionTooLong:
		return syntax(SyntaxYamlVersionTooLong)
	case scanErrMissingYamlVersion:
		return syntax(SyntaxMissingYamlVersion)
	case scanErrInvalidTagDirectiveTerminator:
		return syntax(SyntaxInvalidTagDirectiveTerminator)
	case scanErrInvalidTagTerminator:
		return syntax(SyntaxInvalidTagTerminator)
	case scanErrMissingTagUri:
		return syntax(SyntaxMissingTagUri)
	case scanErrUnclosedVerbatimTag:
		return syntax(SyntaxUnclosedVerbatimTag)
	case scanErrInvalidTagEscape:
		return syntax(SyntaxInvalidTagEscape)
	case scanErrInvalidTagUtf8LeadingByte:
		return syntax(SyntaxInvalidTagUtf8LeadingByte)
	case scanErrInvalidTagUtf8TrailingByte:
		return syntax(SyntaxInvalidTagUtf8TrailingByte)
	case scanErrInvalidTagUtf8:
		return syntax(SyntaxInvalidTagUtf8)
	case scanErrMissingAnchorOrAliasName:
		return syntax(SyntaxMissingAnchorOrAliasName)
	case scanErrMisplacedFlowCollectionEnd:
		return syntax(SyntaxMisplacedFlowCollectionEnd)
	case scanErrMismatchedFlowCollectionEnd:
		return syntax(SyntaxMismatchedFlowCollectionEnd)
	case scanErrUnclosedFlowCollection:
		return syntax(SyntaxUnclosedFlowCollection)
	case scanErrBlockEntryInFlowCollection:
		return syntax(SyntaxBlockEntryInFlowCollection)
	case scanErrBlockSequenceEntryNotAllowed:
		return syntax(SyntaxBlockSequenceEntryNotAllowed)
	case scanErrInvalidBlockEntryWhitespace:
		return syntax(SyntaxInvalidBlockEntryWhitespace)
	case scanErrZeroBlockScalarIndent:
		return syntax(SyntaxZeroBlockScalarIndent)
	case scanErrInvalidBlockScalarHeader:
		return syntax(SyntaxInvalidBlockScalarHeader)
	case scanErrTabAtBlockScalarStart:
		return syntax(SyntaxTabAtBlockScalarStart)
	case scanErrInvalidBlockScalarIndent:
		return syntax(SyntaxInvalidBlockScalarIndent)
	case scanErrDocumentIndicatorInQuotedScalar:
		return syntax(SyntaxDocumentIndicatorInQuotedScalar)
	case scanErrUnclosedQuotedScalar:
		return syntax(SyntaxUnclosedQuotedScalar)
	case scanErrTabInIndentation:
		return syntax(SyntaxTabInIndentation)
	case scanErrInvalidQuotedScalarIndent:
		return syntax(SyntaxInvalidQuotedScalarIndent)
	case scanErrInvalidTrailingSingleQuotedScalar:
		return syntax(SyntaxInvalidTrailingSingleQuotedScalar)
	case scanErrInvalidTrailingDoubleQuotedScalar:
		return syntax(SyntaxInvalidTrailingDoubleQuotedScalar)
	case scanErrUnknownQuotedScalarEscape:
		return syntax(SyntaxUnknownQuotedScalarEscape)
	case scanErrInvalidQuotedScalarHexEscape:
		return syntax(SyntaxInvalidQuotedScalarHexEscape)
	case scanErrInvalidLowSurrogateHexEscape:
		return syntax(SyntaxInvalidLowSurrogateHexEscape)
	case scanErrInvalidLowSurrogate:
		return syntax(SyntaxInvalidLowSurrogate)
	case scanErrMissingLowSurrogate:
		return syntax(SyntaxMissingLowSurrogate)
	case scanErrUnpairedLowSurrogate:
		return syntax(SyntaxUnpairedLowSurrogate)
	case scanErrInvalidUnicodeEscape:
		return syntax(SyntaxInvalidUnicodeEscape)
	case scanErrInvalidFlowScalarIndent:
		return syntax(SyntaxInvalidFlowScalarIndent)
	case scanErrPlainScalarStartsWithDashFlowIndicator:
		return syntax(SyntaxPlainScalarStartsWithDashFlowIndicator)
	case scanErrTabInPlainScalar:
		return syntax(SyntaxTabInPlainScalar)
	case scanErrUnexpectedEndOfPlainScalar:
		return syntax(SyntaxUnexpectedEndOfPlainScalar)
	case scanErrMappingKeyNotAllowed:
		return syntax(SyntaxMappingKeyNotAllowed)
	case scanErrFlowMappingValueAdjacentCollection:
		return syntax(SyntaxFlowMappingValueAdjacentCollection)
	case scanErrInvalidMappingValueWhitespace:
		return syntax(SyntaxInvalidMappingValueWhitespace)
	case scanErrInvalidColonPlacement:
		return syntax(SyntaxInvalidColonPlacement)
	case scanErrMappingValueNotAllowed:
		return syntax(SyntaxMappingValueNotAllowed)
	default:
		return newInternalError("unclassified parser error kind")
	}
}
