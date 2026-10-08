package md_tmpl

import (
	"testing"
)

// ---------------------------------------------------------------------------
// FromSourceWithEnv — compile-time environment variables
// ---------------------------------------------------------------------------

func TestEnvBasicStrSubstitution(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env: [MODEL = str]

params: []
---
Model: {{ MODEL }}`, map[string]any{"MODEL": "gpt-4"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Model: gpt-4" {
		t.Errorf("got %q, want %q", result, "Model: gpt-4")
	}
}

func TestEnvDefaultUsed(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env:
  - MODEL = str := "gpt-3.5"

params: []
---
Model: {{ MODEL }}`, map[string]any{})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Model: gpt-3.5" {
		t.Errorf("got %q, want %q", result, "Model: gpt-3.5")
	}
}

func TestEnvDefaultOverridden(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env:
  - MODEL = str := "gpt-3.5"

params: []
---
Model: {{ MODEL }}`, map[string]any{"MODEL": "gpt-4"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Model: gpt-4" {
		t.Errorf("got %q, want %q", result, "Model: gpt-4")
	}
}

func TestEnvMissingRequiredErrors(t *testing.T) {
	_, err := FromSourceWithEnv(`---
env: [REQUIRED_PATH = str]

params: []
---
{{ REQUIRED_PATH }}`, map[string]any{})
	if err == nil {
		t.Fatal("expected error for missing required env var, got nil")
	}
}

func TestEnvCoexistsWithParams(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env: [PREFIX = str]

params: [name = str]
---
{{ PREFIX }}/{{ name }}`, map[string]any{"PREFIX": "/opt/prompts"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"name": "agent_x"})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "/opt/prompts/agent_x" {
		t.Errorf("got %q, want %q", result, "/opt/prompts/agent_x")
	}
}

func TestEnvIntType(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env: [MAX_RETRIES = int]

params: []
---
Retries: {{ MAX_RETRIES }}`, map[string]any{"MAX_RETRIES": "5"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "Retries: 5" {
		t.Errorf("got %q, want %q", result, "Retries: 5")
	}
}

func TestEnvBoolType(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env: [DEBUG = bool]

params: []
---
> {% if DEBUG %}debug_on{% else %}debug_off{% /if %}`, map[string]any{"DEBUG": "true"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "debug_on" {
		t.Errorf("got %q, want %q", result, "debug_on")
	}
}

func TestEnvMultipleDefaultsPartialOverride(t *testing.T) {
	tmpl, err := FromSourceWithEnv(`---
env:
  - A = str := "alpha"
  - B = int := 42
  - C = bool := true

params: []
---
{{ A }}-{{ B }}-{% if C %}yes{% else %}no{% /if %}`, map[string]any{"A": "beta"})
	if err != nil {
		t.Fatalf("FromSourceWithEnv: %v", err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{})
	if err != nil {
		t.Fatalf("RenderMap: %v", err)
	}
	if result != "beta-42-yes" {
		t.Errorf("got %q, want %q", result, "beta-42-yes")
	}
}
