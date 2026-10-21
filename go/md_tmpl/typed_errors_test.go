package md_tmpl

import (
	"errors"
	"reflect"
	"testing"
)

// asTemplateError unwraps err into a *TemplateError or fails the test.
func asTemplateError(t *testing.T, err error) *TemplateError {
	t.Helper()
	if err == nil {
		t.Fatal("expected an error, got nil")
	}
	var te *TemplateError
	if !errors.As(err, &te) {
		t.Fatalf("expected *TemplateError, got %T: %v", err, err)
	}
	return te
}

func TestTypedErrorPayloadParsed(t *testing.T) {
	te := asTemplateError(t, parseError("type_mismatch"+errKindSep+"type mismatch"+errKindSep+
		`{"path":"user.age","expected":"int","actual":"str"}`))
	if te.Kind != KindTypeMismatch || te.Message != "type mismatch" {
		t.Fatalf("Kind/Message = %q/%q", te.Kind, te.Message)
	}
	if te.Path != "user.age" || te.Expected != "int" || te.Actual != "str" {
		t.Errorf("payload = %q %q %q", te.Path, te.Expected, te.Actual)
	}
}

func TestTypedErrorMalformedPayloadKeepsKind(t *testing.T) {
	te := asTemplateError(t, parseError("syntax"+errKindSep+"bad"+errKindSep+"{not json"))
	if te.Kind != KindSyntax {
		t.Errorf("Kind = %q, want %q", te.Kind, KindSyntax)
	}
}

func TestTypedSyntaxErrorHasFileAbsoluteLine(t *testing.T) {
	_, err := FromSource(`---
params:
  - name = str
---
intro
{{ name | bogus }}`)
	te := asTemplateError(t, err)
	if te.Kind != KindSyntax {
		t.Errorf("Kind = %q, want %q", te.Kind, KindSyntax)
	}
	if te.Line != 6 {
		t.Errorf("Line = %d, want 6 (file-absolute, frontmatter included)", te.Line)
	}
	if te.Snippet != "{{ name | bogus }}" {
		t.Errorf("Snippet = %q", te.Snippet)
	}
}

func TestTypedMissingAndExtraParams(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - a = str
  - b = str
---
{{ a }}{{ b }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderEmpty()
	te := asTemplateError(t, err)
	if te.Kind != KindMissingParams || !reflect.DeepEqual(te.Missing, []string{"a", "b"}) {
		t.Errorf("Kind/Missing = %q/%v", te.Kind, te.Missing)
	}

	_, err = tmpl.RenderMap(map[string]any{"a": "x", "b": "y", "zz": "extra"})
	te = asTemplateError(t, err)
	if te.Kind != KindExtraParams || !reflect.DeepEqual(te.Extra, []string{"zz"}) {
		t.Errorf("Kind/Extra = %q/%v", te.Kind, te.Extra)
	}
}

func TestTypedTypeMismatch(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - n = int
---
{{ n }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()
	_, err = tmpl.RenderMap(map[string]any{"n": "not a number"})
	te := asTemplateError(t, err)
	if te.Kind != KindTypeMismatch || te.Path != "n" || te.Expected != "int" || te.Actual != "str" {
		t.Errorf("got Kind=%q Path=%q Expected=%q Actual=%q", te.Kind, te.Path, te.Expected, te.Actual)
	}
}
