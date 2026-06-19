// Package prompt_templates provides a fast, strongly-typed template engine
// for LLM prompts from Go.
//
// Templates use a declarative YAML frontmatter with typed parameters
// (str, int, float, bool, list, struct, enum, tmpl) and a Jinja2-inspired
// body with for-loops, match/case, filters, includes, and constants.
//
// # Quick Start
//
//	source := `---
//	params:
//	  - name = str
//	---
//	Hello {{ name }}!`
//	tmpl, err := prompt_templates.FromSource(source)
//	if err != nil {
//	    log.Fatal(err)
//	}
//	defer tmpl.Close()
//
//	result, _ := tmpl.RenderMap(map[string]any{"name": "world"})
//	// result == "Hello world!"
//
// # Building
//
// Build the native library before using this package:
//
//	just build-go-ffi
package prompt_templates

/*
#cgo LDFLAGS: -L${SRCDIR}/../../target/release -lprompt_templates_ffi -ldl -lpthread -lm
#cgo CFLAGS: -I${SRCDIR}/../../crates/prompt-templates-ffi/include

#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

// String lifecycle.
extern void pt_free_string(char *ptr);

// Template lifecycle.
extern char *pt_template_from_source(const char *source, void **out);
extern char *pt_template_from_source_allowing_unused(const char *source, void **out);
extern char *pt_template_from_source_with_base_dir(const char *source, const char *base_dir, void **out);
extern char *pt_template_from_file(const char *path, void **out);
extern void pt_template_free(void *tmpl);

// Context lifecycle.
extern void *pt_context_new(void);
extern void pt_context_free(void *ctx);
extern char *pt_context_set_str(void *ctx, const char *key, const char *value);
extern char *pt_context_set_int(void *ctx, const char *key, int64_t value);
extern char *pt_context_set_float(void *ctx, const char *key, double value);
extern char *pt_context_set_bool(void *ctx, const char *key, _Bool value);
extern char *pt_context_set_json(void *ctx, const char *key, const char *json);
extern char *pt_context_set_tmpl(void *ctx, const char *key, const void *tmpl);
extern char *pt_context_merge_json(void *ctx, const char *json);
extern char *pt_context_set_flexbuffers(void *ctx, const char *key, const uint8_t *data, size_t len);
extern char *pt_context_merge_flexbuffers(void *ctx, const uint8_t *data, size_t len);

// Rendering.
extern char *pt_template_render(const void *tmpl, const void *ctx, char **out_err);
extern char *pt_template_render_allowing_extra(const void *tmpl, const void *ctx, char **out_err);
extern char *pt_template_render_json(const void *tmpl, const char *json, _Bool allow_extra, char **out_err);
extern char *pt_template_render_flexbuffers(const void *tmpl, const uint8_t *data, size_t len, _Bool allow_extra, char **out_err);

// Template metadata.
extern uint64_t pt_template_source_hash(const void *tmpl);
extern char *pt_template_body(const void *tmpl);
extern char *pt_template_declarations(const void *tmpl);
extern void pt_template_set_max_include_depth(void *tmpl, size_t depth);
extern char *pt_template_defaults_json(const void *tmpl);
extern char *pt_template_consts_json(const void *tmpl);
extern char *pt_template_imported_consts_json(const void *tmpl);
extern void *pt_template_defaults_context(const void *tmpl);
extern char *pt_template_validate_declarations(const void *tmpl, const char *expected_json);
extern char *pt_template_from_source_with_frontmatter(const char *source, void **out_tmpl, char **out_fm);

// Cache lifecycle.
extern void *pt_cache_new(void);
extern void pt_cache_free(void *cache);
extern char *pt_cache_load(const void *cache, const char *path, void **out);
extern void pt_cache_clear(const void *cache);
extern size_t pt_cache_template_count(const void *cache);
extern size_t pt_cache_include_count(const void *cache);
*/
import "C"

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"runtime"
	"sync"
	"unsafe"
)

// Compile-time interface checks.
var (
	_ io.Closer = (*Template)(nil)
	_ io.Closer = (*Context)(nil)
	_ io.Closer = (*Cache)(nil)
)

// debugLog is a package-level logger for non-critical internal errors
// (e.g. JSON parse failures in metadata methods). It defaults to discarding
// output. Enable it by setting debugLog.SetOutput(os.Stderr) or similar.
var debugLog = log.New(io.Discard, "prompt_templates: ", log.LstdFlags)

// Sentinel errors returned by the package.
var (
	// ErrClosed is returned when operating on a Template, Context, or Cache
	// that has already been closed.
	ErrClosed = errors.New("prompt_templates: resource is closed")

	// ErrNilContext is returned when a nil or closed Context is passed to Render.
	ErrNilContext = errors.New("prompt_templates: context is nil or closed")
)

// Template is a parsed, validated template ready for rendering.
//
// Templates are compiled from source strings or loaded from files.
// Parameters are type-checked against frontmatter declarations at render time.
//
// Template is safe for concurrent use from multiple goroutines after creation.
//
// Close must be called when the template is no longer needed.
type Template struct {
	ptr       unsafe.Pointer
	closeOnce sync.Once
}

// Context holds the variables available during template rendering.
//
// Context is NOT safe for concurrent use. Each goroutine should create its own Context.
//
// Close must be called when the context is no longer needed.
type Context struct {
	ptr       unsafe.Pointer
	closeOnce sync.Once
}

// Cache provides content-hashed template caching for hot-reload scenarios.
//
// Unchanged files return cached compilations with zero re-parsing.
// Cache is safe for concurrent use from multiple goroutines.
//
// Close must be called when the cache is no longer needed.
type Cache struct {
	ptr       unsafe.Pointer
	closeOnce sync.Once
}

// Declaration represents a single parameter declaration from frontmatter.
type Declaration struct {
	Name    string // Parameter name.
	Type    string // Parameter type (e.g. "str", "int", "list<label = str>").
	Default any    // Default value (nil if no default).
}

// String returns the declaration in "name = type" format.
func (d Declaration) String() string {
	return d.Name + " = " + d.Type
}

// Frontmatter holds parsed metadata from the template's YAML frontmatter block.
type Frontmatter struct {
	Name        string   // Template name.
	Description string   // Template description.
	HasParams   bool     // Whether a params: block was present.
	AllowUnused bool     // Whether unused parameters are allowed.
	Params      []string // Parameter names (convenience).
}

// TaggedVariant is an embeddable base struct for creating statically typed,
// zero-allocation enum variants in Go without using maps or magic strings.
//
// By embedding TaggedVariant in your custom structs, you achieve compile-time
// static typing for template enum variants that perfectly mirrors Rust's enum representation.
//
// Example:
//
//	type Confirmed struct {
//	    prompt_templates.TaggedVariant
//	    Evidence string `json:"evidence"`
//	}
//
//	func NewConfirmed(evidence string) Confirmed {
//	    return Confirmed{TaggedVariant: prompt_templates.NewTaggedVariant("Confirmed"), Evidence: evidence}
//	}
type TaggedVariant struct {
	Kind string `json:"__kind__"`
}

// NewTaggedVariant creates a new TaggedVariant with the given enum variant name.
func NewTaggedVariant(kind string) TaggedVariant {
	return TaggedVariant{Kind: kind}
}

// Variant represents a dynamic template enum variant.
//
// For static, compile-time type safety without map allocations, embed [TaggedVariant] instead.
//
// Unit variants (no fields):
//
//	Variant{Kind: "Rejected"}
//
// Struct variants (with fields):
//
//	Variant{Kind: "Confirmed", Fields: map[string]any{"evidence": "found it"}}
type Variant struct {
	Kind   string         // The variant name.
	Fields map[string]any // Optional fields for struct variants.
}

// MarshalJSON encodes the variant using the __kind__ tag convention.
func (v Variant) MarshalJSON() ([]byte, error) {
	if len(v.Fields) == 0 {
		// Unit variant → plain string.
		return json.Marshal(v.Kind)
	}
	// Struct variant → {"__kind__": "Kind", ...fields}
	m := make(map[string]any, len(v.Fields)+1)
	m["__kind__"] = v.Kind
	for k, val := range v.Fields {
		m[k] = val
	}
	return json.Marshal(m)
}

// freeError converts a C error string to a Go error and frees the C string.
// Returns nil if errPtr is nil (no error).
func freeError(errPtr *C.char) error {
	if errPtr == nil {
		return nil
	}
	msg := C.GoString(errPtr)
	C.pt_free_string(errPtr)
	return errors.New(msg)
}

// ---------------------------------------------------------------------------
// Template constructors
// ---------------------------------------------------------------------------

// FromSource parses a template from an in-memory source string.
//
// The source must include YAML frontmatter with parameter declarations.
// Unused declared parameters (present in frontmatter but not in the body)
// are rejected; use [FromSourceAllowingUnused] to suppress this check.
func FromSource(source string) (*Template, error) {
	cSource := C.CString(source)
	defer C.free(unsafe.Pointer(cSource))

	var ptr unsafe.Pointer
	errPtr := C.pt_template_from_source(cSource, &ptr)
	if err := freeError(errPtr); err != nil {
		return nil, err
	}

	t := &Template{ptr: ptr}
	runtime.SetFinalizer(t, func(t *Template) { t.Close() })
	return t, nil
}

// FromSourceAllowingUnused parses a template, allowing declared parameters
// that aren't referenced in the body.
func FromSourceAllowingUnused(source string) (*Template, error) {
	cSource := C.CString(source)
	defer C.free(unsafe.Pointer(cSource))

	var ptr unsafe.Pointer
	errPtr := C.pt_template_from_source_allowing_unused(cSource, &ptr)
	if err := freeError(errPtr); err != nil {
		return nil, err
	}

	t := &Template{ptr: ptr}
	runtime.SetFinalizer(t, func(t *Template) { t.Close() })
	return t, nil
}

// FromSourceWithBaseDir parses a template from source, resolving includes
// relative to the given base directory.
//
// Use this when parsing templates with {% include %} or imports: directives
// that reference other template files.
func FromSourceWithBaseDir(source, baseDir string) (*Template, error) {
	cSource := C.CString(source)
	defer C.free(unsafe.Pointer(cSource))
	cDir := C.CString(baseDir)
	defer C.free(unsafe.Pointer(cDir))

	var ptr unsafe.Pointer
	errPtr := C.pt_template_from_source_with_base_dir(cSource, cDir, &ptr)
	if err := freeError(errPtr); err != nil {
		return nil, err
	}

	t := &Template{ptr: ptr}
	runtime.SetFinalizer(t, func(t *Template) { t.Close() })
	return t, nil
}

// FromSourceWithFrontmatter parses a template and returns both the compiled
// template and its parsed frontmatter metadata.
//
// Use this to access the template's name, description, and other metadata
// from the YAML frontmatter block.
func FromSourceWithFrontmatter(source string) (*Template, *Frontmatter, error) {
	cSource := C.CString(source)
	defer C.free(unsafe.Pointer(cSource))

	var tmplPtr unsafe.Pointer
	var fmPtr *C.char
	errPtr := C.pt_template_from_source_with_frontmatter(cSource, &tmplPtr, &fmPtr)
	if err := freeError(errPtr); err != nil {
		return nil, nil, err
	}

	t := &Template{ptr: tmplPtr}
	runtime.SetFinalizer(t, func(t *Template) { t.Close() })

	// Parse frontmatter JSON.
	fmJSON := C.GoString(fmPtr)
	C.pt_free_string(fmPtr)

	var raw struct {
		Name        string   `json:"name"`
		Description string   `json:"description"`
		HasParams   bool     `json:"has_params"`
		AllowUnused bool     `json:"allow_unused"`
		Params      []string `json:"params"`
	}
	if err := json.Unmarshal([]byte(fmJSON), &raw); err != nil {
		return t, nil, fmt.Errorf("parsing frontmatter JSON: %w", err)
	}

	fm := &Frontmatter{
		Name:        raw.Name,
		Description: raw.Description,
		HasParams:   raw.HasParams,
		AllowUnused: raw.AllowUnused,
		Params:      raw.Params,
	}
	return t, fm, nil
}

// FromFile loads a template from a .tmpl.md file.
func FromFile(path string) (*Template, error) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	var ptr unsafe.Pointer
	errPtr := C.pt_template_from_file(cPath, &ptr)
	if err := freeError(errPtr); err != nil {
		return nil, err
	}

	t := &Template{ptr: ptr}
	runtime.SetFinalizer(t, func(t *Template) { t.Close() })
	return t, nil
}

// Close frees the template resources. Safe to call multiple times and
// concurrently. Implements [io.Closer].
func (t *Template) Close() error {
	t.closeOnce.Do(func() {
		if t.ptr != nil {
			C.pt_template_free(t.ptr)
			t.ptr = nil
		}
	})
	return nil
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

// Render renders the template with the given context (strict mode).
//
// All parameters are validated against frontmatter declarations.
// Missing parameters, type mismatches, and extra undeclared parameters
// produce clear error messages.
//
// Use [RenderAllowingExtra] to permit undeclared parameters.
func (t *Template) Render(ctx *Context) (string, error) {
	if t.ptr == nil {
		return "", ErrClosed
	}
	if ctx == nil || ctx.ptr == nil {
		return "", ErrNilContext
	}

	var errPtr *C.char
	result := C.pt_template_render(t.ptr, ctx.ptr, &errPtr)
	if errPtr != nil {
		err := freeError(errPtr)
		return "", err
	}
	if result == nil {
		return "", fmt.Errorf("render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// RenderAllowingExtra renders the template, allowing extra (undeclared) parameters.
//
// Like [Render], but extra context keys that aren't declared in frontmatter
// are silently ignored instead of producing an error.
func (t *Template) RenderAllowingExtra(ctx *Context) (string, error) {
	if t.ptr == nil {
		return "", ErrClosed
	}
	if ctx == nil || ctx.ptr == nil {
		return "", ErrNilContext
	}

	var errPtr *C.char
	result := C.pt_template_render_allowing_extra(t.ptr, ctx.ptr, &errPtr)
	if errPtr != nil {
		err := freeError(errPtr)
		return "", err
	}
	if result == nil {
		return "", fmt.Errorf("render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// RenderMap is a convenience method that creates a Context from a map,
// renders the template, and returns the result.
//
// Map values are automatically converted:
//   - string → str
//   - int/int64 → int
//   - float64 → float
//   - bool → bool
//   - anything else → JSON-encoded and set via SetJSON
func (t *Template) RenderMap(params map[string]any) (string, error) {
	return t.renderMapWith(params, false)
}

// RenderJSON renders the template using a JSON string as the parameter source.
//
// The JSON string must be a JSON object (`{}`). Each top-level key becomes
// a template parameter. This is the most efficient rendering path: a single
// FFI call populates the entire context.
//
// Example:
//
//	result, err := tmpl.RenderJSON(`{"name": "Alice", "count": 42}`)
func (t *Template) RenderJSON(jsonStr string) (string, error) {
	return t.renderJSONWith(jsonStr, false)
}

// RenderJSONAllowingExtra is like [RenderJSON] but allows extra parameters
// that aren't declared in frontmatter.
func (t *Template) RenderJSONAllowingExtra(jsonStr string) (string, error) {
	return t.renderJSONWith(jsonStr, true)
}

// renderJSONWith is the shared implementation for RenderJSON and RenderJSONAllowingExtra.
//
// Uses a single-shot FFI call (pt_template_render_json) that parses JSON,
// builds the context, and renders entirely on the Rust side — avoiding the
// overhead of 3 separate FFI round-trips.
func (t *Template) renderJSONWith(jsonStr string, allowExtra bool) (string, error) {
	if t.ptr == nil {
		return "", ErrClosed
	}

	cJSON := C.CString(jsonStr)
	defer C.free(unsafe.Pointer(cJSON))

	var errPtr *C.char
	result := C.pt_template_render_json(t.ptr, cJSON, C._Bool(allowExtra), &errPtr)
	if errPtr != nil {
		return "", freeError(errPtr)
	}
	if result == nil {
		return "", fmt.Errorf("render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// RenderStruct renders the template using a Go struct as the parameter source.
//
// Struct fields are mapped to template parameters by their json tag name,
// falling back to the lowercased field name. Unexported fields are skipped.
//
// Internally this marshals the struct to FlexBuffers and populates the context in
// a single FFI call, which is significantly faster than per-field reflection.
//
// Example:
//
//	type Params struct {
//	    Name  string `json:"name"`
//	    Count int64  `json:"count"`
//	}
//	result, err := tmpl.RenderStruct(Params{Name: "Alice", Count: 42})
func (t *Template) RenderStruct(v any) (string, error) {
	return t.renderFlexbuffersWith(v, false)
}

// RenderStructAllowingExtra is like [RenderStruct] but allows extra parameters
// that aren't declared in frontmatter.
func (t *Template) RenderStructAllowingExtra(v any) (string, error) {
	return t.renderFlexbuffersWith(v, true)
}

// RenderMapAllowingExtra is like [RenderMap] but allows extra parameters
// that aren't declared in frontmatter.
func (t *Template) RenderMapAllowingExtra(params map[string]any) (string, error) {
	return t.renderMapWith(params, true)
}

// renderMapWith is the shared implementation for RenderMap and RenderMapAllowingExtra.
//
// Marshals the map to FlexBuffers and uses the single-shot FFI path, replacing
// the per-key Set() calls that each crossed the FFI boundary.
func (t *Template) renderMapWith(params map[string]any, allowExtra bool) (string, error) {
	return t.renderFlexbuffersWith(params, allowExtra)
}

// renderFlexbuffersWith marshals the value to FlexBuffers and uses the single-shot FFI path.
func (t *Template) renderFlexbuffersWith(v any, allowExtra bool) (string, error) {
	if t.ptr == nil {
		return "", ErrClosed
	}

	data, err := marshalFlexbuffers(v)
	if err != nil {
		return "", fmt.Errorf("renderFlexbuffersWith: cannot marshal to flexbuffers: %w", err)
	}
	if len(data) == 0 {
		return "", fmt.Errorf("renderFlexbuffersWith: empty flexbuffers data")
	}

	var errPtr *C.char
	result := C.pt_template_render_flexbuffers(t.ptr, (*C.uint8_t)(&data[0]), C.size_t(len(data)), C._Bool(allowExtra), &errPtr)
	if errPtr != nil {
		return "", freeError(errPtr)
	}
	if result == nil {
		return "", fmt.Errorf("render returned nil without error")
	}

	output := C.GoString(result)
	C.pt_free_string(result)
	return output, nil
}

// ---------------------------------------------------------------------------
// Template metadata
// ---------------------------------------------------------------------------

// SourceHash returns the content hash of the template source.
//
// Two templates compiled from the same source produce the same hash.
func (t *Template) SourceHash() uint64 {
	if t.ptr == nil {
		return 0
	}
	return uint64(C.pt_template_source_hash(t.ptr))
}

// Body returns the template body text after frontmatter stripping.
func (t *Template) Body() string {
	if t.ptr == nil {
		return ""
	}
	cBody := C.pt_template_body(t.ptr)
	body := C.GoString(cBody)
	C.pt_free_string(cBody)
	return body
}

// Declarations returns the declared parameter names, types, and defaults.
func (t *Template) Declarations() []Declaration {
	if t.ptr == nil {
		return nil
	}
	cDecls := C.pt_template_declarations(t.ptr)
	jsonStr := C.GoString(cDecls)
	C.pt_free_string(cDecls)

	var raw [][]string
	if err := json.Unmarshal([]byte(jsonStr), &raw); err != nil {
		debugLog.Printf("Declarations: failed to parse JSON: %v", err)
		return nil
	}

	defaults := t.Defaults()

	decls := make([]Declaration, 0, len(raw))
	for _, pair := range raw {
		if len(pair) == 2 {
			d := Declaration{Name: pair[0], Type: pair[1]}
			if defaults != nil {
				d.Default = defaults[pair[0]] // nil if not present
			}
			decls = append(decls, d)
		}
	}
	return decls
}

// Defaults returns a map of parameter names to their default values.
// Only parameters with defaults are included.
func (t *Template) Defaults() map[string]any {
	if t.ptr == nil {
		return nil
	}
	raw := C.pt_template_defaults_json(t.ptr)
	defer C.pt_free_string(raw)
	jsonStr := C.GoString(raw)

	var result map[string]any
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		debugLog.Printf("Defaults: failed to parse JSON: %v", err)
		return nil
	}
	return result
}

// Constants returns a map of constant names to their values.
func (t *Template) Constants() map[string]any {
	if t.ptr == nil {
		return nil
	}
	raw := C.pt_template_consts_json(t.ptr)
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
//
// These are keyed by "stem.NAME" (e.g. "other.MAX_RETRIES").
// Returns nil if no imported constants exist.
func (t *Template) ImportedConstants() map[string]any {
	if t.ptr == nil {
		return nil
	}
	raw := C.pt_template_imported_consts_json(t.ptr)
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
// Use this as a starting point, then override only the params you need.
//
// Returns nil if the template is closed.
// The caller must call Close on the returned context.
func (t *Template) DefaultsContext() *Context {
	if t.ptr == nil {
		return nil
	}
	ctx := &Context{ptr: C.pt_template_defaults_context(t.ptr)}
	runtime.SetFinalizer(ctx, func(c *Context) { c.Close() })
	return ctx
}

// SetMaxIncludeDepth sets the maximum nesting depth for {% include %} directives.
func (t *Template) SetMaxIncludeDepth(depth int) {
	if t.ptr != nil {
		C.pt_template_set_max_include_depth(t.ptr, C.size_t(depth))
	}
}

// ValidateDeclarations checks that the template's parameter declarations
// match the expected set.
//
// Pass the expected declarations (e.g. from a previous snapshot). If the
// declarations have changed (added, removed, or retyped parameters), an
// error is returned describing the differences.
//
// This is useful for detecting template changes at load time.
func (t *Template) ValidateDeclarations(expected []Declaration) error {
	if t.ptr == nil {
		return ErrClosed
	}

	// Encode expected as JSON array of [name, type] pairs.
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
	return freeError(errPtr)
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

// NewContext creates a new empty rendering context.
//
// Close must be called when the context is no longer needed.
func NewContext() *Context {
	ctx := &Context{ptr: C.pt_context_new()}
	runtime.SetFinalizer(ctx, func(c *Context) { c.Close() })
	return ctx
}

// Close frees the context resources. Safe to call multiple times and
// concurrently. Implements [io.Closer].
func (c *Context) Close() error {
	c.closeOnce.Do(func() {
		if c.ptr != nil {
			C.pt_context_free(c.ptr)
			c.ptr = nil
		}
	})
	return nil
}

// SetStr sets a string value in the context.
func (c *Context) SetStr(key, value string) error {
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))
	cVal := C.CString(value)
	defer C.free(unsafe.Pointer(cVal))

	errPtr := C.pt_context_set_str(c.ptr, cKey, cVal)
	return freeError(errPtr)
}

// SetInt sets an integer value in the context.
func (c *Context) SetInt(key string, value int64) error {
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_int(c.ptr, cKey, C.int64_t(value))
	return freeError(errPtr)
}

// SetFloat sets a float value in the context.
func (c *Context) SetFloat(key string, value float64) error {
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_float(c.ptr, cKey, C.double(value))
	return freeError(errPtr)
}

// SetBool sets a bool value in the context.
func (c *Context) SetBool(key string, value bool) error {
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_bool(c.ptr, cKey, C._Bool(value))
	return freeError(errPtr)
}

// SetJSON sets a complex value (list, struct, enum) in the context from a JSON string.
//
// Example:
//
//	ctx.SetJSON("items", `[{"label":"alpha"},{"label":"beta"}]`)
func (c *Context) SetJSON(key, jsonStr string) error {
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))
	cJSON := C.CString(jsonStr)
	defer C.free(unsafe.Pointer(cJSON))

	errPtr := C.pt_context_set_json(c.ptr, cKey, cJSON)
	return freeError(errPtr)
}

// MergeJSON merges all top-level keys from a JSON object into the context.
//
// The JSON string must be a JSON object. Each key becomes a context variable.
// This is more efficient than calling [Set] in a loop because it crosses the
// FFI boundary only once.
//
// Example:
//
//	ctx.MergeJSON(`{"name": "Alice", "count": 42}`)
func (c *Context) MergeJSON(jsonStr string) error {
	if c.ptr == nil {
		return ErrClosed
	}
	cJSON := C.CString(jsonStr)
	defer C.free(unsafe.Pointer(cJSON))

	errPtr := C.pt_context_merge_json(c.ptr, cJSON)
	return freeError(errPtr)
}

// MergeStruct merges all exported struct fields into the context via FlexBuffers.
//
// Struct fields are mapped to context variables by their json tag name,
// falling back to the lowercased field name. Unexported fields are skipped.
//
// Example:
//
//	ctx.MergeStruct(Params{Name: "Alice", Count: 42})
func (c *Context) MergeStruct(v any) error {
	if c.ptr == nil {
		return ErrClosed
	}
	data, err := marshalFlexbuffers(v)
	if err != nil {
		return fmt.Errorf("MergeStruct: cannot marshal to flexbuffers: %w", err)
	}
	if len(data) == 0 {
		return fmt.Errorf("MergeStruct: empty flexbuffers data")
	}

	errPtr := C.pt_context_merge_flexbuffers(c.ptr, (*C.uint8_t)(&data[0]), C.size_t(len(data)))
	return freeError(errPtr)
}

// MergeMap merges all map keys into the context via FlexBuffers.
func (c *Context) MergeMap(params map[string]any) error {
	if c.ptr == nil {
		return ErrClosed
	}
	data, err := marshalFlexbuffers(params)
	if err != nil {
		return fmt.Errorf("MergeMap: cannot marshal to flexbuffers: %w", err)
	}
	if len(data) == 0 {
		return fmt.Errorf("MergeMap: empty flexbuffers data")
	}

	errPtr := C.pt_context_merge_flexbuffers(c.ptr, (*C.uint8_t)(&data[0]), C.size_t(len(data)))
	return freeError(errPtr)
}

// SetTmpl sets a template-typed parameter in the context.
//
// This is used for tmpl<...> parameters, where one template is passed as a
// parameter to another template. The template is shared via Arc — the caller
// retains ownership of the original.
func (c *Context) SetTmpl(key string, tmpl *Template) error {
	if c.ptr == nil {
		return ErrClosed
	}
	if tmpl == nil || tmpl.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_tmpl(c.ptr, cKey, tmpl.ptr)
	return freeError(errPtr)
}

// Set sets a value in the context, automatically choosing the right type.
//
// Supported types:
//   - string → SetStr
//   - int, int64 → SetInt
//   - float64 → SetFloat
//   - bool → SetBool
//   - *Template → SetTmpl
//   - []any, map[string]any, structs → SetFlexbuffers (via binary encoding)
//   - anything else → SetFlexbuffers (via binary encoding)
func (c *Context) Set(key string, value any) error {
	if c.ptr == nil {
		return ErrClosed
	}
	switch v := value.(type) {
	case string:
		return c.SetStr(key, v)
	case int:
		return c.SetInt(key, int64(v))
	case int8:
		return c.SetInt(key, int64(v))
	case int16:
		return c.SetInt(key, int64(v))
	case int32:
		return c.SetInt(key, int64(v))
	case int64:
		return c.SetInt(key, v)
	case uint:
		return c.SetInt(key, int64(v))
	case uint8:
		return c.SetInt(key, int64(v))
	case uint16:
		return c.SetInt(key, int64(v))
	case uint32:
		return c.SetInt(key, int64(v))
	case float64:
		return c.SetFloat(key, v)
	case float32:
		return c.SetFloat(key, float64(v))
	case bool:
		return c.SetBool(key, v)
	case *Template:
		return c.SetTmpl(key, v)
	case Variant:
		if len(v.Fields) == 0 {
			// Unit variant → set as string directly.
			return c.SetStr(key, v.Kind)
		}
		// Struct variant → FlexBuffers binary encoding.
		data, err := marshalFlexbuffers(v)
		if err != nil {
			return fmt.Errorf("cannot marshal Variant to flexbuffers: %w", err)
		}
		cKey := C.CString(key)
		defer C.free(unsafe.Pointer(cKey))
		errPtr := C.pt_context_set_flexbuffers(c.ptr, cKey, (*C.uint8_t)(&data[0]), C.size_t(len(data)))
		return freeError(errPtr)
	default:
		data, err := marshalFlexbuffers(v)
		if err != nil {
			return fmt.Errorf("cannot marshal %T to flexbuffers: %w", v, err)
		}
		cKey := C.CString(key)
		defer C.free(unsafe.Pointer(cKey))
		errPtr := C.pt_context_set_flexbuffers(c.ptr, cKey, (*C.uint8_t)(&data[0]), C.size_t(len(data)))
		return freeError(errPtr)
	}
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

// NewCache creates a new empty template cache.
//
// Close must be called when the cache is no longer needed.
func NewCache() *Cache {
	cache := &Cache{ptr: C.pt_cache_new()}
	runtime.SetFinalizer(cache, func(c *Cache) { c.Close() })
	return cache
}

// Close frees the cache resources. Safe to call multiple times and
// concurrently. Implements [io.Closer].
func (c *Cache) Close() error {
	c.closeOnce.Do(func() {
		if c.ptr != nil {
			C.pt_cache_free(c.ptr)
			c.ptr = nil
		}
	})
	return nil
}

// Load loads a template through the cache. Unchanged files return cached
// compilations with zero re-parsing.
func (c *Cache) Load(path string) (*Template, error) {
	if c.ptr == nil {
		return nil, ErrClosed
	}

	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	var ptr unsafe.Pointer
	errPtr := C.pt_cache_load(c.ptr, cPath, &ptr)
	if err := freeError(errPtr); err != nil {
		return nil, err
	}

	t := &Template{ptr: ptr}
	runtime.SetFinalizer(t, func(t *Template) { t.Close() })
	return t, nil
}

// Clear invalidates all cached entries.
func (c *Cache) Clear() {
	if c.ptr != nil {
		C.pt_cache_clear(c.ptr)
	}
}

// TemplateCount returns the number of cached main templates.
func (c *Cache) TemplateCount() int {
	if c.ptr == nil {
		return 0
	}
	return int(C.pt_cache_template_count(c.ptr))
}

// IncludeCount returns the number of cached include templates.
func (c *Cache) IncludeCount() int {
	if c.ptr == nil {
		return 0
	}
	return int(C.pt_cache_include_count(c.ptr))
}
