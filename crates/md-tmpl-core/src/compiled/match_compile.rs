//! Compilation of `{% match %}` blocks into pre-compiled [`Segment::Match`] instructions.

use alloc::{borrow::Cow, string::ToString, vec, vec::Vec};

use super::{
    CompiledPath, MatchArm, MatchArms, Segment,
    analysis::parse_condition,
    block_scan::{find_top_level_tag, split_for_else},
    compile_body,
};
use crate::{
    consts::{CLOSE_MATCH, KW_ELSE, TAG_CASE_PREFIX, TAG_MATCH_PREFIX},
    error::TemplateError,
    parser,
};

/// Pre-compiled match-arm case label (`{% case ... %}`).
///
/// Unescapes quoted string literals, pre-compiles embedded `{{ expr }}`
/// interpolations into [`Segment`]s, and pre-splits dotted variable paths once
/// at template compile time so `arm_matches` never scans quotes, unescapes
/// backslashes, splits paths, or invokes [`compile_body`] during rendering.
#[derive(Debug, Clone)]
pub enum CompiledCaseLabel {
    /// Wildcard fallback arm (`_`, emitted for `{% else %}`).
    Default,
    /// Quoted string literal without interpolation (`"Active"`, `'say "hi"'`), unescaped once.
    LiteralStr(Cow<'static, str>),
    /// Quoted string literal containing `{{ expr }}` interpolation, compiled at parse time.
    InterpolatedStr(Vec<Segment>),
    /// Unquoted token: enum variant name, scalar literal, or variable / dotted-path reference.
    Unquoted {
        /// Trimmed raw label token.
        raw: Cow<'static, str>,
        /// Pre-compiled dotted path when `raw` contains `.` (`cfg.expected`, `consts.TAG`).
        dotted_path: Option<CompiledPath>,
    },
}

impl CompiledCaseLabel {
    /// Compile a single trimmed case-label token into a [`CompiledCaseLabel`].
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if an interpolated quoted string contains malformed `{{ ... }}` syntax.
    pub fn compile(label: &str) -> Result<Self, TemplateError> {
        let label = label.trim();
        if label == crate::consts::MATCH_DEFAULT {
            return Ok(Self::Default);
        }
        if let Some(inner) = crate::consts::strip_string_literal(label) {
            let unescaped = crate::consts::unescape_string_literal(inner);
            if unescaped.contains(crate::consts::EXPR_START) {
                let segments = compile_body(&unescaped)?;
                return Ok(Self::InterpolatedStr(segments));
            }
            return Ok(Self::LiteralStr(Cow::Owned(unescaped)));
        }
        let dotted_path = label
            .contains(crate::consts::PATH_SEP)
            .then(|| CompiledPath::compile(label));
        Ok(Self::Unquoted {
            raw: Cow::Owned(label.to_string()),
            dotted_path,
        })
    }

    /// Compile a slice of raw variant strings into [`CompiledCaseLabel`]s.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] if any interpolated label fails compilation.
    pub fn compile_all(variants: &[Cow<'static, str>]) -> Result<Vec<Self>, TemplateError> {
        variants.iter().map(|v| Self::compile(v.as_ref())).collect()
    }
}

/// Split a `{% case ... %}` clause into `(variants_part, optional_guard)` at the
/// first top-level `" && "` outside single or double quotes.
fn split_case_guard(variant: &str) -> (&str, Option<&str>) {
    use crate::consts::{QUOTE_DOUBLE, QUOTE_SINGLE};

    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    for (i, ch) in variant.char_indices() {
        if in_quote.is_some() {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
        }
        match ch {
            QUOTE_DOUBLE | QUOTE_SINGLE if in_quote == Some(ch) => in_quote = None,
            QUOTE_DOUBLE | QUOTE_SINGLE if in_quote.is_none() => in_quote = Some(ch),
            ' ' if in_quote.is_none() && variant[i..].starts_with(" && ") => {
                return (&variant[..i], Some(variant[i + 4..].trim()));
            }
            _ => {}
        }
    }
    (variant, None)
}
///
/// Returns `true` if the match arms use option-style variant names (`Some`/`None`).
///
/// Called once at compile time so the renderer can use a discriminant check
/// (`Value::None` vs not) instead of scanning arm names on every render.
fn arms_are_option(arms: &MatchArms) -> bool {
    arms.iter().any(|arm| {
        arm.variants.iter().any(|v| {
            v.as_ref() == crate::consts::OPTION_SOME || v.as_ref() == crate::consts::OPTION_NONE
        })
    })
}

/// Compile a match block.
///
/// Supports two forms:
/// - **Inline**: `{% match expr case Variant %}body{% /match %}` — single-arm
/// - **Multi-arm**: `{% match expr %}{% case A %}...{% case B %}...{% /match %}`
pub(super) fn compile_match<'a>(
    match_body: &str,
    after_tag: &'a str,
) -> Result<(Segment, &'a str), TemplateError> {
    // Check for inline form: `match expr case Variant`
    let (expr, inline_variant) =
        if let Some(case_pos) = match_body.find(crate::consts::TAG_CASE_SPACED) {
            let expr = match_body[..case_pos].trim();
            let variant = match_body[case_pos + crate::consts::TAG_CASE_SPACED.len()..].trim();
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

    let expr_compiled = CompiledPath::compile(expr);

    let (body_text, rest) = parser::find_closing_block(after_tag, TAG_MATCH_PREFIX, CLOSE_MATCH)?;

    if let Some(variant) = inline_variant {
        // Inline form: check for {% else %} to create a default arm.
        let (case_text, else_text) = split_for_else(body_text)?;
        let case_body = compile_body(case_text)?;

        // Parse guard: `Variant && guard_condition`.
        let (variant_part, guard_str) = split_case_guard(variant);
        let guard = if let Some(g) = guard_str {
            Some(parse_condition(g)?)
        } else {
            None
        };

        let variant_part = variant_part.trim();
        let split_variants = parser::split_filters_aware(variant_part);
        if split_variants
            .iter()
            .any(|v| v.trim() == crate::consts::MATCH_DEFAULT)
        {
            return Err(TemplateError::syntax(
                "match: wildcard '_' in {% case %} is not supported — use {% else %} for fallback"
                    .to_string(),
            ));
        }
        let variants: Vec<Cow<'static, str>> = split_variants
            .into_iter()
            .map(|v| Cow::Owned(v.trim().to_string()))
            .collect();
        let compiled_variants = CompiledCaseLabel::compile_all(&variants)?;
        let mut arms = vec![MatchArm {
            variants,
            compiled_variants,
            guard,
            body: case_body,
        }];
        if let Some(else_body_text) = else_text {
            let else_body = compile_body(else_body_text)?;
            arms.push(MatchArm {
                variants: vec![Cow::Borrowed(crate::consts::MATCH_DEFAULT)],
                compiled_variants: vec![CompiledCaseLabel::Default],
                guard: None,
                body: else_body,
            });
        }
        let is_option = arms_are_option(&arms);
        Ok((
            Segment::Match {
                expr: expr_compiled,
                arms,
                is_option,
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
        let is_option = arms_are_option(&arms);
        Ok((
            Segment::Match {
                expr: expr_compiled,
                arms,
                is_option,
            },
            rest,
        ))
    }
}

fn parse_case_arm<'a>(variant: &str, after: &'a str) -> Result<(MatchArm, &'a str), TemplateError> {
    let (variant_part, guard_str) = split_case_guard(variant);
    let variant_part = variant_part.trim();
    if variant_part.is_empty() {
        return Err(TemplateError::syntax(
            "match: empty variant name in {% case %}".to_string(),
        ));
    }
    let split_variants = parser::split_filters_aware(variant_part);
    if split_variants
        .iter()
        .any(|v| v.trim() == crate::consts::MATCH_DEFAULT)
    {
        return Err(TemplateError::syntax(
            "match: wildcard '_' in {% case %} is not supported — use {% else %} for fallback"
                .to_string(),
        ));
    }

    let arm_body = scan_to_next_case_or_end(after)?;
    let arm_segments = compile_body(arm_body)?;
    let variants: Vec<Cow<'static, str>> = split_variants
        .into_iter()
        .map(|v| Cow::Owned(v.trim().to_string()))
        .collect();
    let compiled_variants = CompiledCaseLabel::compile_all(&variants)?;
    let guard = if let Some(g) = guard_str {
        Some(parse_condition(g)?)
    } else {
        None
    };
    let arm = MatchArm {
        variants,
        compiled_variants,
        guard,
        body: arm_segments,
    };
    Ok((arm, &after[arm_body.len()..]))
}

fn parse_else_arm(after: &str) -> Result<(MatchArm, &str), TemplateError> {
    let arm_body = scan_to_next_case_or_end(after)?;
    let arm_segments = compile_body(arm_body)?;
    let arm = MatchArm {
        variants: vec![Cow::Borrowed(crate::consts::MATCH_DEFAULT)],
        compiled_variants: vec![CompiledCaseLabel::Default],
        guard: None,
        body: arm_segments,
    };
    Ok((arm, &after[arm_body.len()..]))
}

/// Split a match block body into `(variant_name, body_segments)` arms.
///
/// Scans for `{% case Variant %}` tags at the top level (respecting nesting
/// of inner match blocks). Text before the first `{% case %}` is discarded
/// (whitespace only).
fn split_match_arms(body: &str) -> Result<MatchArms, TemplateError> {
    let mut arms = Vec::new();
    let mut remaining = body.trim_start();
    let mut has_default = false;

    if remaining.is_empty() {
        return Ok(arms);
    }

    loop {
        let scan = parser::scan_next_tag(remaining)?;
        match scan {
            parser::ScanResult::Literal(_) => break,
            parser::ScanResult::Found {
                before,
                tag: parser::Tag::Stmt(stmt),
                after,
                ..
            } => {
                if !before.trim().is_empty() && arms.is_empty() {
                    return Err(TemplateError::syntax(
                        "match: unexpected text before first {% case %}".to_string(),
                    ));
                }

                if let Some(variant) = stmt.strip_prefix(TAG_CASE_PREFIX) {
                    if has_default {
                        return Err(TemplateError::syntax(
                            "match: {% case %} after {% else %} is not allowed".to_string(),
                        ));
                    }
                    let (arm, next_remaining) = parse_case_arm(variant, after)?;
                    arms.push(arm);
                    remaining = next_remaining;
                    if remaining.is_empty() {
                        break;
                    }
                } else if stmt == KW_ELSE {
                    if has_default {
                        return Err(TemplateError::syntax(
                            "match: only one {% else %} arm is allowed".to_string(),
                        ));
                    }
                    has_default = true;
                    let (arm, next_remaining) = parse_else_arm(after)?;
                    arms.push(arm);
                    remaining = next_remaining;
                    if remaining.is_empty() {
                        break;
                    }
                } else {
                    return Err(TemplateError::syntax(alloc::format!(
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

/// Return the arm body at the start of `input`: everything up to the next
/// `{% case %}` / `{% else %}` arm of this match (nested `if`, `for`,
/// `match`, and `raw` blocks are skipped), or all of `input` for the last arm.
fn scan_to_next_case_or_end(input: &str) -> Result<&str, TemplateError> {
    let next_arm = find_top_level_tag(input, |stmt| {
        stmt.starts_with(TAG_CASE_PREFIX) || stmt == KW_ELSE
    })?;
    Ok(next_arm.map_or(input, |tag| &input[..tag.start]))
}
