package md_tmpl

/*
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

extern void pt_free_string(char *ptr);

// Context lifecycle.
extern void *pt_context_new(void);
extern void pt_context_free(void *ctx);
extern char *pt_context_set_str(void *ctx, const char *key, const char *value);
extern char *pt_context_set_int(void *ctx, const char *key, int64_t value);
extern char *pt_context_set_float(void *ctx, const char *key, double value);
extern char *pt_context_set_bool(void *ctx, const char *key, _Bool value);
extern char *pt_context_set_none(void *ctx, const char *key);
extern char *pt_context_set_json(void *ctx, const char *key, const char *json_str);
extern char *pt_context_merge_json(void *ctx, const char *json_str);
extern char *pt_context_set_tmpl(void *ctx, const char *key, void *tmpl);
extern char *pt_context_set_flexbuffers(void *ctx, const char *key, const uint8_t *data, size_t len);

// Cache lifecycle.
extern char *pt_context_merge_flexbuffers(void *ctx, const uint8_t *data, size_t len);
extern void *pt_cache_new(void);
extern void pt_cache_free(void *cache);
extern char *pt_cache_load(const void *cache, const char *path, void **out);
extern void pt_cache_clear(const void *cache);
extern size_t pt_cache_template_count(const void *cache);
extern size_t pt_cache_include_count(const void *cache);
*/
import "C"

import (
	"errors"
	"fmt"
	"math"
	"runtime"
	"unsafe"
)

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
	if c == nil {
		return nil
	}
	c.closeOnce.Do(func() {
		c.mu.Lock()
		defer c.mu.Unlock()
		if c.ptr != nil {
			C.pt_context_free(c.ptr)
			c.ptr = nil
		}
	})
	return nil
}

// SetStr sets a string value in the context.
func (c *Context) SetStr(key, value string) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))
	cVal := C.CString(value)
	defer C.free(unsafe.Pointer(cVal))

	errPtr := C.pt_context_set_str(c.ptr, cKey, cVal)
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// SetInt sets an integer value in the context.
func (c *Context) SetInt(key string, value int64) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_int(c.ptr, cKey, C.int64_t(value))
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// SetFloat sets a float value in the context.
func (c *Context) SetFloat(key string, value float64) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_float(c.ptr, cKey, C.double(value))
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// SetBool sets a bool value in the context.
func (c *Context) SetBool(key string, value bool) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_bool(c.ptr, cKey, C._Bool(value))
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// SetNone sets a None (absent) value in the context.
func (c *Context) SetNone(key string) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_none(c.ptr, cKey)
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// SetJSON sets a complex value (list, struct, enum) in the context from a JSON string.
func (c *Context) SetJSON(key, jsonStr string) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))
	cJSON := C.CString(jsonStr)
	defer C.free(unsafe.Pointer(cJSON))

	errPtr := C.pt_context_set_json(c.ptr, cKey, cJSON)
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// MergeJSON merges all top-level keys from a JSON object into the context.
func (c *Context) MergeJSON(jsonStr string) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	cJSON := C.CString(jsonStr)
	defer C.free(unsafe.Pointer(cJSON))

	errPtr := C.pt_context_merge_json(c.ptr, cJSON)
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// MergeStruct merges all exported struct fields into the context via FlexBuffers.
func (c *Context) MergeStruct(v any) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	data, err := marshalFlexbuffers(v)
	if err != nil {
		return fmt.Errorf("MergeStruct: cannot marshal to flexbuffers: %w", err)
	}
	if len(data) == 0 {
		return errors.New("md_tmpl: MergeStruct: empty flexbuffers data")
	}

	errPtr := C.pt_context_merge_flexbuffers(c.ptr, (*C.uint8_t)(&data[0]), C.size_t(len(data)))
	runtime.KeepAlive(data)
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// MergeMap merges all map keys into the context via FlexBuffers.
func (c *Context) MergeMap(params map[string]any) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	data, err := marshalFlexbuffers(params)
	if err != nil {
		return fmt.Errorf("MergeMap: cannot marshal to flexbuffers: %w", err)
	}
	if len(data) == 0 {
		return errors.New("md_tmpl: MergeMap: empty flexbuffers data")
	}

	errPtr := C.pt_context_merge_flexbuffers(c.ptr, (*C.uint8_t)(&data[0]), C.size_t(len(data)))
	runtime.KeepAlive(data)
	runtime.KeepAlive(c)
	return freeError(errPtr)
}

// SetTmpl sets a template-typed parameter in the context.
func (c *Context) SetTmpl(key string, tmpl *Template) error {
	if c == nil || tmpl == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	tmpl.mu.RLock()
	defer tmpl.mu.RUnlock()
	if tmpl.ptr == nil {
		return ErrClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))

	errPtr := C.pt_context_set_tmpl(c.ptr, cKey, tmpl.ptr)
	runtime.KeepAlive(c)
	runtime.KeepAlive(tmpl)
	return freeError(errPtr)
}

func (c *Context) isClosed() bool {
	if c == nil {
		return true
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.ptr == nil
}

// Set sets a value in the context, automatically choosing the right type.
func (c *Context) Set(key string, value any) error {
	if c.isClosed() {
		return ErrClosed
	}
	if value == nil {
		return c.SetNone(key)
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
		if uint64(v) > math.MaxInt64 {
			return fmt.Errorf("md_tmpl: uint value %d overflows int64", v)
		}
		return c.SetInt(key, int64(v))
	case uint8:
		return c.SetInt(key, int64(v))
	case uint16:
		return c.SetInt(key, int64(v))
	case uint32:
		return c.SetInt(key, int64(v))
	case uint64:
		if v > math.MaxInt64 {
			return fmt.Errorf("md_tmpl: uint64 value %d overflows int64", v)
		}
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
			return c.SetStr(key, v.Kind)
		}
		return c.setFlexbuffers(key, v)
	default:
		return c.setFlexbuffers(key, v)
	}
}

// setFlexbuffers marshals any value to FlexBuffers and sets it in the context.
func (c *Context) setFlexbuffers(key string, v any) error {
	if c == nil {
		return ErrClosed
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ptr == nil {
		return ErrClosed
	}
	data, err := marshalFlexbuffers(v)
	if err != nil {
		return fmt.Errorf("cannot marshal %T to flexbuffers: %w", v, err)
	}
	if len(data) == 0 {
		return errors.New("md_tmpl: setFlexbuffers: empty flexbuffers data")
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))
	errPtr := C.pt_context_set_flexbuffers(c.ptr, cKey, (*C.uint8_t)(&data[0]), C.size_t(len(data)))
	runtime.KeepAlive(data)
	runtime.KeepAlive(c)
	return freeError(errPtr)
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
	if c == nil {
		return nil
	}
	c.closeOnce.Do(func() {
		c.mu.Lock()
		defer c.mu.Unlock()
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
	if c == nil {
		return nil, ErrClosed
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	if c.ptr == nil {
		return nil, ErrClosed
	}

	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	var ptr unsafe.Pointer
	errPtr := C.pt_cache_load(c.ptr, cPath, &ptr)
	runtime.KeepAlive(c)
	if err := freeError(errPtr); err != nil {
		return nil, err
	}

	t := &Template{ptr: ptr}
	runtime.SetFinalizer(t, func(t *Template) { t.Close() })
	return t, nil
}

// Clear invalidates all cached entries.
func (c *Cache) Clear() {
	if c == nil {
		return
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	if c.ptr != nil {
		C.pt_cache_clear(c.ptr)
		runtime.KeepAlive(c)
	}
}

// TemplateCount returns the number of cached main templates.
func (c *Cache) TemplateCount() int {
	if c == nil {
		return 0
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	if c.ptr == nil {
		return 0
	}
	n := int(C.pt_cache_template_count(c.ptr))
	runtime.KeepAlive(c)
	return n
}

// IncludeCount returns the number of cached include templates.
func (c *Cache) IncludeCount() int {
	if c == nil {
		return 0
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	if c.ptr == nil {
		return 0
	}
	n := int(C.pt_cache_include_count(c.ptr))
	runtime.KeepAlive(c)
	return n
}
