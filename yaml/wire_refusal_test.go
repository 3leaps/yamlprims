package yaml

import (
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
