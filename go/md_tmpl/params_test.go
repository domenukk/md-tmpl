package md_tmpl

import (
	"errors"
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// Strict validation — extra params
// ---------------------------------------------------------------------------

func TestExtraParamRejected(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "world"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetStr("bogus", "unexpected"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if err == nil {
		t.Fatal("expected error for extra param, got nil")
	}
}

func TestAllowExtraIgnoresExtraParams(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "world"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetStr("bogus", "ignored"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx, AllowExtra())
	if err != nil {
		t.Fatalf("Render(AllowExtra): %v", err)
	}
	if result != "Hello world!" {
		t.Errorf("got %q, want %q", result, "Hello world!")
	}
}

// ---------------------------------------------------------------------------
// Typed lists (via JSON)
// ---------------------------------------------------------------------------

func TestTypedListViaJSON(t *testing.T) {
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

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("items", `[{"label":"alpha"},{"label":"beta"}]`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "alpha\nbeta\n" {
		t.Errorf("got %q, want %q", result, "alpha\nbeta\n")
	}
}

func TestTypedListViaSet(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - tasks = list(title = str, priority = str)
---
> {% for task in tasks %}

{{ task.title }}: {{ task.priority }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	tasks := []map[string]string{
		{"title": "Write documentation", "priority": "High"},
		{"title": "Fix typos", "priority": "Medium"},
	}
	if err := ctx.Set("tasks", tasks); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Write documentation: High") {
		t.Errorf("expected 'Write documentation: High' in output, got %q", result)
	}
}

func TestEmptyList(t *testing.T) {
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

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("items", `[]`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if strings.TrimSpace(result) != "" {
		t.Errorf("expected empty output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// Struct parameters
// ---------------------------------------------------------------------------

func TestStructParam(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("config", `{"host":"localhost","port":8080}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "localhost:8080" {
		t.Errorf("got %q, want %q", result, "localhost:8080")
	}
}

// ---------------------------------------------------------------------------
// Multiple param types
// ---------------------------------------------------------------------------

func TestMultipleParamTypes(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetInt("count", 42); err != nil {
		t.Fatalf("SetInt: %v", err)
	}
	if err := ctx.SetFloat("score", 9.5); err != nil {
		t.Fatalf("SetFloat: %v", err)
	}
	if err := ctx.SetBool("enabled", true); err != nil {
		t.Fatalf("SetBool: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	expected := "Alice: count=42, score=9.5, enabled=true"
	if result != expected {
		t.Errorf("got %q, want %q", result, expected)
	}
}

// ---------------------------------------------------------------------------
// Default values
// ---------------------------------------------------------------------------

func TestDefaultsUsedWhenOmitted(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str := "World"
  - count = int := 1
---
Hello {{ name }}, count={{ count }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello World, count=1!" {
		t.Errorf("got %q, want %q", result, "Hello World, count=1!")
	}
}

func TestDefaultsOverridden(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str := "World"
  - count = int := 1
---
Hello {{ name }}, count={{ count }}!`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "Alice"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetInt("count", 99); err != nil {
		t.Fatalf("SetInt: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello Alice, count=99!" {
		t.Errorf("got %q, want %q", result, "Hello Alice, count=99!")
	}
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

func TestClosedTemplateReturnsError(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("x", "test"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed, got %v", err)
	}
}

func TestClosedTemplateRenderMapReturnsError(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	tmpl.Close()

	_, err = tmpl.RenderMap(map[string]any{"x": "test"})
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed for RenderMap on closed template, got %v", err)
	}
}

func TestDoubleCloseIsNoOp(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	// Should not panic.
	tmpl.Close()
	tmpl.Close()
}

func TestContextSetVariousTypes(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()

	// All of these should succeed.
	if err := ctx.Set("s", "hello"); err != nil {
		t.Errorf("Set string: %v", err)
	}
	if err := ctx.Set("i", 42); err != nil {
		t.Errorf("Set int: %v", err)
	}
	if err := ctx.Set("i64", int64(99)); err != nil {
		t.Errorf("Set int64: %v", err)
	}
	if err := ctx.Set("i32", int32(7)); err != nil {
		t.Errorf("Set int32: %v", err)
	}
	if err := ctx.Set("f64", 3.14); err != nil {
		t.Errorf("Set float64: %v", err)
	}
	if err := ctx.Set("f32", float32(1.5)); err != nil {
		t.Errorf("Set float32: %v", err)
	}
	if err := ctx.Set("b", true); err != nil {
		t.Errorf("Set bool: %v", err)
	}
	if err := ctx.Set("list", []string{"a", "b"}); err != nil {
		t.Errorf("Set slice: %v", err)
	}
	if err := ctx.Set("dict", map[string]string{"k": "v"}); err != nil {
		t.Errorf("Set map: %v", err)
	}
}

func TestRenderMapBadType(t *testing.T) {
	tmpl, err := FromSource(`---
params: [count = int]
---
{{ count }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderMap(map[string]any{"count": "not a number"})
	if err == nil {
		t.Fatal("expected type mismatch error, got nil")
	}
}

// ---------------------------------------------------------------------------
// Sentinel errors
// ---------------------------------------------------------------------------

func TestNilContextReturnsError(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.Render(nil)
	if !errors.Is(err, ErrNilContext) {
		t.Fatalf("expected ErrNilContext, got %v", err)
	}
}

func TestClosedContextSetReturnsError(t *testing.T) {
	ctx := NewContext()
	ctx.Close()

	if err := ctx.SetStr("k", "v"); !errors.Is(err, ErrClosed) {
		t.Errorf("SetStr on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.SetInt("k", 1); !errors.Is(err, ErrClosed) {
		t.Errorf("SetInt on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.SetFloat("k", 1.0); !errors.Is(err, ErrClosed) {
		t.Errorf("SetFloat on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.SetBool("k", true); !errors.Is(err, ErrClosed) {
		t.Errorf("SetBool on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.SetJSON("k", `{}`); !errors.Is(err, ErrClosed) {
		t.Errorf("SetJSON on closed context: expected ErrClosed, got %v", err)
	}
	if err := ctx.Set("k", "v"); !errors.Is(err, ErrClosed) {
		t.Errorf("Set on closed context: expected ErrClosed, got %v", err)
	}
}

func TestClosedCacheLoadReturnsError(t *testing.T) {
	cache := NewCache()
	cache.Close()

	_, err := cache.Load("/some/path.tmpl.md")
	if !errors.Is(err, ErrClosed) {
		t.Fatalf("expected ErrClosed, got %v", err)
	}
}

// ---------------------------------------------------------------------------
// Unicode and special characters
// ---------------------------------------------------------------------------

func TestUnicodeContent(t *testing.T) {
	tmpl, err := FromSource(`---
params: [msg = str]
---
{{ msg }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	unicode := "Hello 🌍 こんにちは 世界 🦀"
	result, err := tmpl.RenderMap(map[string]any{"msg": unicode})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != unicode {
		t.Errorf("got %q, want %q", result, unicode)
	}
}

func TestEmptyStringParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
[{{ x }}]`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": ""})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "[]" {
		t.Errorf("got %q, want %q", result, "[]")
	}
}

// ---------------------------------------------------------------------------
// Large int and negative values
// ---------------------------------------------------------------------------

func TestNegativeInt(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = int]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": int64(-42)})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "-42" {
		t.Errorf("got %q, want %q", result, "-42")
	}
}

func TestLargeInt(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = int]
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"x": int64(9_223_372_036_854_775_807)})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "9223372036854775807" {
		t.Errorf("got %q, want %q", result, "9223372036854775807")
	}
}

// ---------------------------------------------------------------------------
// Boundary values
// ---------------------------------------------------------------------------

func TestZeroIntParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [val = int]
---
{{ val }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"val": 0})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "0" {
		t.Errorf("got %q, want %q", result, "0")
	}
}

func TestFalseBoolInIf(t *testing.T) {
	tmpl, err := FromSource(`---
params: [flag = bool]
---
> {% if flag %}

yes

> {% else %}

no

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"flag": false})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "no\n" {
		t.Errorf("got %q, want %q", result, "no\n")
	}
}

// ---------------------------------------------------------------------------
// Unicode
// ---------------------------------------------------------------------------

func TestUnicodeParam(t *testing.T) {
	tmpl, err := FromSource(`---
params: [greeting = str]
---
{{ greeting }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	unicode := "こんにちは 🌍"
	result, err := tmpl.RenderMap(map[string]any{"greeting": unicode})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != unicode {
		t.Errorf("got %q, want %q", result, unicode)
	}
}

// ---------------------------------------------------------------------------
// Nested structs
// ---------------------------------------------------------------------------

func TestNestedStructViaJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - config = struct(inner = struct(host = str))
---
{{ config.inner.host }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("config", `{"inner":{"host":"example.com"}}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "example.com" {
		t.Errorf("got %q, want %q", result, "example.com")
	}
}
