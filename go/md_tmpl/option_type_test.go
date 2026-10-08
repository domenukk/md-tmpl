package md_tmpl

import (
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// option(T) support
// ---------------------------------------------------------------------------

func TestOptionNoneViaMatch(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% match label %}
> {% case Some %}

got:{{ label }}

> {% case None %}

empty

> {% /match %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"label": nil})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}
}

func TestOptionSomeViaMatch(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% match label %}
> {% case Some %}

got:{{ label }}

> {% case None %}

empty

> {% /match %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"label": "hello",
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "got:hello") {
		t.Errorf("expected 'got:hello' in output, got %q", result)
	}
}

func TestOptionNoneViaHas(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"label": nil})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}
}

func TestOptionSomeViaHas(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{
		"label": "world",
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "got:world") {
		t.Errorf("expected 'got:world' in output, got %q", result)
	}
}

func TestOptionIntNoneViaJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - count = option(int)
---
> {% if has(count) %}

count={{ count }}

> {% else %}

no-count

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("count", `null`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if strings.TrimSpace(result) != "no-count" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "no-count")
	}
}

func TestOptionIntSomeViaJSON(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - count = option(int)
---
> {% if has(count) %}

count={{ count }}

> {% else %}

no-count

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("count", `42`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "count=42") {
		t.Errorf("expected 'count=42' in output, got %q", result)
	}
}

// ---------------------------------------------------------------------------
// option(T) regression tests — transparent API
// ---------------------------------------------------------------------------

func TestOptionSetNoneDirect(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetNone("label"); err != nil {
		t.Fatalf("SetNone: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}
}

func TestOptionSetNilViaSet(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	// Set(key, nil) should transparently call SetNone
	if err := ctx.Set("label", nil); err != nil {
		t.Fatalf("Set(nil): %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}
}

func TestOptionNilInRenderMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = option(str)
  - score = option(int)
---
> {% if has(name) %}

{{ name }}

> {% else %}

anon

> {% /if %}
> {% if has(score) %}

({{ score }})

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// nil for name, 42 for score
	result, err := tmpl.RenderMap(map[string]any{
		"name":  nil,
		"score": 42,
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "anon") {
		t.Errorf("expected 'anon' in output (nil name), got %q", result)
	}
	if !strings.Contains(result, "(42)") {
		t.Errorf("expected '(42)' in output, got %q", result)
	}
}

func TestOptionNilPointerInRenderStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - label = option(str)
---
> {% if has(label) %}

got:{{ label }}

> {% else %}

empty

> {% /if %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	type Params struct {
		Label *string `json:"label"`
	}

	// nil pointer should map to None
	result, err := tmpl.RenderStruct(Params{Label: nil})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if strings.TrimSpace(result) != "empty" {
		t.Errorf("got %q, want %q", strings.TrimSpace(result), "empty")
	}

	// non-nil pointer should map to Some
	s := "hello"
	result, err = tmpl.RenderStruct(Params{Label: &s})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if !strings.Contains(result, "got:hello") {
		t.Errorf("expected 'got:hello' in output, got %q", result)
	}
}

func TestOptionMatchNilViaRenderMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - x = option(int)
---
> {% match x %}
> {% case Some %}

val={{ x }}

> {% case None %}

absent

> {% /match %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// nil → None arm
	result, err := tmpl.RenderMap(map[string]any{"x": nil})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if strings.TrimSpace(result) != "absent" {
		t.Errorf("nil: got %q, want %q", strings.TrimSpace(result), "absent")
	}

	// 99 → Some arm
	result, err = tmpl.RenderMap(map[string]any{"x": 99})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if !strings.Contains(result, "val=99") {
		t.Errorf("99: expected 'val=99' in output, got %q", result)
	}
}
