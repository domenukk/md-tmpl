//! Nesting-aware scanning of block bodies.
//!
//! A block's own branch tags (`{% elif %}`, `{% else %}`, `{% case %}`) are
//! only meaningful at the block's top nesting level: an `{% else %}` inside
//! a nested `if` / `for` / `match` belongs to that inner block, and any tag
//! inside a `{% raw %}` body is literal text. Every splitter in the compiler
//! goes through [`find_top_level_tag`] so they all agree on that structure.

use alloc::{borrow::Cow, format, vec::Vec};
use core::ops::Range;

use crate::{
    consts::{
        CLOSE_FOR, CLOSE_IF, CLOSE_MATCH, CLOSE_RAW, KW_ELSE, KW_RAW, KW_RAW_ASSIGN, STMT_END,
        STMT_START, TAG_ELIF_PREFIX, TAG_FOR_PREFIX, TAG_IF_PREFIX, TAG_MATCH_PREFIX,
    },
    error::TemplateError,
    parser,
};

/// Opening/closing keywords of a `{% raw %}` or `{% raw=DELIM %}` block.
pub(super) struct RawBlockDelimiters {
    /// Opening keyword (`raw` or `raw=DELIM`), used to track nested opens.
    pub open: Cow<'static, str>,
    /// Closing keyword (`/raw` or `/DELIM`).
    pub close: Cow<'static, str>,
}

impl RawBlockDelimiters {
    /// Parse a statement as a raw-block opener.
    ///
    /// Returns `Ok(None)` when `stmt` does not open a raw block.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] for `{% raw= %}` without a delimiter.
    pub(super) fn parse(stmt: &str) -> Result<Option<Self>, TemplateError> {
        if stmt == KW_RAW {
            return Ok(Some(Self {
                open: Cow::Borrowed(KW_RAW),
                close: Cow::Borrowed(CLOSE_RAW),
            }));
        }
        let Some(delim) = stmt.strip_prefix(KW_RAW_ASSIGN) else {
            return Ok(None);
        };
        let delim = delim.trim();
        if delim.is_empty() {
            return Err(TemplateError::syntax(format!(
                "{STMT_START} {KW_RAW_ASSIGN} {STMT_END} requires a delimiter after '='"
            )));
        }
        Ok(Some(Self {
            open: Cow::Owned(format!("{KW_RAW_ASSIGN}{delim}")),
            close: Cow::Owned(format!("/{delim}")),
        }))
    }

    /// Split `after_tag` into the raw body and the text after the close tag.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::Syntax`] when the raw block is unclosed.
    pub(super) fn split_body<'a>(
        &self,
        after_tag: &'a str,
    ) -> Result<(&'a str, &'a str), TemplateError> {
        parser::find_closing_block(after_tag, &self.open, &self.close)
    }
}

/// How a statement tag affects the nesting depth of the enclosing body.
enum Nesting {
    /// `{% if %}`, `{% for %}`, `{% match %}`.
    Open,
    /// `{% /if %}`, `{% /for %}`, `{% /match %}`.
    Close,
    /// `{% raw %}` / `{% raw=DELIM %}` — its body is skipped unscanned.
    Raw(RawBlockDelimiters),
    /// Any other statement (`else`, `elif`, `case`, `include`, …).
    Neutral,
}

impl Nesting {
    fn of(stmt: &str) -> Result<Self, TemplateError> {
        if [TAG_IF_PREFIX, TAG_FOR_PREFIX, TAG_MATCH_PREFIX]
            .iter()
            .any(|prefix| stmt.starts_with(prefix))
        {
            return Ok(Self::Open);
        }
        if [CLOSE_IF, CLOSE_FOR, CLOSE_MATCH].contains(&stmt) {
            return Ok(Self::Close);
        }
        Ok(RawBlockDelimiters::parse(stmt)?.map_or(Self::Neutral, Self::Raw))
    }
}

/// A statement tag found at the top nesting level of a block body.
pub(super) struct TopLevelTag<'a> {
    /// Byte offset of the tag's opening `{%` within the body.
    pub start: usize,
    /// Byte offset just past the tag's closing `%}` within the body.
    pub end: usize,
    /// Trimmed statement content, without trim markers.
    pub stmt: &'a str,
}

/// Find the first statement at nesting depth 0 of `body` accepted by `is_branch`.
///
/// Nested `if` / `for` / `match` blocks and `raw` bodies are skipped, so
/// their inner `else` / `elif` / `case` tags are never returned.
///
/// # Errors
///
/// Propagates tag-scanning errors (unclosed tags, unclosed raw blocks).
pub(super) fn find_top_level_tag(
    body: &str,
    is_branch: impl Fn(&str) -> bool,
) -> Result<Option<TopLevelTag<'_>>, TemplateError> {
    let mut depth: u32 = 0;
    let mut offset: usize = 0;

    while offset < body.len() {
        let rest = &body[offset..];
        let (before, stmt, after) = match parser::scan_next_tag(rest)? {
            parser::ScanResult::Literal(_) => return Ok(None),
            parser::ScanResult::Found {
                before,
                tag: parser::Tag::Stmt(stmt),
                after,
                ..
            } => (before, stmt, after),
            parser::ScanResult::Found { after, .. } => {
                offset = body.len() - after.len();
                continue;
            }
        };

        if depth == 0 && is_branch(stmt) {
            return Ok(Some(TopLevelTag {
                start: offset + before.len(),
                end: body.len() - after.len(),
                stmt,
            }));
        }

        offset = match Nesting::of(stmt)? {
            Nesting::Open => {
                depth += 1;
                body.len() - after.len()
            }
            Nesting::Close => {
                depth = depth.saturating_sub(1);
                body.len() - after.len()
            }
            Nesting::Raw(delimiters) => {
                let (_raw_body, after_raw) = delimiters.split_body(after)?;
                body.len() - after_raw.len()
            }
            Nesting::Neutral => body.len() - after.len(),
        };
    }
    Ok(None)
}

/// Split a `for` body (or inline `match ... case` body) at its own `{% else %}`.
///
/// Returns `(main_body, else_body)`; `else_body` is `None` when the block
/// has no top-level `{% else %}`.
///
/// # Errors
///
/// Propagates tag-scanning errors from [`find_top_level_tag`].
pub(super) fn split_for_else(body: &str) -> Result<(&str, Option<&str>), TemplateError> {
    Ok(match find_top_level_tag(body, |stmt| stmt == KW_ELSE)? {
        Some(tag) => (&body[..tag.start], Some(&body[tag.end..])),
        None => (body, None),
    })
}

/// Branches of an `{% if %}` chain: `(condition, body)` pairs plus the else body.
pub(super) struct ElifChain<'a> {
    /// `(condition, body)` per branch. The first condition is empty because
    /// it lives in the opening `{% if %}` tag.
    pub branches: Vec<(&'a str, &'a str)>,
    /// Body of the top-level `{% else %}`, if any.
    pub else_body: Option<&'a str>,
}

/// Split an if-block body into its `{% elif %}` / `{% else %}` branches.
///
/// # Errors
///
/// Propagates tag-scanning errors from [`find_top_level_tag`].
pub(super) fn split_elif_chain(body: &str) -> Result<ElifChain<'_>, TemplateError> {
    let mut branches = Vec::new();
    let mut condition = "";
    let mut branch_start = 0;

    loop {
        let found = find_top_level_tag(&body[branch_start..], |stmt| {
            stmt == KW_ELSE || stmt.starts_with(TAG_ELIF_PREFIX)
        })?;
        let Some(tag) = found else {
            branches.push((condition, &body[branch_start..]));
            return Ok(ElifChain {
                branches,
                else_body: None,
            });
        };
        branches.push((condition, &body[branch_start..branch_start + tag.start]));
        let next_start = branch_start + tag.end;
        match tag.stmt.strip_prefix(TAG_ELIF_PREFIX) {
            Some(elif_condition) => {
                condition = elif_condition.trim();
                branch_start = next_start;
            }
            None => {
                return Ok(ElifChain {
                    branches,
                    else_body: Some(&body[next_start..]),
                });
            }
        }
    }
}

/// Byte ranges of every top-level `{% raw %}` body in `input`.
///
/// Uses the same tag scanner as the compiler, so the ranges match exactly
/// what [`super::compile_body`] treats as literal raw text.
///
/// # Errors
///
/// Propagates tag-scanning errors (unclosed tags, unclosed raw blocks).
pub(super) fn raw_body_ranges(input: &str) -> Result<Vec<Range<usize>>, TemplateError> {
    let mut ranges = Vec::new();
    let mut offset: usize = 0;

    while offset < input.len() {
        offset = match parser::scan_next_tag(&input[offset..])? {
            parser::ScanResult::Literal(_) => break,
            parser::ScanResult::Found {
                tag: parser::Tag::Stmt(stmt),
                after,
                ..
            } => match RawBlockDelimiters::parse(stmt)? {
                Some(delimiters) => {
                    let (raw_body, after_raw) = delimiters.split_body(after)?;
                    let body_start = input.len() - after.len();
                    ranges.push(body_start..body_start + raw_body.len());
                    input.len() - after_raw.len()
                }
                None => input.len() - after.len(),
            },
            parser::ScanResult::Found { after, .. } => input.len() - after.len(),
        };
    }
    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn split_for_else_without_else() {
        let (main, else_body) = split_for_else("body").unwrap();
        assert_eq!(main, "body");
        assert_eq!(else_body, None);
    }

    #[test]
    fn split_for_else_with_else() {
        let (main, else_body) = split_for_else("yes{% else %}no").unwrap();
        assert_eq!(main, "yes");
        assert_eq!(else_body, Some("no"));
    }

    #[test]
    fn split_for_else_skips_nested_match_else() {
        let body = "{% match s case A %}a{% else %}b{% /match %}{% else %}empty";
        let (main, else_body) = split_for_else(body).unwrap();
        assert_eq!(main, "{% match s case A %}a{% else %}b{% /match %}");
        assert_eq!(else_body, Some("empty"));
    }

    #[test]
    fn split_for_else_skips_raw_else() {
        let body = "{% raw %}{% else %}{% /raw %}{%- else -%}empty";
        let (main, else_body) = split_for_else(body).unwrap();
        assert_eq!(main, "{% raw %}{% else %}{% /raw %}");
        assert_eq!(else_body, Some("empty"));
    }

    #[test]
    fn split_elif_chain_skips_nested_for_else() {
        let body = "{% for x in xs %}x{% else %}none{% /for %}{% elif b %}B{% else %}C";
        let chain = split_elif_chain(body).unwrap();
        assert_eq!(
            chain.branches,
            [
                ("", "{% for x in xs %}x{% else %}none{% /for %}"),
                ("b", "B")
            ]
        );
        assert_eq!(chain.else_body, Some("C"));
    }

    #[test]
    fn split_elif_chain_skips_nested_if_else() {
        let chain = split_elif_chain("{% if a %}A{% else %}B{% /if %}").unwrap();
        assert_eq!(chain.branches, [("", "{% if a %}A{% else %}B{% /if %}")]);
        assert_eq!(chain.else_body, None);
    }

    #[test]
    fn raw_body_ranges_cover_default_and_custom_delimiters() {
        let input = "a{% raw %}R1{% /raw %}b{% raw=# %}{% /raw %}{% /# %}c";
        let ranges = raw_body_ranges(input).unwrap();
        let bodies: Vec<&str> = ranges.into_iter().map(|r| &input[r]).collect();
        assert_eq!(bodies, ["R1", "{% /raw %}"]);
    }

    #[test]
    fn raw_body_ranges_ignore_raw_word_in_other_statements() {
        let ranges = raw_body_ranges(r#"{% if mode == "a raw b" %}x{% /if %}"#).unwrap();
        assert!(ranges.is_empty());
    }

    #[test]
    fn raw_block_delimiters_reject_empty_custom_delimiter() {
        let err = RawBlockDelimiters::parse("raw=")
            .err()
            .expect("empty delimiter");
        assert!(err.to_string().contains("requires a delimiter"));
    }
}
