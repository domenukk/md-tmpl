//! Rendering of compiled `{% match %}` blocks and variant resolution.

use alloc::{borrow::Cow, string::String};

use super::{condition::eval_condition, segments::render_interpolated_str};
use crate::{
    compiled::{CompiledCaseLabel, MatchArm},
    error::TemplateError,
    scope::{CompiledPath, Scope},
    value::Value,
};

/// Render a compiled match block.
///
/// Resolves the expression to determine the active enum variant, then
/// renders the body of the matching `{% case %}` arm. Silently produces
/// no output if no arm matches (non-exhaustive matches are caught at
/// compile time by the type system).
#[cfg(feature = "std")]
pub(super) fn render_match(
    expr: &CompiledPath,
    arms: &[MatchArm],
    is_option: bool,
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let active_variant = resolve_match_variant(expr, is_option, scope)?;

    for arm in arms {
        if arm_matches(&active_variant, &arm.compiled_variants, scope)? {
            // Evaluate guard if present.
            if let Some(ref guard) = arm.guard {
                if !eval_condition(guard, scope)? {
                    continue;
                }
            }
            // Narrow option so inner kind()/match see the unwrapped value.
            let narrowed = is_option
                && active_variant == crate::consts::OPTION_SOME
                && arm.variants.len() == 1
                && arm.variants[0].as_ref() == crate::consts::OPTION_SOME;
            if narrowed {
                scope.narrow_option(expr.as_str());
            }
            let result = super::segments::render_segments_into(&arm.body, scope, base_dir, output);
            if narrowed {
                scope.unnarrow_option(expr.as_str());
            }
            return result;
        }
    }

    // No arm matched — silently skip (non-exhaustive is valid for
    // inline `{% match x case Y %}` single-arm guards).
    Ok(())
}

/// Render a compiled match block (`no_std` variant).
#[cfg(not(feature = "std"))]
pub(super) fn render_match_no_std(
    expr: &CompiledPath,
    arms: &[MatchArm],
    is_option: bool,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let active_variant = resolve_match_variant(expr, is_option, scope)?;

    for arm in arms {
        if arm_matches(&active_variant, &arm.compiled_variants, scope)? {
            if let Some(ref guard) = arm.guard {
                if !eval_condition(guard, scope)? {
                    continue;
                }
            }
            let narrowed = is_option
                && active_variant == crate::consts::OPTION_SOME
                && arm.variants.len() == 1
                && arm.variants[0].as_ref() == crate::consts::OPTION_SOME;
            if narrowed {
                scope.narrow_option(expr.as_str());
            }
            let result = super::segments::render_segments_into_no_std(&arm.body, scope, output);
            if narrowed {
                scope.unnarrow_option(expr.as_str());
            }
            return result;
        }
    }

    Ok(())
}

/// Check if any pre-compiled arm variant matches the active variant.
///
/// Matching rules:
/// - [`CompiledCaseLabel::Default`] (`_`): always matches
/// - [`CompiledCaseLabel::LiteralStr`]: pre-unescaped literal comparison
/// - [`CompiledCaseLabel::InterpolatedStr`]: evaluate pre-compiled [`Segment`]s and compare
/// - [`CompiledCaseLabel::Unquoted`]: compare literally (enum variant / scalar) or resolve
///   from scope as a string variable / pre-compiled dotted path
fn arm_matches(
    active_variant: &str,
    variants: &[CompiledCaseLabel],
    scope: &Scope<'_>,
) -> Result<bool, TemplateError> {
    for v in variants {
        match v {
            CompiledCaseLabel::Default => return Ok(true),
            CompiledCaseLabel::LiteralStr(lit) => {
                if active_variant == lit.as_ref() {
                    return Ok(true);
                }
            }
            CompiledCaseLabel::InterpolatedStr(segments) => {
                let rendered = render_interpolated_str(segments, scope)?;
                if rendered == active_variant {
                    return Ok(true);
                }
            }
            CompiledCaseLabel::Unquoted { raw, dotted_path } => {
                if active_variant == raw.as_ref() {
                    return Ok(true);
                }
                if let Some(s) = resolve_case_label_str(raw.as_ref(), dotted_path.as_ref(), scope)
                    && active_variant == s
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn resolve_case_label_str<'a>(
    label: &str,
    dotted_path: Option<&CompiledPath>,
    scope: &'a Scope<'_>,
) -> Option<&'a str> {
    let resolved = match dotted_path {
        None => scope.resolve(label),
        Some(path) => scope.try_resolve_compiled_path(path),
    };
    match resolved {
        Some(Value::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// Resolve the active variant name for a match expression.
///
/// - **enum**: returns the variant tag (unit or struct variant).
/// - **option**: returns `"Some"` or `"None"`.
/// - **str**: returns the string value itself.
/// - **int/bool/float**: returns the value formatted as a string.
pub(super) fn resolve_match_variant<'a>(
    expr: &CompiledPath,
    is_option: bool,
    scope: &'a Scope<'_>,
) -> Result<Cow<'a, str>, TemplateError> {
    let value = scope.resolve_path(expr)?;

    match value {
        // Absent option value → "None" variant.
        Value::None => Ok(Cow::Borrowed(crate::consts::OPTION_NONE)),
        // For option(T): any non-None value is the "Some" branch.
        _ if is_option => Ok(Cow::Borrowed(crate::consts::OPTION_SOME)),
        // Unit enum variant stored as plain string.
        Value::Str(s) => Ok(Cow::Borrowed(s.as_str())),
        // Struct variant stored as dict with "tag" key.
        Value::Struct(map) => {
            let tag_key = crate::consts::ENUM_TAG_KEY;
            match map.get(tag_key) {
                Some(Value::Str(tag)) => Ok(Cow::Borrowed(tag.as_str())),
                _ => Err(TemplateError::syntax(alloc::format!(
                    "match: '{}' is a dict without a 'tag' field",
                    expr.as_str()
                ))),
            }
        }
        // Scalar types: format as string for label comparison.
        Value::Int(n) => Ok(Cow::Owned(alloc::format!("{n}"))),
        Value::Float(f) => Ok(Cow::Owned(alloc::format!("{f}"))),
        Value::Bool(b) => Ok(Cow::Borrowed(if *b {
            crate::consts::LIT_TRUE
        } else {
            crate::consts::LIT_FALSE
        })),
        Value::List(_) => Err(TemplateError::syntax(alloc::format!(
            "match: '{}' is a list — match requires a scalar or enum value",
            expr.as_str(),
        ))),
        Value::Tmpl(_) => Err(TemplateError::syntax(alloc::format!(
            "match: '{}' is not an enum value (got {})",
            expr.as_str(),
            value.type_name()
        ))),
    }
}
