package md_tmpl

import (
	"testing"
)

func TestMarshalFlexbuffers(t *testing.T) {
	params := map[string]any{
		"name":  "Alice",
		"count": 42,
	}
	data, err := marshalFlexbuffers(params)
	if err != nil {
		t.Fatalf("marshalFlexbuffers failed: %v", err)
	}
	if len(data) == 0 {
		t.Fatalf("expected non-empty data")
	}
}

type ConfirmedVariant struct {
	TaggedVariant
	Evidence string `json:"evidence"`
}

func TestTaggedVariantStaticTyping(t *testing.T) {
	val := ConfirmedVariant{
		TaggedVariant: NewTaggedVariant("Confirmed"),
		Evidence:      "static proof",
	}
	data, err := marshalFlexbuffers(val)
	if err != nil {
		t.Fatalf("marshalFlexbuffers failed for TaggedVariant: %v", err)
	}
	if len(data) == 0 {
		t.Fatalf("expected non-empty data for TaggedVariant")
	}
}

func TestOmitemptySkipsZeroValues(t *testing.T) {
	type Params struct {
		Name   string   `json:"name"`
		Count  int64    `json:"count,omitempty"`
		Score  float64  `json:"score,omitempty"`
		Active bool     `json:"active,omitempty"`
		Tags   []string `json:"tags,omitempty"`
	}

	// With zero values and omitempty, these should still marshal successfully,
	// and the Rust side should be able to render with the remaining fields.
	tmpl, err := FromSourceAllowingUnused(`---
params:
  - name = str
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSourceAllowingUnused failed: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderStructAllowingExtra(Params{Name: "Alice"})
	if err != nil {
		t.Fatalf("RenderStructAllowingExtra failed: %v", err)
	}
	expected := "Hello Alice!"
	if result != expected {
		t.Errorf("expected %q, got %q", expected, result)
	}
}

func TestOmitemptyKeepsNonZero(t *testing.T) {
	type Params struct {
		Name  string `json:"name"`
		Count int64  `json:"count,omitempty"`
	}

	tmpl, err := FromSource(`---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatalf("FromSource failed: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderStruct(Params{Name: "Alice", Count: 42})
	if err != nil {
		t.Fatalf("RenderStruct failed: %v", err)
	}
	expected := "Alice: 42"
	if result != expected {
		t.Errorf("expected %q, got %q", expected, result)
	}
}
