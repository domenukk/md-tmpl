package md_tmpl

import (
	"encoding/json"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// Enum dispatch
// ---------------------------------------------------------------------------

func TestEnumUnitVariant(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("outcome", "Rejected"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "NO\n" {
		t.Errorf("got %q, want %q", result, "NO\n")
	}
}

func TestEnumStructVariant(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("outcome", `{"__kind__":"Confirmed","evidence":"found it"}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "YES: found it\n" {
		t.Errorf("got %q, want %q", result, "YES: found it\n")
	}
}

func TestEnumInvalidVariant(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("outcome", "Unknown"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if err == nil {
		t.Fatal("expected error for invalid variant, got nil")
	}
}

// ---------------------------------------------------------------------------
// Variant type — enum ergonomics
// ---------------------------------------------------------------------------

func TestVariantUnit(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.Set("outcome", Variant{Kind: "Rejected"}); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "NO\n" {
		t.Errorf("got %q, want %q", result, "NO\n")
	}
}

func TestVariantStruct(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.Set("outcome", Variant{
		Kind:   "Confirmed",
		Fields: map[string]any{"evidence": "found it"},
	}); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "YES: found it\n" {
		t.Errorf("got %q, want %q", result, "YES: found it\n")
	}
}

// ---------------------------------------------------------------------------
// TaggedVariant — static typed enum variants
// ---------------------------------------------------------------------------

func TestTaggedVariantEmbedding(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Define a TaggedVariant struct.
	type ConfirmedVariant struct {
		TaggedVariant
		Evidence string `json:"evidence"`
	}

	type Params struct {
		Outcome ConfirmedVariant `json:"outcome"`
	}

	result, err := tmpl.RenderStruct(Params{
		Outcome: ConfirmedVariant{
			TaggedVariant: NewTaggedVariant("Confirmed"),
			Evidence:      "found it",
		},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "YES: found it") {
		t.Errorf("expected 'YES: found it' in output, got %q", result)
	}
}

func TestTaggedVariantUnitWithRenderStruct(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// A unit variant via TaggedVariant — no extra fields.
	type RejectedVariant struct {
		TaggedVariant
	}

	type Params struct {
		Outcome RejectedVariant `json:"outcome"`
	}

	result, err := tmpl.RenderStruct(Params{
		Outcome: RejectedVariant{
			TaggedVariant: NewTaggedVariant("Rejected"),
		},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "NO\n" {
		t.Errorf("got %q, want %q", result, "NO\n")
	}
}

func TestNewTaggedVariantKind(t *testing.T) {
	tv := NewTaggedVariant("MyVariant")
	if tv.Kind != "MyVariant" {
		t.Errorf("got Kind=%q, want %q", tv.Kind, "MyVariant")
	}
}

// ---------------------------------------------------------------------------
// Variant.MarshalJSON
// ---------------------------------------------------------------------------

func TestVariantMarshalJSONUnit(t *testing.T) {
	v := Variant{Kind: "Rejected"}
	data, err := v.MarshalJSON()
	if err != nil {
		t.Fatalf("MarshalJSON: %v", err)
	}
	// Unit variant serializes to plain string.
	if string(data) != `"Rejected"` {
		t.Errorf("got %s, want %q", data, `"Rejected"`)
	}
}

func TestVariantMarshalJSONStruct(t *testing.T) {
	v := Variant{
		Kind:   "Confirmed",
		Fields: map[string]any{"evidence": "found it"},
	}
	data, err := v.MarshalJSON()
	if err != nil {
		t.Fatalf("MarshalJSON: %v", err)
	}

	// Deserialize and verify structure.
	var parsed map[string]any
	if err := json.Unmarshal(data, &parsed); err != nil {
		t.Fatalf("json.Unmarshal: %v", err)
	}
	if parsed["__kind__"] != "Confirmed" {
		t.Errorf("__kind__ = %v, want Confirmed", parsed["__kind__"])
	}
	if parsed["evidence"] != "found it" {
		t.Errorf("evidence = %v, want 'found it'", parsed["evidence"])
	}
}

func TestVariantMarshalJSONEmptyFields(t *testing.T) {
	v := Variant{Kind: "Empty", Fields: map[string]any{}}
	data, err := v.MarshalJSON()
	if err != nil {
		t.Fatalf("MarshalJSON: %v", err)
	}
	// Empty fields map should serialize as unit variant (plain string).
	if string(data) != `"Empty"` {
		t.Errorf("got %s, want %q", data, `"Empty"`)
	}
}

// ---------------------------------------------------------------------------
// Variant with RenderMap and RenderStruct
// ---------------------------------------------------------------------------

func TestVariantUnitWithRenderMap(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
> {% match outcome %}
> {% case Confirmed %}

YES

> {% case Rejected %}

NO

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"outcome": Variant{Kind: "Rejected"},
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "NO\n" {
		t.Errorf("got %q, want %q", result, "NO\n")
	}
}

func TestVariantStructWithRenderMap(t *testing.T) {
	src := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"outcome": Variant{
			Kind:   "Confirmed",
			Fields: map[string]any{"evidence": "proof"},
		},
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "YES: proof") {
		t.Errorf("expected 'YES: proof' in output, got %q", result)
	}
}

func TestVariantStructWithRenderStruct(t *testing.T) {
	src := `---
params:
  - name = str
  - outcome = enum(Confirmed(evidence = str), Rejected)
---
{{ name }}: > {% match outcome %}

> {% case Confirmed %}

YES {{ outcome.evidence }}

> {% case Rejected %}

NO

> {% /match %}`
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name    string  `json:"name"`
		Outcome Variant `json:"outcome"`
	}

	result, err := tmpl.RenderStruct(Params{
		Name:    "test",
		Outcome: Variant{Kind: "Confirmed", Fields: map[string]any{"evidence": "proof"}},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "YES") || !strings.Contains(result, "proof") {
		t.Errorf("expected YES + proof in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// Codegen — enum types
// ---------------------------------------------------------------------------

func TestCodegenEnumProducesCorrectGoTypes(t *testing.T) {
	source := `---
params:
  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)

allow_unused: true
---
> {% match outcome %}

> {% case Confirmed %}

{{ outcome.evidence }}

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`

	code, err := GenerateTypes(source)
	if err != nil {
		t.Fatalf("GenerateTypes: %v", err)
	}

	// Verify sealed interface exists.
	if !strings.Contains(code, "type Outcome interface") {
		t.Errorf("expected 'type Outcome interface' in generated code:\n%s", code)
	}

	// Verify struct variants embed TaggedVariant or have __kind__.
	if !strings.Contains(code, "type OutcomeConfirmed struct") {
		t.Errorf("expected 'type OutcomeConfirmed struct' in generated code:\n%s", code)
	}
	if !strings.Contains(code, "type OutcomeRejected struct") {
		t.Errorf("expected 'type OutcomeRejected struct' in generated code:\n%s", code)
	}
	if !strings.Contains(code, "type OutcomeNeedsWork struct") {
		t.Errorf("expected 'type OutcomeNeedsWork struct' in generated code:\n%s", code)
	}

	// Verify sealed method marker.
	if !strings.Contains(code, "isOutcome()") {
		t.Errorf("expected 'isOutcome()' sealed method:\n%s", code)
	}

	// Verify field in struct variant.
	if !containsNormalized(code, "Evidence string") {
		t.Errorf("expected 'Evidence string' in OutcomeConfirmed:\n%s", code)
	}

	// Verify it compiles.
	assertCompiles(t, code)
}
