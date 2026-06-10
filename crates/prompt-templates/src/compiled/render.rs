//! Rendering pre-compiled template segments.
//!
//! Walks the compiled segment tree and produces output text by
//! evaluating expressions, loops, conditionals, and includes at
//! render time.

use std::{borrow::Cow, fmt::Write, path::Path};

use super::{ComparisonOp, CompiledInclude, Condition, ParsedFilter, Segment};
use crate::{error::TemplateError, parser, scope::Scope, value::Value};

/// Estimated output size for a single expression or include tag.
const ESTIMATED_EXPR_SIZE: usize = 32;

/// Estimated iteration count for for-loop capacity estimation.
const ESTIMATED_LOOP_ITERATIONS: usize = 4;

/// Estimate the total static text size for `String::with_capacity`.
#[must_use]
pub(super) fn estimate_output_capacity(segments: &[Segment]) -> usize {
    let mut size: usize = 0;
    for seg in segments {
        match seg {
            Segment::Static(s) | Segment::Raw(s) => size += s.len(),
            Segment::Expr { .. } | Segment::Include(_) => size += ESTIMATED_EXPR_SIZE,
            Segment::Comment(_) => {} // No output.
            Segment::ForLoop { body, .. } => {
                size += estimate_output_capacity(body) * ESTIMATED_LOOP_ITERATIONS;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                for (_, branch_body) in branches {
                    size += estimate_output_capacity(branch_body);
                }
                size += estimate_output_capacity(else_body);
            }
            Segment::Match { arms, .. } => {
                for (_, arm_body) in arms {
                    size += estimate_output_capacity(arm_body);
                }
            }
        }
    }
    size
}

/// Render pre-compiled segments into a string.
///
/// # Errors
///
/// Returns [`TemplateError`] on variable resolution, filter, or include
/// errors.
pub(crate) fn render_segments(
    segments: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
) -> Result<String, TemplateError> {
    let mut output = String::with_capacity(estimate_output_capacity(segments));
    render_segments_into(segments, scope, base_dir, &mut output)?;
    Ok(output)
}

/// Render segments into an existing output buffer.
pub(crate) fn render_segments_into(
    segments: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for segment in segments {
        match segment {
            Segment::Static(text) | Segment::Raw(text) => output.push_str(text),
            Segment::Expr { path, filters } => {
                eval_compiled_expr_into(path.as_ref(), filters, scope, output)?;
            }
            Segment::ForLoop {
                binding,
                list_path,
                body,
            } => {
                render_for_loop(
                    binding.as_ref(),
                    list_path.as_ref(),
                    body,
                    scope,
                    base_dir,
                    output,
                )?;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                render_if(branches, else_body, scope, base_dir, output)?;
            }
            Segment::Match { expr, arms } => {
                render_match(expr.as_ref(), arms, scope, base_dir, output)?;
            }
            Segment::Include(inc) => {
                render_include(inc, scope, base_dir, output)?;
            }
            Segment::Comment(_) => {
                // Comments produce no output.
            }
        }
    }
    Ok(())
}

/// Write a rendered [`Value`] directly into an output buffer,
/// avoiding an intermediate `String` allocation.
fn render_value_into(val: &Value, output: &mut String) -> Result<(), TemplateError> {
    match val {
        Value::Str(s) => output.push_str(s),
        Value::Bool(b) => write!(output, "{b}").unwrap(),
        Value::Int(i) => write!(output, "{i}").unwrap(),
        Value::Float(f) => write!(output, "{f}").unwrap(),
        Value::List(_) | Value::Dict(_) => {
            return Err(TemplateError::syntax(format!(
                "cannot display value of type '{}'",
                val.type_name()
            )));
        }
    }
    Ok(())
}

/// Evaluate a pre-compiled expression (path + filters) and write
/// the result directly into `output`, avoiding intermediate `String`
/// allocations in the common no-filter path.
fn eval_compiled_expr_into(
    path: &str,
    filters: &[ParsedFilter],
    scope: &Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // Check for function calls first: idx(bug), len(items)
    if let Some(result) = scope.try_call_function(path) {
        let val = result?;
        if filters.is_empty() {
            return render_value_into(&val, output);
        }
        let mut owned = val;
        for f in filters {
            owned = crate::filter::apply_filter_typed(
                f.kind,
                &owned,
                f.args.as_ref().map(AsRef::as_ref),
            )?;
        }
        return render_value_into(&owned, output);
    }

    let value = scope.resolve_path(path)?;

    if filters.is_empty() {
        // Hot path: write the borrowed value directly — no clone needed.
        render_value_into(value, output)
    } else {
        let mut owned_value = value.clone();
        for f in filters {
            owned_value = crate::filter::apply_filter_typed(
                f.kind,
                &owned_value,
                f.args.as_ref().map(AsRef::as_ref),
            )?;
        }
        render_value_into(&owned_value, output)
    }
}

/// Register loop metadata for a for-loop binding.
///
/// After pushing a scope layer and inserting the binding variable,
/// call this to associate `{{ idx(binding) }}` metadata.
pub(crate) fn register_loop_meta(scope: &mut Scope<'_>, binding: &str, i: usize) {
    scope.set_loop_meta(
        binding,
        crate::scope::LoopMeta {
            // Loop iteration count is bounded by collection `.len()`,
            // which cannot exceed `isize::MAX`.
            index: i64::try_from(i).expect("loop index <= isize::MAX < i64::MAX"),
        },
    );
}

/// Render a compiled for-loop.
fn render_for_loop(
    binding: &str,
    list_expr: &str,
    body: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let list_value = parser::eval_expr(list_expr.trim(), scope)?;
    let Value::List(items) = list_value else {
        return Err(TemplateError::syntax(format!(
            "'{list_expr}' is not a list"
        )));
    };

    let binding_key = binding.to_string();
    for (i, item) in items.into_iter().enumerate() {
        let layer = scope.push_layer();
        // Re-use the binding key allocation: after the first iteration the
        // entry already exists in the (cleared-but-reallocated) HashMap,
        // so `insert` can reuse the slot without allocating a fresh key.
        layer.insert(binding_key.clone(), item);
        register_loop_meta(scope, binding, i);
        render_segments_into(body, scope, base_dir, output)?;
        scope.pop_layer();
    }

    Ok(())
}

/// Render a compiled conditional (if/elif/else chain).
///
/// Evaluates each branch's [`Condition`] in order, rendering the body
/// of the first match. Falls through to `else_body` when no branch
/// matches.
fn render_if(
    branches: &[(Condition, Vec<Segment>)],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for (condition, body) in branches {
        if eval_condition(condition, scope)? {
            return render_segments_into(body, scope, base_dir, output);
        }
    }

    if !else_body.is_empty() {
        render_segments_into(else_body, scope, base_dir, output)?;
    }

    Ok(())
}

/// Render a compiled match block.
///
/// Resolves the expression to determine the active enum variant, then
/// renders the body of the matching `{% case %}` arm. Silently produces
/// no output if no arm matches (non-exhaustive matches are caught at
/// compile time by `validate_template!`).
fn render_match(
    expr: &str,
    arms: &[(Vec<Cow<'static, str>>, Vec<Segment>)],
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let value = scope.resolve_path(expr)?;
    let active_variant: &str = match value {
        // Unit variant stored as plain string.
        Value::Str(s) => s.as_str(),
        // Struct variant stored as dict with "tag" key.
        Value::Dict(map) => {
            let tag_key = crate::consts::ENUM_TAG_KEY;
            match map.get(tag_key) {
                Some(Value::Str(tag)) => tag.as_str(),
                _ => {
                    return Err(TemplateError::syntax(format!(
                        "match: '{expr}' is a dict without a 'tag' field"
                    )));
                }
            }
        }
        _ => {
            return Err(TemplateError::syntax(format!(
                "match: '{expr}' is not an enum value (got {})",
                value.type_name()
            )));
        }
    };

    for (variants, body) in arms {
        if variants.iter().any(|v| active_variant == v.as_ref()) {
            return render_segments_into(body, scope, base_dir, output);
        }
    }

    // No arm matched — silently skip (non-exhaustive is valid for
    // inline `{% match x case Y %}` single-arm guards).
    Ok(())
}

// ---------------------------------------------------------------------------
// Value comparison helpers (used by eval_condition)
// ---------------------------------------------------------------------------

/// Compare an `i64` against an `f64` without any `as`-based integer↔float casts.
///
/// Decomposes the `f64` into its integer and fractional parts using IEEE 754 bit
/// manipulation, then compares using only integer arithmetic.
fn cmp_int_float(i: i64, f: f64) -> Option<std::cmp::Ordering> {
    if f.is_nan() {
        return None;
    }
    if f.is_infinite() {
        return if f.is_sign_positive() {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Greater)
        };
    }

    let (f_int, f_has_frac, f_negative) = decompose_f64(f);

    let i_wide = i128::from(i);
    let f_signed = if f_negative {
        -i128::from(f_int)
    } else {
        i128::from(f_int)
    };

    match i_wide.cmp(&f_signed) {
        std::cmp::Ordering::Less => Some(std::cmp::Ordering::Less),
        std::cmp::Ordering::Greater => Some(std::cmp::Ordering::Greater),
        std::cmp::Ordering::Equal => {
            if !f_has_frac {
                Some(std::cmp::Ordering::Equal)
            } else if f_negative {
                Some(std::cmp::Ordering::Greater)
            } else {
                Some(std::cmp::Ordering::Less)
            }
        }
    }
}

/// Decompose a finite `f64` into `(integer_abs: u64, has_frac: bool, negative: bool)`.
///
/// Uses IEEE 754 double-precision bit layout to extract the integer part
/// without any float→int or int→float casts.
fn decompose_f64(f: f64) -> (u64, bool, bool) {
    debug_assert!(f.is_finite(), "decompose_f64 requires finite input");

    let bits = f.to_bits();
    let negative = (bits >> 63) != 0;
    let raw_exp = (bits >> 52) & 0x7FF; // u64, 11 bits
    let mantissa = bits & 0x000F_FFFF_FFFF_FFFF;

    // Zero (positive or negative).
    if raw_exp == 0 && mantissa == 0 {
        return (0, false, negative);
    }
    // Subnormal: |f| < 1 → integer part is 0.
    if raw_exp == 0 {
        return (0, true, negative);
    }
    // Normal: exponent = raw_exp - 1023. If raw_exp < 1023, |f| < 1.
    if raw_exp < 1023 {
        return (0, true, negative);
    }

    let exp = raw_exp - 1023; // >= 0, u64

    // Full mantissa with implicit leading 1: (2^52 + mantissa)
    let full_mantissa = (1_u64 << 52) | mantissa;

    if exp >= 52 {
        // All mantissa bits are integer; no fractional part.
        let shift = exp - 52;
        if shift >= 64 {
            (u64::MAX, false, negative) // overflow; handled via i128 above
        } else {
            (full_mantissa << shift, false, negative)
        }
    } else {
        let shift = 52 - exp;
        let int_part = full_mantissa >> shift;
        let frac_mask = (1_u64 << shift) - 1;
        let has_frac = (full_mantissa & frac_mask) != 0;
        (int_part, has_frac, negative)
    }
}

/// Mixed-type numeric partial ordering.
///
/// Returns `Some(Ordering)` for numeric comparisons (int vs int, float vs float,
/// int vs float, float vs int). Returns `None` for non-numeric types.
fn partial_cmp_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.partial_cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::Float(y)) => cmp_int_float(*x, *y),
        (Value::Float(x), Value::Int(y)) => cmp_int_float(*y, *x).map(std::cmp::Ordering::reverse),
        _ => None,
    }
}

/// Evaluate a pre-parsed condition without any string scanning.
pub(super) fn eval_condition(
    condition: &Condition,
    scope: &Scope<'_>,
) -> Result<bool, TemplateError> {
    match condition {
        Condition::Truthy(var) => {
            let value = scope.resolve_path(var.as_ref())?;
            Ok(value.is_truthy())
        }
        Condition::Comparison { left, op, right } => {
            let left_val = scope.resolve_value_or_literal(left.as_ref())?;
            let right_val = scope.resolve_value_or_literal(right.as_ref())?;
            let result =
                match op {
                    ComparisonOp::Eq => left_val == right_val,
                    ComparisonOp::Ne => left_val != right_val,
                    ComparisonOp::Le => partial_cmp_values(&left_val, &right_val)
                        .is_some_and(std::cmp::Ordering::is_le),
                    ComparisonOp::Ge => partial_cmp_values(&left_val, &right_val)
                        .is_some_and(std::cmp::Ordering::is_ge),
                    ComparisonOp::Lt => partial_cmp_values(&left_val, &right_val)
                        .is_some_and(std::cmp::Ordering::is_lt),
                    ComparisonOp::Gt => partial_cmp_values(&left_val, &right_val)
                        .is_some_and(std::cmp::Ordering::is_gt),
                };
            Ok(result)
        }
    }
}

/// Render a compiled include directive.
///
/// Includes still load files at runtime (since the included file might
/// change), but the host template's structure is pre-compiled.
fn render_include(
    inc: &CompiledInclude,
    scope: &mut Scope<'_>,
    base_dir: Option<&Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // Build an IncludeDirective from the compiled data, borrowing from the
    // owned strings.
    let with_vars: Vec<(&str, &str)> = inc
        .with_vars
        .iter()
        .map(|(k, v)| (k.as_ref(), v.as_ref()))
        .collect();

    let for_each = inc.for_each.as_ref().map(|(b, l)| (b.as_ref(), l.as_ref()));

    let directive = parser::IncludeDirective {
        path: inc.path.as_ref(),
        with_vars,
        for_each,
    };

    // Depth tracking is handled inside resolve_include.
    crate::include::resolve_include_into(
        &directive,
        scope,
        base_dir,
        inc.inline_compiled.as_ref(),
        output,
    )
}
