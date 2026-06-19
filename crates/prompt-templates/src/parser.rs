//! Template tag and expression parsing.
//!
//! Provides scanning for `{{ expr }}` and `{% stmt %}` tags, closing-block
//! detection with nesting support, else-block splitting, and expression
//! evaluation with filter pipelines.

use alloc::{string::ToString, vec::Vec};

use crate::{
    Value,
    consts::{
        CLOSE_IF, COMMENT_END, COMMENT_START, EXPR_END, EXPR_START, KW_ELSE, KW_IN_SPACED,
        STMT_END, STMT_START, TAG_ELIF_PREFIX, TAG_FOR_PREFIX, TAG_IF_PREFIX, TAG_INCLUDE_PREFIX,
        TAG_WITH_PREFIX, TAG_WITH_SPACED, TRIM_MARKER,
    },
    error::TemplateError,
    scope::Scope,
};

/// The types of tags in our templates.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Tag<'a> {
    /// `{{ expr }}` — expression/substitution.
    Expr(&'a str),
    /// `{% stmt %}` — statement (for, /for, if, else, /if, raw, /raw, include).
    Stmt(&'a str),
    /// `{# comment #}` — stripped from output, but scanned for variable refs.
    Comment(&'a str),
}

/// Result of scanning for the next tag in template text.
#[derive(Debug)]
pub(crate) enum ScanResult<'a> {
    /// No more tags — the remaining text is all literal.
    Literal(&'a str),
    /// Found a tag: text before, the tag itself, and text after.
    Found {
        /// Text before the tag.
        before: &'a str,
        /// The parsed tag.
        tag: Tag<'a>,
        /// Text after the tag's closing delimiter.
        after: &'a str,
        /// `true` when the opening delimiter was `{{-` or `{%-`.
        trim_before: bool,
        /// `true` when the closing delimiter was `-}}` or `-%}`.
        trim_after: bool,
    },
}

/// Parsed include directive.
#[derive(Debug)]
pub(crate) struct IncludeDirective<'a> {
    /// Path to the included template.
    pub path: &'a str,
    /// `with key=expr, key2=expr2` overrides.
    pub with_vars: Vec<(&'a str, &'a str)>,
    /// Optional `for var in list` iteration.
    pub for_each: Option<(&'a str, &'a str)>,
}

/// Scan for the next `{{`, `{%`, or `{#` tag in the input.
///
/// Detects Jinja-style whitespace control delimiters:
/// - `{{-` / `-}}` for expressions
/// - `{%-` / `-%}` for statements
///
/// Comments (`{# ... #}`) are returned as `Tag::Comment` so the compiler
/// can extract variable references from them (for unused-variable analysis).
///
/// When `trim_before` is `true`, the caller should strip trailing
/// whitespace from `before`. When `trim_after` is `true`, the caller
/// should strip leading whitespace (up to and including the next newline)
/// from `after`.
pub(crate) fn scan_next_tag(input: &str) -> Result<ScanResult<'_>, TemplateError> {
    let expr_pos = input.find(EXPR_START);
    let stmt_pos = input.find(STMT_START);
    let comment_pos = input.find(COMMENT_START);

    // Find the earliest tag position.
    let first_pos = [expr_pos, stmt_pos, comment_pos]
        .into_iter()
        .flatten()
        .min();

    let Some(tag_start) = first_pos else {
        return Ok(ScanResult::Literal(input));
    };

    let before = &input[..tag_start];
    let tag_source = &input[tag_start..];

    if tag_source.starts_with(COMMENT_START) {
        // Comment tag: {# ... #}
        let inner_start = tag_start + COMMENT_START.len();
        let Some(close) = input[inner_start..].find(COMMENT_END) else {
            return Err(TemplateError::syntax(format!("unclosed `{COMMENT_START}`")));
        };
        let inner = &input[inner_start..inner_start + close];
        let after = &input[inner_start + close + COMMENT_END.len()..];
        Ok(ScanResult::Found {
            before,
            tag: Tag::Comment(inner),
            after,
            trim_before: false,
            trim_after: false,
        })
    } else if tag_source.starts_with(EXPR_START) {
        // Check for `{{-` (trim-left marker).
        let trim_left = input[tag_start + EXPR_START.len()..].starts_with(TRIM_MARKER);
        let inner_start = tag_start + EXPR_START.len() + usize::from(trim_left);

        let Some(close) = input[inner_start..].find(EXPR_END) else {
            return Err(TemplateError::syntax(format!("unclosed `{EXPR_START}`")));
        };

        // Check for `-}}` (trim-right marker).
        let raw_inner = &input[inner_start..inner_start + close];
        let trim_right = raw_inner.ends_with(TRIM_MARKER);
        let inner = if trim_right {
            raw_inner[..raw_inner.len() - 1].trim()
        } else {
            raw_inner.trim()
        };

        let after = &input[inner_start + close + EXPR_END.len()..];
        Ok(ScanResult::Found {
            before,
            tag: Tag::Expr(inner),
            after,
            trim_before: trim_left,
            trim_after: trim_right,
        })
    } else {
        // Check for `{%-` (trim-left marker).
        let trim_left = input[tag_start + STMT_START.len()..].starts_with(TRIM_MARKER);
        let inner_start = tag_start + STMT_START.len() + usize::from(trim_left);

        let Some(close) = input[inner_start..].find(STMT_END) else {
            return Err(TemplateError::syntax(format!("unclosed `{STMT_START}`")));
        };

        // Check for `-%}` (trim-right marker).
        let raw_inner = &input[inner_start..inner_start + close];
        let trim_right = raw_inner.ends_with(TRIM_MARKER);
        let inner = if trim_right {
            raw_inner[..raw_inner.len() - 1].trim()
        } else {
            raw_inner.trim()
        };

        let after = &input[inner_start + close + STMT_END.len()..];
        Ok(ScanResult::Found {
            before,
            tag: Tag::Stmt(inner),
            after,
            trim_before: trim_left,
            trim_after: trim_right,
        })
    }
}

/// Find the matching closing block tag, handling nesting.
///
/// Returns `(body, rest_after_close_tag)`. Strips a trailing newline from
/// the close tag if it's standalone on a line.
///
/// Recognises both plain (`{% /for %}`) and whitespace-control variants
/// (`{%- /for %}`, `{% /for -%}`, `{%- /for -%}`).
pub(crate) fn find_closing_block<'a>(
    input: &'a str,
    open_keyword: &str,
    close_keyword: &str,
) -> Result<(&'a str, &'a str), TemplateError> {
    let mut depth: u32 = 1;
    let mut search_from = 0;

    loop {
        let next_open =
            find_tag_start(&input[search_from..], open_keyword).map(|p| p + search_from);
        let next_close =
            find_block_close(&input[search_from..], close_keyword).map(|(p, _len)| p + search_from);

        let close_first = match (next_open, next_close) {
            (_, None) => {
                return Err(TemplateError::syntax(format!(
                    "unclosed '{{% {open_keyword}%}}' block"
                )));
            }
            (None, Some(_)) => true,
            (Some(o), Some(c)) => c < o,
        };

        if close_first {
            let close_pos = next_close.expect("checked above");
            let (_offset, close_len) =
                find_block_close(&input[close_pos..], close_keyword).expect("just found it");
            depth -= 1;
            if depth == 0 {
                let body = &input[..close_pos];
                let mut rest = &input[close_pos + close_len..];
                if rest.starts_with('\n') {
                    rest = &rest[1..];
                } else if rest.starts_with("\r\n") {
                    rest = &rest[2..];
                }
                return Ok((body, rest));
            }
            search_from = close_pos + close_len;
        } else {
            let open_pos = next_open.expect("checked above");
            depth += 1;
            // Skip past the `{%` (or `{%-`) prefix so we don't re-match.
            search_from = open_pos + STMT_START.len();
        }
    }
}

/// Pre-computed trim-close pattern: `-%}`.
///
/// This is `TRIM_MARKER` + `STMT_END` pre-concatenated so we avoid building
/// it via `format!()` on every call to `find_block_close`.
const TRIM_CLOSE: &str = "-%}";

/// Find the byte-offset of a `{% keyword...` or `{%- keyword...` tag
/// start inside `haystack`. Returns the position of the `{`.
fn find_tag_start(haystack: &str, keyword: &str) -> Option<usize> {
    // Build search needles by scanning for STMT_START then checking
    // whether the keyword follows, with or without a trim marker.
    // This avoids allocating format!() strings on every call.
    let mut pos = 0;
    while pos < haystack.len() {
        let offset = haystack[pos..].find(STMT_START)?;
        let abs = pos + offset;
        let after_open = abs + STMT_START.len();
        let rest = &haystack[after_open..];

        // Check `{% keyword` (plain) — exactly one space then keyword.
        if let Some(r) = rest.strip_prefix(' ')
            && r.starts_with(keyword)
        {
            return Some(abs);
        }

        // Check `{%- keyword` (trim) — dash, space, keyword.
        if let Some(r) = rest.strip_prefix(TRIM_MARKER)
            && let Some(r) = r.strip_prefix(' ')
            && r.starts_with(keyword)
        {
            return Some(abs);
        }

        pos = after_open;
    }
    None
}

/// Find a closing block tag like `{% /for %}` or any whitespace-control
/// variant. Returns `(offset, total_byte_length)` so the caller can skip
/// past the whole tag.
fn find_block_close(haystack: &str, close_keyword: &str) -> Option<(usize, usize)> {
    let mut pos = 0;
    while pos < haystack.len() {
        let start = haystack[pos..].find(STMT_START)?;
        let abs_start = pos + start;
        let after_open = abs_start + STMT_START.len();

        // Skip optional `-`.
        let after_dash = if haystack[after_open..].starts_with(TRIM_MARKER) {
            after_open + 1
        } else {
            after_open
        };

        // Skip whitespace.
        let content_start = after_dash
            + haystack[after_dash..]
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(haystack[after_dash..].len());

        if haystack[content_start..].starts_with(close_keyword) {
            let after_keyword = content_start + close_keyword.len();
            let rest = &haystack[after_keyword..];
            let trimmed = rest.trim_start();
            let close_end = if trimmed.starts_with(TRIM_CLOSE) {
                let ws_len = rest.len() - trimmed.len();
                after_keyword + ws_len + TRIM_CLOSE.len()
            } else if trimmed.starts_with(STMT_END) {
                let ws_len = rest.len() - trimmed.len();
                after_keyword + ws_len + STMT_END.len()
            } else {
                pos = abs_start + STMT_START.len();
                continue;
            };
            return Some((abs_start, close_end - abs_start));
        }
        pos = abs_start + STMT_START.len();
    }
    None
}

/// Split an if-block body into `(condition, body)` branches for
/// `{% if %}` / `{% elif %}` / `{% else %}` chains.
///
/// Returns a list of `(condition_str, branch_body)` pairs plus an
/// optional else body. The first entry's condition string is empty
/// (since the real condition comes from the opening `{% if %}` tag).
pub(crate) fn split_elif_chain(body: &str) -> (Vec<(&str, &str)>, Option<&str>) {
    let mut branches: Vec<(&str, &str)> = Vec::new();
    let mut depth: u32 = 0;
    let mut branch_start: usize = 0;
    // For the first branch the condition is in the opening if-tag, so we
    // pass an empty string as placeholder.
    let mut branch_cond: &str = "";
    let mut search_from: usize = 0;

    while search_from < body.len() {
        let next_if = find_tag_start(&body[search_from..], TAG_IF_PREFIX).map(|p| p + search_from);
        let next_close_if =
            find_block_close(&body[search_from..], CLOSE_IF).map(|(p, _)| p + search_from);
        let next_elif = find_elif_tag(&body[search_from..]).map(|(p, _len, _cond)| p + search_from);
        let next_else =
            find_block_close(&body[search_from..], KW_ELSE).map(|(p, _)| p + search_from);

        let candidates = [
            next_if.map(|p| (p, 'i')),
            next_elif.map(|p| (p, 'l')),
            next_else.map(|p| (p, 'e')),
            next_close_if.map(|p| (p, 'c')),
        ];
        let earliest = candidates.into_iter().flatten().min_by_key(|&(pos, _)| pos);

        match earliest {
            Some((pos, 'i')) => {
                depth += 1;
                search_from = pos + STMT_START.len();
            }
            Some((pos, 'c')) => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                let (_, len) = find_block_close(&body[pos..], CLOSE_IF).expect("just found");
                search_from = pos + len;
            }
            Some((pos, 'l')) if depth == 0 => {
                // Save current branch.
                branches.push((branch_cond, &body[branch_start..pos]));
                // Parse elif condition and advance.
                let (_, tag_len, cond) = find_elif_tag(&body[pos..]).expect("just found");
                branch_cond = cond;
                branch_start = pos + tag_len;
                search_from = branch_start;
            }
            Some((pos, 'e')) if depth == 0 => {
                // else branch — finish the current branch and return.
                branches.push((branch_cond, &body[branch_start..pos]));
                let (_, len) = find_block_close(&body[pos..], KW_ELSE).expect("just found");
                return (branches, Some(&body[pos + len..]));
            }
            Some((pos, tag)) => {
                // Nested elif or else — skip.
                let skip_len = match tag {
                    'l' => find_elif_tag(&body[pos..]).map_or(STMT_START.len(), |(_, l, _)| l),
                    'e' => {
                        find_block_close(&body[pos..], KW_ELSE).map_or(STMT_START.len(), |(_, l)| l)
                    }
                    _ => STMT_START.len(),
                };
                search_from = pos + skip_len;
            }
            None => break,
        }
    }
    // Push the final (or only) branch.
    branches.push((branch_cond, &body[branch_start..]));
    (branches, None)
}

/// Find a `{% elif <condition> %}` tag (or trim variant) at the start
/// of `haystack`. Returns `(offset, tag_len, condition_str)`.
fn find_elif_tag(haystack: &str) -> Option<(usize, usize, &str)> {
    let mut pos = 0;
    while pos < haystack.len() {
        let start = haystack[pos..].find(STMT_START)?;
        let abs_start = pos + start;
        let after_open = abs_start + STMT_START.len();

        let after_dash = if haystack[after_open..].starts_with(TRIM_MARKER) {
            after_open + 1
        } else {
            after_open
        };

        let content_start = after_dash
            + haystack[after_dash..]
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(haystack[after_dash..].len());

        if haystack[content_start..].starts_with(TAG_ELIF_PREFIX) {
            let cond_start = content_start + TAG_ELIF_PREFIX.len();
            // Find closing `%}` or `-%}`.
            let rest = &haystack[cond_start..];
            // Find end: `-%}` or `%}`
            if let Some(close_pos) = rest.find(STMT_END) {
                let before_close = &rest[..close_pos];
                let (cond, tag_end) = if let Some(stripped) = before_close.strip_suffix(TRIM_MARKER)
                {
                    (stripped.trim(), cond_start + close_pos + STMT_END.len())
                } else {
                    (before_close.trim(), cond_start + close_pos + STMT_END.len())
                };
                return Some((abs_start, tag_end - abs_start, cond));
            }
        }
        pos = abs_start + STMT_START.len();
    }
    None
}

/// Split an expression at the first `|` that is NOT inside quotes or parentheses.
///
/// Returns `(path, filter_chain)`. If there's no top-level pipe, the entire
/// string is the path and the filter chain is empty.
///
/// This is needed because filter arguments may contain pipes:
/// `name | default("a | b")` should split into `("name ", " default(\"a | b\")")`.
pub(crate) fn split_pipe_aware(expr: &str) -> (&str, &str) {
    use crate::consts::{PAREN_CLOSE, PAREN_OPEN, PIPE, QUOTE_DOUBLE, QUOTE_SINGLE};

    let mut depth: u32 = 0;
    let mut in_quote: Option<char> = None;

    for (i, ch) in expr.char_indices() {
        match ch {
            QUOTE_DOUBLE | QUOTE_SINGLE if in_quote == Some(ch) => in_quote = None,
            QUOTE_DOUBLE | QUOTE_SINGLE if in_quote.is_none() => in_quote = Some(ch),
            PAREN_OPEN if in_quote.is_none() => depth += 1,
            PAREN_CLOSE if in_quote.is_none() && depth > 0 => depth -= 1,
            PIPE if in_quote.is_none() && depth == 0 => {
                return (&expr[..i], &expr[i + 1..]);
            }
            _ => {}
        }
    }
    (expr, "")
}

/// Split a filter chain string at top-level `|` delimiters, skipping pipes
/// inside quotes and parentheses.
pub(crate) fn split_filters_aware(chain: &str) -> Vec<&str> {
    use crate::consts::{PAREN_CLOSE, PAREN_OPEN, PIPE, QUOTE_DOUBLE, QUOTE_SINGLE};

    let mut result = Vec::new();
    let mut start = 0;
    let mut depth: u32 = 0;
    let mut in_quote: Option<char> = None;

    for (i, ch) in chain.char_indices() {
        match ch {
            QUOTE_DOUBLE | QUOTE_SINGLE if in_quote == Some(ch) => in_quote = None,
            QUOTE_DOUBLE | QUOTE_SINGLE if in_quote.is_none() => in_quote = Some(ch),
            PAREN_OPEN if in_quote.is_none() => depth += 1,
            PAREN_CLOSE if in_quote.is_none() && depth > 0 => depth -= 1,
            PIPE if in_quote.is_none() && depth == 0 => {
                result.push(&chain[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&chain[start..]);
    result
}

/// Evaluate an expression and return its value, with optional pipe-separated filters.
pub(crate) fn eval_expr(expr: &str, scope: &Scope<'_>) -> Result<Value, TemplateError> {
    let (path_part, filter_chain) = split_pipe_aware(expr);
    let path = path_part.trim();

    // Try function calls first: idx(item), len(items)
    let mut value = if let Some(result) = scope.try_call_function(path) {
        result?
    } else {
        scope.resolve_path_str(path)?.clone()
    };

    if !filter_chain.is_empty() {
        value = apply_filter_chain(value, filter_chain)?;
    }
    Ok(value)
}

/// Apply a chain of pipe-separated filters using typed dispatch.
fn apply_filter_chain(mut value: Value, chain: &str) -> Result<Value, TemplateError> {
    for filter_str in split_filters_aware(chain) {
        let filter_str = filter_str.trim();
        if filter_str.is_empty() {
            continue;
        }
        let (name, args) = crate::filter::parse_filter(filter_str);
        let kind = crate::compiled::parse_filter_kind(name)?;
        value = crate::filter::apply_filter_typed(kind, &value, args)?;
    }
    Ok(value)
}

/// Parse a for-tag body like `item in items` or `for item in items`.
pub(crate) fn parse_for_tag(tag_body: &str) -> Result<(&str, &str), TemplateError> {
    let tag_body = tag_body.trim();
    let tag_body = tag_body.strip_prefix(TAG_FOR_PREFIX).unwrap_or(tag_body);
    let Some((binding, list_var)) = tag_body.split_once(KW_IN_SPACED) else {
        return Err(TemplateError::syntax(format!(
            "invalid for tag: expected 'item in list', got '{tag_body}'"
        )));
    };
    let binding = binding.trim();
    let list_var = list_var.trim();
    if binding.is_empty() || list_var.is_empty() {
        return Err(TemplateError::syntax(format!(
            "invalid for tag: empty binding or list in '{tag_body}'"
        )));
    }
    Ok((binding, list_var))
}

/// Parse an include directive.
pub(crate) fn parse_include_tag(tag_body: &str) -> Result<IncludeDirective<'_>, TemplateError> {
    let tag_body = tag_body
        .strip_prefix(TAG_INCLUDE_PREFIX)
        .unwrap_or(tag_body)
        .trim();
    if tag_body.is_empty() || tag_body == "include" {
        return Err(TemplateError::syntax("include: missing path".to_string()));
    }
    let (path, rest) = parse_quoted_path(tag_body)?;
    let rest = rest.trim();

    let mut with_vars = Vec::new();
    let mut for_each = None;

    if rest.is_empty() {
        return Ok(IncludeDirective {
            path,
            with_vars,
            for_each,
        });
    }

    let rest = if let Some(for_rest) = rest.strip_prefix(TAG_FOR_PREFIX) {
        let (binding, after_in) = for_rest
            .split_once(KW_IN_SPACED)
            .ok_or_else(|| TemplateError::syntax(format!("invalid include-for: got '{rest}'")))?;
        let (list_var, after_list) = if let Some(with_pos) = after_in.find(TAG_WITH_SPACED) {
            (&after_in[..with_pos], after_in[with_pos..].trim())
        } else {
            (after_in.trim(), "")
        };
        for_each = Some((binding.trim(), list_var.trim()));
        after_list
    } else {
        rest
    };

    if let Some(with_body) = rest.strip_prefix(TAG_WITH_PREFIX) {
        for assignment in with_body.split(',') {
            let assignment = assignment.trim();
            if assignment.is_empty() {
                continue;
            }
            let (key, val) = assignment.split_once('=').ok_or_else(|| {
                TemplateError::syntax(format!("invalid with: got '{assignment}'"))
            })?;
            with_vars.push((key.trim(), val.trim()));
        }
    }

    Ok(IncludeDirective {
        path,
        with_vars,
        for_each,
    })
}

fn parse_quoted_path(input: &str) -> Result<(&str, &str), TemplateError> {
    let input = input.trim();
    let first = input
        .chars()
        .next()
        .ok_or_else(|| TemplateError::syntax("include: missing path".to_string()))?;

    // Markdown link syntax: [text](path) — required for file includes.
    if first == '[' {
        return parse_markdown_link_path(input);
    }

    // Reject old quoted-path syntax — includes must use [name](path) links.
    if first == '"' || first == '\'' {
        return Err(TemplateError::syntax(format!(
            "include: quoted paths are no longer supported. \
             Use markdown link syntax instead: \
             `include [name]({})` (got `{input}`)",
            input.trim_matches(first),
        )));
    }

    // Bare identifier or dotted path — used for inline templates or variable templates.
    // We stop at the first whitespace.
    if first.is_alphanumeric() || first == '_' {
        let end = input
            .find(|c: char| c.is_whitespace())
            .unwrap_or(input.len());
        let name = &input[..end];
        let rest = &input[end..];
        return Ok((name, rest));
    }

    Err(TemplateError::syntax(format!(
        "include: expected [name](path) or bare identifier, got '{input}'"
    )))
}

/// Parse a markdown link `[text](path)` and return the `path` portion.
///
/// The link text is a short display name (without `.tmpl.md`); the URL is
/// the actual file path used by the engine. This makes includes render as
/// clickable links in markdown preview:
/// ```text
/// >{% include [collaboration_rules](collaboration_rules.tmpl.md) %}
/// ```
fn parse_markdown_link_path(input: &str) -> Result<(&str, &str), TemplateError> {
    let after_bracket = &input[1..]; // skip '['
    let close_bracket = after_bracket
        .find(']')
        .ok_or_else(|| TemplateError::syntax(format!("include: unclosed '[' in '{input}'")))?;
    let after_close = &after_bracket[close_bracket + 1..];

    // Expect `(path)` immediately after `]`.
    let after_close = after_close.trim_start();
    if !after_close.starts_with('(') {
        return Err(TemplateError::syntax(format!(
            "include: expected '(' after ']' in '{input}'"
        )));
    }
    let after_paren = &after_close[1..];
    let close_paren = after_paren
        .find(')')
        .ok_or_else(|| TemplateError::syntax(format!("include: unclosed '(' in '{input}'")))?;
    let path = after_paren[..close_paren].trim();
    let rest = &after_paren[close_paren + 1..];
    Ok((path, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_else_block(body: &str) -> (&str, Option<&str>) {
        let mut depth: u32 = 0;
        let mut search_from = 0;

        while search_from < body.len() {
            let next_if =
                find_tag_start(&body[search_from..], TAG_IF_PREFIX).map(|p| p + search_from);
            let next_close_if =
                find_block_close(&body[search_from..], CLOSE_IF).map(|(p, _)| p + search_from);
            let next_else =
                find_block_close(&body[search_from..], KW_ELSE).map(|(p, _)| p + search_from);

            let candidates = [
                next_if.map(|p| (p, 'i')),
                next_else.map(|p| (p, 'e')),
                next_close_if.map(|p| (p, 'c')),
            ];
            let earliest = candidates.into_iter().flatten().min_by_key(|&(pos, _)| pos);

            match earliest {
                Some((pos, 'i')) => {
                    depth += 1;
                    search_from = pos + STMT_START.len();
                }
                Some((pos, 'c')) => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    let (_, len) = find_block_close(&body[pos..], CLOSE_IF).expect("just found");
                    search_from = pos + len;
                }
                Some((pos, 'e')) if depth == 0 => {
                    let (_, len) = find_block_close(&body[pos..], KW_ELSE).expect("just found");
                    return (&body[..pos], Some(&body[pos + len..]));
                }
                Some((pos, _)) => {
                    let (_, len) = find_block_close(&body[pos..], KW_ELSE).expect("just found");
                    search_from = pos + len;
                }
                None => break,
            }
        }
        (body, None)
    }

    #[test]
    fn scan_literal_only() {
        match scan_next_tag("Hello world!").unwrap() {
            ScanResult::Literal(t) => assert_eq!(t, "Hello world!"),
            ScanResult::Found { .. } => panic!("Expected Literal"),
        }
    }

    #[test]
    fn scan_expr_tag() {
        match scan_next_tag("Hello {{ name }}!").unwrap() {
            ScanResult::Found {
                before, tag, after, ..
            } => {
                assert_eq!(before, "Hello ");
                assert_eq!(tag, Tag::Expr("name"));
                assert_eq!(after, "!");
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn scan_stmt_tag() {
        match scan_next_tag("{% for item in items %}body").unwrap() {
            ScanResult::Found {
                before, tag, after, ..
            } => {
                assert_eq!(before, "");
                assert_eq!(tag, Tag::Stmt("for item in items"));
                assert_eq!(after, "body");
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn scan_unclosed_expr() {
        scan_next_tag("Hello {{ name").expect_err("unclosed expression should fail");
    }

    #[test]
    fn scan_unclosed_stmt() {
        scan_next_tag("{% for item").expect_err("unclosed statement should fail");
    }

    #[test]
    fn find_closing_for_block() {
        let input = "body text\n{% /for %}\nafter";
        let (body, rest) = find_closing_block(input, "for ", "/for").unwrap();
        assert_eq!(body, "body text\n");
        assert_eq!(rest, "after");
    }

    #[test]
    fn find_closing_block_nested() {
        let input = "{% for x in xs %}inner{% /for %}\nouter{% /for %}\nafter";
        let (body, rest) = find_closing_block(input, "for ", "/for").unwrap();
        assert!(body.contains("{% for x in xs %}"));
        assert_eq!(rest, "after");
    }

    #[test]
    fn find_closing_block_missing() {
        find_closing_block("body", "for ", "/for").expect_err("missing closing block should fail");
    }

    #[test]
    fn split_no_else() {
        let (then, else_part) = split_else_block("then body");
        assert_eq!(then, "then body");
        assert!(else_part.is_none());
    }

    #[test]
    fn split_with_else() {
        let (then, else_part) = split_else_block("then{% else %}else body");
        assert_eq!(then, "then");
        assert_eq!(else_part.unwrap(), "else body");
    }

    #[test]
    fn split_else_nested_if() {
        let body = "{% if x %}a{% else %}b{% /if %}{% else %}outer";
        let (then, else_part) = split_else_block(body);
        assert!(then.contains("{% /if %}"));
        assert_eq!(else_part.unwrap(), "outer");
    }

    #[test]
    fn parse_for_simple() {
        let (b, l) = parse_for_tag("item in items").unwrap();
        assert_eq!(b, "item");
        assert_eq!(l, "items");
    }

    #[test]
    fn parse_for_with_prefix() {
        let (b, l) = parse_for_tag("for task in tasks").unwrap();
        assert_eq!(b, "task");
        assert_eq!(l, "tasks");
    }

    #[test]
    fn parse_for_invalid() {
        parse_for_tag("just_a_word").expect_err("for tag without 'in' keyword should fail");
    }

    #[test]
    fn parse_include_simple() {
        let d = parse_include_tag("include [header](header.tmpl.md)").unwrap();
        assert_eq!(d.path, "header.tmpl.md");
        assert!(d.with_vars.is_empty());
        assert!(d.for_each.is_none());
    }

    #[test]
    fn parse_include_with_vars() {
        let d =
            parse_include_tag("include [card](card.tmpl.md) with title=name, count=total").unwrap();
        assert_eq!(d.path, "card.tmpl.md");
        assert_eq!(d.with_vars, vec![("title", "name"), ("count", "total")]);
    }

    #[test]
    fn parse_include_for_each() {
        let d = parse_include_tag("include [row](row.tmpl.md) for item in items").unwrap();
        assert_eq!(d.for_each, Some(("item", "items")));
    }

    #[test]
    fn parse_include_missing_path() {
        parse_include_tag("include").expect_err("include without path should fail");
    }

    // -- markdown link include syntax ----------------------------------------

    #[test]
    fn parse_include_markdown_link() {
        let d = parse_include_tag("include [collaboration_rules](collaboration_rules.tmpl.md)")
            .unwrap();
        assert_eq!(d.path, "collaboration_rules.tmpl.md");
        assert!(d.with_vars.is_empty());
        assert!(d.for_each.is_none());
    }

    #[test]
    fn parse_include_markdown_link_with_vars() {
        let d =
            parse_include_tag("include [task_planning](task_planning.tmpl.md) with tasks=tasks")
                .unwrap();
        assert_eq!(d.path, "task_planning.tmpl.md");
        assert_eq!(d.with_vars, vec![("tasks", "tasks")]);
    }

    #[test]
    fn parse_include_markdown_link_for_each() {
        let d = parse_include_tag("include [row](row.tmpl.md) for item in items").unwrap();
        assert_eq!(d.path, "row.tmpl.md");
        assert_eq!(d.for_each, Some(("item", "items")));
    }

    #[test]
    fn parse_include_markdown_link_unclosed_bracket() {
        parse_include_tag("include [no_close")
            .expect_err("unclosed bracket in include should fail");
    }

    #[test]
    fn parse_include_markdown_link_missing_paren() {
        parse_include_tag("include [text] no_paren")
            .expect_err("include without '(' after ']' should fail");
    }

    // -- whitespace control scan tests ----------------------------------------

    #[test]
    fn scan_expr_trim_left() {
        match scan_next_tag("hello {{- name }}!").unwrap() {
            ScanResult::Found {
                before,
                tag,
                after,
                trim_before,
                trim_after,
            } => {
                assert_eq!(before, "hello ");
                assert_eq!(tag, Tag::Expr("name"));
                assert_eq!(after, "!");
                assert!(trim_before);
                assert!(!trim_after);
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn scan_expr_trim_right() {
        match scan_next_tag("hello {{ name -}} !").unwrap() {
            ScanResult::Found {
                trim_before,
                trim_after,
                tag,
                ..
            } => {
                assert_eq!(tag, Tag::Expr("name"));
                assert!(!trim_before);
                assert!(trim_after);
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn scan_expr_trim_both() {
        match scan_next_tag("x {{- name -}} y").unwrap() {
            ScanResult::Found {
                trim_before,
                trim_after,
                tag,
                ..
            } => {
                assert_eq!(tag, Tag::Expr("name"));
                assert!(trim_before);
                assert!(trim_after);
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn scan_stmt_trim_left() {
        match scan_next_tag("{%- if show %}body").unwrap() {
            ScanResult::Found {
                tag,
                trim_before,
                trim_after,
                ..
            } => {
                assert_eq!(tag, Tag::Stmt("if show"));
                assert!(trim_before);
                assert!(!trim_after);
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn scan_stmt_trim_right() {
        match scan_next_tag("{% if show -%}body").unwrap() {
            ScanResult::Found {
                tag,
                trim_before,
                trim_after,
                ..
            } => {
                assert_eq!(tag, Tag::Stmt("if show"));
                assert!(!trim_before);
                assert!(trim_after);
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn scan_stmt_trim_both() {
        match scan_next_tag("{%- if show -%}body").unwrap() {
            ScanResult::Found {
                tag,
                trim_before,
                trim_after,
                ..
            } => {
                assert_eq!(tag, Tag::Stmt("if show"));
                assert!(trim_before);
                assert!(trim_after);
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn scan_no_trim_markers() {
        match scan_next_tag("{{ name }}").unwrap() {
            ScanResult::Found {
                trim_before,
                trim_after,
                ..
            } => {
                assert!(!trim_before);
                assert!(!trim_after);
            }
            ScanResult::Literal(_) => panic!("Expected Found"),
        }
    }

    #[test]
    fn find_closing_block_with_trim_variant() {
        // Closing tag with whitespace control: `{%- /for -%}`
        let input = "body text\n{%- /for -%}\nafter";
        let (body, rest) = find_closing_block(input, "for ", "/for").unwrap();
        assert_eq!(body, "body text\n");
        assert_eq!(rest, "after");
    }

    #[test]
    fn find_closing_block_trim_open_close() {
        // Mixed plain open + trim close
        let input = "body{% /for -%}\nafter";
        let (body, rest) = find_closing_block(input, "for ", "/for").unwrap();
        assert_eq!(body, "body");
        assert_eq!(rest, "after");
    }
}
