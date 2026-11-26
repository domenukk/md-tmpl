package md_tmpl

/*
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

extern void pt_free_string(char *ptr);

// Rendering.
extern char *pt_template_render(const void *tmpl, const void *ctx, char **out_err);
extern char *pt_template_render_allowing_extra(const void *tmpl, const void *ctx, char **out_err);
extern char *pt_template_render_empty(const void *tmpl, char **out_err);
extern char *pt_template_render_cached(const void *tmpl, const void *ctx, const void *cache, char **out_err);
extern char *pt_template_render_json(const void *tmpl, const char *json, _Bool allow_extra, char **out_err);
extern char *pt_template_render_flexbuffers(const void *tmpl, const uint8_t *data, size_t len, _Bool allow_extra, char **out_err);

// Template metadata.
extern uint64_t pt_template_source_hash(const void *tmpl);
extern char *pt_template_name(const void *tmpl);
extern char *pt_template_description(const void *tmpl);
extern char *pt_template_body(const void *tmpl);
extern char *pt_template_declarations(const void *tmpl);
extern void pt_template_set_max_include_depth(void *tmpl, size_t depth);
extern char *pt_template_defaults_json(const void *tmpl);
extern char *pt_template_consts_json(const void *tmpl);
extern char *pt_template_imported_consts_json(const void *tmpl);
extern void *pt_template_defaults_context(const void *tmpl);
extern char *pt_template_validate_declarations(const void *tmpl, const char *expected_json);
extern char *pt_template_validate_against_template(const void *tmpl, const void *expected);
*/
import "C"

import (
	"encoding/json"
	"errors"
	"fmt"
	"runtime"
	"unsafe"
)

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

// RenderOption configures a single render call.
//
// Options are applied left-to-right; the empty set renders in strict mode.
// The same options apply to every render form ([Template.Render],
// [Template.RenderMap], [Template.RenderJSON], [Template.RenderStruct]).
type RenderOption func(*renderConfig)

// renderConfig holds the resolved settings for a single render call.
type renderConfig struct {
	allowExtra bool
}

// resolveRenderConfig folds the supplied options into a renderConfig.
func resolveRenderConfig(opts []RenderOption) renderConfig {
	var cfg renderConfig
	for _, opt := range opts {
		opt(&cfg)
	}
	return cfg
}

// AllowExtra permits context keys that are not declared in the template
// frontmatter.
//
// Without it, rendering is strict: an undeclared parameter yields an
// ExtraParams [TemplateError].
func AllowExtra() RenderOption {
	return func(c *renderConfig) { c.allowExtra = true }
}

// Render renders the template with the given context (strict mode by default).
func (t *Template) Render(ctx *Context, opts ...RenderOption) (string, error) {
	if t == nil {
		return "", ErrClosed
	}
	if ctx == nil {
		return "", ErrNilContext
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return "", ErrClosed
	}
	ctx.mu.Lock()
	defer ctx.mu.Unlock()
	if ctx.ptr == nil {
		return "", ErrNilContext
	}

	var errPtr *C.char
	var result *C.char
	if resolveRenderConfig(opts).allowExtra {
		result = C.pt_template_render_allowing_extra(t.ptr, ctx.ptr, &errPtr)
	} else {
		result = C.pt_template_render(t.ptr, ctx.ptr, &errPtr)
	}
	runtime.KeepAlive(t)
	runtime.KeepAlive(ctx)
	if errPtr != nil {
		return "", freeError(errPtr)
	}
	if result == nil {
		return "", errors.New("md_tmpl: render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// RenderEmpty renders the template with an empty context.
func (t *Template) RenderEmpty() (string, error) {
	if t == nil {
		return "", ErrClosed
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return "", ErrClosed
	}

	var errPtr *C.char
	result := C.pt_template_render_empty(t.ptr, &errPtr)
	runtime.KeepAlive(t)
	if errPtr != nil {
		return "", freeError(errPtr)
	}
	if result == nil {
		return "", errors.New("md_tmpl: render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// RenderCached renders the template, resolving {% include %} directives through
// the given [Cache].
func (t *Template) RenderCached(ctx *Context, cache *Cache) (string, error) {
	if t == nil {
		return "", ErrClosed
	}
	if ctx == nil {
		return "", ErrNilContext
	}
	if cache == nil {
		return "", ErrClosed
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return "", ErrClosed
	}
	ctx.mu.Lock()
	defer ctx.mu.Unlock()
	if ctx.ptr == nil {
		return "", ErrNilContext
	}
	cache.mu.RLock()
	defer cache.mu.RUnlock()
	if cache.ptr == nil {
		return "", ErrClosed
	}

	var errPtr *C.char
	result := C.pt_template_render_cached(t.ptr, ctx.ptr, cache.ptr, &errPtr)
	runtime.KeepAlive(t)
	runtime.KeepAlive(ctx)
	runtime.KeepAlive(cache)
	if errPtr != nil {
		return "", freeError(errPtr)
	}
	if result == nil {
		return "", errors.New("md_tmpl: render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// RenderMap creates a Context from a map, renders the template, and returns the result.
func (t *Template) RenderMap(params map[string]any, opts ...RenderOption) (string, error) {
	return t.renderMapWith(params, resolveRenderConfig(opts).allowExtra)
}

// RenderJSON renders the template using a JSON string as the parameter source.
func (t *Template) RenderJSON(jsonStr string, opts ...RenderOption) (string, error) {
	return t.renderJSONWith(jsonStr, resolveRenderConfig(opts).allowExtra)
}

func (t *Template) renderJSONWith(jsonStr string, allowExtra bool) (string, error) {
	if t == nil {
		return "", ErrClosed
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return "", ErrClosed
	}

	cJSON := C.CString(jsonStr)
	defer C.free(unsafe.Pointer(cJSON))

	var errPtr *C.char
	result := C.pt_template_render_json(t.ptr, cJSON, C._Bool(allowExtra), &errPtr)
	runtime.KeepAlive(t)
	if errPtr != nil {
		return "", freeError(errPtr)
	}
	if result == nil {
		return "", errors.New("md_tmpl: render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// RenderStruct renders the template using a Go struct as the parameter source.
func (t *Template) RenderStruct(v any, opts ...RenderOption) (string, error) {
	return t.renderFlexbuffersWith(v, resolveRenderConfig(opts).allowExtra)
}

func (t *Template) renderMapWith(params map[string]any, allowExtra bool) (string, error) {
	return t.renderFlexbuffersWith(params, allowExtra)
}

func (t *Template) renderFlexbuffersWith(v any, allowExtra bool) (string, error) {
	if t == nil {
		return "", ErrClosed
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return "", ErrClosed
	}

	data, err := marshalFlexbuffers(v)
	if err != nil {
		return "", fmt.Errorf("renderFlexbuffersWith: cannot marshal to flexbuffers: %w", err)
	}
	if len(data) == 0 {
		return "", errors.New("md_tmpl: renderFlexbuffersWith: empty flexbuffers data")
	}

	var errPtr *C.char
	result := C.pt_template_render_flexbuffers(t.ptr, (*C.uint8_t)(&data[0]), C.size_t(len(data)), C._Bool(allowExtra), &errPtr)
	runtime.KeepAlive(data)
	runtime.KeepAlive(t)
	if errPtr != nil {
		return "", freeError(errPtr)
	}
	if result == nil {
		return "", errors.New("md_tmpl: render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// ---------------------------------------------------------------------------
// Template metadata
// ---------------------------------------------------------------------------

// SourceHash returns the content hash of the template source.
func (t *Template) SourceHash() uint64 {
	if t == nil {
		return 0
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return 0
	}
	h := uint64(C.pt_template_source_hash(t.ptr))
	runtime.KeepAlive(t)
	return h
}

// Body returns the template body text after frontmatter stripping.
func (t *Template) Body() string {
	if t == nil {
		return ""
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return ""
	}
	cBody := C.pt_template_body(t.ptr)
	runtime.KeepAlive(t)
	if cBody == nil {
		return ""
	}
	body := C.GoString(cBody)
	C.pt_free_string(cBody)
	return body
}

// Name returns the template name declared in frontmatter and whether one was present.
func (t *Template) Name() (string, bool) {
	if t == nil {
		return "", false
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return "", false
	}
	c := C.pt_template_name(t.ptr)
	runtime.KeepAlive(t)
	if c == nil {
		return "", false
	}
	name := C.GoString(c)
	C.pt_free_string(c)
	return name, true
}

// Description returns the template description declared in frontmatter and whether one was present.
func (t *Template) Description() (string, bool) {
	if t == nil {
		return "", false
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return "", false
	}
	c := C.pt_template_description(t.ptr)
	runtime.KeepAlive(t)
	if c == nil {
		return "", false
	}
	desc := C.GoString(c)
	C.pt_free_string(c)
	return desc, true
}

// Declarations returns the declared parameter names, types, and defaults.
func (t *Template) Declarations() []Declaration {
	if t == nil {
		return nil
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return nil
	}
	cDecls := C.pt_template_declarations(t.ptr)
	runtime.KeepAlive(t)
	if cDecls == nil {
		return nil
	}
	jsonStr := C.GoString(cDecls)
	C.pt_free_string(cDecls)

	var raw [][]string
	if err := json.Unmarshal([]byte(jsonStr), &raw); err != nil {
		debugLog.Printf("Declarations: failed to parse JSON: %v", err)
		return nil
	}
	if len(raw) == 0 {
		return nil
	}

	defaults := t.defaultsLocked()

	decls := make([]Declaration, 0, len(raw))
	for _, pair := range raw {
		if len(pair) == 2 {
			d := Declaration{Name: pair[0], Type: pair[1]}
			if defaults != nil {
				d.Default = defaults[pair[0]]
			}
			decls = append(decls, d)
		}
	}
	return decls
}

func (t *Template) defaultsLocked() map[string]any {
	raw := C.pt_template_defaults_json(t.ptr)
	runtime.KeepAlive(t)
	if raw == nil {
		return nil
	}
	defer C.pt_free_string(raw)
	jsonStr := C.GoString(raw)

	var result map[string]any
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		debugLog.Printf("Defaults: failed to parse JSON: %v", err)
		return nil
	}
	return result
}

// Defaults returns a map of parameter names to their default values.
func (t *Template) Defaults() map[string]any {
	if t == nil {
		return nil
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return nil
	}
	return t.defaultsLocked()
}

// Constants returns a map of constant names to their values.
func (t *Template) Constants() map[string]any {
	if t == nil {
		return nil
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return nil
	}
	raw := C.pt_template_consts_json(t.ptr)
	runtime.KeepAlive(t)
	if raw == nil {
		return nil
	}
	defer C.pt_free_string(raw)
	jsonStr := C.GoString(raw)

	var result map[string]any
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		debugLog.Printf("Constants: failed to parse JSON: %v", err)
		return nil
	}
	return result
}

// ImportedConstants returns constants imported from other templates.
func (t *Template) ImportedConstants() map[string]any {
	if t == nil {
		return nil
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return nil
	}
	raw := C.pt_template_imported_consts_json(t.ptr)
	runtime.KeepAlive(t)
	if raw == nil {
		return nil
	}
	defer C.pt_free_string(raw)
	jsonStr := C.GoString(raw)

	var result map[string]any
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		debugLog.Printf("ImportedConstants: failed to parse JSON: %v", err)
		return nil
	}
	if len(result) == 0 {
		return nil
	}
	return result
}

// DefaultsContext returns a new Context pre-filled with all default values.
func (t *Template) DefaultsContext() *Context {
	if t == nil {
		return nil
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return nil
	}
	ptr := C.pt_template_defaults_context(t.ptr)
	runtime.KeepAlive(t)
	if ptr == nil {
		return nil
	}
	ctx := &Context{ptr: ptr}
	runtime.SetFinalizer(ctx, func(c *Context) { c.Close() })
	return ctx
}

// SetMaxIncludeDepth sets the maximum nesting depth for {% include %} directives.
func (t *Template) SetMaxIncludeDepth(depth int) {
	if t == nil || depth < 0 {
		return
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr != nil {
		C.pt_template_set_max_include_depth(t.ptr, C.size_t(depth))
		runtime.KeepAlive(t)
	}
}

// ValidateDeclarations checks that the template's parameter declarations
// match the expected set.
func (t *Template) ValidateDeclarations(expected []Declaration) error {
	if t == nil {
		return ErrClosed
	}
	t.mu.RLock()
	defer t.mu.RUnlock()
	if t.ptr == nil {
		return ErrClosed
	}

	pairs := make([][]string, len(expected))
	for i, d := range expected {
		pairs[i] = []string{d.Name, d.Type}
	}
	data, err := json.Marshal(pairs)
	if err != nil {
		return fmt.Errorf("cannot marshal declarations: %w", err)
	}

	cJSON := C.CString(string(data))
	defer C.free(unsafe.Pointer(cJSON))

	errPtr := C.pt_template_validate_declarations(t.ptr, cJSON)
	runtime.KeepAlive(t)
	return freeError(errPtr)
}

// ValidateAgainstTemplate checks that this template's parameter declarations and
// sanitization contracts match expected, returning an [ErrDeclarationsMutated]
// [*TemplateError] on any difference.
func (t *Template) ValidateAgainstTemplate(expected *Template) error {
	if t == nil || expected == nil {
		return ErrClosed
	}
	if t == expected {
		t.mu.RLock()
		defer t.mu.RUnlock()
		if t.ptr == nil {
			return ErrClosed
		}
	} else {
		first, second := t, expected
		if uintptr(unsafe.Pointer(first)) > uintptr(unsafe.Pointer(second)) {
			first, second = second, first
		}
		first.mu.RLock()
		defer first.mu.RUnlock()
		second.mu.RLock()
		defer second.mu.RUnlock()
		if t.ptr == nil || expected.ptr == nil {
			return ErrClosed
		}
	}
	errPtr := C.pt_template_validate_against_template(t.ptr, expected.ptr)
	runtime.KeepAlive(t)
	runtime.KeepAlive(expected)
	return freeError(errPtr)
}
