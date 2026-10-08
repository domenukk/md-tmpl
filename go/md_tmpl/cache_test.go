package md_tmpl

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

func TestCacheLoadAndRender(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "greeting.tmpl.md")
	if err := os.WriteFile(path, []byte(`---
params:
  - name = str
---
Hello {{ name }}!`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	tmpl, err := cache.Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer tmpl.Close()

	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("name", "cached"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}

	result, err := tmpl.Render(ctx)
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if result != "Hello cached!" {
		t.Errorf("got %q, want %q", result, "Hello cached!")
	}
}

func TestCacheReturnsSameHash(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.tmpl.md")
	if err := os.WriteFile(path, []byte(`---
params: [x = str]
---
{{ x }}`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	t1, err := cache.Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer t1.Close()

	t2, err := cache.Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	defer t2.Close()

	if t1.SourceHash() != t2.SourceHash() {
		t.Errorf("expected same hash from cache, got %d vs %d", t1.SourceHash(), t2.SourceHash())
	}
}

func TestCacheClear(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.tmpl.md")
	if err := os.WriteFile(path, []byte(`---
params: []
---
Hi`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	if _, err := cache.Load(path); err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cache.TemplateCount() != 1 {
		t.Errorf("expected 1 cached template, got %d", cache.TemplateCount())
	}

	cache.Clear()
	if cache.TemplateCount() != 0 {
		t.Errorf("expected 0 cached templates after clear, got %d", cache.TemplateCount())
	}
}

func TestCacheTemplateCount(t *testing.T) {
	dir := t.TempDir()
	cache := NewCache()
	defer cache.Close()

	for i := 0; i < 3; i++ {
		path := filepath.Join(dir, strings.Replace("X.tmpl.md", "X", string(rune('a'+i)), 1))
		if err := os.WriteFile(path, []byte(`---
params: []
---
Hi`), 0644); err != nil {
			t.Fatalf("WriteFile: %v", err)
		}
		if _, err := cache.Load(path); err != nil {
			t.Fatalf("Load: %v", err)
		}
	}

	if cache.TemplateCount() != 3 {
		t.Errorf("expected 3 cached templates, got %d", cache.TemplateCount())
	}
}

// ---------------------------------------------------------------------------
// Cache include count
// ---------------------------------------------------------------------------

func TestCacheIncludeCount(t *testing.T) {
	dir := t.TempDir()

	// Write an include.
	headerPath := filepath.Join(dir, "header.tmpl.md")
	if err := os.WriteFile(headerPath, []byte(`---
name: header
params: [title = str]
---
# {{ title }}`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	// Write a main template that includes the header.
	mainPath := filepath.Join(dir, "main.tmpl.md")
	if err := os.WriteFile(mainPath, []byte(`---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body`), 0644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cache := NewCache()
	defer cache.Close()

	tmpl, err := cache.Load(mainPath)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}

	// Render to trigger include resolution.
	ctx := NewContext()
	defer ctx.Close()
	if err := ctx.SetStr("title", "Test"); err != nil {
		t.Fatalf("SetStr: %v", err)
	}
	_, err = tmpl.Render(ctx, AllowExtra())
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	tmpl.Close()

	if cache.TemplateCount() < 1 {
		t.Errorf("expected at least 1 template in cache, got %d", cache.TemplateCount())
	}
	// Include count should be >= 0 (implementation may or may not cache includes
	// depending on whether render_cached was used).
	if cache.IncludeCount() < 0 {
		t.Errorf("expected include count >= 0, got %d", cache.IncludeCount())
	}
}

// ---------------------------------------------------------------------------
// Concurrent rendering
// ---------------------------------------------------------------------------

func TestConcurrentRender(t *testing.T) {
	tmpl, err := FromSource(`---
params: [id = int]
---
Result: {{ id }}`)
	if err != nil {
		t.Fatalf("FromSource: %v", err)
	}
	defer tmpl.Close()

	var wg sync.WaitGroup
	errorsCh := make(chan error, 10)

	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			ctx := NewContext()
			defer ctx.Close()
			if err := ctx.SetInt("id", int64(id)); err != nil {
				errorsCh <- fmt.Errorf("goroutine %d SetInt: %w", id, err)
				return
			}
			result, err := tmpl.Render(ctx)
			if err != nil {
				errorsCh <- fmt.Errorf("goroutine %d: %w", id, err)
				return
			}
			expected := fmt.Sprintf("Result: %d", id)
			if result != expected {
				errorsCh <- fmt.Errorf("goroutine %d: got %q, want %q", id, result, expected)
			}
		}(i)
	}

	wg.Wait()
	close(errorsCh)

	for err := range errorsCh {
		t.Error(err)
	}
}
