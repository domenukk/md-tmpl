//! WebAssembly bindings for `md-tmpl` security, escaping, token-sanitization, and quarantine primitives.

use js_sys::Array;
use wasm_bindgen::prelude::*;

use crate::{js_error, u32_from_usize};

fn delimiters_to_js(pairs: &[(&str, &str)]) -> Array {
    let arr = Array::new_with_length(u32_from_usize(pairs.len()));
    for (i, &(raw, escaped)) in pairs.iter().enumerate() {
        let pair = Array::new_with_length(2);
        pair.set(0, JsValue::from_str(raw));
        pair.set(1, JsValue::from_str(escaped));
        arr.set(u32_from_usize(i), pair.into());
    }
    arr
}

/// Return the default XML tag name (`"untrusted_content"`) used by `quarantine`.
#[wasm_bindgen(js_name = "defaultQuarantineTag")]
#[must_use]
pub fn default_quarantine_tag() -> String {
    md_tmpl::DEFAULT_QUARANTINE_TAG.to_string()
}

/// Return `TOKEN_DELIMITERS` as a JS array of `[raw, escaped]` pairs.
#[wasm_bindgen(js_name = "tokenDelimiters")]
#[must_use]
pub fn token_delimiters() -> Array {
    delimiters_to_js(md_tmpl::TOKEN_DELIMITERS)
}

/// Return `ROLE_TOKEN_DELIMITERS` as a JS array of `[raw, escaped]` pairs.
#[wasm_bindgen(js_name = "roleTokenDelimiters")]
#[must_use]
pub fn role_token_delimiters() -> Array {
    delimiters_to_js(md_tmpl::ROLE_TOKEN_DELIMITERS)
}

/// Escape XML/HTML special characters and strip illegal XML 1.0 control characters.
#[wasm_bindgen(js_name = "escapeXmlString")]
#[must_use]
pub fn escape_xml_string(s: &str) -> String {
    md_tmpl::escape_xml_str(s).into_owned()
}

/// Escape JSON string body characters (including `/` and `U+2028`/`U+2029`).
#[wasm_bindgen(js_name = "escapeJsonString")]
#[must_use]
pub fn escape_json_string(s: &str) -> String {
    md_tmpl::escape_json_str(s).into_owned()
}

/// Return `true` if `s` contains any known LLM control token or `<|...|>` / `<｜...｜>` special token.
#[wasm_bindgen(js_name = "hasControlTokens")]
#[must_use]
pub fn has_control_tokens(s: &str) -> bool {
    md_tmpl::has_control_tokens(s)
}

/// Return `true` if `s` contains any role/turn control token in `ROLE_TOKEN_DELIMITERS` or generic pipe token.
#[wasm_bindgen(js_name = "hasRoleControlTokens")]
#[must_use]
pub fn has_role_control_tokens(s: &str) -> bool {
    md_tmpl::has_role_control_tokens(s)
}

/// Neutralize all known LLM control tokens and `<|...|>` / `<｜...｜>` special tokens in a single pass.
#[wasm_bindgen(js_name = "sanitizeTokensString")]
#[must_use]
pub fn sanitize_tokens_string(s: &str) -> String {
    md_tmpl::sanitize_tokens_str(s).into_owned()
}

/// Neutralize role/turn headers (`ROLE_TOKEN_DELIMITERS`) and generic pipe tokens in a single pass while preserving XML tool docs.
#[wasm_bindgen(js_name = "sanitizeRoleTokensString")]
#[must_use]
pub fn sanitize_role_tokens_string(s: &str) -> String {
    md_tmpl::sanitize_role_tokens_str(s).into_owned()
}

/// Wrap `s` in Markdown code fences with adaptive backtick counts.
///
/// # Errors
///
/// Returns a JS `Error` (`kind = "syntax"`) if `lang` contains backticks or whitespace.
#[wasm_bindgen(js_name = "fenceString")]
pub fn fence_string(s: &str, lang: Option<String>) -> Result<String, JsValue> {
    let res = md_tmpl::fence_str(s, lang.as_deref()).map_err(|e| js_error(&e));
    drop(lang);
    res
}

/// Return `true` if `s` contains any opening/closing XML tag matching `tag_spec`.
#[wasm_bindgen(js_name = "hasQuarantineTagBreakout")]
#[must_use]
pub fn has_quarantine_tag_breakout(s: &str, tag_spec: &str) -> bool {
    md_tmpl::has_quarantine_tag_breakout(s, tag_spec)
}

/// Return `true` if `s` contains any LLM control token, generic pipe token, or XML tag matching `tag_spec` in a single pass.
#[wasm_bindgen(js_name = "hasUntrustedBreakout")]
#[must_use]
pub fn has_untrusted_breakout(s: &str, tag_spec: &str) -> bool {
    md_tmpl::has_untrusted_breakout(s, tag_spec)
}

/// Sanitize untrusted content within quarantine boundaries for one or more comma-separated XML tag names.
#[wasm_bindgen(js_name = "sanitizeQuarantinePayload")]
#[must_use]
pub fn sanitize_quarantine_payload(s: &str, tag_spec: &str) -> String {
    md_tmpl::sanitize_quarantine_payload(s, tag_spec).into_owned()
}

/// Neutralize all known LLM control tokens, generic pipe tokens, and opening/closing XML boundary tags matching `tag_spec` in a single pass.
#[wasm_bindgen(js_name = "sanitizeUntrustedString")]
#[must_use]
pub fn sanitize_untrusted_string(s: &str, tag_spec: &str) -> String {
    md_tmpl::sanitize_untrusted_str(s, tag_spec).into_owned()
}

/// Wrap `s` in boundary XML tags (`<primary_tag>\n...\n</primary_tag>`), escaping any embedded tags matching `tag_spec`.
///
/// # Errors
///
/// Returns a JS `Error` (`kind = "syntax"`) if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
#[wasm_bindgen(js_name = "quarantineString")]
pub fn quarantine_string(s: &str, tag_spec: Option<String>) -> Result<String, JsValue> {
    let res = md_tmpl::quarantine_str(s, tag_spec.as_deref()).map_err(|e| js_error(&e));
    drop(tag_spec);
    res
}

/// Sanitize both control tokens and boundary XML tags in a single pass and wrap in `<primary_tag>\n...\n</primary_tag>`.
///
/// # Errors
///
/// Returns a JS `Error` (`kind = "syntax"`) if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
#[wasm_bindgen(js_name = "quarantineUntrustedString")]
pub fn quarantine_untrusted_string(s: &str, tag_spec: Option<String>) -> Result<String, JsValue> {
    let res = md_tmpl::quarantine_untrusted_str(s, tag_spec.as_deref()).map_err(|e| js_error(&e));
    drop(tag_spec);
    res
}

/// Return `true` if `s` is wrapped in `<primary_tag>...</primary_tag>` with zero unescaped boundary tags or control tokens.
#[wasm_bindgen(js_name = "isQuarantinedString")]
#[must_use]
pub fn is_quarantined_string(s: &str, tag_spec: Option<String>) -> bool {
    let res = md_tmpl::is_quarantined_str(s, tag_spec.as_deref());
    drop(tag_spec);
    res
}

/// Return the default XML tag name (`"untrusted_content"`) used by `sanitize`.
#[wasm_bindgen(js_name = "defaultSanitizeTag")]
#[must_use]
pub fn default_sanitize_tag() -> String {
    md_tmpl::DEFAULT_SANITIZE_TAG.to_string()
}

/// Return the default untrusted-data boundary notice inserted inside `<tag>...</tag>` blocks by `sanitize`.
#[wasm_bindgen(js_name = "defaultSanitizeNotice")]
#[must_use]
pub fn default_sanitize_notice() -> String {
    md_tmpl::DEFAULT_SANITIZE_NOTICE.to_string()
}

/// Neutralize all known LLM control tokens, generic pipe tokens, and optional XML boundary tags (`tag_spec`).
///
/// # Errors
///
/// Returns a JS `Error` (`kind = "syntax"`) if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
#[wasm_bindgen(js_name = "sanitizeString")]
pub fn sanitize_string(s: &str, tag_spec: Option<String>) -> Result<String, JsValue> {
    let res = md_tmpl::sanitize_str(s, tag_spec.as_deref())
        .map(std::borrow::Cow::into_owned)
        .map_err(|e| js_error(&e));
    drop(tag_spec);
    res
}

/// Sanitize control tokens and boundary XML tags in a single pass and wrap `s` in
/// `<primary_tag>\n{notice}\n{sanitized}\n</primary_tag>`.
///
/// # Errors
///
/// Returns a JS `Error` (`kind = "syntax"`) if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
#[wasm_bindgen(js_name = "sanitizeBlockString")]
pub fn sanitize_block_string(
    s: &str,
    tag_spec: Option<String>,
    custom_notice: Option<String>,
) -> Result<String, JsValue> {
    let res = md_tmpl::sanitize_block_str(s, tag_spec.as_deref(), custom_notice.as_deref())
        .map_err(|e| js_error(&e));
    drop(tag_spec);
    drop(custom_notice);
    res
}

/// Return `true` if `s` is wrapped in `<primary_tag>\n{notice}\n...\n</primary_tag>` with zero unescaped boundary tags or control tokens.
#[wasm_bindgen(js_name = "isSanitizedBlockString")]
#[must_use]
pub fn is_sanitized_block_string(
    s: &str,
    tag_spec: Option<String>,
    custom_notice: Option<String>,
) -> bool {
    let res = md_tmpl::is_sanitized_block_str(s, tag_spec.as_deref(), custom_notice.as_deref());
    drop(tag_spec);
    drop(custom_notice);
    res
}

/// Extract the inner sanitized payload from a verified `sanitizeBlockString` envelope, or `None` if invalid.
#[wasm_bindgen(js_name = "unsanitizeBlockString")]
#[must_use]
pub fn unsanitize_block_string(
    s: &str,
    tag_spec: Option<String>,
    custom_notice: Option<String>,
) -> Option<String> {
    let res = md_tmpl::unsanitize_block_str(s, tag_spec.as_deref(), custom_notice.as_deref())
        .map(str::to_owned);
    drop(tag_spec);
    drop(custom_notice);
    res
}

/// Return the `{tag}` placeholder substituted in sanitize boundary notices.
#[wasm_bindgen(js_name = "sanitizeNoticeTagPlaceholder")]
#[must_use]
pub fn sanitize_notice_tag_placeholder() -> String {
    md_tmpl::SANITIZE_NOTICE_TAG_PLACEHOLDER.to_string()
}

/// Return the canonical `tool_output_quarantine` tag name.
#[wasm_bindgen(js_name = "toolOutputQuarantineTag")]
#[must_use]
pub fn tool_output_quarantine_tag() -> String {
    md_tmpl::TOOL_OUTPUT_QUARANTINE_TAG.to_string()
}

/// Return the canonical `<tool_output_quarantine>` opening tag.
#[wasm_bindgen(js_name = "toolOutputQuarantineOpenTag")]
#[must_use]
pub fn tool_output_quarantine_open_tag() -> String {
    md_tmpl::TOOL_OUTPUT_QUARANTINE_OPEN_TAG.to_string()
}

/// Return the canonical `</tool_output_quarantine>` closing tag.
#[wasm_bindgen(js_name = "toolOutputQuarantineCloseTag")]
#[must_use]
pub fn tool_output_quarantine_close_tag() -> String {
    md_tmpl::TOOL_OUTPUT_QUARANTINE_CLOSE_TAG.to_string()
}

/// Return the canonical `untrusted_tool_output` tag name.
#[wasm_bindgen(js_name = "untrustedToolOutputTag")]
#[must_use]
pub fn untrusted_tool_output_tag() -> String {
    md_tmpl::UNTRUSTED_TOOL_OUTPUT_TAG.to_string()
}

/// Return the canonical `<untrusted_tool_output>` opening tag.
#[wasm_bindgen(js_name = "untrustedQuarantineOpenTag")]
#[must_use]
pub fn untrusted_quarantine_open_tag() -> String {
    md_tmpl::UNTRUSTED_QUARANTINE_OPEN_TAG.to_string()
}

/// Return the canonical `</untrusted_tool_output>` closing tag.
#[wasm_bindgen(js_name = "untrustedQuarantineCloseTag")]
#[must_use]
pub fn untrusted_quarantine_close_tag() -> String {
    md_tmpl::UNTRUSTED_QUARANTINE_CLOSE_TAG.to_string()
}

/// Return `XML_DECODE_PAIRS` as a JS array of `[entity, decoded]` pairs.
#[wasm_bindgen(js_name = "xmlDecodePairs")]
#[must_use]
pub fn xml_decode_pairs() -> Array {
    delimiters_to_js(md_tmpl::XML_DECODE_PAIRS)
}

/// Decode the standard XML entity references in a single pass.
#[wasm_bindgen(js_name = "decodeXmlString")]
#[must_use]
pub fn decode_xml_string(s: &str) -> String {
    md_tmpl::decode_xml_str(s).into_owned()
}

/// Escape a string for an XML attribute value.
#[wasm_bindgen(js_name = "escapeXmlAttrString")]
#[must_use]
pub fn escape_xml_attr_string(s: &str) -> String {
    md_tmpl::escape_xml_attr_str(s).into_owned()
}

/// Escape an XML element body value (`&`, `<`, `>`, and NUL).
#[wasm_bindgen(js_name = "escapeXmlBodyString")]
#[must_use]
pub fn escape_xml_body_string(s: &str) -> String {
    md_tmpl::escape_xml_body_str(s).into_owned()
}

/// Format the boundary notice for `primary_tag` (`custom_notice` defaults to the standard notice).
#[wasm_bindgen(js_name = "formatSanitizeNotice")]
#[must_use]
pub fn format_sanitize_notice(primary_tag: &str, custom_notice: Option<String>) -> String {
    let res = md_tmpl::format_sanitize_notice(primary_tag, custom_notice.as_deref()).into_owned();
    drop(custom_notice);
    res
}

/// Strip outer `<tag_name>` / `</tag_name>` tags (and one envelope newline per side), or `None`.
#[wasm_bindgen(js_name = "stripOuterQuarantineTag")]
#[must_use]
pub fn strip_outer_quarantine_tag(s: &str, tag_name: &str) -> Option<String> {
    md_tmpl::strip_outer_quarantine_tag(s, tag_name).map(str::to_owned)
}

/// Extract the inner payload of a verified quarantine envelope, or `None` if invalid.
#[wasm_bindgen(js_name = "unquarantineString")]
#[must_use]
pub fn unquarantine_string(s: &str, tag_spec: Option<String>) -> Option<String> {
    let res = md_tmpl::unquarantine_str(s, tag_spec.as_deref()).map(str::to_owned);
    drop(tag_spec);
    res
}

/// Idempotently sanitize and wrap `s` in a quarantine envelope.
///
/// # Errors
///
/// Returns a JS `Error` (`kind = "syntax"`) if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
#[wasm_bindgen(js_name = "quarantineUntrustedIdempotentString")]
pub fn quarantine_untrusted_idempotent_string(
    s: &str,
    tag_spec: Option<String>,
) -> Result<String, JsValue> {
    let res = md_tmpl::quarantine_untrusted_idempotent_str(s, tag_spec.as_deref())
        .map_err(|e| js_error(&e));
    drop(tag_spec);
    res
}

/// Idempotently sanitize and wrap `s` in a `sanitizeBlockString` envelope.
///
/// # Errors
///
/// Returns a JS `Error` (`kind = "syntax"`) if any tag name in `tag_spec` is not a valid ASCII XML `NCName`.
#[wasm_bindgen(js_name = "sanitizeBlockIdempotentString")]
pub fn sanitize_block_idempotent_string(
    s: &str,
    tag_spec: Option<String>,
    custom_notice: Option<String>,
) -> Result<String, JsValue> {
    let res =
        md_tmpl::sanitize_block_idempotent_str(s, tag_spec.as_deref(), custom_notice.as_deref())
            .map_err(|e| js_error(&e));
    drop(tag_spec);
    drop(custom_notice);
    res
}

/// Truncate `s` in the middle, keeping at most `limit` UTF-8 bytes (`marker` may use `{skipped}`).
#[wasm_bindgen(js_name = "truncateMiddleString")]
#[must_use]
pub fn truncate_middle_string(s: &str, limit: usize, marker: Option<String>) -> String {
    let res = md_tmpl::truncate_middle_str(s, limit, marker.as_deref()).into_owned();
    drop(marker);
    res
}

/// Truncate a JS array in the middle, keeping at most `limit` items around one marker item.
///
/// # Errors
///
/// Returns a JS `Error` if an item cannot be converted to a template value.
#[wasm_bindgen(js_name = "truncateMiddleList")]
pub fn truncate_middle_list(
    items: &Array,
    limit: usize,
    marker: Option<String>,
) -> Result<Array, JsValue> {
    let values = items
        .iter()
        .map(|item| crate::js_to_env_value(&item))
        .collect::<Result<Vec<_>, _>>()?;
    let truncated = md_tmpl::truncate_middle_list(&values, limit, marker.as_deref());
    drop(marker);
    let out = Array::new_with_length(u32_from_usize(truncated.len()));
    for (i, value) in truncated.iter().enumerate() {
        out.set(u32_from_usize(i), crate::value_to_js(value));
    }
    Ok(out)
}
