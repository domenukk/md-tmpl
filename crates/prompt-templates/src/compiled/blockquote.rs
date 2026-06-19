//! Blockquote prefix stripping for template statement tags.
//!
//! Markdown blockquote `>` prefixes on `{% ... %}` lines are
//! transparently stripped before compilation so the template engine
//! sees plain tags.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

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

/// Returns `true` when a line is a valid neighbor for a standalone tag line.
///
/// Valid neighbors are:
/// - Empty / blank lines
/// - `---` (frontmatter delimiter)
/// - Other blockquote tag lines (`> {% ... %}`)
///
/// Content lines starting with `>` that do NOT contain `{% %}` are NOT valid.
fn is_valid_tag_neighbor(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with("---") {
        return true;
    }
    // A `>` line is only valid if it's itself a blockquote tag line.
    if trimmed.starts_with('>') {
        let stripped = strip_blockquote_line(line);
        let was_stripped = !core::ptr::eq(stripped, line);
        return was_stripped && stripped.trim_start().starts_with(STMT_START);
    }
    false
}

/// Validate that every line starting with `{% ` has a blockquote `>` prefix,
/// and that standalone tag lines are surrounded by blank lines or other tags.
///
/// This check runs on the raw body **before** blockquote stripping. Lines
/// inside `{% raw %}` blocks are exempted since their content is literal.
///
/// Only lines whose first non-whitespace characters are `{%` are checked —
/// `{{ }}` expressions and mid-line `{% %}` tags are always allowed without
/// a `>` prefix.
pub(super) fn validate_blockquote_prefix(input: &str) -> Result<(), TemplateError> {
    let mut in_raw = false;
    let lines: Vec<&str> = input.lines().collect();
    for (i, &line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();

        if in_raw {
            // Inside raw block — look for close tag (with or without `>`)
            // to resume checking. Raw blocks don't nest.
            if trimmed.contains("{%") && (trimmed.contains("/raw") || trimmed.contains("- /raw")) {
                in_raw = false;
                let stripped = strip_blockquote_line(line);
                let was_stripped = !core::ptr::eq(stripped, line);
                if was_stripped && is_standalone_tag(stripped) {
                    validate_tag_neighbors(&lines, i, line)?;
                }
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

        let stripped = strip_blockquote_line(line);
        let was_stripped = !core::ptr::eq(stripped, line);
        if was_stripped && is_standalone_tag(stripped) {
            validate_tag_neighbors(&lines, i, line)?;
        }
    }
    Ok(())
}

/// Check that a standalone tag at index `i` is surrounded by valid neighbors.
fn validate_tag_neighbors(lines: &[&str], i: usize, line: &str) -> Result<(), TemplateError> {
    if i > 0 {
        if let Some(&prev_line) = lines.get(i - 1) {
            if !is_valid_tag_neighbor(prev_line) {
                return Err(TemplateError::syntax(format!(
                    "Standalone statement tag '{}' must be preceded by a blank line or another blockquote tag line (> {{%...%}})",
                    line.trim()
                )));
            }
        }
    }

    if let Some(&next_line) = lines.get(i + 1) {
        if !is_valid_tag_neighbor(next_line) {
            return Err(TemplateError::syntax(format!(
                "Standalone statement tag '{}' must be followed by a blank line or another blockquote tag line (> {{%...%}})",
                line.trim()
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
pub(super) fn strip_blockquote_tags(input: &str) -> alloc::borrow::Cow<'_, str> {
    // Fast path: no blockquote tags present.
    if !input.contains(BLOCKQUOTE_COMPACT_OPEN) && !input.contains(BLOCKQUOTE_SPACED_OPEN) {
        return alloc::borrow::Cow::Borrowed(input);
    }

    let lines: Vec<&str> = input.split('\n').collect();
    let mut result = String::with_capacity(input.len());
    let mut skip_next_newline = false;
    for (i, &line) in lines.iter().enumerate() {
        let stripped = strip_blockquote_line(line);
        let was_stripped = !core::ptr::eq(stripped, line);
        if skip_next_newline && stripped.trim().is_empty() {
            continue;
        }
        if i > 0 && !skip_next_newline {
            result.push('\n');
        }
        skip_next_newline = false;
        // When a blockquote-stripped line is a standalone tag (only `{% … %}`),
        // consume the trailing newline so it doesn't leak into the block body.
        // Also pop the preceding blank line if present, matching TypeScript parity.
        if was_stripped && is_standalone_tag(stripped) {
            if result.ends_with("\n\n") || result == "\n" {
                result.pop();
            }
            skip_next_newline = true;
        }
        result.push_str(stripped);
    }
    alloc::borrow::Cow::Owned(result)
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
    let trimmed = line.trim_start();
    // Try `> {% ...` (with space after >).
    if let Some(rest) = trimmed.strip_prefix(BLOCKQUOTE_PREFIX_SPACED)
        && rest.trim_start().starts_with(STMT_START)
    {
        return rest;
    }
    // Try `>{% ...` (no space).
    if let Some(rest) = trimmed.strip_prefix(BLOCKQUOTE_PREFIX)
        && rest.trim_start().starts_with(STMT_START)
    {
        return rest;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_indented_blockquote_tags() {
        let input = r"  > {% for task in tasks %}
- **{{ task.title }}**
  > {% /for %}";
        let expected = r"{% for task in tasks %}- **{{ task.title }}**
{% /for %}";
        assert_eq!(strip_blockquote_tags(input).as_ref(), expected);
    }

    #[test]
    fn strip_preserve_literal_blockquote_between_tags() {
        let input = "> {% if condition %}\n\n> This is a literal blockquote inside the block.\n\n> {% /if %}";
        let expected =
            "{% if condition %}> This is a literal blockquote inside the block.\n{% /if %}";
        assert_eq!(strip_blockquote_tags(input).as_ref(), expected);
    }

    #[test]
    fn validate_rejects_indented_bare_tag() {
        let input = r"Some prose
  {% for task in tasks %}
- {{ task.title }}
  {% /for %}";
        let err = validate_blockquote_prefix(input).unwrap_err();
        assert!(err.to_string().contains(ERR_BARE_STMT_TAG));
    }

    #[test]
    fn validate_rejects_content_with_blockquote_prefix() {
        // Content line starting with `>` but no `{% %}` — NOT a valid tag neighbor
        let input = "> {% if empty %}\n> _No items._\n> {% /if %}";
        let err = validate_blockquote_prefix(input).unwrap_err();
        assert!(
            err.to_string().contains("must be followed by a blank line"),
            "got: {err}"
        );
    }

    #[test]
    fn validate_accepts_blank_lines_around_tags() {
        let input = "\n> {% if show %}\n\nContent here.\n\n> {% /if %}\n";
        assert!(validate_blockquote_prefix(input).is_ok());
    }

    #[test]
    fn validate_accepts_consecutive_tags() {
        let input = "\n> {% if x %}\n> {% for item in items %}\n\n{{ item }}\n\n> {% /for %}\n> {% /if %}\n";
        assert!(validate_blockquote_prefix(input).is_ok());
    }

    #[test]
    fn validate_rejects_content_directly_after_tag() {
        let input = "\n> {% if show %}\nContent without blank line.\n\n> {% /if %}\n";
        let err = validate_blockquote_prefix(input).unwrap_err();
        assert!(
            err.to_string().contains("must be followed by a blank line"),
            "got: {err}"
        );
    }

    #[test]
    fn validate_rejects_content_directly_before_tag() {
        let input = "\n> {% if show %}\n\nContent without blank line.\n> {% /if %}\n";
        let err = validate_blockquote_prefix(input).unwrap_err();
        assert!(
            err.to_string().contains("must be preceded by a blank line"),
            "got: {err}"
        );
    }

    #[test]
    fn is_valid_tag_neighbor_empty() {
        assert!(is_valid_tag_neighbor(""));
        assert!(is_valid_tag_neighbor("   "));
    }

    #[test]
    fn is_valid_tag_neighbor_frontmatter() {
        assert!(is_valid_tag_neighbor("---"));
    }

    #[test]
    fn is_valid_tag_neighbor_blockquote_tag() {
        assert!(is_valid_tag_neighbor("> {% if x %}"));
        assert!(is_valid_tag_neighbor("> {% /for %}"));
    }

    #[test]
    fn is_valid_tag_neighbor_rejects_blockquote_content() {
        assert!(!is_valid_tag_neighbor("> some content"));
        assert!(!is_valid_tag_neighbor("> _No items._"));
    }

    #[test]
    fn is_valid_tag_neighbor_rejects_plain_content() {
        assert!(!is_valid_tag_neighbor("some content"));
        assert!(!is_valid_tag_neighbor("- list item"));
    }
}
