package md_tmpl

import (
	"strings"
	"testing"
)

// ---------------------------------------------------------------------------
// Duck typing / structural typing — extra fields silently ignored
// ---------------------------------------------------------------------------

func TestDuckTypingStructExtraFields(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - config = struct(host = str, port = int)
---
{{ config.host }}:{{ config.port }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// JSON value has extra fields "timeout" and "debug" not declared in the schema.
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("config", `{"host":"localhost","port":8080,"timeout":30,"debug":true}`); err != nil {
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

func TestDuckTypingListItemsExtraFields(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - items = list(name = str, value = int)
---
> {% for item in items %}

{{ item.name }}={{ item.value }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Each list item carries extra fields ("color", "weight") not in the declaration.
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("items", `[
		{"name":"alpha","value":1,"color":"red","weight":0.5},
		{"name":"beta","value":2,"color":"blue","weight":1.5}
	]`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	expected := "alpha=1\nbeta=2\n"
	if result != expected {
		t.Errorf("got %q, want %q", result, expected)
	}
}

func TestDuckTypingNestedStructExtraFieldsAtEveryDepth(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - root = struct(child = struct(leaf = struct(val = str)))
---
{{ root.child.leaf.val }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Extra fields at every nesting depth:
	//   root: has "extra_root"
	//   child: has "extra_child"
	//   leaf: has "extra_leaf"
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("root", `{
		"extra_root": 999,
		"child": {
			"extra_child": "ignored",
			"leaf": {
				"val": "deep",
				"extra_leaf": [1, 2, 3]
			}
		}
	}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "deep" {
		t.Errorf("got %q, want %q", result, "deep")
	}
}

func TestDuckTypingStructExtraFieldsViaRenderMap(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - user = struct(name = str, age = int)
---
{{ user.name }} is {{ user.age }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Provide extra fields via a Go map — they should be silently ignored.
	result, err := tmpl.RenderMap(map[string]any{
		"user": map[string]any{
			"name":     "Alice",
			"age":      int64(30),
			"email":    "alice@example.com",
			"verified": true,
		},
	})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Alice is 30" {
		t.Errorf("got %q, want %q", result, "Alice is 30")
	}
}

func TestDuckTypingListOfStructsExtraFieldsViaSet(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - tasks = list(title = str)
---
> {% for t in tasks %}

- {{ t.title }}

> {% /for %}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Each map item has extra keys beyond the declared "title".
	ctx := NewContext()
	defer ctx.Close()
	tasks := []map[string]any{
		{"title": "Write docs", "priority": "high", "done": false},
		{"title": "Fix bugs", "priority": "medium", "done": true},
	}
	if err := ctx.Set("tasks", tasks); err != nil {
		t.Fatalf("Set: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(result, "- Write docs") {
		t.Errorf("expected '- Write docs' in output, got %q", result)
	}
	if !strings.Contains(result, "- Fix bugs") {
		t.Errorf("expected '- Fix bugs' in output, got %q", result)
	}
}

func TestDuckTypingStructExtraFieldsViaRenderStruct(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - server = struct(host = str, port = int)
---
{{ server.host }}:{{ server.port }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Go struct has extra fields (Protocol, MaxConns) not declared in the template.
	type Server struct {
		Host     string `json:"host"`
		Port     int    `json:"port"`
		Protocol string `json:"protocol"`
		MaxConns int    `json:"max_conns"`
	}
	type Params struct {
		Server Server `json:"server"`
	}

	result, err := tmpl.RenderStruct(Params{Server: Server{
		Host:     "example.com",
		Port:     443,
		Protocol: "https",
		MaxConns: 1000,
	}})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "example.com:443" {
		t.Errorf("got %q, want %q", result, "example.com:443")
	}
}

func TestDuckTypingRejectsMissingRequiredFields(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - item = struct(name = str, score = int)
---
{{ item.name }}: {{ item.score }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	// Has extra fields but is MISSING the required "score" field.
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetJSON("item", `{"name":"Alice","bonus":99,"extra":true}`); err != nil {
		t.Fatalf("SetJSON: %v", err)
	}

	_, err = tmpl.Render(ctx)
	if err == nil {
		t.Fatal("expected error for missing required struct field 'score', got nil")
	}
	if !strings.Contains(err.Error(), "score") {
		t.Errorf("error should mention missing field 'score': %v", err)
	}
}
