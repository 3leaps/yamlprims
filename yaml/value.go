package yaml

import (
	"crypto/rand"
	"encoding/binary"
	"math"
	"math/big"
	"math/bits"
	"strconv"
	"sync/atomic"
)

// Location is a byte and character coordinate in the original input stream (spec-v1.4 §3, §7.2).
//
// ByteOffset is 0-based in the original input bytes (including any leading UTF-8 BOM).
// Line and Column are 1-based; Column counts Unicode scalar values since the last line break
// (excluding an initial UTF-8 BOM on line 1).
type Location struct {
	ByteOffset int
	Line       int
	Column     int
}

// Span is a half-open source range [Start, End) in original-byte coordinates.
type Span struct {
	Start Location
	End   Location
}

// NodeSpanKind distinguishes source-backed nodes from caller-constructed synthetic nodes.
type NodeSpanKind uint8

const (
	NodeSpanSource NodeSpanKind = iota
	NodeSpanSynthetic
)

// NodeSpan records whether a Node came from input source or was constructed synthetically.
type NodeSpan struct {
	Kind   NodeSpanKind
	Source Span
}

// SourceNodeSpan wraps a source Span.
func SourceNodeSpan(span Span) NodeSpan {
	return NodeSpan{Kind: NodeSpanSource, Source: span}
}

// SyntheticNodeSpan returns a synthetic NodeSpan with no source coordinates.
func SyntheticNodeSpan() NodeSpan {
	return NodeSpan{Kind: NodeSpanSynthetic}
}

// AsSource returns the source Span if the node is source-backed.
func (ns NodeSpan) AsSource() (Span, bool) {
	if ns.Kind == NodeSpanSource {
		return ns.Source, true
	}
	return Span{}, false
}

// ViaKind identifies how a copied node entered the document tree.
type ViaKind uint8

const (
	ViaKindAlias ViaKind = iota
	ViaKindMerge
)

// Via records provenance for a node materialized via alias expansion or merge-key application.
type Via struct {
	Kind ViaKind
	Span Span
}

// ViaAlias constructs alias provenance pointing to the `*name` token Span.
func ViaAlias(useSite Span) Via {
	return Via{Kind: ViaKindAlias, Span: useSite}
}

// ViaMerge constructs merge provenance pointing to the `<<` key token Span.
func ViaMerge(directive Span) Via {
	return Via{Kind: ViaKindMerge, Span: directive}
}

// Integer is an immutable YAML Core Schema integer in the range i128::MIN..=u128::MAX
// (-170141183460469231731687303715884105728 ..= 340282366920938463463374607431768211455).
//
// Invariant: negative is true only when the value is strictly less than 0, with
// 128-bit magnitude (hi<<64 | lo) in 1..=(1<<127). Negative zero is unrepresentable.
type Integer struct {
	hi       uint64
	lo       uint64
	negative bool
}

const i128MinMagHi uint64 = 1 << 63

// IntegerFromInt64 constructs an Integer from an int64.
func IntegerFromInt64(v int64) Integer {
	if v < 0 {
		return Integer{hi: 0, lo: uint64(^v) + 1, negative: true}
	}
	return Integer{hi: 0, lo: uint64(v), negative: false}
}

// IntegerFromUint64 constructs an Integer from a uint64.
func IntegerFromUint64(v uint64) Integer {
	return Integer{hi: 0, lo: v, negative: false}
}

// IntegerFromUint128 constructs a non-negative Integer from a 128-bit unsigned magnitude.
func IntegerFromUint128(hi, lo uint64) Integer {
	return Integer{hi: hi, lo: lo, negative: false}
}

// IntegerFromInt128 constructs a signed 128-bit Integer from two's-complement (hi, lo).
func IntegerFromInt128(hi int64, lo uint64) Integer {
	if hi < 0 {
		// Negate two's complement 128-bit value: (~hi, ~lo) + 1
		notLo := ^lo
		notHi := ^uint64(hi)
		magLo, carry := bits.Add64(notLo, 1, 0)
		magHi := notHi + carry
		return Integer{hi: magHi, lo: magLo, negative: true}
	}
	return Integer{hi: uint64(hi), lo: lo, negative: false}
}

// NewSignedMagnitudeInteger constructs an Integer from a sign and 128-bit magnitude,
// returning false if negative && magnitude > (1 << 127). Negative zero is normalized to +0.
func NewSignedMagnitudeInteger(negative bool, hi, lo uint64) (Integer, bool) {
	if hi == 0 && lo == 0 {
		return Integer{}, true
	}
	if negative {
		if hi > i128MinMagHi || (hi == i128MinMagHi && lo > 0) {
			return Integer{}, false
		}
		return Integer{hi: hi, lo: lo, negative: true}, true
	}
	return Integer{hi: hi, lo: lo, negative: false}, true
}

// IntegerFromBigInt constructs an Integer from b if b is within i128::MIN..=u128::MAX.
func IntegerFromBigInt(b *big.Int) (Integer, bool) {
	if b == nil {
		return Integer{}, false
	}
	if b.Sign() == 0 {
		return Integer{}, true
	}
	neg := b.Sign() < 0
	if b.BitLen() > 128 {
		return Integer{}, false
	}
	var abs big.Int
	abs.Abs(b)
	var buf [16]byte
	abs.FillBytes(buf[:])
	hi := binary.BigEndian.Uint64(buf[:8])
	lo := binary.BigEndian.Uint64(buf[8:])
	return NewSignedMagnitudeInteger(neg, hi, lo)
}

// IsNegative reports whether i < 0.
func (i Integer) IsNegative() bool {
	return i.negative
}

// Magnitude returns the 128-bit unsigned magnitude (hi, lo) of i.
func (i Integer) Magnitude() (hi, lo uint64) {
	return i.hi, i.lo
}

// Int64 converts i to int64 if it fits in math.MinInt64..=math.MaxInt64.
func (i Integer) Int64() (int64, bool) {
	if i.hi != 0 {
		return 0, false
	}
	if i.negative {
		const minMag = uint64(1) << 63
		if i.lo == minMag {
			return math.MinInt64, true
		}
		if i.lo < minMag {
			return -int64(i.lo), true
		}
		return 0, false
	}
	if i.lo <= math.MaxInt64 {
		return int64(i.lo), true
	}
	return 0, false
}

// Uint64 converts i to uint64 if it fits in 0..=math.MaxUint64.
func (i Integer) Uint64() (uint64, bool) {
	if i.negative || i.hi != 0 {
		return 0, false
	}
	return i.lo, true
}

// Int128 converts i to a two's-complement signed 128-bit pair (hi, lo) if it fits in i128::MIN..=i128::MAX.
func (i Integer) Int128() (hi int64, lo uint64, ok bool) {
	if i.negative {
		if i.hi > i128MinMagHi || (i.hi == i128MinMagHi && i.lo > 0) {
			return 0, 0, false
		}
		// Two's complement negation of (i.hi, i.lo)
		notLo := ^i.lo
		notHi := ^i.hi
		tcLo, carry := bits.Add64(notLo, 1, 0)
		tcHi := notHi + carry
		return int64(tcHi), tcLo, true
	}
	if i.hi >= i128MinMagHi {
		return 0, 0, false
	}
	return int64(i.hi), i.lo, true
}

// Uint128 converts i to an unsigned 128-bit pair (hi, lo) if i >= 0.
func (i Integer) Uint128() (hi, lo uint64, ok bool) {
	if i.negative {
		return 0, 0, false
	}
	return i.hi, i.lo, true
}

// BigInt returns a fresh defensive *big.Int copy of i.
// Mutating the returned *big.Int never mutates i.
func (i Integer) BigInt() *big.Int {
	out := new(big.Int)
	if i.hi == 0 && i.lo == 0 {
		return out
	}
	var buf [16]byte
	binary.BigEndian.PutUint64(buf[:8], i.hi)
	binary.BigEndian.PutUint64(buf[8:], i.lo)
	out.SetBytes(buf[:])
	if i.negative {
		out.Neg(out)
	}
	return out
}

// Float64Exact converts i to an IEEE 754 binary64 float64 only if the integer is
// exactly representable without rounding (significant bits between highest and
// lowest 1-bit <= 53).
func (i Integer) Float64Exact() (float64, bool) {
	if i.hi == 0 && i.lo == 0 {
		return 0.0, true
	}
	lead := leadingZeros128(i.hi, i.lo)
	trail := trailingZeros128(i.hi, i.lo)
	sigBits := 128 - lead
	if sigBits-trail > 53 {
		return 0, false
	}
	mant := shiftRight128(i.hi, i.lo, uint(trail))
	v := math.Ldexp(float64(mant), trail)
	if i.negative {
		return -v, true
	}
	return v, true
}

// ToFloat64Exact is an alias for Float64Exact matching the Rust method name.
func (i Integer) ToFloat64Exact() (float64, bool) {
	return i.Float64Exact()
}

// String returns the canonical base-10 string representation of i.
func (i Integer) String() string {
	if i.hi == 0 {
		if i.negative {
			return "-" + strconv.FormatUint(i.lo, 10)
		}
		return strconv.FormatUint(i.lo, 10)
	}
	// Format 128-bit magnitude by dividing by 10^19.
	const ten19 uint64 = 10_000_000_000_000_000_000
	var buf [44]byte
	pos := len(buf)
	hi, lo := i.hi, i.lo
	for hi != 0 {
		var rem uint64
		qHi := hi / ten19
		rHi := hi % ten19
		qLo, rLo := bits.Div64(rHi, lo, ten19)
		hi, lo, rem = qHi, qLo, rLo
		for d := 0; d < 19; d++ {
			pos--
			buf[pos] = byte('0' + rem%10)
			rem /= 10
		}
	}
	for lo != 0 {
		pos--
		buf[pos] = byte('0' + lo%10)
		lo /= 10
	}
	if i.negative {
		pos--
		buf[pos] = '-'
	}
	return string(buf[pos:])
}

func leadingZeros128(hi, lo uint64) int {
	if hi != 0 {
		return bits.LeadingZeros64(hi)
	}
	return 64 + bits.LeadingZeros64(lo)
}

func trailingZeros128(hi, lo uint64) int {
	if lo != 0 {
		return bits.TrailingZeros64(lo)
	}
	return 64 + bits.TrailingZeros64(hi)
}

func shiftRight128(hi, lo uint64, shift uint) uint64 {
	if shift == 0 {
		return lo
	}
	if shift >= 64 {
		return hi >> (shift - 64)
	}
	return (lo >> shift) | (hi << (64 - shift))
}

// Tag is a preserved non-Core YAML tag attached to a Tagged node.
type Tag struct {
	uri    string
	handle string
	suffix string
}

// NewTag constructs a Tag with its resolved URI, handle, and suffix.
func NewTag(uri, handle, suffix string) Tag {
	return Tag{uri: uri, handle: handle, suffix: suffix}
}

// URI returns the resolved tag URI (key equality compares only URI).
func (t Tag) URI() string {
	return t.uri
}

// Handle returns the source tag handle (for example "!" or "!!").
func (t Tag) Handle() string {
	return t.handle
}

// Suffix returns the source tag suffix.
func (t Tag) Suffix() string {
	return t.suffix
}

// Tagged pairs a preserved Tag with its inner Node.
type Tagged struct {
	tag        Tag
	value      Node
	innerPlain bool
}

// NewTagged constructs a Tagged wrapper around value.
func NewTagged(tag Tag, value Node) *Tagged {
	return &Tagged{tag: tag, value: value}
}

func newTaggedWithPlain(tag Tag, value Node, innerPlain bool) *Tagged {
	return &Tagged{tag: tag, value: value, innerPlain: innerPlain}
}

// Tag returns the Tag metadata.
func (t *Tagged) Tag() Tag {
	if t == nil {
		return Tag{}
	}
	return t.tag
}

// Value returns a defensive copy of the wrapped inner Node.
func (t *Tagged) Value() *Node {
	if t == nil {
		return nil
	}
	cp := t.value
	return &cp
}

// InsertErrorKind classifies why Mapping.InsertNew rejected a key.
type InsertErrorKind uint8

const (
	InsertErrorInvalidKey InsertErrorKind = iota
	InsertErrorDuplicate
)

// InsertError is returned when Mapping.InsertNew rejects an invalid (NaN-bearing) or duplicate key.
type InsertError struct {
	Kind     InsertErrorKind
	Existing Node
}

// MappingEntry is an ordered (Key, Value) pair in a Mapping.
type MappingEntry struct {
	Key   Node
	Value Node
}

type mappingState struct {
	owner   *Mapping
	count   int
	buckets map[uint64][]int
}

// Mapping is an ordered YAML mapping with randomized hash-indexed key lookup
// and duplicate-key rejection (spec-v1.4 §5.1, §5.2).
//
// Copying a Mapping by value or obtaining a view via Value.AsMapping shares
// read-only access to the existing entries and hash index in O(1); any subsequent
// InsertNew call on a non-owning or value-copied handle detaches (copy-on-write)
// before mutating so another Mapping handle or stored key is never corrupted.
type Mapping struct {
	entries []MappingEntry
	state   *mappingState
	seed0   uint64
	seed1   uint64
}

var (
	processSeed0 uint64
	processSeed1 uint64
	seedCounter  uint64
)

func init() {
	var b [16]byte
	if _, err := rand.Read(b[:]); err == nil {
		processSeed0 = binary.LittleEndian.Uint64(b[:8])
		processSeed1 = binary.LittleEndian.Uint64(b[8:])
	} else {
		processSeed0 = 0x736f6d6570736575
		processSeed1 = 0x646f72616e646f6d
	}
}

func nextMappingSeeds() (uint64, uint64) {
	c := atomic.AddUint64(&seedCounter, 1)
	return processSeed0 ^ (c * 0x9e3779b97f4a7c15), processSeed1 ^ bits.RotateLeft64(c, 32)
}

// NewMapping constructs an empty Mapping.
func NewMapping() *Mapping {
	s0, s1 := nextMappingSeeds()
	m := &Mapping{
		seed0: s0,
		seed1: s1,
	}
	m.state = &mappingState{
		owner: m,
	}
	return m
}

// NewMappingWithCapacity constructs an empty Mapping pre-sized for cap entries.
func NewMappingWithCapacity(cap int) *Mapping {
	s0, s1 := nextMappingSeeds()
	var entries []MappingEntry
	var buckets map[uint64][]int
	if cap > 0 {
		entries = make([]MappingEntry, 0, cap)
		buckets = make(map[uint64][]int, cap)
	}
	m := &Mapping{
		entries: entries,
		seed0:   s0,
		seed1:   s1,
	}
	m.state = &mappingState{
		owner:   m,
		buckets: buckets,
	}
	return m
}

func (m *Mapping) ensureOwned() {
	if m.seed0 == 0 && m.seed1 == 0 {
		m.seed0, m.seed1 = nextMappingSeeds()
	}
	if m.state == nil || m.state.owner != m || m.state.count != len(m.entries) {
		m.detach()
	}
}

func (m *Mapping) detach() {
	var entries []MappingEntry
	var buckets map[uint64][]int
	if len(m.entries) > 0 {
		entries = make([]MappingEntry, len(m.entries), len(m.entries)+1)
		copy(entries, m.entries)
		buckets = make(map[uint64][]int, len(entries))
		for idx := range entries {
			h := m.hashValue(entries[idx].Key.value)
			buckets[h] = append(buckets[h], idx)
		}
	} else {
		buckets = make(map[uint64][]int, 4)
	}
	m.entries = entries
	m.state = &mappingState{
		owner:   m,
		count:   len(entries),
		buckets: buckets,
	}
}

func (m *Mapping) freezeView() *Mapping {
	if m == nil {
		cp := NewMapping()
		cp.state.owner = nil
		return cp
	}
	if m.seed0 == 0 && m.seed1 == 0 {
		m.seed0, m.seed1 = nextMappingSeeds()
	}
	if m.state == nil || m.state.count != len(m.entries) {
		m.detach()
	}
	m.state.owner = nil
	var entries []MappingEntry
	if len(m.entries) > 0 {
		entries = m.entries[:len(m.entries):len(m.entries)]
	}
	return &Mapping{
		entries: entries,
		state:   m.state,
		seed0:   m.seed0,
		seed1:   m.seed1,
	}
}

func (m *Mapping) freezeInPlace() *Mapping {
	if m == nil {
		cp := NewMapping()
		cp.state.owner = nil
		return cp
	}
	if m.state == nil || m.state.count != len(m.entries) {
		m.detach()
	}
	m.state.owner = nil
	if len(m.entries) > 0 {
		m.entries = m.entries[:len(m.entries):len(m.entries)]
	}
	return m
}

// Len returns the number of entries in m.
func (m *Mapping) Len() int {
	if m == nil {
		return 0
	}
	return len(m.entries)
}

// IsEmpty reports whether m has 0 entries.
func (m *Mapping) IsEmpty() bool {
	return m.Len() == 0
}

// At returns the i-th (key, value) pair in insertion order.
func (m *Mapping) At(i int) (Node, Node) {
	if m == nil || i < 0 || i >= len(m.entries) {
		return Node{}, Node{}
	}
	e := m.entries[i]
	return e.Key, e.Value
}

// Entries returns a shallow defensive copy of the ordered entries slice.
func (m *Mapping) Entries() []MappingEntry {
	if m == nil || len(m.entries) == 0 {
		return nil
	}
	out := make([]MappingEntry, len(m.entries))
	copy(out, m.entries)
	return out
}

// Clone returns a deep structural clone of m.
func (m *Mapping) Clone() *Mapping {
	if m == nil {
		return NewMapping()
	}
	cp := NewMappingWithCapacity(len(m.entries))
	cp.seed0 = m.seed0
	cp.seed1 = m.seed1
	for _, e := range m.entries {
		cp.entries = append(cp.entries, MappingEntry{
			Key:   e.Key.Clone(),
			Value: e.Value.Clone(),
		})
	}
	cp.state.count = len(cp.entries)
	if len(cp.entries) > 0 {
		if cp.state.buckets == nil {
			cp.state.buckets = make(map[uint64][]int, len(cp.entries))
		}
		for idx := range cp.entries {
			h := cp.hashValue(cp.entries[idx].Key.value)
			cp.state.buckets[h] = append(cp.state.buckets[h], idx)
		}
	}
	return cp
}

// InsertNew inserts (key, value) if key contains no NaN at any depth and does
// not equal any existing key under KeyEq.
func (m *Mapping) InsertNew(key, value Node) *InsertError {
	return m.insertNew(key, value)
}

func (m *Mapping) insertNewOwned(key, value Node) *InsertError {
	return m.insertNew(key, value)
}

func (m *Mapping) insertNew(key, value Node) *InsertError {
	m.ensureOwned()
	if containsNaN(key.value) {
		return &InsertError{Kind: InsertErrorInvalidKey}
	}
	hash := m.hashValue(key.value)
	if m.state.buckets != nil {
		for _, idx := range m.state.buckets[hash] {
			if idx >= 0 && idx < len(m.entries) && KeyEq(m.entries[idx].Key.value, key.value) {
				return &InsertError{Kind: InsertErrorDuplicate, Existing: m.entries[idx].Key}
			}
		}
	} else {
		m.state.buckets = make(map[uint64][]int, 4)
	}
	idx := len(m.entries)
	m.entries = append(m.entries, MappingEntry{Key: key, Value: value})
	m.state.count = len(m.entries)
	m.state.buckets[hash] = append(m.state.buckets[hash], idx)
	return nil
}

// Get looks up a String key in m and returns a defensive copy of the associated Node.
func (m *Mapping) Get(key string) (*Node, bool) {
	return m.GetValue(StringValue(key))
}

// GetValue looks up an arbitrary Value key in m using KeyEq and returns a defensive copy of the associated Node.
func (m *Mapping) GetValue(key Value) (*Node, bool) {
	if m == nil || len(m.entries) == 0 || containsNaN(key) {
		return nil, false
	}
	if m.state != nil && m.state.buckets != nil {
		hash := m.hashValue(key)
		for _, idx := range m.state.buckets[hash] {
			if idx >= 0 && idx < len(m.entries) && KeyEq(m.entries[idx].Key.value, key) {
				cp := m.entries[idx].Value
				return &cp, true
			}
		}
		return nil, false
	}
	for i := range m.entries {
		if KeyEq(m.entries[i].Key.value, key) {
			cp := m.entries[i].Value
			return &cp, true
		}
	}
	return nil, false
}

// ContainsValue reports whether key exists in m under KeyEq.
func (m *Mapping) ContainsValue(key Value) bool {
	_, ok := m.GetValue(key)
	return ok
}

func (m *Mapping) hashValue(v Value) uint64 {
	var h sipHasher
	h.init(m.seed0, m.seed1)
	keyHash(&v, &h)
	return h.finish()
}

// ValueKind is the closed discriminant of a YAML Value.
type ValueKind uint8

const (
	ValueKindNull ValueKind = iota
	ValueKindBool
	ValueKindInt
	ValueKindFloat
	ValueKindString
	ValueKindSequence
	ValueKindMapping
	ValueKindTagged
)

// Name returns the snake_case wire name of the value kind.
func (k ValueKind) Name() string {
	switch k {
	case ValueKindNull:
		return "null"
	case ValueKindBool:
		return "bool"
	case ValueKindInt:
		return "int"
	case ValueKindFloat:
		return "float"
	case ValueKindString:
		return "string"
	case ValueKindSequence:
		return "sequence"
	case ValueKindMapping:
		return "mapping"
	case ValueKindTagged:
		return "tagged"
	default:
		return "null"
	}
}

// String returns the snake_case wire name of the value kind.
func (k ValueKind) String() string {
	return k.Name()
}

// Value is a resolved YAML 1.2.2 Core Schema value (spec-v1.4 §3).
type Value struct {
	kind   ValueKind
	b      bool
	i      Integer
	f      float64
	s      string
	seq    []Node
	m      *Mapping
	tagged *Tagged
}

// NullValue constructs a Null Value.
func NullValue() Value {
	return Value{kind: ValueKindNull}
}

// BoolValue constructs a Bool Value.
func BoolValue(b bool) Value {
	return Value{kind: ValueKindBool, b: b}
}

// IntValue constructs an Int Value.
func IntValue(i Integer) Value {
	return Value{kind: ValueKindInt, i: i}
}

// FloatValue constructs a Float Value.
func FloatValue(f float64) Value {
	return Value{kind: ValueKindFloat, f: f}
}

// StringValue constructs a String Value.
func StringValue(s string) Value {
	return Value{kind: ValueKindString, s: s}
}

// SequenceValue constructs a Sequence Value from a defensive copy of the items slice.
func SequenceValue(items []Node) Value {
	var cp []Node
	if len(items) > 0 {
		cp = make([]Node, len(items))
		copy(cp, items)
	}
	return Value{kind: ValueKindSequence, seq: cp}
}

func sequenceValueOwned(items []Node) Value {
	return Value{kind: ValueKindSequence, seq: items}
}

// MappingValue constructs a Mapping Value from m, marking its current state
// copy-on-write so subsequent InsertNew calls on m or on any AsMapping() view
// detach without mutating the stored Value.
func MappingValue(m *Mapping) Value {
	return Value{kind: ValueKindMapping, m: m.freezeView()}
}

func mappingValueOwned(m *Mapping) Value {
	return Value{kind: ValueKindMapping, m: m.freezeInPlace()}
}

// TaggedValue constructs a Tagged Value from a defensive copy of t.
func TaggedValue(t *Tagged) Value {
	if t == nil {
		return Value{kind: ValueKindTagged}
	}
	cp := *t
	return Value{
		kind:   ValueKindTagged,
		tagged: &cp,
	}
}

func taggedValueOwned(t *Tagged) Value {
	return Value{kind: ValueKindTagged, tagged: t}
}

// Kind returns the ValueKind discriminant.
func (v Value) Kind() ValueKind {
	return v.kind
}

// KindName returns the snake_case wire name of v's kind.
func (v Value) KindName() string {
	return v.kind.Name()
}

// IsNull reports whether v is ValueKindNull.
func (v Value) IsNull() bool {
	return v.kind == ValueKindNull
}

// AsBool returns the bool payload if v is ValueKindBool.
func (v Value) AsBool() (bool, bool) {
	if v.kind == ValueKindBool {
		return v.b, true
	}
	return false, false
}

// AsInt returns the Integer payload if v is ValueKindInt.
func (v Value) AsInt() (Integer, bool) {
	if v.kind == ValueKindInt {
		return v.i, true
	}
	return Integer{}, false
}

// AsFloat returns the float64 payload if v is ValueKindFloat.
func (v Value) AsFloat() (float64, bool) {
	if v.kind == ValueKindFloat {
		return v.f, true
	}
	return 0, false
}

// AsStr returns the string payload if v is ValueKindString.
func (v Value) AsStr() (string, bool) {
	if v.kind == ValueKindString {
		return v.s, true
	}
	return "", false
}

// AsSequence returns a defensive copy of the sequence slice if v is ValueKindSequence.
func (v Value) AsSequence() ([]Node, bool) {
	if v.kind == ValueKindSequence {
		if len(v.seq) == 0 {
			return nil, true
		}
		cp := make([]Node, len(v.seq))
		copy(cp, v.seq)
		return cp, true
	}
	return nil, false
}

// AsMapping returns a copy-on-write Mapping view in O(1) if v is ValueKindMapping.
// Read operations (Len, Get, GetValue, At, Entries) share the existing entries
// and hash index without allocation; any InsertNew call detaches before mutating.
func (v Value) AsMapping() (*Mapping, bool) {
	if v.kind == ValueKindMapping {
		if v.m == nil {
			cp := NewMapping()
			cp.state.owner = nil
			return cp, true
		}
		cp := *v.m
		return &cp, true
	}
	return nil, false
}

// AsTagged returns a defensive copy of the Tagged wrapper if v is ValueKindTagged.
func (v Value) AsTagged() (*Tagged, bool) {
	if v.kind == ValueKindTagged && v.tagged != nil {
		cp := *v.tagged
		return &cp, true
	}
	return nil, false
}

// Clone returns a deep copy of v.
func (v Value) Clone() Value {
	switch v.kind {
	case ValueKindSequence:
		if len(v.seq) == 0 {
			return Value{kind: ValueKindSequence}
		}
		cp := make([]Node, len(v.seq))
		for idx := range v.seq {
			cp[idx] = v.seq[idx].Clone()
		}
		return Value{kind: ValueKindSequence, seq: cp}
	case ValueKindMapping:
		return Value{kind: ValueKindMapping, m: v.m.Clone().freezeInPlace()}
	case ValueKindTagged:
		if v.tagged == nil {
			return Value{kind: ValueKindTagged}
		}
		return Value{
			kind: ValueKindTagged,
			tagged: &Tagged{
				tag:        v.tagged.tag,
				value:      v.tagged.value.Clone(),
				innerPlain: v.tagged.innerPlain,
			},
		}
	default:
		return v
	}
}

// Node is a Value paired with its source Span, optional Via provenance, and optional TagStart coordinate.
type Node struct {
	value    Value
	span     NodeSpan
	via      *Via
	tagStart *Location
}

// NewNode constructs a source-backed Node with the given Value and Span.
func NewNode(value Value, span Span) Node {
	return Node{
		value: value,
		span:  SourceNodeSpan(span),
	}
}

func newNodeOwned(value Value, span Span) Node {
	return Node{
		value: value,
		span:  SourceNodeSpan(span),
	}
}

// NewSyntheticNode constructs a synthetic Node with no source Span.
func NewSyntheticNode(value Value) Node {
	return Node{
		value: value,
		span:  SyntheticNodeSpan(),
	}
}

func newSyntheticNodeOwned(value Value) Node {
	return Node{
		value: value,
		span:  SyntheticNodeSpan(),
	}
}

// Value returns the Node's Value.
func (n Node) Value() Value {
	return n.value
}

// ValuePtr returns a pointer to a copy of the Node's Value.
func (n *Node) ValuePtr() *Value {
	cp := n.value
	return &cp
}

// Span returns the source Span if the node is source-backed.
func (n Node) Span() (Span, bool) {
	return n.span.AsSource()
}

// NodeSpan returns the full NodeSpan (source or synthetic).
func (n Node) NodeSpan() NodeSpan {
	return n.span
}

// Via returns a defensive copy of the optional alias or merge provenance of n.
func (n Node) Via() *Via {
	if n.via == nil {
		return nil
	}
	v := *n.via
	return &v
}

// TagStart returns a defensive copy of the optional source Location of the node's tag token.
func (n Node) TagStart() *Location {
	if n.tagStart == nil {
		return nil
	}
	l := *n.tagStart
	return &l
}

// WithVia returns a copy of n with Via set to via.
func (n Node) WithVia(via Via) Node {
	v := via
	n.via = &v
	return n
}

// WithTagStart returns a copy of n with TagStart set to loc.
func (n Node) WithTagStart(loc Location) Node {
	l := loc
	n.tagStart = &l
	return n
}

// Clone returns a deep copy of n.
func (n Node) Clone() Node {
	out := Node{
		value: n.value.Clone(),
		span:  n.span,
	}
	if n.via != nil {
		v := *n.via
		out.via = &v
	}
	if n.tagStart != nil {
		l := *n.tagStart
		out.tagStart = &l
	}
	return out
}

func (n Node) sourceSpan() Span {
	if n.span.Kind == NodeSpanSource {
		return n.span.Source
	}
	return Span{}
}

func (n Node) positionSpan() (Span, bool) {
	if n.via != nil && n.via.Kind == ViaKindAlias {
		return n.via.Span, true
	}
	return n.Span()
}

// Document is a single loaded YAML document within a stream (spec-v1.4 §3).
type Document struct {
	root          Node
	yamlVersion   *[2]uint32
	explicitStart bool
}

// NewDocument constructs a Document with the given root Node, optional %YAML version, and explicit-start flag.
func NewDocument(root Node, yamlVersion *[2]uint32, explicitStart bool) Document {
	return newDocumentOwned(root, yamlVersion, explicitStart)
}

func newDocumentOwned(root Node, yamlVersion *[2]uint32, explicitStart bool) Document {
	var verCopy *[2]uint32
	if yamlVersion != nil {
		v := *yamlVersion
		verCopy = &v
	}
	return Document{
		root:          root,
		yamlVersion:   verCopy,
		explicitStart: explicitStart,
	}
}

// Root returns a pointer to a copy of the Document's root Node.
func (d *Document) Root() *Node {
	cp := d.root
	return &cp
}

// IntoRoot returns the Document's root Node by value.
func (d Document) IntoRoot() Node {
	return d.root
}

// YamlVersion returns the document's %YAML directive (major, minor, true) if present.
func (d Document) YamlVersion() (uint32, uint32, bool) {
	if d.yamlVersion != nil {
		return d.yamlVersion[0], d.yamlVersion[1], true
	}
	return 0, 0, false
}

// ExplicitStart reports whether the document began with an explicit `---` marker.
func (d Document) ExplicitStart() bool {
	return d.explicitStart
}

func containsNaN(root Value) bool {
	stack := []Value{root}
	for len(stack) > 0 {
		v := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		switch v.kind {
		case ValueKindFloat:
			if math.IsNaN(v.f) {
				return true
			}
		case ValueKindTagged:
			if v.tagged != nil {
				stack = append(stack, v.tagged.value.value)
			}
		case ValueKindSequence:
			for i := range v.seq {
				stack = append(stack, v.seq[i].value)
			}
		case ValueKindMapping:
			if v.m != nil {
				for i := range v.m.entries {
					stack = append(stack, v.m.entries[i].Key.value, v.m.entries[i].Value.value)
				}
			}
		}
	}
	return false
}

// KeyEq reports whether two YAML Values are equal under the spec-v1.4 §5.1 key equality relation:
//   - Kind and value must match (Bool(true) != Int(1) != Float(1.0) != String("true"))
//   - Float: 0.0 == -0.0; NaN != NaN at any depth
//   - Tagged: Tag.URI() and inner value must match (handle/suffix ignored)
//   - Sequence: length and element-wise KeyEq
//   - Mapping: entry order is ignored; equal entry count, no NaN at any depth, and every
//     (k, v) in a is present in b with KeyEq(v, b[k])
//   - Spans, Via, and TagStart are ignored.
func KeyEq(a, b Value) bool {
	type pair struct {
		a, b Value
	}
	stack := []pair{{a: a, b: b}}
	for len(stack) > 0 {
		p := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if p.a.kind != p.b.kind {
			return false
		}
		switch p.a.kind {
		case ValueKindNull:
			// equal
		case ValueKindBool:
			if p.a.b != p.b.b {
				return false
			}
		case ValueKindInt:
			if p.a.i != p.b.i {
				return false
			}
		case ValueKindFloat:
			if math.IsNaN(p.a.f) || math.IsNaN(p.b.f) {
				return false
			}
			if p.a.f == 0 && p.b.f == 0 {
				continue
			}
			if math.Float64bits(p.a.f) != math.Float64bits(p.b.f) {
				return false
			}
		case ValueKindString:
			if p.a.s != p.b.s {
				return false
			}
		case ValueKindTagged:
			if p.a.tagged == nil || p.b.tagged == nil {
				if p.a.tagged != p.b.tagged {
					return false
				}
				continue
			}
			if p.a.tagged.tag.uri != p.b.tagged.tag.uri {
				return false
			}
			stack = append(stack, pair{a: p.a.tagged.value.value, b: p.b.tagged.value.value})
		case ValueKindSequence:
			if len(p.a.seq) != len(p.b.seq) {
				return false
			}
			for i := range p.a.seq {
				stack = append(stack, pair{a: p.a.seq[i].value, b: p.b.seq[i].value})
			}
		case ValueKindMapping:
			lenA := p.a.m.Len()
			lenB := p.b.m.Len()
			if lenA != lenB {
				return false
			}
			if containsNaN(p.a) || containsNaN(p.b) {
				return false
			}
			if lenA == 0 {
				continue
			}
			for i := 0; i < lenA; i++ {
				e := p.a.m.entries[i]
				bv, ok := p.b.m.GetValue(e.Key.value)
				if !ok {
					return false
				}
				stack = append(stack, pair{a: e.Value.value, b: bv.value})
			}
		}
	}
	return true
}

func keyHash(root *Value, h *sipHasher) {
	type workItem struct {
		v        *Value
		hasToken bool
		token    uint8
		u64      uint64
	}
	stack := []workItem{{v: root}}
	for len(stack) > 0 {
		it := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if it.hasToken {
			h.writeByte(it.token)
			h.writeUint64(it.u64)
			continue
		}
		v := it.v
		switch v.kind {
		case ValueKindNull:
			h.writeByte(0)
		case ValueKindBool:
			h.writeByte(1)
			if v.b {
				h.writeByte(1)
			} else {
				h.writeByte(0)
			}
		case ValueKindInt:
			h.writeByte(2)
			if v.i.negative {
				h.writeByte(1)
			} else {
				h.writeByte(0)
			}
			h.writeUint64(v.i.hi)
			h.writeUint64(v.i.lo)
		case ValueKindFloat:
			h.writeByte(3)
			if v.f == 0 {
				h.writeUint64(0)
			} else {
				h.writeUint64(math.Float64bits(v.f))
			}
		case ValueKindString:
			h.writeByte(4)
			h.writeString(v.s)
		case ValueKindTagged:
			h.writeByte(5)
			if v.tagged != nil {
				h.writeString(v.tagged.tag.uri)
				stack = append(stack, workItem{v: &v.tagged.value.value})
			}
		case ValueKindSequence:
			h.writeByte(6)
			h.writeUint64(uint64(len(v.seq)))
			for i := len(v.seq) - 1; i >= 0; i-- {
				stack = append(stack, workItem{v: &v.seq[i].value})
			}
		case ValueKindMapping:
			// Order-independent combine via commutative wrapping sum of per-entry hashes.
			h.writeByte(7)
			if v.m == nil {
				h.writeUint64(0)
				h.writeUint64(0)
				continue
			}
			h.writeUint64(uint64(len(v.m.entries)))
			var sum uint64
			for i := range v.m.entries {
				var entryH sipHasher
				entryH.init(0x517cc1b727220a95, 0x9e3779b97f4a7c15)
				keyHash(&v.m.entries[i].Key.value, &entryH)
				keyHash(&v.m.entries[i].Value.value, &entryH)
				sum += entryH.finish()
			}
			h.writeUint64(sum)
		}
	}
}

// sipHasher implements SipHash-1-3 for collision-resistant mapping indexing.
type sipHasher struct {
	v0, v1, v2, v3 uint64
	buf            uint64
	nbuf           uint8
	length         uint64
}

func (s *sipHasher) init(k0, k1 uint64) {
	s.v0 = k0 ^ 0x736f6d6570736575
	s.v1 = k1 ^ 0x646f72616e646f6d
	s.v2 = k0 ^ 0x6c7967656e657261
	s.v3 = k1 ^ 0x7465646279746573
	s.buf = 0
	s.nbuf = 0
	s.length = 0
}

func (s *sipHasher) round() {
	s.v0 += s.v1
	s.v1 = bits.RotateLeft64(s.v1, 13)
	s.v1 ^= s.v0
	s.v0 = bits.RotateLeft64(s.v0, 32)
	s.v2 += s.v3
	s.v3 = bits.RotateLeft64(s.v3, 16)
	s.v3 ^= s.v2
	s.v0 += s.v3
	s.v3 = bits.RotateLeft64(s.v3, 21)
	s.v3 ^= s.v0
	s.v2 += s.v1
	s.v1 = bits.RotateLeft64(s.v1, 17)
	s.v1 ^= s.v2
	s.v2 = bits.RotateLeft64(s.v2, 32)
}

func (s *sipHasher) writeByte(b byte) {
	s.length++
	s.buf |= uint64(b) << (8 * s.nbuf)
	s.nbuf++
	if s.nbuf == 8 {
		s.v3 ^= s.buf
		s.round()
		s.v0 ^= s.buf
		s.buf = 0
		s.nbuf = 0
	}
}

func (s *sipHasher) writeUint64(x uint64) {
	var b [8]byte
	binary.LittleEndian.PutUint64(b[:], x)
	for i := 0; i < 8; i++ {
		s.writeByte(b[i])
	}
}

func (s *sipHasher) writeString(str string) {
	s.writeUint64(uint64(len(str)))
	for i := 0; i < len(str); i++ {
		s.writeByte(str[i])
	}
}

func (s *sipHasher) finish() uint64 {
	b := s.buf | (s.length << 56)
	s.v3 ^= b
	s.round()
	s.v0 ^= b
	s.v2 ^= 0xff
	s.round()
	s.round()
	s.round()
	return s.v0 ^ s.v1 ^ s.v2 ^ s.v3
}
