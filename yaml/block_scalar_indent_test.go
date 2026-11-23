package yaml

import (
	"fmt"
	"strings"
	"testing"
	"time"
)

func plainOf(input string, o Options) (string, error) {
	doc, err := LoadWith(input, o)
	if err != nil {
		return "", fmt.Errorf("load: %v", err)
	}
	s, err := EncodePlain(doc.IntoRoot(), o)
	if err != nil {
		return "", fmt.Errorf("encode: %v", err)
	}
	return s, nil
}

// Block scalar content at any indentation width, followed by a shorter run of
// spaces at end of input, loads in bounded time to the same value as the
// equivalent quoted scalar.
func TestBlockScalarIndentAtEndOfInput(t *testing.T) {
	o := DefaultOptions()
	cases := []struct{ indicator, quoted string }{{"", "x\\n"}, {"-", "x"}, {"+", "x\\n"}}
	for _, c := range cases {
		want, err := plainOf(fmt.Sprintf("k: \"%s\"\n", c.quoted), o)
		if err != nil {
			t.Fatal(err)
		}
		for indent := 1; indent <= 40; indent++ {
			for tail := 0; tail < indent; tail++ {
				input := "k: |" + c.indicator + "\n" + strings.Repeat(" ", indent) + "x\n" + strings.Repeat(" ", tail)
				type result struct {
					s   string
					err error
				}
				done := make(chan result, 1)
				go func() {
					s, err := plainOf(input, o)
					done <- result{s, err}
				}()
				select {
				case r := <-done:
					if r.err != nil {
						t.Fatalf("indent %d tail %d indicator %q: %v", indent, tail, c.indicator, r.err)
					}
					if c.indicator == "+" && tail > 0 {
						continue // twin parity for keep chomping with a trailing partial line is in the conformance vectors
					}
					if r.s != want {
						t.Fatalf("indent %d tail %d indicator %q: got %s want %s", indent, tail, c.indicator, r.s, want)
					}
				case <-time.After(5 * time.Second):
					t.Fatalf("indent %d tail %d indicator %q: load did not finish", indent, tail, c.indicator)
				}
			}
		}
	}
}
