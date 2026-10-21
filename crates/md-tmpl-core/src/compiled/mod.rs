//! Pre-compiled template segments.
//!
//! Compiles a template body into a flat instruction list at parse time,
//! so rendering is a simple walk over segments — no string scanning
//! per render call.

pub(crate) mod analysis;
mod block_scan;
mod blockquote;
pub(crate) mod include_sanitize;
mod inline;
mod match_compile;
pub mod render;
pub(crate) mod sanitize_pass;
pub(crate) mod type_check;
pub(crate) mod type_resolve;

use alloc::{
    borrow::Cow,
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

// Re-export submodule items used by the rest of the crate.
pub use analysis::{collect_referenced_params, collect_unquoted_case_labels};
pub use include_sanitize::{
    IncludeContextMemo, IncludeSanitizeContext, apply_include_context,
    check_untrusted_include_paths,
};
use match_compile::compile_match;
#[cfg(feature = "std")]
pub(crate) use render::register_loop_meta;
pub(crate) use render::render_interpolated_str;
#[cfg(feature = "std")]
pub(crate) use render::render_segments;
#[cfg(feature = "std")]
pub(crate) use render::render_segments_into;
#[cfg(not(feature = "std"))]
pub(crate) use render::render_segments_into_no_std;
pub use sanitize_pass::{
    SanitizeFilterMode, apply_enclosing_xml_tags, apply_frontmatter_sanitization,
};
pub use type_check::{
    validate_field_accesses, validate_field_accesses_full, validate_field_accesses_with_opaque,
    validate_match_labels,
};

pub use crate::scope::{CompiledExpr, CompiledPath, ConditionOperand};
use crate::{
    compat::HashMap,
    consts::{
        CLOSE_FOR, CLOSE_IF, CLOSE_MATCH, CLOSE_RAW, KW_ELSE, TAG_CASE_PREFIX, TAG_ELIF_PREFIX,
        TAG_FOR_PREFIX, TAG_IF_PREFIX, TAG_INCLUDE_PREFIX, TAG_MATCH_PREFIX,
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
    /// Resolved constant values from inline frontmatter `consts:` block.
    pub consts: Arc<HashMap<String, crate::value::Value>>,
    /// Imported constants from inline frontmatter `imports:` block.
    pub imported_consts: Arc<HashMap<String, crate::value::Value>>,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single arm in a match block.
///
/// Each arm may match one or more variants: `{% case A | B %}`,
/// and may have an optional guard condition (`&& guard`).
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// Variant names matched by this arm.
    pub variants: Vec<Cow<'static, str>>,
    /// Optional guard condition: `{% case Variant && guard %}`.
    pub guard: Option<Condition>,
    /// Body segments to render when this arm matches.
    pub body: Vec<Segment>,
}

/// A list of match arms.
pub type MatchArms = Vec<MatchArm>;

/// Pre-compiled template instruction.
#[derive(Debug, Clone)]
pub enum Segment {
    /// Static text — just append to output.
    Static(Cow<'static, str>),
    /// Expression `{{ path | filter1 | filter2(arg) }}`.
    Expr {
        expr: CompiledExpr,
        filters: Vec<ParsedFilter>,
    },
    /// `{% for binding in list_expr | filter %}body{% /for %}`.
    ForLoop {
        binding: Cow<'static, str>,
        list_expr: CompiledExpr,
        filters: Vec<ParsedFilter>,
        body: Vec<Segment>,
        else_body: Vec<Segment>,
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
        expr: CompiledPath,
        /// `(variant_name, body)` pairs.
        arms: MatchArms,
        /// `true` when matching on `option(T)` — resolved via `Value::None`
        /// discriminant rather than string-based variant tag lookup.
        /// Computed once at compile time from arm names.
        is_option: bool,
    },
    /// `{% raw %}literal text{% /raw %}`.
    Raw(Cow<'static, str>),
    /// `{% include <path> [with ...] [for ...] %}`.
    Include(CompiledInclude),
    /// `{# comment #}` — produces no output, but variable references
    /// inside the comment are tracked for unused-variable analysis.
    Comment(Vec<Cow<'static, str>>),
    /// `{% panic(...) %}` — aborts template rendering with an error message.
    Panic(Vec<Segment>),
}

/// Pre-compiled condition for `{% if %}` blocks.
///
/// Parsed once at compile time so rendering never re-scans the condition
/// string for operators.  Supports recursive boolean expressions with
/// `&&`, `||`, `!`, and `()` grouping.
#[derive(Debug, Clone)]
pub enum Condition {
    /// Simple truthiness check: `{% if show %}` or `{% if has(opt) %}`.
    Truthy(ConditionOperand),
    /// Unary negation: `{% if !show %}` or `{% if !(a > 0) %}`.
    Not(Box<Condition>),
    /// Binary comparison: `{% if count > 0 %}`.
    Comparison {
        left: ConditionOperand,
        op: ComparisonOp,
        right: ConditionOperand,
    },
    /// Logical AND (short-circuit): `{% if a > 0 && b > 0 %}`.
    And(Box<Condition>, Box<Condition>),
    /// Logical OR (short-circuit): `{% if a > 0 || b > 0 %}`.
    Or(Box<Condition>, Box<Condition>),
    /// Match-as-condition: `{% if match status case Active %}`.
    MatchVariant {
        /// Expression path to match on.
        expr: CompiledPath,
        /// Variant names to check against.
        variants: Vec<Cow<'static, str>>,
        /// `true` when matching on `option(T)` values.
        is_option: bool,
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
    /// `in`
    In,
}

/// A pre-parsed filter with typed kind and optional argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFilter {
    /// The resolved filter kind.
    pub kind: FilterKind,
    /// Optional argument string (e.g. the separator for `join`).
    pub args: Option<Cow<'static, str>>,
    /// Pre-parsed numeric argument for filters like `fixed(n)`, `limit(n)`, `add(n)`, `sub(n)`.
    /// Avoids repeated `str::parse::<usize>()` on every render call.
    pub parsed_num: Option<usize>,
    /// Pre-resolved execution mode for [`FilterKind::Sanitize`].
    pub sanitize_mode: Option<SanitizeFilterMode>,
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
    /// `| escape_xml` / `| xml` — escape XML entities.
    EscapeXml,
    /// `| escape_json` — escape JSON string characters.
    EscapeJson,
    /// `| tojson` / `| json` — serialize value to valid JSON string with optional indentation.
    ToJson,
    /// `| sanitize_tokens` — neutralize LLM control tokens.
    SanitizeTokens,
    /// `| fence(lang)` — wrap in adaptive $N$-backtick code fence.
    Fence,
    /// `| quarantine(tag)` — wrap in XML quarantine tags.
    Quarantine,
    /// `| sanitize` / `| sanitize(tag)` / `| sanitize(tag, notice)` — unified untrusted-data sanitization.
    Sanitize,
    /// `| truncate(limit)` / `| truncate(limit, marker)` — middle-out truncation.
    Truncate,
}

impl FilterKind {
    /// Canonical filter name as used in template syntax.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Upper => crate::consts::FILTER_UPPER,
            Self::Lower => crate::consts::FILTER_LOWER,
            Self::Trim => crate::consts::FILTER_TRIM,
            Self::Fixed => crate::consts::FILTER_FIXED,
            Self::Join => crate::consts::FILTER_JOIN,
            Self::Limit => crate::consts::FILTER_LIMIT,
            Self::Add => crate::consts::FILTER_ADD,
            Self::Sub => crate::consts::FILTER_SUB,
            Self::EscapeXml => crate::consts::FILTER_ESCAPE_XML,
            Self::EscapeJson => crate::consts::FILTER_ESCAPE_JSON,
            Self::ToJson => crate::consts::FILTER_TOJSON,
            Self::SanitizeTokens => crate::consts::FILTER_SANITIZE_TOKENS,
            Self::Fence => crate::consts::FILTER_FENCE,
            Self::Quarantine => crate::consts::FILTER_QUARANTINE,
            Self::Sanitize => crate::consts::FILTER_SANITIZE,
            Self::Truncate => crate::consts::FILTER_TRUNCATE,
        }
    }
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
    /// Sanitization context captured at this call site for includes resolved at render
    /// time (untrusted `with` / `for` taint, enclosing XML tags, heading state).
    pub sanitize_ctx: Option<Arc<IncludeSanitizeContext>>,
    /// Per-call-site memo of contextualized child segments (see [`IncludeContextMemo`]).
    pub memo: IncludeContextMemo,
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
    let empty_sanitize = HashMap::new();
    compile_with_parent_scope(input, parent_type_aliases, &empty_sanitize)
}

/// Compile a template body into a list of pre-parsed segments, propagating
/// both `parent_type_aliases` and `parent_type_alias_sanitize` into inline
/// `{% tmpl %}` definitions.
///
/// # Errors
///
/// Returns [`TemplateError::Syntax`] on malformed tags or blocks.
pub fn compile_with_parent_scope(
    input: &str,
    parent_type_aliases: &HashMap<String, crate::types::VarType>,
    parent_type_alias_sanitize: &HashMap<String, HashMap<String, crate::types::SanitizeSpec>>,
) -> Result<(Vec<Segment>, HashMap<String, CompiledInlineTemplate>), TemplateError> {
    // Validate that line-start statement tags carry the `>` blockquote prefix,
    // then strip that prefix from statement-tag lines.
    let processed = blockquote::preprocess_blockquotes(input)?;
    // Extract inline template definitions before compiling the body.
    let (cleaned, inline_templates) = inline::extract_inline_templates_with_sanitize(
        &processed,
        parent_type_aliases,
        parent_type_alias_sanitize,
    )?;
    let mut segments = compile_body(&cleaned)?;
    inline::bind_inline_includes(
        &mut segments,
        &inline_templates,
        &mut crate::compat::HashSet::new(),
    );
    apply_enclosing_xml_tags(&mut segments);
    Ok((segments, inline_templates))
}

/// Attach a **file-absolute** line number to an error raised while compiling `body`,
/// the template body that follows the frontmatter in `source`.
///
/// Errors that already carry a body-relative line (e.g. blockquote validation) are
/// shifted by the number of lines preceding the body; other syntax errors are located
/// by searching the body for the offending text. Runs only on the error path, so it
/// has no cost for successful compiles.
#[must_use]
pub fn locate_error_in_source(err: TemplateError, source: &str, body: &str) -> TemplateError {
    let offset = if source.ends_with(body) {
        source.len() - body.len()
    } else {
        source.find(body).unwrap_or(0)
    };
    let base = source[..offset].bytes().filter(|&b| b == b'\n').count();
    match enrich_error(err, &source[offset..]) {
        TemplateError::Syntax(mut syn) => {
            if let Some(line) = syn.line.as_mut() {
                *line += base;
            }
            TemplateError::Syntax(syn)
        }
        other => other,
    }
}

/// [`compile_with_parent_scope`] for a template `body` sliced from `source`, reporting
/// compile errors with file-absolute line numbers (see [`locate_error_in_source`]).
///
/// # Errors
///
/// Returns [`TemplateError::Syntax`] on malformed tags or blocks.
pub fn compile_source_body(
    source: &str,
    body: &str,
    parent_type_aliases: &HashMap<String, crate::types::VarType>,
    parent_type_alias_sanitize: &HashMap<String, HashMap<String, crate::types::SanitizeSpec>>,
) -> Result<(Vec<Segment>, HashMap<String, CompiledInlineTemplate>), TemplateError> {
    compile_with_parent_scope(body, parent_type_aliases, parent_type_alias_sanitize)
        .map_err(|e| locate_error_in_source(e, source, body))
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
pub(super) fn enrich_error(err: TemplateError, source: &str) -> TemplateError {
    match err {
        TemplateError::Syntax(ref syn) if syn.line.is_none() => {
            // Try to locate the problematic text in the source.
            let hint = extract_error_hint(&syn.message);
            if let Some(offset) = hint.and_then(|h| source.find(h)) {
                let line_num = line_at_offset(source, offset);
                let line_text = extract_line(source, line_num).trim();
                let snippet = if line_text.len() > SNIPPET_MAX_LEN {
                    // Truncate at a character boundary to avoid panicking on multi-byte UTF-8.
                    let truncate_at =
                        crate::error::floor_char_boundary(line_text, SNIPPET_MAX_LEN - 3);
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
    // "... instead of '{% if x %}'" -> search for `{% if x %}`
    if let Some(start) = msg.find(" instead of '")
        && let Some(hint) = msg[start + " instead of '".len()..].strip_suffix('\'')
        && !hint.is_empty()
    {
        return Some(hint);
    }
    // "filter 'lower' cannot follow 'quarantine': ..." -> search for `lower`
    if let Some(inner) = msg.strip_prefix("filter '")
        && let Some(end) = inner.find("' cannot follow '")
    {
        return Some(&inner[..end]);
    }
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
pub fn compile_body(input: &str) -> Result<Vec<Segment>, TemplateError> {
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

/// Push a static text segment, coalescing with the previous one if possible.
///
/// Adjacent `Static` segments are common after comment/tag stripping.
/// Merging them at compile time reduces per-segment dispatch overhead
/// in the render loop.
fn push_static(segments: &mut Vec<Segment>, text: &str) {
    if text.is_empty() {
        return;
    }
    // Coalesce with previous Static segment if possible.
    if let Some(Segment::Static(cow)) = segments.last_mut() {
        cow.to_mut().push_str(text);
        return;
    }
    segments.push(Segment::Static(Cow::Owned(text.to_string())));
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

/// Parse a pipe-separated filter chain string into [`ParsedFilter`]s.
///
/// # Errors
/// Returns [`TemplateError::UnknownFilter`] for unknown filter names, and
/// [`TemplateError::Syntax`] when a case-mapping or truncating filter follows a
/// sanitizing filter (see [`check_post_sanitize_filter`]).
fn parse_filter_chain(filter_chain: &str) -> Result<Vec<ParsedFilter>, TemplateError> {
    let mut filters = Vec::new();
    if !filter_chain.is_empty() {
        let mut sanitizer: Option<&str> = None;
        for filter_str in crate::parser::split_filters_aware(filter_chain) {
            let filter_str = filter_str.trim();
            if filter_str.is_empty() {
                continue;
            }
            let (name, args) = crate::filter::parse_filter(filter_str);
            let kind = parse_filter_kind(name)?;
            check_post_sanitize_filter(sanitizer, kind, name)?;
            if matches!(
                kind,
                FilterKind::Sanitize | FilterKind::SanitizeTokens | FilterKind::Quarantine
            ) {
                sanitizer = Some(name);
            }
            filters.push(ParsedFilter::parse(kind, args)?);
        }
    }
    Ok(filters)
}

/// Reject `upper` / `lower` / `truncate` after a sanitizing filter
/// (`sanitize`, `sanitize_tokens`, `quarantine`).
///
/// Case mapping can re-create control tokens that only exist in one case
/// (e.g. `<TOOL_CALL>` → `<tool_call>`), and truncation can cut the closing
/// boundary tag of a sanitized block, so these must run *before* sanitization.
///
/// # Errors
/// Returns [`TemplateError::Syntax`] describing the required reordering.
pub(crate) fn check_post_sanitize_filter(
    sanitizer: Option<&str>,
    kind: FilterKind,
    name: &str,
) -> Result<(), TemplateError> {
    if let Some(sanitizer) = sanitizer
        && matches!(
            kind,
            FilterKind::Upper | FilterKind::Lower | FilterKind::Truncate
        )
    {
        return Err(TemplateError::syntax(format!(
            "filter '{name}' cannot follow '{sanitizer}': case mapping or truncation after sanitization can re-create control tokens or cut boundary tags; apply '{name}' before '{sanitizer}'"
        )));
    }
    Ok(())
}

/// Compile an expression tag into a `Segment::Expr`.
fn compile_expr(expr: &str) -> Result<Segment, TemplateError> {
    let (base_expr, filter_chain) = crate::parser::split_pipe_aware(expr);
    let expr_obj = CompiledExpr::compile(base_expr)?;
    let filters = parse_filter_chain(filter_chain)?;

    Ok(Segment::Expr {
        expr: expr_obj,
        filters,
    })
}

/// Resolve a filter name to a strongly-typed [`FilterKind`].
pub(crate) fn parse_filter_kind(name: &str) -> Result<FilterKind, TemplateError> {
    use crate::consts::{
        FILTER_ADD, FILTER_ESCAPE_JSON, FILTER_ESCAPE_XML, FILTER_FENCE, FILTER_FIXED, FILTER_JOIN,
        FILTER_JSON, FILTER_LIMIT, FILTER_LOWER, FILTER_QUARANTINE, FILTER_SANITIZE,
        FILTER_SANITIZE_TOKENS, FILTER_SUB, FILTER_TO_JSON, FILTER_TOJSON, FILTER_TRIM,
        FILTER_UPPER, FILTER_XML,
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
        FILTER_ESCAPE_XML | FILTER_XML => Ok(FilterKind::EscapeXml),
        FILTER_ESCAPE_JSON => Ok(FilterKind::EscapeJson),
        FILTER_TOJSON | FILTER_TO_JSON | FILTER_JSON => Ok(FilterKind::ToJson),
        FILTER_SANITIZE_TOKENS => Ok(FilterKind::SanitizeTokens),
        FILTER_FENCE => Ok(FilterKind::Fence),
        FILTER_QUARANTINE => Ok(FilterKind::Quarantine),
        FILTER_SANITIZE => Ok(FilterKind::Sanitize),
        crate::consts::FILTER_TRUNCATE | crate::consts::FILTER_TRUNCATE_MIDDLE => {
            Ok(FilterKind::Truncate)
        }
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
    } else if let Some(delimiters) = block_scan::RawBlockDelimiters::parse(stmt)? {
        compile_raw_block(after_tag, &delimiters)
    } else if stmt.starts_with(TAG_INCLUDE_PREFIX) {
        compile_include(stmt, after_tag)
    } else if stmt.starts_with(crate::consts::TAG_PANIC_PAREN)
        || stmt.starts_with(crate::consts::TAG_PANIC_PREFIX)
        || stmt == crate::consts::KW_PANIC
    {
        let panic_arg = if stmt.starts_with(crate::consts::TAG_PANIC_PAREN) {
            &stmt[5..]
        } else {
            stmt.strip_prefix(crate::consts::TAG_PANIC_PREFIX)
                .unwrap_or("")
        };
        compile_panic(panic_arg, after_tag)
    } else if stmt == KW_ELSE
        || stmt.starts_with(TAG_ELIF_PREFIX)
        || stmt == CLOSE_IF
        || stmt == CLOSE_FOR
        || stmt == CLOSE_RAW
        || stmt == CLOSE_MATCH
        || stmt.starts_with(TAG_CASE_PREFIX)
    {
        Err(TemplateError::syntax(format!(
            "unexpected '{{% {stmt} %}}' without matching opening tag"
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
    let (base_path, filter_str) = parser::split_pipe_aware(list_path);
    let list_compiled = CompiledExpr::compile(base_path.trim())?;
    let filters = parse_filter_chain(filter_str)?;
    for f in &filters {
        if !matches!(f.kind, FilterKind::Limit | FilterKind::Truncate) {
            return Err(TemplateError::syntax(format!(
                "for-loop iterable: filter '{}' does not return a list (only 'limit' and 'truncate' are valid on for-loop iterables)",
                f.kind.as_str()
            )));
        }
    }
    let (body_text, rest) = parser::find_closing_block(after_tag, TAG_FOR_PREFIX, CLOSE_FOR)?;

    // Split body at {% else %} if present (respecting nesting).
    let (for_text, else_text) = block_scan::split_for_else(body_text)?;
    let body = compile_body(for_text)?;
    let else_body = match else_text {
        Some(text) => compile_body(text)?,
        None => Vec::new(),
    };

    Ok((
        Segment::ForLoop {
            binding: Cow::Owned(binding.to_string()),
            list_expr: list_compiled,
            filters,
            body,
            else_body,
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
    let chain = block_scan::split_elif_chain(body_text)?;

    let mut branches = Vec::with_capacity(chain.branches.len());
    for (i, &(cond_str, branch_body)) in chain.branches.iter().enumerate() {
        let cond = if i == 0 {
            // First branch uses the condition from the opening `{% if ... %}` tag.
            analysis::parse_condition(condition)?
        } else {
            analysis::parse_condition(cond_str)?
        };
        branches.push((cond, compile_body(branch_body)?));
    }

    let else_body = match chain.else_body {
        Some(text) => compile_body(text)?,
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
/// Supports custom delimiters: `{% raw=# %}...{% /# %}`.
fn compile_raw_block<'a>(
    after_tag: &'a str,
    delimiters: &block_scan::RawBlockDelimiters,
) -> Result<(Segment, &'a str), TemplateError> {
    let (body, rest) = delimiters.split_body(after_tag)?;
    Ok((Segment::Raw(Cow::Owned(body.to_string())), rest))
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
            sanitize_ctx: None,
            memo: IncludeContextMemo::default(),
        }),
        after_tag,
    ))
}

/// Compile a panic directive.
fn compile_panic<'a>(
    panic_arg: &str,
    after_tag: &'a str,
) -> Result<(Segment, &'a str), TemplateError> {
    let mut arg = panic_arg.trim();
    if let Some(stripped) = arg.strip_prefix('(') {
        if let Some(inner) = stripped.strip_suffix(')') {
            arg = inner.trim();
        } else {
            return Err(TemplateError::syntax(
                "panic(...) is missing closing parenthesis",
            ));
        }
    }
    if arg.is_empty() {
        return Err(TemplateError::syntax(
            "panic statement requires an argument",
        ));
    }
    let body_src = if (arg.starts_with('"') && arg.ends_with('"') && arg.len() >= 2)
        || (arg.starts_with('\'') && arg.ends_with('\'') && arg.len() >= 2)
    {
        alloc::borrow::Cow::Borrowed(&arg[1..arg.len() - 1])
    } else {
        alloc::borrow::Cow::Owned(alloc::format!("{{{{ {arg} }}}}"))
    };
    let segments = compile_body(&body_src)?;
    Ok((Segment::Panic(segments), after_tag))
}

#[cfg(all(test, feature = "std"))]
mod test_helpers;
#[cfg(all(test, feature = "std"))]
mod tests_blockquote;
#[cfg(all(test, feature = "std"))]
mod tests_compile;
#[cfg(all(test, feature = "std"))]
mod tests_conditions;
#[cfg(all(test, feature = "std"))]
mod tests_expressions;
#[cfg(all(test, feature = "std"))]
mod tests_for_else;
#[cfg(all(test, feature = "std"))]
mod tests_include;
#[cfg(all(test, feature = "std"))]
mod tests_inline_templates;
#[cfg(all(test, feature = "std"))]
mod tests_match;
#[cfg(all(test, feature = "std"))]
mod tests_refs;
#[cfg(all(test, feature = "std"))]
mod tests_render;
#[cfg(all(test, feature = "std"))]
mod tests_whitespace;
