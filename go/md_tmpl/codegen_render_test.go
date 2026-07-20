package md_tmpl

// End-to-end wire-format tests for generated sum-type (enum) code.
//
// These types mirror exactly what codegen emits for
//
//	outcome = enum(Confirmed(evidence = str, score = int), Rejected, NeedsWork)
//
// (a sealed interface, one struct per variant, and Kind/AsVariant/MarshalJSON
// on each). Rendering them through the real engine exercises the production
// path a generated Params.Render takes: ctx.Set -> FlexBuffers -> the
// VariantMarshaler hook -> the shared __kind__ wire format. If the encoder
// dropped __kind__ (the bug these tests guard against), the match below would
// fail to dispatch and the assertions would catch it.

import (
	"encoding/json"
	"strings"
	"testing"
)

// --- Mirror of generated code for enum(Confirmed(...), Rejected, NeedsWork) ---

type teOutcome interface {
	isTeOutcome()
	Kind() string
	AsVariant() Variant
}

type teConfirmed struct {
	Evidence string `json:"evidence"`
	Score    int64  `json:"score"`
}

func (teConfirmed) isTeOutcome() {}
func (teConfirmed) Kind() string { return "Confirmed" }
func (v teConfirmed) AsVariant() Variant {
	return Variant{
		Kind: "Confirmed",
		Fields: map[string]any{
			"evidence": v.Evidence,
			"score":    v.Score,
		},
	}
}
func (v teConfirmed) MarshalJSON() ([]byte, error) { return v.AsVariant().MarshalJSON() }

type teRejected struct{}

func (teRejected) isTeOutcome()       {}
func (teRejected) Kind() string       { return "Rejected" }
func (teRejected) AsVariant() Variant { return Variant{Kind: "Rejected"} }
func (teRejected) MarshalJSON() ([]byte, error) {
	return json.Marshal("Rejected")
}

type teNeedsWork struct{}

func (teNeedsWork) isTeOutcome()       {}
func (teNeedsWork) Kind() string       { return "NeedsWork" }
func (teNeedsWork) AsVariant() Variant { return Variant{Kind: "NeedsWork"} }
func (teNeedsWork) MarshalJSON() ([]byte, error) {
	return json.Marshal("NeedsWork")
}

// Compile-time assertion that every variant implements VariantMarshaler, just
// like generated code relies on.
var (
	_ VariantMarshaler = teConfirmed{}
	_ VariantMarshaler = teRejected{}
	_ VariantMarshaler = teNeedsWork{}
	_ teOutcome        = teConfirmed{}
	_ teOutcome        = teRejected{}
	_ teOutcome        = teNeedsWork{}
)

// teItem mirrors a generated list/struct item that carries an enum-typed field,
// exercising nested-variant marshaling inside a struct.
type teItem struct {
	Name   string    `json:"name"`
	Status teOutcome `json:"status"`
}

const matchOutcomeTemplate = `---
params:
  - outcome = enum(Confirmed(evidence = str, score = int), Rejected, NeedsWork)
---
> {% match outcome %}
> {% case Confirmed %}

YES: {{ outcome.evidence }} ({{ outcome.score }})

> {% case Rejected %}

NO

> {% case NeedsWork %}

MAYBE

> {% /match %}`

// renderOutcome sets a statically-typed variant and renders the match template.
func renderOutcome(t *testing.T, outcome teOutcome) string {
	t.Helper()
	tmpl, err := FromSource(matchOutcomeTemplate)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.Set("outcome", outcome); err != nil {
		t.Fatalf("Set: %v", err)
	}
	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	return result
}

// A unit variant must reach the engine as a bare string so the match dispatches.
func TestGeneratedEnumUnitVariantRenders(t *testing.T) {
	if got := renderOutcome(t, teRejected{}); got != "NO\n" {
		t.Errorf("Rejected: got %q, want %q", got, "NO\n")
	}
	if got := renderOutcome(t, teNeedsWork{}); got != "MAYBE\n" {
		t.Errorf("NeedsWork: got %q, want %q", got, "MAYBE\n")
	}
}

// A data variant must reach the engine as a {"__kind__":...} object carrying
// its fields, so the match dispatches and the fields are accessible.
func TestGeneratedEnumDataVariantRenders(t *testing.T) {
	got := renderOutcome(t, teConfirmed{Evidence: "found it", Score: 7})
	if got != "YES: found it (7)\n" {
		t.Errorf("Confirmed: got %q, want %q", got, "YES: found it (7)\n")
	}
}

// The same value set through the interface type (as a generated Params field
// would hold it) must render identically — this is the exact production path.
func TestGeneratedEnumViaInterfaceRenders(t *testing.T) {
	var outcome teOutcome = teConfirmed{Evidence: "x", Score: 1}
	if got := renderOutcome(t, outcome); got != "YES: x (1)\n" {
		t.Errorf("via interface: got %q, want %q", got, "YES: x (1)\n")
	}
}

// MarshalJSON must emit the shared wire format: a bare string for unit
// variants, a __kind__-tagged object for data variants.
func TestGeneratedEnumMarshalJSON(t *testing.T) {
	unit, err := json.Marshal(teRejected{})
	if err != nil {
		t.Fatalf("marshal unit: %v", err)
	}
	if string(unit) != `"Rejected"` {
		t.Errorf("unit variant JSON: got %s, want %q", unit, `"Rejected"`)
	}

	data, err := json.Marshal(teConfirmed{Evidence: "e", Score: 3})
	if err != nil {
		t.Fatalf("marshal data: %v", err)
	}
	var decoded map[string]any
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("re-decode data: %v", err)
	}
	if decoded["__kind__"] != "Confirmed" {
		t.Errorf("data variant missing __kind__: %s", data)
	}
	if decoded["evidence"] != "e" {
		t.Errorf("data variant missing evidence: %s", data)
	}
	if decoded["score"].(float64) != 3 {
		t.Errorf("data variant missing score: %s", data)
	}
}

// A list of structs, each carrying an enum field, must round-trip every
// element's variant through the wire format. (The language has no bare
// list-of-enum type; enums live inside struct/list fields.)
func TestGeneratedEnumInListRenders(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - items = list(name = str, status = enum(Confirmed(evidence = str, score = int), Rejected, NeedsWork))
---
> {% for i in items %}
> {% match i.status %}
> {% case Confirmed %}

{{ i.name }}:C:{{ i.status.evidence }}

> {% case Rejected %}

{{ i.name }}:R

> {% case NeedsWork %}

{{ i.name }}:W

> {% /match %}
> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	items := []teItem{
		{Name: "a", Status: teConfirmed{Evidence: "alpha", Score: 1}},
		{Name: "b", Status: teRejected{}},
		{Name: "c", Status: teNeedsWork{}},
	}
	if err := ctx.Set("items", items); err != nil {
		t.Fatalf("Set: %v", err)
	}
	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	for _, want := range []string{"a:C:alpha", "b:R", "c:W"} {
		if !strings.Contains(result, want) {
			t.Errorf("list output %q missing %q", result, want)
		}
	}
}

// An option(enum) must render its Some payload through the wire format and
// omit the block entirely when None.
func TestGeneratedEnumOptionRenders(t *testing.T) {
	src := `---
params:
  - maybe = option(enum(Confirmed(evidence = str, score = int), Rejected, NeedsWork))
---
> {% if maybe %}
> {% match maybe %}
> {% case Confirmed %}

SOME:{{ maybe.evidence }}

> {% case Rejected %}

SOME:rejected

> {% case NeedsWork %}

SOME:needswork

> {% /match %}
> {% else %}

NONE

> {% /if %}`

	// Some(Confirmed)
	tmpl, err := FromSource(src)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	var some teOutcome = teConfirmed{Evidence: "z", Score: 2}
	if err := ctx.Set("maybe", &some); err != nil {
		t.Fatalf("Set(Some): %v", err)
	}
	got, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render(Some): %v", err)
	}
	if !strings.Contains(got, "SOME:z") {
		t.Errorf("option Some: got %q, want it to contain %q", got, "SOME:z")
	}

	// None
	ctxNone := NewContext()
	defer ctxNone.Close()
	if err := ctxNone.SetNone("maybe"); err != nil {
		t.Fatalf("SetNone: %v", err)
	}
	gotNone, err := tmpl.Render(ctxNone)
	if err != nil {
		t.Fatalf("Render(None): %v", err)
	}
	if !strings.Contains(gotNone, "NONE") {
		t.Errorf("option None: got %q, want it to contain %q", gotNone, "NONE")
	}
}
