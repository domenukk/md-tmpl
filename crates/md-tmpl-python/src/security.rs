//! Python bindings for `md-tmpl` security, escaping, token-sanitization, and quarantine primitives.

use pyo3::{prelude::*, types::PyList};

use crate::{
    convert::{py_to_value, value_to_py},
    errors::template_error_to_py,
};

/// String constants exported verbatim from the Rust core (`name`, `value`).
const STRING_CONSTANTS: &[(&str, &str)] = &[
    ("DEFAULT_QUARANTINE_TAG", md_tmpl::DEFAULT_QUARANTINE_TAG),
    ("DEFAULT_SANITIZE_TAG", md_tmpl::DEFAULT_SANITIZE_TAG),
    ("DEFAULT_SANITIZE_NOTICE", md_tmpl::DEFAULT_SANITIZE_NOTICE),
    (
        "SANITIZE_NOTICE_TAG_PLACEHOLDER",
        md_tmpl::SANITIZE_NOTICE_TAG_PLACEHOLDER,
    ),
    (
        "TOOL_OUTPUT_QUARANTINE_TAG",
        md_tmpl::TOOL_OUTPUT_QUARANTINE_TAG,
    ),
    (
        "TOOL_OUTPUT_QUARANTINE_OPEN_TAG",
        md_tmpl::TOOL_OUTPUT_QUARANTINE_OPEN_TAG,
    ),
    (
        "TOOL_OUTPUT_QUARANTINE_CLOSE_TAG",
        md_tmpl::TOOL_OUTPUT_QUARANTINE_CLOSE_TAG,
    ),
    (
        "UNTRUSTED_TOOL_OUTPUT_TAG",
        md_tmpl::UNTRUSTED_TOOL_OUTPUT_TAG,
    ),
    (
        "UNTRUSTED_QUARANTINE_OPEN_TAG",
        md_tmpl::UNTRUSTED_QUARANTINE_OPEN_TAG,
    ),
    (
        "UNTRUSTED_QUARANTINE_CLOSE_TAG",
        md_tmpl::UNTRUSTED_QUARANTINE_CLOSE_TAG,
    ),
    ("DEFAULT_TRUNCATE_MARKER", md_tmpl::DEFAULT_TRUNCATE_MARKER),
    (
        "DEFAULT_TRUNCATE_MARKER_COMPACT",
        md_tmpl::DEFAULT_TRUNCATE_MARKER_COMPACT,
    ),
    (
        "DEFAULT_LIST_TRUNCATE_MARKER",
        md_tmpl::DEFAULT_LIST_TRUNCATE_MARKER,
    ),
];

/// Escape XML/HTML special characters and strip illegal XML 1.0 control characters.
#[pyfunction]
fn escape_xml(s: &str) -> String {
    md_tmpl::escape_xml_str(s).into_owned()
}

/// Escape JSON string body characters (including `/` and `U+2028`/`U+2029`).
#[pyfunction]
fn escape_json(s: &str) -> String {
    md_tmpl::escape_json_str(s).into_owned()
}

/// Return `True` if `s` contains any known LLM control token or `<|...|>` / `<｜...｜>` special token.
#[pyfunction]
fn has_control_tokens(s: &str) -> bool {
    md_tmpl::has_control_tokens(s)
}

/// Return `True` if `s` contains any role/turn control token in `ROLE_TOKEN_DELIMITERS` or generic pipe token.
#[pyfunction]
fn has_role_control_tokens(s: &str) -> bool {
    md_tmpl::has_role_control_tokens(s)
}

/// Neutralize all known LLM control tokens and `<|...|>` / `<｜...｜>` special tokens in a single pass.
#[pyfunction]
fn sanitize_tokens(s: &str) -> String {
    md_tmpl::sanitize_tokens_str(s).into_owned()
}

/// Neutralize role/turn headers (`ROLE_TOKEN_DELIMITERS`) and generic pipe tokens in a single pass while preserving XML tool docs.
#[pyfunction]
fn sanitize_role_tokens(s: &str) -> String {
    md_tmpl::sanitize_role_tokens_str(s).into_owned()
}

/// Wrap `s` in Markdown code fences with adaptive backtick counts.
#[pyfunction]
#[pyo3(signature = (s, lang = None))]
fn fence(s: &str, lang: Option<&str>) -> PyResult<String> {
    md_tmpl::fence_str(s, lang).map_err(|e| template_error_to_py(&e))
}

/// Return `True` if `s` contains any opening or closing XML tag matching `tag_spec`.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = md_tmpl::DEFAULT_QUARANTINE_TAG))]
fn has_quarantine_tag_breakout(s: &str, tag_spec: &str) -> bool {
    md_tmpl::has_quarantine_tag_breakout(s, tag_spec)
}

/// Return `True` if `s` contains any LLM control token, generic pipe token, or XML tag matching `tag_spec` in a single pass.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = md_tmpl::DEFAULT_QUARANTINE_TAG))]
fn has_untrusted_breakout(s: &str, tag_spec: &str) -> bool {
    md_tmpl::has_untrusted_breakout(s, tag_spec)
}

/// Sanitize untrusted content within quarantine boundaries for one or more comma-separated XML tag names.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = md_tmpl::DEFAULT_QUARANTINE_TAG))]
fn sanitize_quarantine_payload(s: &str, tag_spec: &str) -> String {
    md_tmpl::sanitize_quarantine_payload(s, tag_spec).into_owned()
}

/// Neutralize all known LLM control tokens, generic pipe tokens, and opening/closing XML boundary tags matching `tag_spec` in a single pass.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = md_tmpl::DEFAULT_QUARANTINE_TAG))]
fn sanitize_untrusted(s: &str, tag_spec: &str) -> String {
    md_tmpl::sanitize_untrusted_str(s, tag_spec).into_owned()
}

/// Wrap `s` in boundary XML tags (`<primary_tag>\n...\n</primary_tag>`), escaping any embedded tags matching `tag_spec`.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None))]
fn quarantine(s: &str, tag_spec: Option<&str>) -> PyResult<String> {
    md_tmpl::quarantine_str(s, tag_spec).map_err(|e| template_error_to_py(&e))
}

/// Sanitize both control tokens and boundary XML tags in a single pass and wrap in `<primary_tag>\n...\n</primary_tag>`.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None))]
fn quarantine_untrusted(s: &str, tag_spec: Option<&str>) -> PyResult<String> {
    md_tmpl::quarantine_untrusted_str(s, tag_spec).map_err(|e| template_error_to_py(&e))
}

/// Return `True` if `s` is wrapped in `<primary_tag>...</primary_tag>` with zero unescaped boundary tags or control tokens.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None))]
fn is_quarantined(s: &str, tag_spec: Option<&str>) -> bool {
    md_tmpl::is_quarantined_str(s, tag_spec)
}

/// Neutralize all known LLM control tokens, generic pipe tokens, and optional XML boundary tags (`tag_spec`).
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None))]
fn sanitize(s: &str, tag_spec: Option<&str>) -> PyResult<String> {
    md_tmpl::sanitize_str(s, tag_spec)
        .map(std::borrow::Cow::into_owned)
        .map_err(|e| template_error_to_py(&e))
}

/// Sanitize control tokens and boundary XML tags in a single pass and wrap `s` in
/// `<primary_tag>\n{notice}\n{sanitized}\n</primary_tag>`.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None, custom_notice = None))]
fn sanitize_block(
    s: &str,
    tag_spec: Option<&str>,
    custom_notice: Option<&str>,
) -> PyResult<String> {
    md_tmpl::sanitize_block_str(s, tag_spec, custom_notice).map_err(|e| template_error_to_py(&e))
}

/// Return `True` if `s` is wrapped in `<primary_tag>\n{notice}\n...\n</primary_tag>` with zero unescaped boundary tags or control tokens.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None, custom_notice = None))]
fn is_sanitized_block(s: &str, tag_spec: Option<&str>, custom_notice: Option<&str>) -> bool {
    md_tmpl::is_sanitized_block_str(s, tag_spec, custom_notice)
}

/// Extract the inner sanitized payload from a verified `sanitize_block` envelope, or `None` if invalid.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None, custom_notice = None))]
fn unsanitize_block(
    s: &str,
    tag_spec: Option<&str>,
    custom_notice: Option<&str>,
) -> Option<String> {
    md_tmpl::unsanitize_block_str(s, tag_spec, custom_notice).map(str::to_owned)
}

/// Format the untrusted-data boundary notice for `primary_tag` (`{tag}` is substituted).
#[pyfunction]
#[pyo3(signature = (primary_tag, custom_notice = None))]
fn format_sanitize_notice(primary_tag: &str, custom_notice: Option<&str>) -> String {
    md_tmpl::format_sanitize_notice(primary_tag, custom_notice).into_owned()
}

/// Decode the standard XML entity references (`&quot;`, `&#39;`, `&apos;`, `&lt;`, `&gt;`, `&amp;`).
#[pyfunction]
fn decode_xml(s: &str) -> String {
    md_tmpl::decode_xml_str(s).into_owned()
}

/// Escape a string for an XML attribute value.
#[pyfunction]
fn escape_xml_attr(s: &str) -> String {
    md_tmpl::escape_xml_attr_str(s).into_owned()
}

/// Escape an XML element body value (`&`, `<`, `>`, and NUL).
#[pyfunction]
fn escape_xml_body(s: &str) -> String {
    md_tmpl::escape_xml_body_str(s).into_owned()
}

/// Strip outer `<tag_name>` / `</tag_name>` tags (and one envelope newline on each side), or `None`.
#[pyfunction]
fn strip_outer_quarantine_tag(s: &str, tag_name: &str) -> Option<String> {
    md_tmpl::strip_outer_quarantine_tag(s, tag_name).map(str::to_owned)
}

/// Extract the inner payload of a verified quarantine envelope, or `None` if invalid.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None))]
fn unquarantine(s: &str, tag_spec: Option<&str>) -> Option<String> {
    md_tmpl::unquarantine_str(s, tag_spec).map(str::to_owned)
}

/// Idempotently sanitize and wrap `s` in a quarantine envelope.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None))]
fn quarantine_untrusted_idempotent(s: &str, tag_spec: Option<&str>) -> PyResult<String> {
    md_tmpl::quarantine_untrusted_idempotent_str(s, tag_spec).map_err(|e| template_error_to_py(&e))
}

/// Idempotently sanitize and wrap `s` in a `sanitize_block` envelope.
#[pyfunction]
#[pyo3(signature = (s, tag_spec = None, custom_notice = None))]
fn sanitize_block_idempotent(
    s: &str,
    tag_spec: Option<&str>,
    custom_notice: Option<&str>,
) -> PyResult<String> {
    md_tmpl::sanitize_block_idempotent_str(s, tag_spec, custom_notice)
        .map_err(|e| template_error_to_py(&e))
}

/// Truncate `s` in the middle, keeping at most `limit` UTF-8 bytes (`marker` may use `{skipped}`).
#[pyfunction]
#[pyo3(signature = (s, limit, marker = None))]
fn truncate_middle(s: &str, limit: usize, marker: Option<&str>) -> String {
    md_tmpl::truncate_middle_str(s, limit, marker).into_owned()
}

/// Truncate `items` in the middle, keeping at most `limit` items around one marker item.
#[pyfunction]
#[pyo3(signature = (items, limit, marker = None))]
fn truncate_middle_list(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    limit: usize,
    marker: Option<&str>,
) -> PyResult<Vec<Py<PyAny>>> {
    let values = items
        .iter()
        .map(|item| py_to_value(&item))
        .collect::<PyResult<Vec<_>>>()?;
    md_tmpl::truncate_middle_list(&values, limit, marker)
        .iter()
        .map(|value| value_to_py(py, value))
        .collect()
}

/// Register security constants and functions on the `_md_tmpl` Python module.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    for &(name, value) in STRING_CONSTANTS {
        m.add(name, value)?;
    }
    m.add(
        "XML_DECODE_PAIRS",
        PyList::new(py, md_tmpl::XML_DECODE_PAIRS)?,
    )?;
    let token_delimiters = PyList::new(py, md_tmpl::TOKEN_DELIMITERS)?;
    m.add("TOKEN_DELIMITERS", token_delimiters)?;
    let role_token_delimiters = PyList::new(py, md_tmpl::ROLE_TOKEN_DELIMITERS)?;
    m.add("ROLE_TOKEN_DELIMITERS", role_token_delimiters)?;

    m.add_function(wrap_pyfunction!(escape_xml, m)?)?;
    m.add_function(wrap_pyfunction!(escape_json, m)?)?;
    m.add_function(wrap_pyfunction!(has_control_tokens, m)?)?;
    m.add_function(wrap_pyfunction!(has_role_control_tokens, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_tokens, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_role_tokens, m)?)?;
    m.add_function(wrap_pyfunction!(fence, m)?)?;
    m.add_function(wrap_pyfunction!(has_quarantine_tag_breakout, m)?)?;
    m.add_function(wrap_pyfunction!(has_untrusted_breakout, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_quarantine_payload, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_untrusted, m)?)?;
    m.add_function(wrap_pyfunction!(quarantine, m)?)?;
    m.add_function(wrap_pyfunction!(quarantine_untrusted, m)?)?;
    m.add_function(wrap_pyfunction!(is_quarantined, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_block, m)?)?;
    m.add_function(wrap_pyfunction!(is_sanitized_block, m)?)?;
    m.add_function(wrap_pyfunction!(unsanitize_block, m)?)?;
    m.add_function(wrap_pyfunction!(format_sanitize_notice, m)?)?;
    m.add_function(wrap_pyfunction!(decode_xml, m)?)?;
    m.add_function(wrap_pyfunction!(escape_xml_attr, m)?)?;
    m.add_function(wrap_pyfunction!(escape_xml_body, m)?)?;
    m.add_function(wrap_pyfunction!(strip_outer_quarantine_tag, m)?)?;
    m.add_function(wrap_pyfunction!(unquarantine, m)?)?;
    m.add_function(wrap_pyfunction!(quarantine_untrusted_idempotent, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_block_idempotent, m)?)?;
    m.add_function(wrap_pyfunction!(truncate_middle, m)?)?;
    m.add_function(wrap_pyfunction!(truncate_middle_list, m)?)?;
    Ok(())
}
