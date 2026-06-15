//! Blockquote prefix stripping for template statement tags.
//!
//! Markdown blockquote `>` prefixes on `{% ... %}` lines are
//! transparently stripped before compilation so the template engine
//! sees plain tags.

use crate::{
    consts::{
        BLOCKQUOTE_COMPACT_OPEN, BLOCKQUOTE_PREFIX, BLOCKQUOTE_PREFIX_SPACED,
        BLOCKQUOTE_SPACED_OPEN, ERR_BARE_STMT_TAG, STMT_END, STMT_START,
    },
    error::TemplateError,
};

/// Maximum display length for error-message snippets of blockquote lines.
const SNIPPET_MAX_DISPLAY_LEN: usize = 60;
/// Safe truncation boundary: leaves room for the trailing `…` (3 UTF-8 bytes).
const SNIPPET_TRUNCATION_BOUNDARY: usize = SNIPPET_MAX_DISPLAY_LEN - 3;

/// Validate that every line starting with `{% ` has a blockquote `>` prefix.
///
/// This check runs on the raw body **before** blockquote stripping. Lines
/// inside `{% raw %}` blocks are exempted since their content is literal.
///
/// Only lines whose first non-whitespace characters are `{%` are checked —
/// `{{ }}` expressions and mid-line `{% %}` tags are always allowed without
/// a `>` prefix.
pub(super) fn validate_blockquote_prefix(input: &str) -> Result<(), TemplateError> {
    let mut in_raw = false;
    for line in input.lines() {
        let trimmed = line.trim_start();

        if in_raw {
            // Inside raw block — look for close tag (with or without `>`)
            // to resume checking. Raw blocks don't nest.
            if trimmed.contains("{%") && (trimmed.contains("/raw") || trimmed.contains("- /raw")) {
                in_raw = false;
            }
            continue;
        }

        // Detect raw block open: `> {% raw %}` or `> {% raw=X %}`
        if trimmed.starts_with('>')
            && trimmed.contains("{%")
            && (trimmed.contains(" raw ") || trimmed.contains(" raw=") || trimmed.contains(" raw%"))
        {
            in_raw = true;
            continue;
        }

        // Main check: line starts with `{%` (or `{%-`) without `>` prefix.
        if trimmed.starts_with(STMT_START) {
            // Truncate for a clean error message.
            let snippet = if trimmed.len() > SNIPPET_MAX_DISPLAY_LEN {
                // Find a safe truncation point at a char boundary.
                let end = trimmed
                    .char_indices()
                    .map(|(i, _)| i)
                    .take_while(|&i| i <= SNIPPET_TRUNCATION_BOUNDARY)
                    .last()
                    .unwrap_or(0);
                format!("{}…", &trimmed[..end])
            } else {
                trimmed.to_string()
            };
            return Err(TemplateError::syntax(format!(
                "{ERR_BARE_STMT_TAG}: write '> {snippet}' instead of '{snippet}'"
            )));
        }
    }
    Ok(())
}

/// Strip markdown blockquote `>` prefix from lines containing `{%` tags.
///
/// Allows authors to write `>{% if x %}` which renders as a visually-distinct
/// blockquote in markdown preview. The `>` prefix is transparently removed
/// before compilation so the template engine sees plain `{% if x %}`.
///
/// Supports both `>{%` (compact) and `> {%` (spaced). Lines without `{%`
/// are left untouched, preserving actual markdown blockquotes.
///
/// This function is idempotent — calling it on already-processed text is safe.
pub(super) fn strip_blockquote_tags(input: &str) -> std::borrow::Cow<'_, str> {
    // Fast path: no blockquote tags present.
    if !input.contains(BLOCKQUOTE_COMPACT_OPEN) && !input.contains(BLOCKQUOTE_SPACED_OPEN) {
        return std::borrow::Cow::Borrowed(input);
    }

    let lines: Vec<&str> = input.split('\n').collect();
    let mut result = String::with_capacity(input.len());
    let mut skip_next_newline = false;
    for (i, &line) in lines.iter().enumerate() {
        let stripped = strip_blockquote_line(line);
        let was_stripped = !std::ptr::eq(stripped, line);
        if i > 0 && !skip_next_newline {
            result.push('\n');
        }
        skip_next_newline = false;
        result.push_str(stripped);
        // When a blockquote-stripped line is a standalone tag (only `{% … %}`),
        // consume the trailing newline so it doesn't leak into the block body.
        if was_stripped && is_standalone_tag(stripped) {
            skip_next_newline = true;
        }
    }
    std::borrow::Cow::Owned(result)
}

/// Returns `true` when the line is a standalone template tag — the entire
/// line (after trimming) is a single `{% ... %}` with no other content.
///
/// Lines like `{% if x %}yes{% /if %}` are NOT standalone because they
/// contain content between/around the tags.
pub(super) fn is_standalone_tag(line: &str) -> bool {
    let trimmed = line.trim();
    // Must start with `{%` and end with `%}`.
    if !trimmed.starts_with(STMT_START) || !trimmed.ends_with(STMT_END) {
        return false;
    }
    // Find the FIRST `%}` — if it's the last one (at the end), the line
    // is a single tag. If there's content after the first `%}`, it's not.
    let after_open = &trimmed[STMT_START.len()..]; // skip `{%`
    let Some(close_pos) = after_open.find(STMT_END) else {
        return false;
    };
    // The close should be at the end of the trimmed line.
    close_pos + STMT_END.len() == after_open.len()
}

/// Strip a leading `>` or `> ` from a single line if the remainder starts
/// with `{%` (optionally after whitespace).
fn strip_blockquote_line(line: &str) -> &str {
    // Try `> {% ...` (with space after >).
    if let Some(rest) = line.strip_prefix(BLOCKQUOTE_PREFIX_SPACED)
        && rest.trim_start().starts_with(STMT_START)
    {
        return rest;
    }
    // Try `>{% ...` (no space).
    if let Some(rest) = line.strip_prefix(BLOCKQUOTE_PREFIX)
        && rest.trim_start().starts_with(STMT_START)
    {
        return rest;
    }
    line
}
