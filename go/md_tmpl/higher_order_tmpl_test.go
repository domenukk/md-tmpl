package md_tmpl

import (
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// Template-typed parameters (tmpl())
// ---------------------------------------------------------------------------

func TestSetTmplParam(t *testing.T) {
	// Card template: renders a single item
	card, err := FromSourceAllowingUnused(`---
name: card
params: [title = str]
---
* {{ title }}`)
	if err != nil {
		t.Fatalf("card FromSource: %v", err)
	}
	defer card.Close()

	// Main template: takes card as tmpl() param
	main, err := FromSource(`---
params:
  - card = tmpl(title = str)
  - items = list(name = str)
---
> {% for item in items %}
> {% include card with title=item.name %}

> {% /for %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("card", card); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}
	if err := ctx.SetJSON("items", `[{"name":"Alpha"},{"name":"Beta"}]`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Alpha") {
		t.Errorf("expected 'Alpha' in output, got %q", result)
	}
	if !strings.Contains(result, "Beta") {
		t.Errorf("expected 'Beta' in output, got %q", result)
	}
}

func TestSetTmplNilErrors(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()

	err := ctx.SetTmpl("card", nil)
	if err == nil {
		t.Fatal("expected error for nil template, got nil")
	}
}

// ---------------------------------------------------------------------------
// Higher-order tmpl() comprehensive tests
// ---------------------------------------------------------------------------

func TestSetTmplTypeMismatch(t *testing.T) {
	// Helper declares (age = int), but main expects tmpl(name = str).
	helper, err := FromSourceAllowingUnused(`---
params: [age = int]
---
Age: {{ age }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [test = tmpl(name = str)]
---
> {% include test with name="World" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("test", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	_, err = main.Render(ctx)
	if err == nil {
		t.Fatal("expected error for tmpl type mismatch, got nil")
	}
	if !strings.Contains(err.Error(), "type mismatch") {
		t.Errorf("error should mention 'type mismatch': %v", err)
	}
}

func TestSetTmplWithDefaults(t *testing.T) {
	// Helper has extra defaulted param "greeting"; main only requires tmpl(name = str).
	helper, err := FromSourceAllowingUnused(`---
params:
  - name = str
  - greeting = str := "Hi"
---
{{ greeting }} {{ name }}!`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [test = tmpl(name = str)]
---
> {% include test with name="World" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("test", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hi World!" {
		t.Errorf("got %q, want %q", result, "Hi World!")
	}
}

func TestSetTmplNested(t *testing.T) {
	// Three-level nesting: main → processor → inner
	inner, err := FromSourceAllowingUnused(`---
params: [val = str]
---
Inner: {{ val }}`)
	if err != nil {
		t.Fatalf("inner FromSource: %v", err)
	}
	defer inner.Close()

	middle, err := FromSourceAllowingUnused(`---
params:
  - target = tmpl(val = str)
  - value = str
---
> {% include target with val=value %}`)
	if err != nil {
		t.Fatalf("middle FromSource: %v", err)
	}
	defer middle.Close()

	main, err := FromSource(`---
params:
  - processor = tmpl(target = tmpl(val = str), value = str)
  - callback = tmpl(val = str)
---
> {% include processor with target=callback, value="Success" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("processor", middle); err != nil {
		t.Fatalf("SetTmpl processor: %v", err)
	}
	if err := ctx.SetTmpl("callback", inner); err != nil {
		t.Fatalf("SetTmpl callback: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Inner: Success" {
		t.Errorf("got %q, want %q", result, "Inner: Success")
	}
}

func TestSetTmplEmptySignature(t *testing.T) {
	// tmpl() with empty parens: accepts a no-param template.
	helper, err := FromSourceAllowingUnused(`---
params: []
---
Preamble content here.`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [preamble = tmpl()]
---

> {% include preamble %}

Done.`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("preamble", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Preamble content here.") {
		t.Errorf("expected 'Preamble content here.' in output, got %q", result)
	}
	if !strings.Contains(result, "Done.") {
		t.Errorf("expected 'Done.' in output, got %q", result)
	}
}

func TestSetTmplExtraDefaultedParamsMatch(t *testing.T) {
	// Helper has extra defaulted param. Main expects tmpl(x = str).
	// This should match because the extra param has a default.
	helper, err := FromSourceAllowingUnused(`---
params:
  - x = str
  - extra = str := "fallback"
---
{{ x }} {{ extra }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [widget = tmpl(x = str)]
---
> {% include widget with x="hello" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("widget", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "hello fallback" {
		t.Errorf("got %q, want %q", result, "hello fallback")
	}
}

func TestSetTmplExtraRequiredParamsRejected(t *testing.T) {
	// Helper has extra REQUIRED param (no default). Main expects tmpl(x = str).
	// This should be rejected as a type mismatch.
	helper, err := FromSourceAllowingUnused(`---
params:
  - x = str
  - extra = str
---
{{ x }} {{ extra }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [widget = tmpl(x = str)]
---
> {% include widget with x="hello" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("widget", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	_, err = main.Render(ctx)
	if err == nil {
		t.Fatal("expected error for extra required param, got nil")
	}
}

func TestSetTmplMultiParamForwarding(t *testing.T) {
	// Main passes multiple params to tmpl(x = str, y = int).
	helper, err := FromSourceAllowingUnused(`---
params:
  - x = str
  - y = int
---
{{ x }}-{{ y }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params:
  - widget = tmpl(x = str, y = int)
  - label = str
  - num = int
---
Label: {{ label }}

> {% include widget with x="hello", y=num %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("widget", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}
	if err := ctx.SetStr("label", "test"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetInt("num", 42); err != nil {
		t.Fatalf("SetInt: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Label: test") {
		t.Errorf("expected 'Label: test' in output, got %q", result)
	}
	if !strings.Contains(result, "hello-42") {
		t.Errorf("expected 'hello-42' in output, got %q", result)
	}
}

func TestSetTmplNonTemplateValueRejected(t *testing.T) {
	// Passing a plain string for a tmpl() param must be rejected.
	main, err := FromSource(`---
params: [widget = tmpl(name = str)]
---
> {% include widget with name="test" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("widget", "not a template"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	_, err = main.Render(ctx)
	if err == nil {
		t.Fatal("expected error for non-template value in tmpl param, got nil")
	}
}

func TestSetTmplIsTruthy(t *testing.T) {
	// A tmpl() param should be truthy when set (works in {% if %} guards).
	helper, err := FromSourceAllowingUnused(`---
params: []
---
present`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [widget = tmpl()]
---
> {% if widget %}

yes

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("widget", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "yes") {
		t.Errorf("tmpl should be truthy, got %q", result)
	}
}

func TestOptionTmplSome(t *testing.T) {
	// option(tmpl(test = str)) with a template provided.
	helper, err := FromSourceAllowingUnused(`---
params: [test = str]
---
Helper: {{ test }}`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [cb = option(tmpl(test = str))]
---
> {% if has(cb) %}
> {% include cb with test="Hello Option" %}
> {% else %}

No callback

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("cb", helper); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Helper: Hello Option") {
		t.Errorf("expected 'Helper: Hello Option' in output, got %q", result)
	}
}

func TestOptionTmplNone(t *testing.T) {
	// option(tmpl(test = str)) with None (absent value).
	main, err := FromSource(`---
params: [cb = option(tmpl(test = str))]
---
> {% if has(cb) %}
> {% include cb with test="Hello" %}
> {% else %}

No callback

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetNone("cb"); err != nil {
		t.Fatalf("SetNone: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "No callback") {
		t.Errorf("expected 'No callback' in output, got %q", result)
	}
}

func TestOptionTmplNilViaSet(t *testing.T) {
	// option(tmpl(test = str)) with nil via Set → should render None path.
	main, err := FromSource(`---
params: [cb = option(tmpl(test = str))]
---
> {% if has(cb) %}
> {% include cb with test="Hello" %}
> {% else %}

absent

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.Set("cb", nil); err != nil {
		t.Fatalf("Set nil: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "absent") {
		t.Errorf("expected 'absent' in output, got %q", result)
	}
}

func TestNestedOptionTmpl(t *testing.T) {
	// Three-level nesting with option(tmpl()) at each level.
	inner, err := FromSourceAllowingUnused(`---
params: [test = str]
---
Inner: {{ test }}`)
	if err != nil {
		t.Fatalf("inner FromSource: %v", err)
	}
	defer inner.Close()

	middle, err := FromSourceAllowingUnused(`---
params: [sub = option(tmpl(test = str))]
---
> {% if has(sub) %}
> {% include sub with test="Nested Success" %}
> {% else %}

No sub

> {% /if %}`)
	if err != nil {
		t.Fatalf("middle FromSource: %v", err)
	}
	defer middle.Close()

	main, err := FromSource(`---
params:
  - cb = option(tmpl(sub = option(tmpl(test = str))))
  - target = option(tmpl(test = str))
---
> {% if has(cb) %}
> {% include cb with sub=target %}
> {% else %}

No cb

> {% /if %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetTmpl("cb", middle); err != nil {
		t.Fatalf("SetTmpl cb: %v", err)
	}
	if err := ctx.SetTmpl("target", inner); err != nil {
		t.Fatalf("SetTmpl target: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "Inner: Nested Success") {
		t.Errorf("expected 'Inner: Nested Success' in output, got %q", result)
	}
}

func TestSetTmplViaSetGenericAPI(t *testing.T) {
	// Using ctx.Set(key, *Template) should automatically route to SetTmpl.
	helper, err := FromSourceAllowingUnused(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatalf("helper FromSource: %v", err)
	}
	defer helper.Close()

	main, err := FromSource(`---
params: [greet = tmpl(name = str)]
---
> {% include greet with name="Generic" %}`)
	if err != nil {
		t.Fatalf("main FromSource: %v", err)
	}
	defer main.Close()

	ctx := NewContext()
	defer ctx.Close()
	// Use Set() instead of SetTmpl()
	if err := ctx.Set("greet", helper); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := main.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello Generic!" {
		t.Errorf("got %q, want %q", result, "Hello Generic!")
	}
}
