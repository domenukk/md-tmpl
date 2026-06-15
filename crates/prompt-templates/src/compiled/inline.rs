//! Inline template extraction (`{% tmpl NAME %}...{% /tmpl %}`).
//!
//! Extracts inline template definitions from the input, removing them
//! from the body. Each template is pre-compiled (frontmatter parsed,
//! segments compiled) at extraction time.

use std::{collections::HashMap, sync::Arc};

use super::{CompiledInlineTemplate, compile_body};
use crate::{
    consts::{
        CLOSE_RAW, CLOSE_TMPL, KW_RAW, KW_RAW_ASSIGN, STMT_END, STMT_START, TAG_TMPL_PREFIX,
        TRIM_MARKER,
    },
    error::TemplateError,
    frontmatter, parser,
};

/// Extract `{% tmpl NAME %}...{% /tmpl %}` inline template definitions from
/// the input, removing them from the body.
///
/// `parent_type_aliases` are the type aliases from the enclosing template's
/// frontmatter. Inline templates inherit these aliases (their own `types:`
/// block shadows the parent's).
///
/// Returns `(cleaned_body, inline_templates)` where `inline_templates` maps
/// each template name to a pre-compiled [`CompiledInlineTemplate`] (frontmatter
/// parsed and segments compiled once).
///
/// # Errors
///
/// Returns [`TemplateError::Syntax`] on duplicate names, empty names, or
/// compilation failures in inline template bodies.
pub fn extract_inline_templates(
    input: &str,
    parent_type_aliases: &std::collections::HashMap<String, crate::types::VarType>,
) -> Result<(String, HashMap<String, CompiledInlineTemplate>), TemplateError> {
    let mut templates: HashMap<String, CompiledInlineTemplate> = HashMap::new();
    let mut cleaned = String::with_capacity(input.len());
    let mut remaining = input;

    loop {
        // Find the next `{% tmpl ` or `{% raw` tag, whichever comes first.
        let tmpl_pos = find_tmpl_open(remaining);
        let raw_pos = find_raw_open(remaining);

        // Determine which tag comes first.
        let next_is_raw = match (tmpl_pos, raw_pos) {
            (None, None) => {
                cleaned.push_str(remaining);
                break;
            }
            (None, Some(_)) => true,
            (Some(_), None) => false,
            (Some(tp), Some(rp)) => rp < tp,
        };

        if next_is_raw {
            let rp = raw_pos.expect("checked above");
            // Copy text up to and including the raw open tag.
            cleaned.push_str(&remaining[..rp]);
            let after_raw_start = &remaining[rp..];

            // Skip the entire raw block, copying it verbatim.
            let (raw_text, after_raw_close) = skip_raw_block(after_raw_start)?;
            cleaned.push_str(raw_text);
            remaining = after_raw_close;
        } else {
            let tp = tmpl_pos.expect("checked above");
            cleaned.push_str(&remaining[..tp]);
            let after_open = &remaining[tp..];
            let (name, after_tag) = parse_tmpl_open_tag(after_open)?;

            let (body, after_close) =
                parser::find_closing_block(after_tag, TAG_TMPL_PREFIX, CLOSE_TMPL)?;

            if name.is_empty() {
                return Err(TemplateError::syntax(format!(
                    "{STMT_START} {KW_RAW} {STMT_END} requires a name"
                )));
            }
            if templates.contains_key(&name) {
                return Err(TemplateError::syntax(format!(
                    "duplicate inline template name: '{name}'"
                )));
            }

            // Parse frontmatter with parent scope for type alias inheritance.
            let (fm, tmpl_body) =
                frontmatter::parse_frontmatter_with_parent_scope(body, parent_type_aliases)?;
            let segments = compile_body(tmpl_body)?;

            templates.insert(
                name,
                CompiledInlineTemplate {
                    segments: Arc::from(segments),
                    declarations: Arc::from(fm.declarations),
                },
            );
            remaining = after_close;
        }
    }

    Ok((cleaned, templates))
}

/// Find the byte offset of the first `{% <keyword>` or `{%- <keyword>` tag.
///
/// Shared implementation for locating both `{% raw` and `{% tmpl ` open tags.
fn find_tag_open(input: &str, keyword: &str) -> Option<usize> {
    let plain_pattern = format!("{STMT_START} {keyword}");
    let trim_pattern = format!("{STMT_START}{TRIM_MARKER} {keyword}");
    let plain = input.find(&plain_pattern);
    let trim = input.find(&trim_pattern);
    match (plain, trim) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

/// Find the byte offset of the next `{% raw` tag (including `{% raw %}`, `{% raw=X %}`).
fn find_raw_open(input: &str) -> Option<usize> {
    find_tag_open(input, KW_RAW)
}

/// Skip past a `{% raw %}...{% /raw %}` or `{% raw=DELIM %}...{% /DELIM %}`
/// block. Returns the full text of the block (open tag + body + close tag) and
/// the remaining input after the close tag.
///
/// Raw blocks do NOT nest — the first matching close tag terminates the block.
fn skip_raw_block(input: &str) -> Result<(&str, &str), TemplateError> {
    // Parse the raw open tag to determine the close keyword.
    let tag_end = input.find(STMT_END).ok_or_else(|| {
        TemplateError::syntax(format!("unclosed {STMT_START} {KW_RAW} ... {STMT_END}"))
    })?;
    let tag_content = input[STMT_START.len()..tag_end]
        .trim_start_matches(TRIM_MARKER)
        .trim();

    // Determine close tag: "raw" closes with `{% /raw %}`, "raw=X" closes with `{% /X %}`.
    let close_tag = if let Some(delim) = tag_content.strip_prefix(KW_RAW_ASSIGN) {
        let delim = delim.trim();
        if delim.is_empty() {
            return Err(TemplateError::syntax(format!(
                "{STMT_START} {KW_RAW_ASSIGN} {STMT_END} — empty custom delimiter"
            )));
        }
        format!("{STMT_START} /{delim} {STMT_END}")
    } else {
        format!("{STMT_START} {CLOSE_RAW} {STMT_END}")
    };

    // Skip past the open tag's `%}`.
    let after_open_tag = &input[tag_end + STMT_END.len()..];

    // Find the closing tag via literal search (raw blocks don't nest).
    let close_pos = after_open_tag.find(&close_tag).ok_or_else(|| {
        TemplateError::syntax(format!("unclosed raw block, expected {close_tag}"))
    })?;

    let total_consumed = (tag_end + STMT_END.len()) + close_pos + close_tag.len();

    // Skip one trailing newline after the close tag.
    let rest = &input[total_consumed..];
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))
        .unwrap_or(rest);

    let consumed = input.len() - rest.len();
    Ok((&input[..consumed], rest))
}

/// Find the byte offset of the next `{% tmpl ` or `{%- tmpl ` tag.
fn find_tmpl_open(input: &str) -> Option<usize> {
    find_tag_open(input, TAG_TMPL_PREFIX)
}

/// Parse the opening `{% tmpl NAME %}` tag and return `(name, rest_after_tag)`.
fn parse_tmpl_open_tag(input: &str) -> Result<(String, &str), TemplateError> {
    // Skip `{%` or `{%-`
    let trim_start = format!("{STMT_START}{TRIM_MARKER}");
    let after_open =
        input
            .strip_prefix(&trim_start)
            .unwrap_or(input.strip_prefix(STMT_START).ok_or_else(|| {
                TemplateError::syntax(format!(
                    "expected {STMT_START} {TAG_TMPL_PREFIX}... {STMT_END}"
                ))
            })?);
    let inner_and_rest = after_open.find(STMT_END).ok_or_else(|| {
        TemplateError::syntax(format!(
            "unclosed {STMT_START} {TAG_TMPL_PREFIX}... {STMT_END}"
        ))
    })?;

    let inner = after_open[..inner_and_rest].trim();

    // inner should be like "tmpl NAME" — strip the "tmpl " prefix.
    let name = inner
        .strip_prefix(TAG_TMPL_PREFIX)
        .ok_or_else(|| TemplateError::syntax(format!("expected 'tmpl NAME', got '{inner}'")))?
        .trim()
        .to_string();

    // Check for `-%}` vs `%}`.
    let rest = &after_open[inner_and_rest + STMT_END.len()..];
    // Strip one trailing newline if present (standalone tag convention).
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))
        .unwrap_or(rest);

    Ok((name, rest))
}
