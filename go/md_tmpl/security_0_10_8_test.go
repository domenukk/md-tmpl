package md_tmpl

import (
	"errors"
	"math"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func TestFenceStripsEmbeddedNulBeforeBacktickCount(t *testing.T) {
	fenced, err := Fence("``\x00``\nESCAPED", "")
	if err != nil {
		t.Fatalf("Fence failed: %v", err)
	}
	if !strings.HasPrefix(fenced, "`````\n") || !strings.HasSuffix(fenced, "\n`````") {
		t.Fatalf("expected 5-backtick fence for NUL-split 4-backtick payload, got %q", fenced)
	}
}

func TestContextRejectsReservedKindKeyWithoutPanic(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()

	if err := ctx.SetStr("__kind__", "Admin"); err == nil {
		t.Fatal("expected SetStr(\"__kind__\", ...) to fail")
	}
	if err := ctx.MergeJSON(`{"__kind__":"Admin"}`); err == nil {
		t.Fatal("expected MergeJSON with __kind__ to fail")
	}

	tmpl, err := FromSource(`---
params: []
---
ok`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	if _, err := tmpl.RenderJSON(`{"__kind__":"Admin"}`, AllowExtra()); err == nil {
		t.Fatal("expected RenderJSON with __kind__ to fail")
	}
	if _, err := tmpl.RenderMap(map[string]any{"__kind__": "Admin"}, AllowExtra()); err == nil {
		t.Fatal("expected RenderMap with __kind__ to fail")
	}
}

func TestContextAndFlexbuffersRejectUint64Overflow(t *testing.T) {
	ctx := NewContext()
	defer ctx.Close()

	var over uint64 = uint64(math.MaxInt64) + 1
	if err := ctx.Set("n", over); err == nil {
		t.Fatal("expected Context.Set(uint64 > MaxInt64) to return overflow error")
	}
	if err := ctx.Set("u", uint(over)); err == nil {
		t.Fatal("expected Context.Set(uint > MaxInt64) to return overflow error")
	}

	tmpl, err := FromSource(`---
params:
  - n = int
---
{{ n }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	if _, err := tmpl.RenderMap(map[string]any{"n": over}); err == nil {
		t.Fatal("expected RenderMap(uint64 > MaxInt64) to return overflow error")
	}
}

func TestUnquarantineRejectsInvalidUTF8MatchingIsQuarantined(t *testing.T) {
	invalidEnv := "<untrusted_content>\n\xff\n</untrusted_content>"
	if IsQuarantined(invalidEnv, "") {
		t.Fatal("IsQuarantined must reject invalid UTF-8")
	}
	if got, ok := Unquarantine(invalidEnv, ""); ok {
		t.Fatalf("Unquarantine must reject invalid UTF-8 matching IsQuarantined, got %q", got)
	}
}

func TestValidateDeclarationsRejectsStrToTrustedStrDowngrade(t *testing.T) {
	safeTmpl, err := FromSource(`---
params:
  - x = str
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource safe: %v", err)
	}
	defer safeTmpl.Close()

	downgraded, err := FromSource(`---
params:
  - x = trusted str
---
{{ x }}`)
	if err != nil {
		t.Fatalf("FromSource downgraded: %v", err)
	}
	defer downgraded.Close()

	decls := safeTmpl.Declarations()
	if err := safeTmpl.ValidateDeclarations(decls); err != nil {
		t.Fatalf("expected matching declarations to pass: %v", err)
	}
	if err := downgraded.ValidateDeclarations(decls); err == nil {
		t.Fatal("expected str -> trusted str downgrade to fail ValidateDeclarations")
	} else if !errors.Is(err, ErrDeclarationsMutated) {
		t.Fatalf("expected ErrDeclarationsMutated, got %v", err)
	}
}

func TestTruncatePlaceholdersValidateAgainstTemplateAndCodegenImports(t *testing.T) {
	if TruncatePlaceholderSkipped != "{skipped}" || TruncatePlaceholderCount != "{count}" {
		t.Fatalf("unexpected truncate placeholders: %q / %q", TruncatePlaceholderSkipped, TruncatePlaceholderCount)
	}

	base, err := FromSource(`---
params:
  - a = int
  - doc = str | sanitize("doc", "Notice A")
---
{{ a }} {{ doc }}`)
	if err != nil {
		t.Fatalf("FromSource base: %v", err)
	}
	defer base.Close()

	reordered, err := FromSource(`---
params:
  - doc = str | sanitize("doc", "Notice A")
  - a = int
---
{{ a }} {{ doc }}`)
	if err != nil {
		t.Fatalf("FromSource reordered: %v", err)
	}
	defer reordered.Close()

	dropped, err := FromSource(`---
params:
  - a = int
  - doc = str
---
{{ a }} {{ doc }}`)
	if err != nil {
		t.Fatalf("FromSource dropped: %v", err)
	}
	defer dropped.Close()

	if err := reordered.ValidateAgainstTemplate(base); err != nil {
		t.Fatalf("expected reordered matching template to pass: %v", err)
	}
	if err := dropped.ValidateAgainstTemplate(base); !errors.Is(err, ErrDeclarationsMutated) {
		t.Fatalf("expected ErrDeclarationsMutated on block sanitize drop, got %v", err)
	}

	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "shared.tmpl.md"), []byte(`---
allow_unused: true
types:
  - Mode = enum(Fast, Safe)
---
shared`), 0o600); err != nil {
		t.Fatalf("write shared: %v", err)
	}
	consumerPath := filepath.Join(dir, "consumer.tmpl.md")
	if err := os.WriteFile(consumerPath, []byte(`---
imports:
  - "[shared](./shared.tmpl.md)"

params:
  - mode = shared.Mode
  - doc = str | sanitize("doc")
---
{{ kind(mode) }} {{ doc }}`), 0o600); err != nil {
		t.Fatalf("write consumer: %v", err)
	}

	code, err := GenerateTypesFromFile(consumerPath)
	if err != nil {
		t.Fatalf("GenerateTypesFromFile with imports failed: %v", err)
	}
	if !strings.Contains(code, "type Mode string") || !strings.Contains(code, "Doc string") {
		t.Fatalf("unexpected generated Go source:\n%s", code)
	}
}

func TestConcurrentSetMaxIncludeDepthAndRender(t *testing.T) {
	tmpl, err := FromSource(`---
params:
  - name = str
---
Hello {{ name }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(2)
		go func(d int) {
			defer wg.Done()
			tmpl.SetMaxIncludeDepth(8 + d)
		}(i)
		go func() {
			defer wg.Done()
			out, rerr := tmpl.RenderMap(map[string]any{"name": "world"})
			if rerr != nil || out != "Hello world" {
				t.Errorf("concurrent RenderMap = %q, %v", out, rerr)
			}
		}()
	}
	wg.Wait()
}

func TestConcurrentSetTmplRenderAndValidateAgainstTemplateNoDeadlock(t *testing.T) {
	child, err := FromSource(`---
params:
  - msg = str
---
{{ msg }}`)
	if err != nil {
		t.Fatalf("FromSource child: %v", err)
	}
	defer child.Close()

	host, err := FromSource(`---
params:
  - slot = tmpl(msg = str)
  - msg = str
---
> {% include slot with msg = msg %}`)
	if err != nil {
		t.Fatalf("FromSource host: %v", err)
	}
	defer host.Close()

	if err := child.ValidateAgainstTemplate(child); err != nil {
		t.Fatalf("self ValidateAgainstTemplate failed: %v", err)
	}

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("msg", "hello"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	if err := ctx.SetTmpl("slot", child); err != nil {
		t.Fatalf("SetTmpl: %v", err)
	}

	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(4)
		go func() {
			defer wg.Done()
			if serr := ctx.SetTmpl("slot", child); serr != nil {
				t.Errorf("concurrent SetTmpl = %v", serr)
			}
		}()
		go func() {
			defer wg.Done()
			out, rerr := host.Render(ctx)
			if rerr != nil || out != "hello" {
				t.Errorf("concurrent host.Render = %q, %v", out, rerr)
			}
		}()
		go func() {
			defer wg.Done()
			if verr := child.ValidateAgainstTemplate(child); verr != nil {
				t.Errorf("self ValidateAgainstTemplate = %v", verr)
			}
		}()
		go func(idx int) {
			defer wg.Done()
			var verr error
			if idx%2 == 0 {
				verr = host.ValidateAgainstTemplate(child)
			} else {
				verr = child.ValidateAgainstTemplate(host)
			}
			if verr == nil {
				t.Errorf("expected declaration mismatch between host and child")
			}
		}(i)
	}
	wg.Wait()

	childH1, err := FromSource(`---
params:
  - heading = trusted str
---
# {{ heading }}`)
	if err != nil {
		t.Fatalf("FromSource childH1: %v", err)
	}
	defer childH1.Close()

	hostH1, err := FromSource(`---
params:
  - slot = tmpl(heading = str)
  - title = str
---
> {% include slot with heading = title %}`)
	if err != nil {
		t.Fatalf("FromSource hostH1: %v", err)
	}
	defer hostH1.Close()

	ctxH1 := NewContext()
	defer ctxH1.Close()
	if err := ctxH1.SetTmpl("slot", childH1); err != nil {
		t.Fatalf("SetTmpl slot: %v", err)
	}
	if err := ctxH1.SetStr("title", "Safe\n## Injected"); err != nil {
		t.Fatalf("SetStr title: %v", err)
	}
	renderedH1, err := hostH1.Render(ctxH1)
	if err != nil {
		t.Fatalf("hostH1.Render: %v", err)
	}
	if renderedH1 != "# Safe ## Injected" {
		t.Fatalf("expected collapsed ATX heading, got %q", renderedH1)
	}
}
