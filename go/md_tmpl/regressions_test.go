package md_tmpl

import (
	"fmt"
	"io"
	"sync"
	"testing"
)

// ---------------------------------------------------------------------------
// Regression: io.Closer interface (Template, Context, Cache)
// ---------------------------------------------------------------------------

func TestTemplateImplementsIOCloser(t *testing.T) {
	var _ io.Closer = (*Template)(nil) // compile-time check

	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatal(err)
	}
	err = tmpl.Close()
	if err != nil {
		t.Fatal("Close should return nil:", err)
	}
}

func TestContextImplementsIOCloser(t *testing.T) {
	var _ io.Closer = (*Context)(nil) // compile-time check

	ctx := NewContext()
	err := ctx.Close()
	if err != nil {
		t.Fatal("Close should return nil:", err)
	}
}

func TestCacheImplementsIOCloser(t *testing.T) {
	var _ io.Closer = (*Cache)(nil) // compile-time check

	cache := NewCache()
	err := cache.Close()
	if err != nil {
		t.Fatal("Close should return nil:", err)
	}
}

// ---------------------------------------------------------------------------
// Regression: sync.Once double-close safety
// ---------------------------------------------------------------------------

func TestDoubleCloseTemplateNoPanic(t *testing.T) {
	tmpl, err := FromSource(`---
params: [x = str]
---
{{ x }}`)
	if err != nil {
		t.Fatal(err)
	}
	tmpl.Close()
	tmpl.Close() // must not panic
}

func TestDoubleCloseContextNoPanic(t *testing.T) {
	ctx := NewContext()
	ctx.Close()
	ctx.Close() // must not panic
}

func TestDoubleCloseCacheNoPanic(t *testing.T) {
	cache := NewCache()
	cache.Close()
	cache.Close() // must not panic
}

// ---------------------------------------------------------------------------
// Regression: json.Marshal preserves exact field names (no lowercasing)
// ---------------------------------------------------------------------------

func TestRenderStructPreservesFieldNames(t *testing.T) {
	type NoTags struct {
		UserName string
		Score    int64
	}

	// json.Marshal uses the exact Go field name (no lowercasing),
	// so template params must match exactly.
	tmpl, err := FromSource(`---
params: [UserName = str, Score = int]
---
{{ UserName }}: {{ Score }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderStruct(NoTags{UserName: "alice", Score: 99})
	if err != nil {
		t.Fatalf("RenderStruct: %v", err)
	}
	if result != "alice: 99" {
		t.Errorf("got %q, want %q", result, "alice: 99")
	}
}

// ---------------------------------------------------------------------------
// Regression: concurrent rendering safety
// ---------------------------------------------------------------------------

func TestConcurrentRenderMapRegression(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
Hello {{ name }}!`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			result, err := tmpl.RenderMap(map[string]any{"name": fmt.Sprintf("goroutine-%d", n)})
			if err != nil {
				t.Errorf("render failed: %v", err)
				return
			}
			expected := fmt.Sprintf("Hello goroutine-%d!", n)
			if result != expected {
				t.Errorf("got %q, want %q", result, expected)
			}
		}(i)
	}
	wg.Wait()
}

// ---------------------------------------------------------------------------
// Regression: numeric type coverage in Context.Set()
// ---------------------------------------------------------------------------

func TestAllNumericTypes(t *testing.T) {
	tmpl, err := FromSource(`---
params: [n = int]
---
{{ n }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	testCases := []struct {
		name string
		val  any
	}{
		{"int", int(42)},
		{"int8", int8(42)},
		{"int16", int16(42)},
		{"int32", int32(42)},
		{"int64", int64(42)},
		{"uint", uint(42)},
		{"uint8", uint8(42)},
		{"uint16", uint16(42)},
		{"uint32", uint32(42)},
	}
	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			result, err := tmpl.RenderMap(map[string]any{"n": tc.val})
			if err != nil {
				t.Fatalf("type %s: %v", tc.name, err)
			}
			if result != "42" {
				t.Errorf("type %s: got %q, want %q", tc.name, result, "42")
			}
		})
	}
}

// ---------------------------------------------------------------------------
// Regression: RenderMap / RenderMap(AllowExtra()) refactoring (renderMapWith)
// ---------------------------------------------------------------------------

func TestRenderMapStillRejectsExtra(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	_, err = tmpl.RenderMap(map[string]any{"name": "ok", "extra": "bad"})
	if err == nil {
		t.Fatal("RenderMap should reject extra params")
	}
}

func TestRenderMapAllowExtraStillWorks(t *testing.T) {
	tmpl, err := FromSource(`---
params: [name = str]
---
{{ name }}`)
	if err != nil {
		t.Fatal(err)
	}
	defer tmpl.Close()

	result, err := tmpl.RenderMap(map[string]any{"name": "ok", "extra": "ignored"}, AllowExtra())
	if err != nil {
		t.Fatal(err)
	}
	if result != "ok" {
		t.Errorf("got %q, want %q", result, "ok")
	}
}
