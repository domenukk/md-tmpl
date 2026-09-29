//! Python bindings for `md-tmpl` security, escaping, token-sanitization, and quarantine primitives.

use pyo3::{prelude::*, types::PyList};

use crate::errors::template_error_to_py;

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

/// Register security constants and functions on the `_md_tmpl` Python module.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add("DEFAULT_QUARANTINE_TAG", md_tmpl::DEFAULT_QUARANTINE_TAG)?;
    m.add("DEFAULT_SANITIZE_TAG", md_tmpl::DEFAULT_SANITIZE_TAG)?;
    m.add("DEFAULT_SANITIZE_NOTICE", md_tmpl::DEFAULT_SANITIZE_NOTICE)?;
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
    Ok(())
}
