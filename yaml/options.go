package yaml

// MergeKeys configures handling of the YAML 1.1 `<<` merge key in mappings.
type MergeKeys uint8

const (
	// MergeKeysReject rejects `<<` keys with ErrorKindMergeRejected (default).
	MergeKeysReject MergeKeys = iota
	// MergeKeysApply expands `<<` mapping or sequence-of-mappings entries into the containing mapping.
	MergeKeysApply
	// MergeKeysAsKey preserves `<<` as an ordinary string key `"<<"` with no special behavior.
	MergeKeysAsKey
)

// String returns the canonical snake_case name of the merge-key policy.
func (m MergeKeys) String() string {
	switch m {
	case MergeKeysReject:
		return "reject"
	case MergeKeysApply:
		return "apply"
	case MergeKeysAsKey:
		return "as_key"
	default:
		return ""
	}
}

func (m MergeKeys) isValid() bool {
	return m <= MergeKeysAsKey
}

// UnknownTags configures handling of tags outside the YAML 1.2.2 Core Schema.
type UnknownTags uint8

const (
	// UnknownTagsPreserve wraps nodes carrying non-Core tags in ValueKindTagged (default).
	UnknownTagsPreserve UnknownTags = iota
	// UnknownTagsReject fails loading on any non-Core tag with ErrorKindUnknownTag.
	UnknownTagsReject
)

// String returns the canonical snake_case name of the unknown-tag policy.
func (u UnknownTags) String() string {
	switch u {
	case UnknownTagsPreserve:
		return "preserve"
	case UnknownTagsReject:
		return "reject"
	default:
		return ""
	}
}

func (u UnknownTags) isValid() bool {
	return u <= UnknownTagsReject
}

// Limits configures the 12 explicit resource budgets enforced during loading,
// typed conversion, and wire/plain encoding (spec-v1.4 §6).
type Limits struct {
	MaxInputBytes     int
	MaxDepth          int
	MaxNodes          int
	MaxExpandedNodes  int
	MaxAliasEvents    int
	MaxOwnedBytes     int
	MaxScalarBytes    int
	MaxKeyBytes       int
	MaxMappingEntries int
	MaxDocuments      int
	MaxOutputBytes    int
	MaxWarnings       int
}

// DefaultLimits returns the normative default Limits from spec-v1.4 §6.
func DefaultLimits() Limits {
	return Limits{
		MaxInputBytes:     8 << 20,
		MaxDepth:          128,
		MaxNodes:          250_000,
		MaxExpandedNodes:  100_000,
		MaxAliasEvents:    10_000,
		MaxOwnedBytes:     64 << 20,
		MaxScalarBytes:    1 << 20,
		MaxKeyBytes:       64 << 10,
		MaxMappingEntries: 100_000,
		MaxDocuments:      1_024,
		MaxOutputBytes:    64 << 20,
		MaxWarnings:       1_000,
	}
}

// Options configures policy and resource budgets for loading and encoding.
//
// Passing an uninitialized zero-value Options{} fails Validate() with
// ErrorKindInvalidOptions because zero-value Limits are 0; use DefaultOptions()
// to start from normative defaults.
type Options struct {
	MergeKeys   MergeKeys
	UnknownTags UnknownTags
	Limits      Limits
}

// DefaultOptions returns Options initialized with MergeKeysReject,
// UnknownTagsPreserve, and DefaultLimits().
func DefaultOptions() Options {
	return Options{
		MergeKeys:   MergeKeysReject,
		UnknownTags: UnknownTagsPreserve,
		Limits:      DefaultLimits(),
	}
}

// Validate checks all option policies and limit invariants per spec-v1.4 §6.
func (o Options) Validate() *Error {
	if !o.MergeKeys.isValid() || !o.UnknownTags.isValid() {
		return errInvalidOptions("invalid policy option")
	}
	l := o.Limits
	vals := [12]int{
		l.MaxInputBytes,
		l.MaxDepth,
		l.MaxNodes,
		l.MaxExpandedNodes,
		l.MaxAliasEvents,
		l.MaxOwnedBytes,
		l.MaxScalarBytes,
		l.MaxKeyBytes,
		l.MaxMappingEntries,
		l.MaxDocuments,
		l.MaxOutputBytes,
		l.MaxWarnings,
	}
	for _, v := range vals {
		if v <= 0 {
			return errInvalidOptions("every limit must be at least 1")
		}
	}
	if l.MaxDepth > 255 {
		return errInvalidOptions("max_depth must be at most 255")
	}
	if l.MaxScalarBytes > l.MaxOwnedBytes || l.MaxKeyBytes > l.MaxOwnedBytes {
		return errInvalidOptions("max_scalar_bytes and max_key_bytes must not exceed max_owned_bytes")
	}
	return nil
}
