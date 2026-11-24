package yaml

import (
	"math"
	"strconv"
	"testing"
)

// Plain JSON float text (spec revision 1.7 §11.3, 1.7/E-5). Expected strings
// are derived from the rule: fewest significant digits that read back as the
// same binary64 value, the nearest such decimal, and an even final digit when
// two are equally near.
func plainFloat(t *testing.T, f float64) string {
	t.Helper()
	s, err := EncodePlain(NewSyntheticNode(FloatValue(f)), DefaultOptions())
	if err != nil {
		t.Fatalf("EncodePlain(%v): %v", f, err)
	}
	return s
}

func TestPlainFloatExactTiesChooseEvenDigit(t *testing.T) {
	x := 1e15 + 0.25
	if math.Float64bits(x) != 0x430c6bf526340002 {
		t.Fatalf("bits %016x", math.Float64bits(x))
	}
	y := 1e15 + 0.75
	cases := []struct {
		f    float64
		want string
	}{
		{x, "1000000000000000.2"},
		{-x, "-1000000000000000.2"},
		{y, "1000000000000000.8"},
		{-y, "-1000000000000000.8"},
	}
	for _, c := range cases {
		if got := plainFloat(t, c.f); got != c.want {
			t.Errorf("%016x: got %s, want %s", math.Float64bits(c.f), got, c.want)
		}
	}
}

func TestPlainFloatLayoutBoundaries(t *testing.T) {
	cases := []struct {
		f    float64
		want string
	}{
		{0.0, "0.0"},
		{math.Copysign(0, -1), "-0.0"},
		{0.1, "0.1"},
		{123.456, "123.456"},
		{1e15, "1000000000000000.0"},
		{9999999999999998, "9999999999999998.0"},
		{1e16, "1e16"},
		{1.5e16, "1.5e16"},
		{0.0001, "0.0001"},
		{0.00012, "0.00012"},
		{0.00001, "1e-5"},
		{1.5e-5, "1.5e-5"},
		{-2.5e-7, "-2.5e-7"},
		{5e-324, "5e-324"},
		{2.2250738585072014e-308, "2.2250738585072014e-308"},
		{1.7976931348623157e308, "1.7976931348623157e308"},
	}
	for _, c := range cases {
		if got := plainFloat(t, c.f); got != c.want {
			t.Errorf("%016x: got %s, want %s", math.Float64bits(c.f), got, c.want)
		}
	}
}

func TestPlainFloatReadsBackToSameBits(t *testing.T) {
	state := uint64(0x9e3779b97f4a7c15)
	for i := 0; i < 200000; i++ {
		state ^= state << 13
		state ^= state >> 7
		state ^= state << 17
		f := math.Float64frombits(state)
		if math.IsNaN(f) || math.IsInf(f, 0) {
			continue
		}
		s := plainFloat(t, f)
		back, err := strconv.ParseFloat(s, 64)
		if err != nil || math.Float64bits(back) != math.Float64bits(f) {
			t.Fatalf("%016x: %s does not read back", state, s)
		}
	}
}
