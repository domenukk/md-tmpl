// Package md_tmpl provides a fast, strongly-typed template engine
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
//	tmpl, err := md_tmpl.FromSource(source)
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
package md_tmpl

/*
#cgo LDFLAGS: -L${SRCDIR}/../../target/release -lmd_tmpl_ffi -ldl -lpthread -lm
#cgo CFLAGS: -I${SRCDIR}/../../crates/md-tmpl-ffi/include

#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

// String lifecycle.
extern void pt_free_string(char *ptr);

// Template lifecycle.
extern char *pt_template_from_source(const char *source, void **out);
extern char *pt_template_from_source_with_options(const char *source, const char *base_dir, const char *env_json, _Bool allow_unused, void **out);
extern char *pt_template_from_file(const char *path, void **out);
extern char *pt_template_from_file_with_options(const char *path, const char *base_dir, const char *env_json, _Bool allow_unused, void **out);
extern char *pt_template_from_source_with_frontmatter(const char *source, void **out_tmpl, char **out_fm);
extern void pt_template_free(void *tmpl);
*/
import "C"

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"runtime"
	"strings"
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
var debugLog = log.New(io.Discard, "md_tmpl: ", log.LstdFlags)

// Sentinel errors returned by the package.
var (
	// ErrClosed is returned when operating on a Template, Context, or Cache
	// that has already been closed.
	ErrClosed = errors.New("md_tmpl: resource is closed")

	// ErrNilContext is returned when a nil or closed Context is passed to Render.
	ErrNilContext = errors.New("md_tmpl: context is nil or closed")
)

// ErrorKind is a stable, machine-readable classification of an engine error.
//
// The values mirror md_tmpl::ErrorKind on the Rust side and are stable across
// releases, so they are safe to switch on or compare against.
type ErrorKind string

// Known error kinds. These match the stable ids emitted by the core engine.
const (
	KindIO                  ErrorKind = "io"
	KindUndefinedVariable   ErrorKind = "undefined_variable"
	KindSyntax              ErrorKind = "syntax"
	KindMissingParams       ErrorKind = "missing_params"
	KindTypeMismatch        ErrorKind = "type_mismatch"
	KindUnknownFilter       ErrorKind = "unknown_filter"
	KindIncludeNotFound     ErrorKind = "include_not_found"
	KindDeclarationsMutated ErrorKind = "declarations_mutated"
	KindExtraParams         ErrorKind = "extra_params"
	KindPanic               ErrorKind = "panic"
	// KindUnknown is used when the engine returns an error without a
	// recognizable kind prefix (e.g. errors originating in the FFI shim).
	KindUnknown ErrorKind = ""
)

// TemplateError is a structured error returned by the template engine.
//
// It carries a machine-readable [ErrorKind] alongside the human-readable
// message and the typed payload of that kind, so callers never need to parse
// Message. Fields that do not apply to Kind are zero. Match specific kinds with
// [errors.Is] against the package sentinels (e.g. [ErrMissingParams]) or
// inspect Kind directly. The payload fields mirror the Python exception
// attributes and the TypeScript error properties.
type TemplateError struct {
	Kind    ErrorKind
	Message string

	// Line is the 1-based, file-absolute line of a [KindSyntax] error
	// (0 when unknown).
	Line int
	// Snippet is the offending source line of a [KindSyntax] error.
	Snippet string
	// Variable is the name that was not found ([KindUndefinedVariable]).
	Variable string
	// Missing lists required parameters that were not provided
	// ([KindMissingParams]).
	Missing []string
	// Extra lists undeclared parameters that were provided ([KindExtraParams]).
	Extra []string
	// Path is the dotted path of the offending value ([KindTypeMismatch]).
	Path string
	// Expected is the declared type ([KindTypeMismatch]).
	Expected string
	// Actual is the provided type ([KindTypeMismatch]).
	Actual string
	// Filter is the unknown filter name ([KindUnknownFilter]).
	Filter string
	// Include is the include path that could not be resolved
	// ([KindIncludeNotFound]).
	Include string
	// Details describes how the parameter contract changed
	// ([KindDeclarationsMutated]).
	Details string
}

// errorPayload is the JSON payload appended to transported errors by the FFI
// (see pt error strings: "<kind>\x1f<message>[\x1f<json>]").
type errorPayload struct {
	Line     int      `json:"line"`
	Snippet  string   `json:"snippet"`
	Variable string   `json:"variable"`
	Missing  []string `json:"missing"`
	Extra    []string `json:"extra"`
	Path     string   `json:"path"`
	Expected string   `json:"expected"`
	Actual   string   `json:"actual"`
	Filter   string   `json:"filter"`
	Include  string   `json:"include"`
	Details  string   `json:"details"`
}

// Error implements the error interface.
func (e *TemplateError) Error() string { return e.Message }

// Is reports whether target is a *TemplateError of the same kind, enabling
// errors.Is(err, ErrMissingParams) style matching.
func (e *TemplateError) Is(target error) bool {
	t, ok := target.(*TemplateError)
	return ok && t.Kind == e.Kind
}

// Per-kind sentinels for use with errors.Is.
var (
	ErrIO                  = &TemplateError{Kind: KindIO}
	ErrUndefinedVariable   = &TemplateError{Kind: KindUndefinedVariable}
	ErrSyntax              = &TemplateError{Kind: KindSyntax}
	ErrMissingParams       = &TemplateError{Kind: KindMissingParams}
	ErrTypeMismatch        = &TemplateError{Kind: KindTypeMismatch}
	ErrUnknownFilter       = &TemplateError{Kind: KindUnknownFilter}
	ErrIncludeNotFound     = &TemplateError{Kind: KindIncludeNotFound}
	ErrDeclarationsMutated = &TemplateError{Kind: KindDeclarationsMutated}
	ErrExtraParams         = &TemplateError{Kind: KindExtraParams}
	ErrPanic               = &TemplateError{Kind: KindPanic}
)

// errKindSep is the ASCII Unit Separator (U+001F) the FFI uses to delimit the
// stable error kind from the human-readable message: "<kind>\x1f<message>".
const errKindSep = "\x1f"

// Template is a parsed, validated template ready for rendering.
//
// Templates are compiled from source strings or loaded from files.
// Parameters are type-checked against frontmatter declarations at render time.
//
// Template is safe for concurrent use from multiple goroutines after creation.
//
// Close must be called when the template is no longer needed.
type Template struct {
	mu        sync.RWMutex
	ptr       unsafe.Pointer
	closeOnce sync.Once
}

// Context holds the variables available during template rendering.
//
// Context is synchronized against concurrent Close/finalization.
//
// Close must be called when the context is no longer needed.
type Context struct {
	mu        sync.Mutex
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
	mu        sync.RWMutex
	ptr       unsafe.Pointer
	closeOnce sync.Once
}

// Declaration represents a single parameter declaration from frontmatter.
type Declaration struct {
	Name    string // Parameter name.
	Type    string // Parameter type (e.g. "str", "int", "list(label = str)").
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
// By embedding TaggedVariant in your custom structs, you get static
// typing for template enum variants that mirrors Rust's enum representation.
//
// Example:
//
//	type Confirmed struct {
//	    md_tmpl.TaggedVariant
//	    Evidence string `json:"evidence"`
//	}
//
//	func NewConfirmed(evidence string) Confirmed {
//	    return Confirmed{TaggedVariant: md_tmpl.NewTaggedVariant("Confirmed"), Evidence: evidence}
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
// For static type safety without map allocations, embed [TaggedVariant] instead.
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

// VariantMarshaler is implemented by statically-typed enum variant types (such
// as those emitted by the code generator) to expose their dynamic,
// __kind__-tagged representation.
//
// It lets the FlexBuffers and JSON encoders serialize a concrete variant using
// the shared cross-language wire format — a bare string for unit variants, or a
// {"__kind__": "...", ...fields} object for data variants — without the
// encoder needing to know the concrete type. Generated code implements this;
// you rarely call it directly.
type VariantMarshaler interface {
	AsVariant() Variant
}

// VariantKind extracts the discriminant name from a variant's JSON encoding.
//
// It accepts both wire forms: a bare JSON string (a unit variant, e.g.
// "Rejected") and an object carrying a "__kind__" field (a data variant, e.g.
// {"__kind__":"Confirmed","evidence":"x"}). Generated UnmarshalX dispatchers
// use it to decide which concrete variant to decode into.
func VariantKind(data []byte) (string, error) {
	trimmed := bytes.TrimSpace(data)
	if len(trimmed) == 0 {
		return "", errors.New("md_tmpl: empty variant JSON")
	}
	if trimmed[0] == '"' {
		var name string
		if err := json.Unmarshal(trimmed, &name); err != nil {
			return "", fmt.Errorf("md_tmpl: decoding unit variant: %w", err)
		}
		return name, nil
	}
	var probe struct {
		Kind string `json:"__kind__"`
	}
	if err := json.Unmarshal(trimmed, &probe); err != nil {
		return "", fmt.Errorf("md_tmpl: decoding data variant: %w", err)
	}
	if probe.Kind == "" {
		return "", errors.New("md_tmpl: variant object missing __kind__")
	}
	return probe.Kind, nil
}

// freeError converts a C error string to a Go error and frees the C string.
// Returns nil if errPtr is nil (no error).
//
// The FFI encodes errors as "<kind>\x1f<message>". When the separator is
// present the result is a [*TemplateError] carrying the stable [ErrorKind];
// otherwise a plain error with the raw message is returned.
func freeError(errPtr *C.char) error {
	if errPtr == nil {
		return nil
	}
	msg := C.GoString(errPtr)
	C.pt_free_string(errPtr)
	return parseError(msg)
}

// parseError splits a transported error string into a typed error.
func parseError(raw string) error {
	if raw == "" {
		return nil
	}
	if kind, rest, found := strings.Cut(raw, errKindSep); found {
		message, payload, hasPayload := strings.Cut(rest, errKindSep)
		te := &TemplateError{Kind: ErrorKind(kind), Message: message}
		if hasPayload {
			var p errorPayload
			if err := json.Unmarshal([]byte(payload), &p); err == nil {
				te.Line, te.Snippet, te.Variable = p.Line, p.Snippet, p.Variable
				te.Missing, te.Extra = p.Missing, p.Extra
				te.Path, te.Expected, te.Actual = p.Path, p.Expected, p.Actual
				te.Filter, te.Include, te.Details = p.Filter, p.Include, p.Details
			} else {
				// Keep the typed kind and message even if the payload is malformed.
				te.Message = rest
			}
		}
		return te
	}
	// No kind prefix (e.g. an error from the FFI shim itself).
	return errors.New(raw)
}

// ---------------------------------------------------------------------------
// Template constructors
// ---------------------------------------------------------------------------

// Option configures how a template is compiled by [FromSource] and [FromFile].
//
// Options compose freely, e.g.:
//
//	tmpl, err := md_tmpl.FromSource(src,
//	    md_tmpl.WithBaseDir("/prompts"),
//	    md_tmpl.WithEnv(map[string]any{"MAX_RETRIES": 5}),
//	    md_tmpl.WithAllowUnused(),
//	)
type Option func(*compileOptions)

// compileOptions accumulates the effect of the applied [Option] values.
type compileOptions struct {
	baseDir     string
	env         map[string]any
	hasEnv      bool
	allowUnused bool
}

// WithBaseDir resolves {% include %} and imports: directives relative to dir.
func WithBaseDir(dir string) Option {
	return func(o *compileOptions) { o.baseDir = dir }
}

// WithEnv supplies compile-time environment variables.
//
// Values are resolved against `env:` declarations in the frontmatter, typed,
// and serialized as JSON to the engine. Passing WithEnv(nil) is valid and
// means "no environment values", which still satisfies env declarations that
// provide defaults.
func WithEnv(env map[string]any) Option {
	return func(o *compileOptions) {
		o.env = env
		o.hasEnv = true
	}
}

// WithAllowUnused permits declared parameters that are not referenced in the
// template body (rejected by default).
func WithAllowUnused() Option {
	return func(o *compileOptions) { o.allowUnused = true }
}

// applyOptions folds opts into a compileOptions value.
func applyOptions(opts []Option) compileOptions {
	var co compileOptions
	for _, opt := range opts {
		opt(&co)
	}
	return co
}

// cBaseDir returns a C string for the base dir (or nil), plus a cleanup func.
// The cleanup func must always be called.
func (co compileOptions) cBaseDir() (*C.char, func()) {
	if co.baseDir == "" {
		return nil, func() {}
	}
	c := C.CString(co.baseDir)
	return c, func() { C.free(unsafe.Pointer(c)) }
}

// cEnvJSON marshals the env map to a C JSON string (or nil), plus a cleanup
// func. The cleanup func must always be called. A nil env map is encoded as an
// empty JSON object ("{}") rather than "null", so WithEnv(nil) resolves env
// declarations purely from their defaults.
func (co compileOptions) cEnvJSON() (*C.char, func(), error) {
	if !co.hasEnv {
		return nil, func() {}, nil
	}
	env := co.env
	if env == nil {
		env = map[string]any{}
	}
	envJSON, err := json.Marshal(env)
	if err != nil {
		return nil, func() {}, fmt.Errorf("cannot marshal env: %w", err)
	}
	c := C.CString(string(envJSON))
	return c, func() { C.free(unsafe.Pointer(c)) }, nil
}

// newTemplate wraps a raw FFI pointer in a finalized *Template.
func newTemplate(ptr unsafe.Pointer) *Template {
	t := &Template{ptr: ptr}
	runtime.SetFinalizer(t, func(t *Template) { t.Close() })
	return t
}

// FromSource parses a template from an in-memory source string.
//
// The source must include YAML frontmatter with parameter declarations.
// Unused declared parameters (present in frontmatter but not in the body)
// are rejected unless [WithAllowUnused] is supplied. Includes are resolved
// relative to the directory given by [WithBaseDir], and compile-time
// environment values may be supplied with [WithEnv].
func FromSource(source string, opts ...Option) (*Template, error) {
	co := applyOptions(opts)

	cSource := C.CString(source)
	defer C.free(unsafe.Pointer(cSource))

	cDir, freeDir := co.cBaseDir()
	defer freeDir()
	cEnv, freeEnv, err := co.cEnvJSON()
	if err != nil {
		return nil, err
	}
	defer freeEnv()

	var ptr unsafe.Pointer
	errPtr := C.pt_template_from_source_with_options(cSource, cDir, cEnv, C._Bool(co.allowUnused), &ptr)
	if err := freeError(errPtr); err != nil {
		return nil, err
	}
	return newTemplate(ptr), nil
}

// FromSourceAllowingUnused parses a template, allowing declared parameters
// that aren't referenced in the body.
//
// It is shorthand for FromSource(source, WithAllowUnused()).
func FromSourceAllowingUnused(source string) (*Template, error) {
	return FromSource(source, WithAllowUnused())
}

// FromSourceWithBaseDir parses a template from source, resolving includes
// relative to the given base directory.
//
// It is shorthand for FromSource(source, WithBaseDir(baseDir)).
func FromSourceWithBaseDir(source, baseDir string) (*Template, error) {
	return FromSource(source, WithBaseDir(baseDir))
}

// FromSourceWithEnv parses a template with compile-time environment variables.
//
// It is shorthand for FromSource(source, WithEnv(env)).
func FromSourceWithEnv(source string, env map[string]any) (*Template, error) {
	return FromSource(source, WithEnv(env))
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
//
// The same compile [Option] values accepted by [FromSource] apply here. When
// [WithBaseDir] is omitted, includes are resolved relative to the file's own
// parent directory.
func FromFile(path string, opts ...Option) (*Template, error) {
	co := applyOptions(opts)

	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	cDir, freeDir := co.cBaseDir()
	defer freeDir()
	cEnv, freeEnv, err := co.cEnvJSON()
	if err != nil {
		return nil, err
	}
	defer freeEnv()

	var ptr unsafe.Pointer
	errPtr := C.pt_template_from_file_with_options(cPath, cDir, cEnv, C._Bool(co.allowUnused), &ptr)
	if err := freeError(errPtr); err != nil {
		return nil, err
	}
	return newTemplate(ptr), nil
}

// Close frees the template resources. Safe to call multiple times and
// concurrently. Implements [io.Closer].
func (t *Template) Close() error {
	if t == nil {
		return nil
	}
	t.closeOnce.Do(func() {
		t.mu.Lock()
		defer t.mu.Unlock()
		if t.ptr != nil {
			C.pt_template_free(t.ptr)
			t.ptr = nil
		}
	})
	return nil
}
