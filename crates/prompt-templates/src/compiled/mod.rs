//! Pre-compiled template segments.
//!
//! Compiles a template body into a flat instruction list at parse time,
//! so rendering is a simple walk over segments — no string scanning
//! per render call.

pub(crate) mod analysis;
mod blockquote;
mod inline;
pub(crate) mod render;
pub(crate) mod type_check;

use std::{borrow::Cow, collections::HashMap, sync::Arc};

// Re-export submodule items used by the rest of the crate.
pub use analysis::collect_referenced_params;
pub(crate) use render::{register_loop_meta, render_segments, render_segments_into};
pub use type_check::{validate_field_accesses, validate_field_accesses_with_opaque};

use crate::{
    consts::{
        CLOSE_FOR, CLOSE_IF, CLOSE_MATCH, CLOSE_RAW, KW_DEFAULT, KW_ELSE, KW_RAW, KW_RAW_ASSIGN,
        LEGACY_ENDFOR, LEGACY_ENDIF, LEGACY_ENDRAW, STMT_END, STMT_START, TAG_CASE_PREFIX,
        TAG_ELIF_PREFIX, TAG_FOR_PREFIX, TAG_IF_PREFIX, TAG_INCLUDE_PREFIX, TAG_MATCH_PREFIX,
    },
    error::TemplateError,
    parser,
    types::VarDecl,
};

/// A pre-compiled inline template definition (`{% tmpl name %}...{% /tmpl %}`).
///
/// Frontmatter is parsed and segments are compiled once at extraction time,
/// so rendering an inline template via `{% include name %}` avoids
/// re-parsing and re-compiling.
#[derive(Debug, Clone)]
pub struct CompiledInlineTemplate {
    /// Pre-compiled segment instructions.
    pub segments: Arc<[Segment]>,
    /// Declared variables from inline frontmatter.
    pub declarations: Arc<[VarDecl]>,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A list of match-arm pairs: `(variant_names, body_segments)`.
///
/// Each arm may match one or more variants: `{% case A | B %}`.
pub type MatchArms = Vec<(Vec<Cow<'static, str>>, Vec<Segment>)>;

/// Pre-compiled template instruction.
#[derive(Debug, Clone)]
pub enum Segment {
    /// Static text — just append to output.
    Static(Cow<'static, str>),
    /// Expression `{{ path | filter1 | filter2(arg) }}`.
    Expr {
        path: Cow<'static, str>,
        filters: Vec<ParsedFilter>,
    },
    /// `{% for binding in list_path %}body{% /for %}`.
    ForLoop {
        binding: Cow<'static, str>,
        list_path: Cow<'static, str>,
        body: Vec<Segment>,
    },
    /// `{% if cond %}...{% elif cond %}...{% else %}...{% /if %}`.
    If {
        /// `(condition, body)` pairs — first is the `if`, rest are `elif`.
        branches: Vec<(Condition, Vec<Segment>)>,
        else_body: Vec<Segment>,
    },
    /// `{% match expr %}{% case Variant %}...{% /match %}`.
    ///
    /// Variant names are validated at compile time against the enum
    /// declaration.  Non-exhaustive matches produce compile errors.
    Match {
        /// The expression to match on (e.g. `outcome`).
        expr: Cow<'static, str>,
        /// `(variant_name, body)` pairs.
        arms: MatchArms,
    },
    /// `{% raw %}literal text{% /raw %}`.
    Raw(Cow<'static, str>),
    /// `{% include <path> [with ...] [for ...] %}`.
    Include(CompiledInclude),
    /// `{# comment #}` — produces no output, but variable references
    /// inside the comment are tracked for unused-variable analysis.
    Comment(Vec<Cow<'static, str>>),
}

/// Pre-compiled condition for `{% if %}` blocks.
///
/// Parsed once at compile time so rendering never re-scans the condition
/// string for operators.
#[derive(Debug, Clone)]
pub enum Condition {
    /// Simple truthiness check: `{% if show %}`.
    Truthy(Cow<'static, str>),
    /// Binary comparison: `{% if count > 0 %}`.
    Comparison {
        left: Cow<'static, str>,
        op: ComparisonOp,
        right: Cow<'static, str>,
    },
}

/// Comparison operators for condition expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `<`
    Lt,
    /// `>`
    Gt,
}

/// A pre-parsed filter with typed kind and optional argument.
#[derive(Debug, Clone)]
pub struct ParsedFilter {
    /// The resolved filter kind.
    pub kind: FilterKind,
    /// Optional argument string (e.g. the separator for `join`).
    pub args: Option<Cow<'static, str>>,
}

/// Strongly-typed filter names, resolved at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// `| upper` — uppercase.
    Upper,
    /// `| lower` — lowercase.
    Lower,
    /// `| trim` — strip whitespace.
    Trim,
    /// `| fixed(n)` — fixed-width format.
    Fixed,
    /// `| join(sep)` — join list items.
    Join,
    /// `| limit(n)` — truncate to *n* items.
    Limit,
    /// `| add(n)` — add a number.
    Add,
    /// `| sub(n)` — subtract a number.
    Sub,
}

/// A `(key, expression)` pair used in `{% include ... with key=expr %}` overrides.
pub type WithVarPair = (Cow<'static, str>, Cow<'static, str>);

/// A pre-parsed include directive.
#[derive(Debug, Clone)]
pub struct CompiledInclude {
    /// Path to the included template file.
    pub path: Cow<'static, str>,
    /// `with key=expr` variable overrides.
    pub with_vars: Vec<WithVarPair>,
    /// Optional `for var in list` iteration.
    pub for_each: Option<WithVarPair>,
    /// Pre-compiled inline template body (for `{% tmpl %}` blocks).
    pub inline_compiled: Option<CompiledInlineTemplate>,
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

/// Compile a template body into a list of pre-parsed segments.
///
/// `parent_type_aliases` are passed to inline template extraction so that
/// `{% tmpl %}` blocks can reference the enclosing template's type aliases.
///
/// Recursively compiles for-loop and if-block bodies so the entire
/// template tree is pre-parsed.
///
/// # Errors
///
/// Returns [`TemplateError::Syntax`] on malformed tags or blocks.
pub fn compile(
    input: &str,
    parent_type_aliases: &HashMap<String, crate::types::VarType>,
) -> Result<(Vec<Segment>, HashMap<String, CompiledInlineTemplate>), TemplateError> {
    // Validate: statement tags at line start must have `>` blockquote prefix.
    blockquote::validate_blockquote_prefix(input)?;
    // Preprocess: strip blockquote `>` prefix from statement-tag lines if present.
    let processed = blockquote::strip_blockquote_tags(input);
    // Extract inline template definitions before compiling the body.
    let (cleaned, inline_templates) =
        inline::extract_inline_templates(&processed, parent_type_aliases)?;
    let segments = compile_body(&cleaned).map_err(|e| enrich_error(e, &cleaned))?;
    Ok((segments, inline_templates))
}

/// Compute 1-based line number for a byte offset into `source`.
fn line_at_offset(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

/// Extract the line of text at the given 1-based line number.
fn extract_line(source: &str, line_num: usize) -> &str {
    source
        .split('\n')
        .nth(line_num.saturating_sub(1))
        .unwrap_or("")
}

/// Maximum length for source-line snippets in error messages.
const SNIPPET_MAX_LEN: usize = 80;

/// Enrich a `TemplateError` with line number and context from the original
/// source when the error does not already contain line information.
fn enrich_error(err: TemplateError, source: &str) -> TemplateError {
    match err {
        TemplateError::Syntax(ref syn) if syn.line.is_none() => {
            // Try to locate the problematic text in the source.
            let hint = extract_error_hint(&syn.message);
            if let Some(offset) = hint.and_then(|h| source.find(h)) {
                let line_num = line_at_offset(source, offset);
                let line_text = extract_line(source, line_num).trim();
                let snippet = if line_text.len() > SNIPPET_MAX_LEN {
                    // Truncate at a character boundary to avoid panicking on multi-byte UTF-8.
                    let truncate_at = line_text
                        .char_indices()
                        .map(|(i, _)| i)
                        .take_while(|&i| i <= SNIPPET_MAX_LEN - 3)
                        .last()
                        .unwrap_or(0);
                    format!("{}…", &line_text[..truncate_at])
                } else {
                    line_text.to_string()
                };
                TemplateError::Syntax(
                    crate::error::SyntaxError::new(&syn.message).at_line(line_num, snippet),
                )
            } else {
                err
            }
        }
        TemplateError::UnknownFilter(ref name) => {
            let needle = format!("| {name}");
            if let Some(offset) = source.find(&needle) {
                let line_num = line_at_offset(source, offset);
                let line_text = extract_line(source, line_num).trim();
                TemplateError::Syntax(
                    crate::error::SyntaxError::new(format!("unknown filter '{name}'"))
                        .at_line(line_num, line_text),
                )
            } else {
                err
            }
        }
        _ => err,
    }
}

/// Try to extract a searchable substring from an error message to locate
/// the problematic text in the original source.
fn extract_error_hint(msg: &str) -> Option<&str> {
    // "unclosed `{{`" -> search for `{{`
    if let Some(start) = msg.find('`') {
        let rest = &msg[start + 1..];
        if let Some(end) = rest.find('`') {
            let hint = &rest[..end];
            if !hint.is_empty() {
                return Some(hint);
            }
        }
    }
    // "invalid for tag: expected 'item in list', got 'foo'" -> search for `{% for`
    if msg.contains("for tag") {
        return Some("{% for");
    }
    // "unknown statement: 'xxx'" -> search for the statement
    if let Some(inner) = msg.strip_prefix("unknown statement: '")
        && let Some(end) = inner.find('\'')
    {
        return Some(&inner[..end]);
    }
    // "unclosed '{% for %}'" -> search for `{% for`
    if msg.starts_with("unclosed '{% ") {
        return Some("{% ");
    }
    None
}
/// Internal compilation — used for block bodies that have already been
/// validated and stripped.
fn compile_body(input: &str) -> Result<Vec<Segment>, TemplateError> {
    let mut segments = Vec::new();
    let mut remaining: &str = input;

    loop {
        let scan = parser::scan_next_tag(remaining)?;

        match scan {
            parser::ScanResult::Literal(text) => {
                push_static(&mut segments, text);
                break;
            }
            parser::ScanResult::Found {
                before,
                tag,
                after,
                trim_before,
                trim_after,
            } => {
                push_static(&mut segments, before);

                // Jinja-style `{%-` / `{{-`: strip trailing whitespace
                // from the previous static segment (back to last newline
                // or start of text).
                if trim_before {
                    trim_trailing_whitespace_from_last_static(&mut segments);
                }

                // Jinja-style `-%}` / `-}}`: strip leading whitespace
                // (up to and including the next newline) from whatever
                // text follows this tag.
                let effective_after = if trim_after {
                    trim_leading_whitespace_through_newline(after)
                } else {
                    after
                };

                match tag {
                    parser::Tag::Expr(expr) => {
                        segments.push(compile_expr(expr)?);
                        remaining = effective_after;
                    }
                    parser::Tag::Stmt(stmt) => {
                        let (segment, rest) = compile_statement(stmt, effective_after)?;
                        segments.push(segment);
                        remaining = rest;
                    }
                    parser::Tag::Comment(content) => {
                        // Comments produce no output, but we scan for
                        // {{ var }} references so they count as "used"
                        // for unused-variable analysis.
                        let refs = extract_comment_variable_refs(content);
                        if !refs.is_empty() {
                            segments.push(Segment::Comment(refs));
                        }
                        remaining = effective_after;
                    }
                }
            }
        }
    }

    Ok(segments)
}

/// Push a static text segment, skipping empty strings.
fn push_static(segments: &mut Vec<Segment>, text: &str) {
    if !text.is_empty() {
        segments.push(Segment::Static(Cow::Owned(text.to_string())));
    }
}

/// Strip trailing whitespace from the last `Segment::Static` entry.
///
/// Trims back to the last newline (keeping the newline), or removes all
/// trailing whitespace if there is no newline.
fn trim_trailing_whitespace_from_last_static(segments: &mut Vec<Segment>) {
    if let Some(Segment::Static(cow)) = segments.last_mut() {
        let s = cow.to_mut();
        let trimmed = s.trim_end_matches([' ', '\t']);
        // If there's a newline right before the whitespace we just
        // stripped, keep everything up through that newline.
        if trimmed.ends_with('\n') {
            s.truncate(trimmed.len());
        } else {
            // No newline — strip all trailing whitespace including
            // newlines.
            let fully_trimmed = s.trim_end();
            s.truncate(fully_trimmed.len());
        }
        if s.is_empty() {
            segments.pop();
        }
    }
}

/// Strip leading whitespace (spaces, tabs) and at most one newline from
/// `text`. This is used for `-%}` / `-}}` whitespace control.
fn trim_leading_whitespace_through_newline(text: &str) -> &str {
    let after_ws = text.trim_start_matches([' ', '\t']);
    if let Some(rest) = after_ws.strip_prefix('\n') {
        rest
    } else if let Some(rest) = after_ws.strip_prefix("\r\n") {
        rest
    } else {
        after_ws
    }
}

/// Extract variable names referenced inside a comment via `{{ var }}` syntax.
///
/// Comments are stripped from rendered output, but variables mentioned in
/// them count as "used" for the unused-variable check. This enables the
/// reload/stable-frontmatter pattern: declare a variable in frontmatter,
/// reference it in a comment to suppress the unused-variable error.
fn extract_comment_variable_refs(content: &str) -> Vec<Cow<'static, str>> {
    let mut refs = Vec::new();
    let mut search = content;
    while let Some(start) = search.find(crate::consts::EXPR_START) {
        let rest = &search[start + crate::consts::EXPR_START.len()..];
        if let Some(end) = rest.find(crate::consts::EXPR_END) {
            let inner = rest[..end].trim();
            // Extract just the root variable name (before any `.` or `|`).
            let root = inner
                .split([
                    crate::consts::PATH_SEP,
                    crate::consts::PIPE,
                    crate::consts::PAREN_OPEN,
                ])
                .next()
                .unwrap_or("")
                .trim();
            if !root.is_empty() && crate::consts::strip_string_literal(root).is_none() {
                refs.push(Cow::Owned(root.to_string()));
            }
            search = &rest[end + crate::consts::EXPR_END.len()..];
        } else {
            break;
        }
    }
    refs
}

/// Compile an expression tag into a `Segment::Expr`.
fn compile_expr(expr: &str) -> Result<Segment, TemplateError> {
    let parts: Vec<&str> = expr.splitn(2, crate::consts::PIPE).collect();
    let path = Cow::Owned(parts[0].trim().to_string());
    let mut filters = Vec::new();

    if parts.len() > 1 {
        for filter_str in parts[1].split(crate::consts::PIPE) {
            let filter_str = filter_str.trim();
            if filter_str.is_empty() {
                continue;
            }
            let (name, args) = crate::filter::parse_filter(filter_str);
            let kind = parse_filter_kind(name)?;
            filters.push(ParsedFilter {
                kind,
                args: args.map(|a| Cow::Owned(a.to_string())),
            });
        }
    }

    Ok(Segment::Expr { path, filters })
}

/// Resolve a filter name to a strongly-typed [`FilterKind`].
pub(crate) fn parse_filter_kind(name: &str) -> Result<FilterKind, TemplateError> {
    use crate::consts::{
        FILTER_ADD, FILTER_FIXED, FILTER_JOIN, FILTER_LIMIT, FILTER_LOWER, FILTER_SUB, FILTER_TRIM,
        FILTER_UPPER,
    };
    match name {
        FILTER_UPPER => Ok(FilterKind::Upper),
        FILTER_LOWER => Ok(FilterKind::Lower),
        FILTER_TRIM => Ok(FilterKind::Trim),
        FILTER_FIXED => Ok(FilterKind::Fixed),
        FILTER_JOIN => Ok(FilterKind::Join),
        FILTER_LIMIT => Ok(FilterKind::Limit),
        FILTER_ADD => Ok(FilterKind::Add),
        FILTER_SUB => Ok(FilterKind::Sub),
        _ => Err(TemplateError::UnknownFilter(name.to_string())),
    }
}

/// Compile a statement tag, consuming the body for block statements.
///
/// Returns `(segment, remaining_text_after_block)`.
fn compile_statement<'a>(
    stmt: &str,
    after_tag: &'a str,
) -> Result<(Segment, &'a str), TemplateError> {
    if let Some(stmt_body) = stmt.strip_prefix(TAG_FOR_PREFIX) {
        compile_for_loop(stmt_body, after_tag)
    } else if let Some(condition) = stmt.strip_prefix(TAG_IF_PREFIX) {
        compile_conditional(condition.trim(), after_tag)
    } else if let Some(match_body) = stmt.strip_prefix(TAG_MATCH_PREFIX) {
        compile_match(match_body.trim(), after_tag)
    } else if stmt == KW_RAW {
        compile_raw_block(after_tag, KW_RAW, CLOSE_RAW)
    } else if let Some(delim) = stmt.strip_prefix(KW_RAW_ASSIGN) {
        let delim = delim.trim();
        if delim.is_empty() {
            return Err(TemplateError::syntax(format!(
                "{STMT_START} {KW_RAW_ASSIGN} {STMT_END} requires a delimiter after '='"
            )));
        }
        compile_raw_block(
            after_tag,
            &format!("{KW_RAW_ASSIGN}{delim}"),
            &format!("/{delim}"),
        )
    } else if stmt.starts_with(TAG_INCLUDE_PREFIX) {
        compile_include(stmt, after_tag)
    } else if stmt == KW_ELSE
        || stmt.starts_with(TAG_ELIF_PREFIX)
        || stmt == CLOSE_IF
        || stmt == CLOSE_FOR
        || stmt == CLOSE_RAW
        || stmt == CLOSE_MATCH
        || stmt.starts_with(TAG_CASE_PREFIX)
        || stmt == KW_DEFAULT
    {
        Err(TemplateError::syntax(format!(
            "unexpected '{{% {stmt} %}}' without matching opening tag"
        )))
    } else if stmt == LEGACY_ENDIF || stmt == LEGACY_ENDFOR || stmt == LEGACY_ENDRAW {
        let new_tag = stmt.replacen("end", "/", 1);
        Err(TemplateError::syntax(format!(
            "'{{% {stmt} %}}' is not valid — use '{{% {new_tag} %}}' instead"
        )))
    } else {
        Err(TemplateError::syntax(format!(
            "unknown statement: '{stmt}'"
        )))
    }
}

/// Compile a for-loop block.
fn compile_for_loop<'a>(
    stmt_body: &str,
    after_tag: &'a str,
) -> Result<(Segment, &'a str), TemplateError> {
    let (binding, list_path) = parser::parse_for_tag(stmt_body)?;
    let (body_text, rest) = parser::find_closing_block(after_tag, TAG_FOR_PREFIX, CLOSE_FOR)?;
    let body = compile_body(body_text).map_err(|e| enrich_error(e, body_text))?;

    Ok((
        Segment::ForLoop {
            binding: Cow::Owned(binding.to_string()),
            list_path: Cow::Owned(list_path.to_string()),
            body,
        },
        rest,
    ))
}

/// Compile a conditional block (with elif chain support).
fn compile_conditional<'a>(
    condition: &str,
    after_tag: &'a str,
) -> Result<(Segment, &'a str), TemplateError> {
    let (body_text, rest) = parser::find_closing_block(after_tag, TAG_IF_PREFIX, CLOSE_IF)?;
    let (raw_branches, else_text) = parser::split_elif_chain(body_text);

    let mut branches = Vec::with_capacity(raw_branches.len());
    for (i, &(cond_str, branch_body)) in raw_branches.iter().enumerate() {
        let cond = if i == 0 {
            // First branch uses the condition from the opening `{% if ... %}` tag.
            analysis::parse_condition(condition)
        } else {
            analysis::parse_condition(cond_str)
        };
        branches.push((
            cond,
            compile_body(branch_body).map_err(|e| enrich_error(e, branch_body))?,
        ));
    }

    let else_body = match else_text {
        Some(text) => compile_body(text).map_err(|e| enrich_error(e, text))?,
        None => Vec::new(),
    };

    Ok((
        Segment::If {
            branches,
            else_body,
        },
        rest,
    ))
}

/// Compile a raw block (the body is kept as literal text).
///
/// Supports custom delimiters: `{% raw=# %}...{% /# %}` uses
/// `open_keyword="raw=#"` and `close_keyword="/#"`.
fn compile_raw_block<'a>(
    after_tag: &'a str,
    open_keyword: &str,
    close_keyword: &str,
) -> Result<(Segment, &'a str), TemplateError> {
    let (body, rest) = parser::find_closing_block(after_tag, open_keyword, close_keyword)?;
    Ok((Segment::Raw(Cow::Owned(body.to_string())), rest))
}

/// Compile a match block.
///
/// Supports two forms:
/// - **Inline**: `{% match expr case Variant %}body{% /match %}` — single-arm
/// - **Multi-arm**: `{% match expr %}{% case A %}...{% case B %}...{% /match %}`
fn compile_match<'a>(
    match_body: &str,
    after_tag: &'a str,
) -> Result<(Segment, &'a str), TemplateError> {
    // Check for inline form: `match expr case Variant`
    let (expr, inline_variant) = if let Some(case_pos) = match_body.find(" case ") {
        let expr = match_body[..case_pos].trim();
        let variant = match_body[case_pos + " case ".len()..].trim();
        if variant.is_empty() {
            return Err(TemplateError::syntax(
                "match: empty variant name after 'case'".to_string(),
            ));
        }
        (expr, Some(variant))
    } else {
        (match_body, None)
    };

    if expr.is_empty() {
        return Err(TemplateError::syntax(
            "match: missing expression".to_string(),
        ));
    }

    let (body_text, rest) = parser::find_closing_block(after_tag, TAG_MATCH_PREFIX, CLOSE_MATCH)?;

    if let Some(variant) = inline_variant {
        // Inline form: entire body is one arm.
        let body = compile_body(body_text).map_err(|e| enrich_error(e, body_text))?;
        Ok((
            Segment::Match {
                expr: Cow::Owned(expr.to_string()),
                arms: vec![(
                    variant
                        .split(crate::consts::PIPE)
                        .map(|v| Cow::Owned(v.trim().to_string()))
                        .collect(),
                    body,
                )],
            },
            rest,
        ))
    } else {
        // Multi-arm form: split body at `{% case Variant %}` tags.
        let arms = split_match_arms(body_text)?;
        if arms.is_empty() {
            return Err(TemplateError::syntax(
                "match: no {% case %} arms found".to_string(),
            ));
        }
        Ok((
            Segment::Match {
                expr: Cow::Owned(expr.to_string()),
                arms,
            },
            rest,
        ))
    }
}

/// Split a match block body into `(variant_name, body_segments)` arms.
///
/// Scans for `{% case Variant %}` tags at the top level (respecting nesting
/// of inner match blocks). Text before the first `{% case %}` is discarded
/// (whitespace only).
fn split_match_arms(body: &str) -> Result<MatchArms, TemplateError> {
    let mut arms = Vec::new();
    let mut remaining = body;
    let mut has_default = false;

    // Skip whitespace before the first {% case %}.
    remaining = remaining.trim_start();

    // If remaining is empty, no arms.
    if remaining.is_empty() {
        return Ok(arms);
    }

    loop {
        // Find the next {% case Variant %} or {% default %} tag.
        let scan = parser::scan_next_tag(remaining)?;
        match scan {
            parser::ScanResult::Literal(_) => break,
            parser::ScanResult::Found {
                before,
                tag: parser::Tag::Stmt(stmt),
                after,
                ..
            } => {
                // Only whitespace allowed before the first case.
                if !before.trim().is_empty() && arms.is_empty() {
                    return Err(TemplateError::syntax(
                        "match: unexpected text before first {% case %}".to_string(),
                    ));
                }

                if let Some(variant) = stmt.strip_prefix(TAG_CASE_PREFIX) {
                    // {% case %} after {% default %} is not allowed.
                    if has_default {
                        return Err(TemplateError::syntax(
                            "match: {% case %} after {% default %} is not allowed".to_string(),
                        ));
                    }

                    let variant = variant.trim();
                    if variant.is_empty() {
                        return Err(TemplateError::syntax(
                            "match: empty variant name in {% case %}".to_string(),
                        ));
                    }

                    // Scan forward to find the next {% case %}, {% default %}, or end.
                    let arm_body = scan_to_next_case_or_end(after)?;
                    let arm_segments =
                        compile_body(arm_body).map_err(|e| enrich_error(e, arm_body))?;
                    let variants = variant
                        .split(crate::consts::PIPE)
                        .map(|v| Cow::Owned(v.trim().to_string()))
                        .collect();
                    arms.push((variants, arm_segments));

                    remaining = &after[arm_body.len()..];
                    if remaining.is_empty() {
                        break;
                    }
                } else if stmt == KW_DEFAULT {
                    if has_default {
                        return Err(TemplateError::syntax(
                            "match: only one {% default %} arm is allowed".to_string(),
                        ));
                    }
                    has_default = true;

                    // Scan forward to find the next {% case %}, {% default %}, or end.
                    let arm_body = scan_to_next_case_or_end(after)?;
                    let arm_segments =
                        compile_body(arm_body).map_err(|e| enrich_error(e, arm_body))?;
                    arms.push((vec![Cow::Borrowed("_")], arm_segments));

                    remaining = &after[arm_body.len()..];
                    if remaining.is_empty() {
                        break;
                    }
                } else {
                    return Err(TemplateError::syntax(format!(
                        "match: expected '{{% case Variant %}}', got '{{% {stmt} %}}'"
                    )));
                }
            }
            parser::ScanResult::Found { .. } => {
                return Err(TemplateError::syntax(
                    "match: expected {% case %} tag".to_string(),
                ));
            }
        }
    }

    Ok(arms)
}

/// Scan forward from `input` until we find a top-level `{% case %}` tag
/// (respecting nesting of inner match blocks). Returns the text slice
/// that constitutes the arm body.
fn scan_to_next_case_or_end(input: &str) -> Result<&str, TemplateError> {
    let mut depth: u32 = 0;
    let mut search_from: usize = 0;

    while search_from < input.len() {
        let rest = &input[search_from..];
        let scan = parser::scan_next_tag(rest)?;
        match scan {
            parser::ScanResult::Literal(_) => return Ok(input),
            parser::ScanResult::Found {
                before,
                tag: parser::Tag::Stmt(stmt),
                after,
                ..
            } => {
                let tag_pos = search_from + before.len();

                if stmt.starts_with(TAG_MATCH_PREFIX) {
                    depth += 1;
                } else if stmt == CLOSE_MATCH {
                    if depth == 0 {
                        // End of our match block — return everything before.
                        return Ok(&input[..tag_pos]);
                    }
                    depth -= 1;
                } else if (stmt.starts_with(TAG_CASE_PREFIX) || stmt == KW_DEFAULT) && depth == 0 {
                    // Next case/default at our level — return everything before.
                    return Ok(&input[..tag_pos]);
                }

                // Skip past the tag.
                search_from = input.len() - after.len();
            }
            parser::ScanResult::Found { after, .. } => {
                // Expr or comment — skip past the tag.
                search_from = input.len() - after.len();
            }
        }
    }

    Ok(input)
}

/// Compile an include directive.
fn compile_include<'a>(
    stmt: &str,
    after_tag: &'a str,
) -> Result<(Segment, &'a str), TemplateError> {
    let directive = parser::parse_include_tag(stmt)?;

    let with_vars: Vec<(Cow<'static, str>, Cow<'static, str>)> = directive
        .with_vars
        .iter()
        .map(|&(k, v)| (Cow::Owned(k.to_string()), Cow::Owned(v.to_string())))
        .collect();

    let for_each = directive
        .for_each
        .map(|(b, l)| (Cow::Owned(b.to_string()), Cow::Owned(l.to_string())));

    Ok((
        Segment::Include(CompiledInclude {
            path: Cow::Owned(directive.path.to_string()),
            with_vars,
            for_each,
            inline_compiled: None,
        }),
        after_tag,
    ))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_analysis.rs"]
mod tests_analysis;
