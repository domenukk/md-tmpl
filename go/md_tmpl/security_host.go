package md_tmpl

/*
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

extern void pt_free_string(char *ptr);
extern char *pt_security_constants_json(void);
extern char *pt_decode_xml(const uint8_t *data, size_t len, char **out_err);
extern char *pt_escape_xml_attr(const uint8_t *data, size_t len, char **out_err);
extern char *pt_escape_xml_body(const uint8_t *data, size_t len, char **out_err);
extern char *pt_format_sanitize_notice(const char *primary_tag, const char *custom_notice, char **out_err);
extern char *pt_strip_outer_quarantine_tag(const uint8_t *data, size_t len, const char *tag_name);
extern char *pt_unquarantine(const uint8_t *data, size_t len, const char *tag_spec);
extern char *pt_quarantine_untrusted_idempotent(const uint8_t *data, size_t len, const char *tag_spec, char **out_err);
extern char *pt_sanitize_block_idempotent(const uint8_t *data, size_t len, const char *tag_spec, const char *custom_notice, char **out_err);
extern char *pt_truncate_middle(const uint8_t *data, size_t len, size_t limit, const char *marker, char **out_err);
extern char *pt_truncate_middle_list_json(const char *items_json, size_t limit, const char *marker, char **out_err);
*/
import "C"

import (
	"encoding/json"
	"sync"
	"unsafe"
)

// Canonical envelope tag names and markers (verified against the Rust core by
// TestHostConstantsMatchCore via SecurityConstants).
const (
	// SanitizeNoticeTagPlaceholder is replaced with the primary tag in sanitize notices.
	SanitizeNoticeTagPlaceholder = "{tag}"
	// ToolOutputQuarantineTag is the canonical `<tool_output_quarantine>` tag name.
	ToolOutputQuarantineTag = "tool_output_quarantine"
	// ToolOutputQuarantineOpenTag is the canonical `<tool_output_quarantine>` opening tag.
	ToolOutputQuarantineOpenTag = "<" + ToolOutputQuarantineTag + ">"
	// ToolOutputQuarantineCloseTag is the canonical `</tool_output_quarantine>` closing tag.
	ToolOutputQuarantineCloseTag = "</" + ToolOutputQuarantineTag + ">"
	// UntrustedToolOutputTag is the canonical `<untrusted_tool_output>` tag name.
	UntrustedToolOutputTag = "untrusted_tool_output"
	// UntrustedQuarantineOpenTag is the canonical `<untrusted_tool_output>` opening tag.
	UntrustedQuarantineOpenTag = "<" + UntrustedToolOutputTag + ">"
	// UntrustedQuarantineCloseTag is the canonical `</untrusted_tool_output>` closing tag.
	UntrustedQuarantineCloseTag = "</" + UntrustedToolOutputTag + ">"
	// DefaultTruncateMarker is the default multi-line marker inserted by TruncateMiddle.
	DefaultTruncateMarker = "\n[TRUNCATED: {skipped} bytes omitted]\n"
	// DefaultTruncateMarkerCompact is the default single-line string truncation marker.
	DefaultTruncateMarkerCompact = " [TRUNCATED: {skipped} bytes omitted] "
	// DefaultListTruncateMarker is the default marker item inserted by TruncateMiddleList.
	DefaultListTruncateMarker = "[TRUNCATED: {skipped} items omitted]"
)

// xmlDecodePairsKey is the SecurityConstants entry holding the XML decode pairs.
const xmlDecodePairsKey = "XML_DECODE_PAIRS"

var (
	securityConstantsOnce sync.Once
	securityConstantsVal  map[string]json.RawMessage
)

// SecurityConstants returns every host sanitization string constant exported by the
// Rust core, keyed by its Rust name (e.g. "TOOL_OUTPUT_QUARANTINE_TAG").
func SecurityConstants() map[string]string {
	out := make(map[string]string)
	for name, raw := range rawSecurityConstants() {
		if name == xmlDecodePairsKey {
			continue
		}
		var s string
		if err := json.Unmarshal(raw, &s); err != nil {
			panic("md_tmpl: malformed security constant " + name + ": " + err.Error())
		}
		out[name] = s
	}
	return out
}

// XMLDecodePairs returns the canonical `[entity, decoded]` pairs used by DecodeXML.
func XMLDecodePairs() [][2]string {
	var pairs [][2]string
	if err := json.Unmarshal(rawSecurityConstants()[xmlDecodePairsKey], &pairs); err != nil {
		panic("md_tmpl: malformed XML_DECODE_PAIRS from core: " + err.Error())
	}
	return pairs
}

func rawSecurityConstants() map[string]json.RawMessage {
	securityConstantsOnce.Do(func() {
		res := C.pt_security_constants_json()
		defer C.pt_free_string(res)
		if err := json.Unmarshal([]byte(C.GoString(res)), &securityConstantsVal); err != nil {
			panic("md_tmpl: malformed security constants from core: " + err.Error())
		}
	})
	return securityConstantsVal
}

// takeString converts an owned FFI result (with optional error) into a Go string.
func takeString(res *C.char, errPtr *C.char) (string, error) {
	if errPtr != nil {
		return "", freeError(errPtr)
	}
	defer C.pt_free_string(res)
	return C.GoString(res), nil
}

// takeOptional converts a nullable owned FFI result into (value, ok).
func takeOptional(res *C.char) (string, bool) {
	if res == nil {
		return "", false
	}
	defer C.pt_free_string(res)
	return C.GoString(res), true
}

// DecodeXML decodes the standard XML entity references in a single pass.
func DecodeXML(s string) string {
	ptr, n := strPtrAndLen(s)
	var errPtr *C.char
	out, err := takeString(C.pt_decode_xml(ptr, n, &errPtr), errPtr)
	if err != nil {
		panic(err)
	}
	return out
}

// EscapeXMLAttr escapes s for an XML attribute value.
func EscapeXMLAttr(s string) string {
	ptr, n := strPtrAndLen(s)
	var errPtr *C.char
	out, err := takeString(C.pt_escape_xml_attr(ptr, n, &errPtr), errPtr)
	if err != nil {
		panic(err)
	}
	return out
}

// EscapeXMLBody escapes s for an XML element body (`&`, `<`, `>`, and NUL).
func EscapeXMLBody(s string) string {
	ptr, n := strPtrAndLen(s)
	var errPtr *C.char
	out, err := takeString(C.pt_escape_xml_body(ptr, n, &errPtr), errPtr)
	if err != nil {
		panic(err)
	}
	return out
}

// FormatSanitizeNotice formats DefaultSanitizeNotice for primaryTag.
func FormatSanitizeNotice(primaryTag string) (string, error) {
	cTag := C.CString(primaryTag)
	defer C.free(unsafe.Pointer(cTag))
	var errPtr *C.char
	return takeString(C.pt_format_sanitize_notice(cTag, nil, &errPtr), errPtr)
}

// FormatSanitizeNoticeWithTemplate formats customNotice for primaryTag (substituting `{tag}`);
// an empty customNotice yields "".
func FormatSanitizeNoticeWithTemplate(primaryTag string, customNotice string) (string, error) {
	cTag := C.CString(primaryTag)
	defer C.free(unsafe.Pointer(cTag))
	cNotice := C.CString(customNotice)
	defer C.free(unsafe.Pointer(cNotice))
	var errPtr *C.char
	return takeString(C.pt_format_sanitize_notice(cTag, cNotice, &errPtr), errPtr)
}

// StripOuterQuarantineTag removes outer <tagName>...</tagName> tags (and one envelope newline
// on each side); ok is false when s is not wrapped.
func StripOuterQuarantineTag(s string, tagName string) (string, bool) {
	ptr, n := strPtrAndLen(s)
	cTag := C.CString(tagName)
	defer C.free(unsafe.Pointer(cTag))
	return takeOptional(C.pt_strip_outer_quarantine_tag(ptr, n, cTag))
}

// Unquarantine extracts the payload of a verified quarantine envelope; ok is false if invalid.
// Pass "" for tagSpec to use DefaultQuarantineTag.
func Unquarantine(s string, tagSpec string) (string, bool) {
	ptr, n := strPtrAndLen(s)
	cSpec, freeSpec := optCString(tagSpec)
	defer freeSpec()
	return takeOptional(C.pt_unquarantine(ptr, n, cSpec))
}

// QuarantineUntrustedIdempotent sanitizes and wraps s in a quarantine envelope unless it is
// already a valid one. Pass "" for tagSpec to use DefaultQuarantineTag.
func QuarantineUntrustedIdempotent(s string, tagSpec string) (string, error) {
	ptr, n := strPtrAndLen(s)
	cSpec, freeSpec := optCString(tagSpec)
	defer freeSpec()
	var errPtr *C.char
	return takeString(C.pt_quarantine_untrusted_idempotent(ptr, n, cSpec, &errPtr), errPtr)
}

// SanitizeBlockIdempotent sanitizes and wraps s in a SanitizeBlock envelope unless it is
// already a valid one. Pass "" for tagSpec to use DefaultSanitizeTag.
func SanitizeBlockIdempotent(s string, tagSpec string) (string, error) {
	ptr, n := strPtrAndLen(s)
	cSpec, freeSpec := optCString(tagSpec)
	defer freeSpec()
	var errPtr *C.char
	return takeString(C.pt_sanitize_block_idempotent(ptr, n, cSpec, nil, &errPtr), errPtr)
}

// SanitizeBlockIdempotentWithNotice is SanitizeBlockIdempotent with a custom notice
// ("" omits the notice line).
func SanitizeBlockIdempotentWithNotice(s string, tagSpec string, customNotice string) (string, error) {
	ptr, n := strPtrAndLen(s)
	cSpec, freeSpec := optCString(tagSpec)
	defer freeSpec()
	cNotice := C.CString(customNotice)
	defer C.free(unsafe.Pointer(cNotice))
	var errPtr *C.char
	return takeString(C.pt_sanitize_block_idempotent(ptr, n, cSpec, cNotice, &errPtr), errPtr)
}

// TruncateMiddle keeps at most limit UTF-8 bytes of s, replacing the middle with the default marker.
func TruncateMiddle(s string, limit uint) (string, error) {
	ptr, n := strPtrAndLen(s)
	var errPtr *C.char
	return takeString(C.pt_truncate_middle(ptr, n, C.size_t(limit), nil, &errPtr), errPtr)
}

// TruncateMiddleWithMarker is TruncateMiddle with a custom marker (may use `{skipped}`).
func TruncateMiddleWithMarker(s string, limit uint, marker string) (string, error) {
	ptr, n := strPtrAndLen(s)
	cMarker := C.CString(marker)
	defer C.free(unsafe.Pointer(cMarker))
	var errPtr *C.char
	return takeString(C.pt_truncate_middle(ptr, n, C.size_t(limit), cMarker, &errPtr), errPtr)
}

func truncateMiddleList(items []any, limit uint, cMarker *C.char) ([]any, error) {
	encoded, err := json.Marshal(items)
	if err != nil {
		return nil, err
	}
	cItems := C.CString(string(encoded))
	defer C.free(unsafe.Pointer(cItems))
	var errPtr *C.char
	out, err := takeString(C.pt_truncate_middle_list_json(cItems, C.size_t(limit), cMarker, &errPtr), errPtr)
	if err != nil {
		return nil, err
	}
	var result []any
	if err := json.Unmarshal([]byte(out), &result); err != nil {
		return nil, err
	}
	return result, nil
}

// TruncateMiddleList keeps at most limit items, replacing the omitted middle with one
// DefaultListTruncateMarker item.
func TruncateMiddleList(items []any, limit uint) ([]any, error) {
	return truncateMiddleList(items, limit, nil)
}

// TruncateMiddleListWithMarker is TruncateMiddleList with a custom marker ("" inserts no marker).
func TruncateMiddleListWithMarker(items []any, limit uint, marker string) ([]any, error) {
	cMarker := C.CString(marker)
	defer C.free(unsafe.Pointer(cMarker))
	return truncateMiddleList(items, limit, cMarker)
}
