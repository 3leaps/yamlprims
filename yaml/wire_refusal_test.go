package yaml

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Malformed-wire refusals (spec revision 1.7 §11.1, 1.7/E-6): one public
// message in every binding, the specific reason kept in the bounded,
// non-serialized EngineDetail.
const wireRefusalMessage = "invalid yamlprims wire payload"

func TestWireRefusalUsesCommonMessageAndKeepsReason(t *testing.T) {
	d := DefaultOptions()
	for _, text := range []string{
		"nope",
		"{}",
		"[]",
		`{"format":"other","version":1,"documents":[]}`,
		`{"format":"yamlprims-value","version":2,"documents":[]}`,
	} {
		_, err := DecodeWire(text, d)
		if err == nil || err.Kind != ErrorKindInternal || err.Message != wireRefusalMessage {
			t.Fatalf("%s: got %+v", text, err)
		}
		if err.EngineDetail() == "" {
			t.Fatalf("%s: reason must be kept", text)
		}
		wire, encErr := EncodeError(err, d)
		if encErr != nil || strings.Contains(wire, err.EngineDetail()) {
			t.Fatalf("%s: reason must not be serialized: %s", text, wire)
		}
	}
}

func TestWireRefusalReasonIsBounded(t *testing.T) {
	text := `{"format":"yamlprims-value","version":1,"` + strings.Repeat("k", 20000) + `":0,"documents":[]}`
	_, err := DecodeWire(text, DefaultOptions())
	if err == nil || err.Message != wireRefusalMessage || len(err.EngineDetail()) > MaxMessageBytes {
		t.Fatalf("got %+v (detail %d bytes)", err, len(err.EngineDetail()))
	}
}

func TestDecodedErrorEnvelopeKeepsCallerMessage(t *testing.T) {
	d := DefaultOptions()
	text := `{"format":"yamlprims-value","version":1,"error":{"kind":"internal","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"caller text"}}`
	env, err := DecodeWire(text, d)
	if err != nil || env.HasDocuments || env.Error.Message != "caller text" {
		t.Fatalf("got %+v %+v", env, err)
	}
	if out, _ := EncodeError(env.Error, d); out != text {
		t.Fatalf("round trip changed: %s", out)
	}
}

func TestWireLimitsAndOptionsAreNotFolded(t *testing.T) {
	o := DefaultOptions()
	o.Limits.MaxInputBytes = 3
	_, err := DecodeWire("nope", o)
	if err == nil || err.Kind != ErrorKindLimitExceeded || err.Message != "wire payload exceeds max_input_bytes (3)" {
		t.Fatalf("got %+v", err)
	}
	bad := DefaultOptions()
	bad.Limits.MaxDepth = 0
	if _, err := DecodeWire("nope", bad); err == nil || err.Kind != ErrorKindInvalidOptions {
		t.Fatalf("got %+v", err)
	}
}

func TestWireRefusalEnvelopeExactOutputBudget(t *testing.T) {
	d := DefaultOptions()
	_, err := DecodeWire("{}", d)
	full, encErr := EncodeError(err, d)
	want := `{"format":"yamlprims-value","version":1,"error":{"kind":"internal","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"invalid yamlprims wire payload"}}`
	if encErr != nil || full != want {
		t.Fatalf("got %s %v", full, encErr)
	}
	exact := DefaultOptions()
	exact.Limits.MaxOutputBytes = len(full)
	if out, e := EncodeError(err, exact); e != nil || out != full {
		t.Fatalf("exact budget: %s %v", out, e)
	}
	less := DefaultOptions()
	less.Limits.MaxOutputBytes = len(full) - 1
	if _, e := EncodeError(err, less); e == nil || e.Kind != ErrorKindOutputLimitExceeded {
		t.Fatalf("one less: %v", e)
	}
}

func TestConstructorRefusalsMatchSharedTable(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("..", "conformance", "vectors", "constructor-error-cases.json"))
	if err != nil {
		t.Fatal(err)
	}
	var table struct {
		Cases []struct{ ID, Kind, Message string } `json:"cases"`
	}
	if err := json.Unmarshal(raw, &table); err != nil {
		t.Fatal(err)
	}
	d := DefaultOptions()
	local, _ := TryNewTag("!t")
	merge, mErr := TryNewTag("tag:yaml.org,2002:merge")
	if mErr != nil {
		t.Fatal(mErr)
	}
	for _, c := range table.Cases {
		var e *Error
		switch c.ID {
		case "tag.empty-uri":
			_, e = TryNewTag("")
		case "tag.core-uri":
			_, e = TryNewTag("tag:yaml.org,2002:str")
		case "tag.inconsistent-spelling":
			_, e = TryNewTagWithSpelling("!x", "!!", "y")
		case "tagged.bool-inner":
			_, e = TryNewTagged(local, NewSyntheticNode(BoolValue(true)))
		case "tagged.merge-sequence-inner":
			_, e = TryNewTagged(merge, NewSyntheticNode(SequenceValue(nil)))
		case "document.version-2-0":
			_, e = TryNewDocumentWithMetadata(NewSyntheticNode(NullValue()), &[2]uint32{2, 0}, false)
		default:
			t.Fatalf("unknown case %s", c.ID)
		}
		if e == nil {
			t.Fatalf("%s: no error", c.ID)
		}
		got, encErr := EncodeError(e, d)
		want := `{"format":"yamlprims-value","version":1,"error":{"kind":"` + c.Kind +
			`","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"` + c.Message + `"}}`
		if encErr != nil || got != want {
			t.Errorf("%s:\n got %s\nwant %s", c.ID, got, want)
		}
	}
}

func TestOutOfRangeMetadataMatchesSharedTable(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("..", "conformance", "vectors", "constructor-error-cases.json"))
	if err != nil {
		t.Fatal(err)
	}
	var table struct {
		Cases []struct{ ID, Message string } `json:"encode_error_cases"`
	}
	if err := json.Unmarshal(raw, &table); err != nil {
		t.Fatal(err)
	}
	const big = 1 << 53
	ns := SiteNotSource(nil)
	for _, c := range table.Cases {
		var e *Error
		switch c.ID {
		case "path-index-2^53":
			e = &Error{Kind: ErrorKindInternal, Site: ns, Message: "m",
				Path: []PathSegment{{Kind: PathSegmentIndex, Index: big}}}
		case "site-offset-2^53":
			e = &Error{Kind: ErrorKindInternal, Message: "m",
				Site: SiteSource(Location{ByteOffset: big, Line: 1, Column: 1})}
		case "configured-2^53":
			e = &Error{Kind: ErrorKindLimitExceeded, Site: ns, Message: "m",
				Detail: ErrorDetail{Limit: LimitMaxNodes, Configured: big, HasConfigured: true}}
		case "configured-zero":
			e = &Error{Kind: ErrorKindLimitExceeded, Site: ns, Message: "m",
				Detail: ErrorDetail{Limit: LimitMaxNodes, Configured: 0, HasConfigured: true}}
		default:
			t.Fatalf("unknown case %s", c.ID)
		}
		_, refused := EncodeError(e, DefaultOptions())
		if refused == nil || refused.Kind != ErrorKindInternal || refused.Message != c.Message ||
			refused.Site != SiteNotSource(nil) {
			t.Errorf("%s: got %+v", c.ID, refused)
		}
	}
}
