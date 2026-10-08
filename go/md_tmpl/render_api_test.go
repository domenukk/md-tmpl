package md_tmpl

import (
	"errors"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// RenderMap convenience
// ---------------------------------------------------------------------------

func TestRenderMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"name":  "Alice",
		"count": int64(42),
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Alice: 42" {
		t.Errorf("got %q, want %q", result, "Alice: 42")
	}
}

func TestRenderMapWithInt(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = int]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": 7})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "7" {
		t.Errorf("got %q, want %q", result, "7")
	}
}

func TestRenderMapWithFloat(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = float]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": 2.5})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "2.5" {
		t.Errorf("got %q, want %q", result, "2.5")
	}
}

func TestRenderMapWithBool(t *testing.T) {
	tmpl, err := FromSource(`---
params: [flag = bool]
---
{{ flag }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"flag": true})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "true" {
		t.Errorf("got %q, want %q", result, "true")
	}
}

// ---------------------------------------------------------------------------
// RenderMap with AllowExtra
// ---------------------------------------------------------------------------

func TestRenderMapAllowExtra(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"name":  "Alice",
		"extra": "ignored",
	}, AllowExtra())
	if err != nil {
		t.Fatalf("RenderMap(AllowExtra): %v", err)
	}
	if result != "Hello Alice!" {
		t.Errorf("got %q, want %q", result, "Hello Alice!")
	}
}

// ---------------------------------------------------------------------------
// JSON parse errors
// ---------------------------------------------------------------------------

func TestSetJSONParseError(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()
	err := ctx.SetJSON("x", "invalid json {{")
	if err == nil {
		t.Fatal("expected error for invalid JSON, got nil")
	}
}

// ---------------------------------------------------------------------------
// RenderStruct — struct-based rendering
// ---------------------------------------------------------------------------

func TestRenderStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name  string `json:"name"`
		Count int    `json:"count"`
	}

	result, err := tmpl.RenderStruct(Params{Name: "Alice", Count: 42})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "Alice: 42" {
		t.Errorf("got %q, want %q", result, "Alice: 42")
	}
}

func TestRenderStructWithPointer(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name string `json:"name"`
	}

	result, err := tmpl.RenderStruct(&Params{Name: "Bob"})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "Bob" {
		t.Errorf("got %q, want %q", result, "Bob")
	}
}

func TestRenderStructWithNestedStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Config struct {
		Host string `json:"host"`
		Port int    `json:"port"`
	}
	type Params struct {
		Config Config `json:"config"`
	}

	result, err := tmpl.RenderStruct(Params{Config: Config{Host: "localhost", Port: 8080}})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "localhost:8080" {
		t.Errorf("got %q, want %q", result, "localhost:8080")
	}
}

func TestRenderStructWithEnum(t *testing.T) {
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

func TestRenderStructWithSlice(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - items = list(label = str)
---
> {% for item in items %}

{{ item.label }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Item struct {
		Label string `json:"label"`
	}
	type Params struct {
		Items []Item `json:"items"`
	}

	result, err := tmpl.RenderStruct(Params{
		Items: []Item{{Label: "alpha"}, {Label: "beta"}},
	})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "alpha\nbeta\n" {
		t.Errorf("got %q, want %q", result, "alpha\nbeta\n")
	}
}

func TestRenderStructNotAStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderStruct("not a struct")
	if err == nil {
		t.Fatal("expected error for non-struct, got nil")
	}
}

func TestRenderStructOmitempty(t *testing.T) {
	tmpl, err := FromSourceAllowingUnused(`---
params:
  - name = str
  - tag = str := "default"
---
{{ name }} {{ tag }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Name string `json:"name"`
		Tag  string `json:"tag,omitempty"`
	}

	result, err := tmpl.RenderStruct(Params{Name: "Alice"})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "Alice") {
		t.Errorf("expected Alice in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// RenderJSON — JSON string-based rendering
// ---------------------------------------------------------------------------

func TestRenderJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str, count = int]
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderJSON(`{"name": "Alice", "count": 42}`)
	if err != nil {
		t.Fatal(err)
	}
	if result != "Alice: 42" {
		t.Errorf("got %q, want %q", result, "Alice: 42")
	}
}

func TestRenderJSONAllTypes(t *testing.T) {
	tmpl, err := FromSource(`---
params: [s = str, i = int, f = float, b = bool]
---
{{ s }} {{ i }} {{ f }} {{ b }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderJSON(`{"s": "hello", "i": 99, "f": 3.14, "b": true}`)
	if err != nil {
		t.Fatal(err)
	}
	if result != "hello 99 3.14 true" {
		t.Errorf("got %q, want %q", result, "hello 99 3.14 true")
	}
}

func TestRenderJSONInvalidJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderJSON(`not json`)
	if err == nil {
		t.Fatal("expected error for invalid JSON")
	}
}

func TestRenderJSONNotObject(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderJSON(`[1, 2, 3]`)
	if err == nil {
		t.Fatal("expected error for non-object JSON")
	}
}

func TestRenderJSONAllowExtra(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderJSON(`{"name": "Alice", "extra": "ignored"}`, AllowExtra())
	if err != nil {
		t.Fatal(err)
	}
	if result != "Hello Alice!" {
		t.Errorf("got %q, want %q", result, "Hello Alice!")
	}
}

// TestAllowExtraOption verifies the unified AllowExtra render option: strict
// mode (the default) rejects undeclared parameters with a typed ExtraParams
// error, while AllowExtra() permits them across every render form.
func TestAllowExtraOption(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	extra := map[string]any{"name": "Alice", "extra": "ignored"}

	// Strict (default): the undeclared "extra" key must produce a typed error.
	if _, err := tmpl.RenderMap(extra); err == nil {
		t.Fatal("strict RenderMap: expected ExtraParams error, got nil")
	} else if !errors.Is(err, ErrExtraParams) {
		t.Fatalf("strict RenderMap: expected ErrExtraParams, got %v", err)
	}

	// AllowExtra() permits the undeclared key for each render form.
	got, err := tmpl.RenderMap(extra, AllowExtra())
	if err != nil {
		t.Fatalf("RenderMap(AllowExtra): %v", err)
	}
	if got != "Hello Alice!" {
		t.Errorf("RenderMap(AllowExtra): got %q, want %q", got, "Hello Alice!")
	}

	got, err = tmpl.RenderJSON(`{"name": "Alice", "extra": "ignored"}`, AllowExtra())
	if err != nil {
		t.Fatalf("RenderJSON(AllowExtra): %v", err)
	}
	if got != "Hello Alice!" {
		t.Errorf("RenderJSON(AllowExtra): got %q, want %q", got, "Hello Alice!")
	}

	type params struct {
		Name  string `json:"name"`
		Extra string `json:"extra"`
	}
	got, err = tmpl.RenderStruct(params{Name: "Alice", Extra: "ignored"}, AllowExtra())
	if err != nil {
		t.Fatalf("RenderStruct(AllowExtra): %v", err)
	}
	if got != "Hello Alice!" {
		t.Errorf("RenderStruct(AllowExtra): got %q, want %q", got, "Hello Alice!")
	}

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetStr("extra", "ignored"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	got, err = tmpl.Render(ctx, AllowExtra())
	if err != nil {
		t.Fatalf("Render(AllowExtra): %v", err)
	}
	if got != "Hello Alice!" {
		t.Errorf("Render(AllowExtra): got %q, want %q", got, "Hello Alice!")
	}
}

func TestRenderStructUsesJSONPath(t *testing.T) {
	// Existing RenderStruct should still pass via the JSON marshal path.
	tmpl, err := FromSource(`---
params: [name = str, count = int]
---
{{ name }}: {{ count }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	type P struct {
		Name  string `json:"name"`
		Count int    `json:"count"`
	}
	result, err := tmpl.RenderStruct(P{Name: "Bob", Count: 7})
	if err != nil {
		t.Fatal(err)
	}
	if result != "Bob: 7" {
		t.Errorf("got %q, want %q", result, "Bob: 7")
	}
}

func TestMergeJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params: [a = str, b = int]
---
{{ a }} {{ b }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.MergeJSON(`{"a": "hello", "b": 42}`); err != nil {
		t.Fatal(err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result != "hello 42" {
		t.Errorf("got %q, want %q", result, "hello 42")
	}
}
