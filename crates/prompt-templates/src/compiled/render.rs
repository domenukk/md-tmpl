//! Rendering pre-compiled template segments.
//!
//! Walks the compiled segment tree and produces output text by
//! evaluating expressions, loops, conditionals, and includes at
//! render time.

#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{borrow::Cow, string::String, sync::Arc};

use super::{ComparisonOp, CompiledInclude, Condition, ParsedFilter, Segment};
use crate::{
    error::TemplateError,
    parser,
    scope::{CompiledExpr, CompiledPath, Scope},
    value::Value,
};

/// Estimated output size for a single expression or include tag.
const ESTIMATED_EXPR_SIZE: usize = 32;

/// Estimated iteration count for for-loop capacity estimation.
const ESTIMATED_LOOP_ITERATIONS: usize = 4;

/// Estimate the total static text size for `String::with_capacity`.
#[must_use]
pub fn estimate_output_capacity(segments: &[Segment]) -> usize {
    let mut size: usize = 0;
    for seg in segments {
        match seg {
            Segment::Static(s) | Segment::Raw(s) => size += s.len(),
            Segment::Expr { .. } | Segment::Include(_) => size += ESTIMATED_EXPR_SIZE,
            Segment::Comment(_) => {} // No output.
            Segment::ForLoop {
                body, else_body, ..
            } => {
                size += estimate_output_capacity(body) * ESTIMATED_LOOP_ITERATIONS;
                size += estimate_output_capacity(else_body);
            }
            Segment::If {
                branches,
                else_body,
            } => {
                // At most one branch is rendered — use the max, not the sum.
                let mut max_branch = estimate_output_capacity(else_body);
                for (_, branch_body) in branches {
                    max_branch = max_branch.max(estimate_output_capacity(branch_body));
                }
                size += max_branch;
            }
            Segment::Match { arms, .. } => {
                // At most one arm is rendered — use the max, not the sum.
                let mut max_arm = 0;
                for (_, arm_body) in arms {
                    max_arm = max_arm.max(estimate_output_capacity(arm_body));
                }
                size += max_arm;
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
#[cfg(feature = "std")]
pub(crate) fn render_segments(
    segments: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
) -> Result<String, TemplateError> {
    let mut output = String::with_capacity(estimate_output_capacity(segments));
    render_segments_into(segments, scope, base_dir, &mut output)?;
    Ok(output)
}

/// Render segments into an existing output buffer.
#[cfg(feature = "std")]
#[inline]
pub(crate) fn render_segments_into(
    segments: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for segment in segments {
        match segment {
            Segment::Static(text) | Segment::Raw(text) => output.push_str(text),
            Segment::Expr { expr, filters } => {
                eval_compiled_expr_into(expr, filters, scope, output)?;
            }
            Segment::ForLoop {
                binding,
                list_path,
                body,
                else_body,
            } => {
                render_for_loop(
                    binding.as_ref(),
                    list_path,
                    body,
                    else_body,
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
            Segment::Match {
                expr,
                arms,
                is_option,
            } => {
                render_match(expr, arms, *is_option, scope, base_dir, output)?;
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

/// Render segments into an existing output buffer (`no_std` variant).
///
/// Include directives are not supported and produce a runtime error.
#[cfg(not(feature = "std"))]
pub(crate) fn render_segments_into_no_std(
    segments: &[Segment],
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for segment in segments {
        match segment {
            Segment::Static(text) | Segment::Raw(text) => output.push_str(text),
            Segment::Expr { expr, filters } => {
                eval_compiled_expr_into(expr, filters, scope, output)?;
            }
            Segment::ForLoop {
                binding,
                list_path,
                body,
                else_body,
            } => {
                render_for_loop_no_std(
                    binding.as_ref(),
                    list_path,
                    body,
                    else_body,
                    scope,
                    output,
                )?;
            }
            Segment::If {
                branches,
                else_body,
            } => {
                render_if_no_std(branches, else_body, scope, output)?;
            }
            Segment::Match {
                expr,
                arms,
                is_option,
            } => {
                render_match_no_std(expr, arms, *is_option, scope, output)?;
            }
            Segment::Include(inc) => {
                render_include_no_std(inc, scope, output)?;
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
#[inline]
fn render_value_into(val: &Value, output: &mut String) -> Result<(), TemplateError> {
    match val {
        Value::Str(s) => output.push_str(s),
        // Direct push avoids the `write!` → `fmt` machinery.
        Value::Bool(true) => output.push_str("true"),
        Value::Bool(false) => output.push_str("false"),
        // itoa/ryu are ~3x faster than `write!` for number formatting.
        Value::Int(i) => {
            let mut buf = itoa::Buffer::new();
            output.push_str(buf.format(*i));
        }
        // Float formatting via Display — benchmarks show it's faster
        // than ryu+strip_suffix for whole numbers (the common case).
        Value::Float(f) => {
            use core::fmt::Write;
            // SAFETY: `fmt::Write for String` is infallible — it only
            // forwards to `String::push_str` which cannot fail.
            write!(output, "{f}").expect("fmt::Write for String is infallible");
        }
        Value::None => { /* Absent value renders as empty. */ }
        Value::List(_) | Value::Struct(_) | Value::Tmpl(_) => {
            return Err(TemplateError::syntax(alloc::format!(
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
#[allow(clippy::too_many_lines)]
fn eval_compiled_expr_into(
    expr: &CompiledExpr,
    filters: &[ParsedFilter],
    scope: &Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // Fast path: single `fixed(n)` filter on a numeric value — write directly
    // into `output` without allocating an intermediate String or Value.
    if filters.len() == 1 && filters[0].kind == super::FilterKind::Fixed {
        if let CompiledExpr::Path(path) = expr {
            if let Some(precision) = filters[0].parsed_num {
                let val = scope.resolve_path(path)?;
                match val {
                    Value::Float(f) => {
                        use core::fmt::Write;
                        write!(output, "{f:.precision$}")
                            .expect("fmt::Write for String is infallible");
                        return Ok(());
                    }
                    Value::Int(i) => {
                        // Use itoa for the integer part to avoid
                        // i64→f64 precision loss for values > 2^53.
                        let mut buf = itoa::Buffer::new();
                        let int_str = buf.format(*i);
                        output.push_str(int_str);
                        if precision > 0 {
                            output.push('.');
                            for _ in 0..precision {
                                output.push('0');
                            }
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }

    let value = match expr {
        CompiledExpr::Path(path) => {
            let val = scope.resolve_path(path)?;
            if filters.is_empty() {
                // Hot path: render borrowed value directly
                return render_value_into(val, output);
            }
            Cow::Borrowed(val)
        }
        CompiledExpr::Idx(binding) => {
            let meta = scope.get_loop_meta(binding).ok_or_else(|| {
                TemplateError::syntax(alloc::format!(
                    "idx() requires active loop binding '{binding}'"
                ))
            })?;
            Cow::Owned(Value::Int(meta.index))
        }
        CompiledExpr::Len(path) => {
            let val = scope.resolve_path(path)?;
            // Collection lengths are bounded by available memory and cannot
            // exceed isize::MAX (< i64::MAX) on any supported platform.
            let count = match val {
                Value::List(l) => i64::try_from(l.len()).expect("collection length fits i64"),
                Value::Str(s) => i64::try_from(s.len()).expect("string length fits i64"),
                Value::Struct(d) => i64::try_from(d.len()).expect("struct length fits i64"),
                _ => {
                    return Err(TemplateError::syntax(
                        "len() requires a list, string, or struct",
                    ));
                }
            };
            Cow::Owned(Value::Int(count))
        }
        CompiledExpr::Kind(path) => {
            let val = scope.resolve_path(path)?;
            // Fast path: kind() is almost never filtered, so write directly
            // to output without cloning the string.
            if filters.is_empty() {
                match val {
                    Value::Struct(d) => {
                        if let Some(Value::Str(k)) = d.get(crate::consts::ENUM_TAG_KEY) {
                            output.push_str(k);
                            return Ok(());
                        }
                        return Err(TemplateError::syntax(
                            "kind() requires an enum value (dict with variant tag)",
                        ));
                    }
                    Value::Str(s) => {
                        output.push_str(s);
                        return Ok(());
                    }
                    _ => {
                        return Err(TemplateError::syntax(alloc::format!(
                            "kind() requires an enum value, got {}",
                            val.type_name()
                        )));
                    }
                }
            }
            // Slow path: filters present — need to clone into a Value.
            let kind = match val {
                Value::Struct(d) => {
                    if let Some(Value::Str(k)) = d.get(crate::consts::ENUM_TAG_KEY) {
                        k.clone()
                    } else {
                        return Err(TemplateError::syntax(
                            "kind() requires an enum value (dict with variant tag)",
                        ));
                    }
                }
                Value::Str(s) => s.clone(),
                _ => {
                    return Err(TemplateError::syntax(alloc::format!(
                        "kind() requires an enum value, got {}",
                        val.type_name()
                    )));
                }
            };
            Cow::Owned(Value::Str(kind))
        }
        CompiledExpr::Has(path) => {
            let val = scope.resolve_path(path)?;
            let result = Scope::is_option_some(val);
            if filters.is_empty() {
                output.push_str(if result { "true" } else { "false" });
                return Ok(());
            }
            Cow::Owned(Value::Bool(result))
        }
    };

    if filters.is_empty() {
        render_value_into(&value, output)
    } else {
        let mut owned_value = value.into_owned();
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
#[cfg(feature = "std")]
#[inline]
fn render_for_loop(
    binding: &str,
    list_expr: &CompiledPath,
    body: &[Segment],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // Borrow the value and extract a clone of the Arc (O(1) refcount bump)
    // without cloning the entire Value enum.
    let list_ref = scope.resolve_path(list_expr)?;
    let items = match list_ref {
        Value::List(items) => Arc::clone(items),
        _ => {
            return Err(TemplateError::syntax(alloc::format!(
                "'{}' is not a list",
                list_expr.as_str()
            )));
        }
    };

    if items.is_empty() && !else_body.is_empty() {
        return render_segments_into(else_body, scope, base_dir, output);
    }

    for (i, item) in items.iter().enumerate() {
        scope.push_loop_binding(binding, item.clone());
        register_loop_meta(scope, binding, i);
        render_segments_into(body, scope, base_dir, output)?;
        scope.pop_loop_binding();
    }

    Ok(())
}

/// Render a compiled for-loop (`no_std` variant).
#[cfg(not(feature = "std"))]
fn render_for_loop_no_std(
    binding: &str,
    list_expr: &CompiledPath,
    body: &[Segment],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let list_ref = scope.resolve_path(list_expr)?;
    let items = match list_ref {
        Value::List(items) => Arc::clone(items),
        _ => {
            return Err(TemplateError::syntax(alloc::format!(
                "'{}' is not a list",
                list_expr.as_str()
            )));
        }
    };

    if items.is_empty() && !else_body.is_empty() {
        return render_segments_into_no_std(else_body, scope, output);
    }

    for (i, item) in items.iter().enumerate() {
        scope.push_loop_binding(binding, item.clone());
        register_loop_meta(scope, binding, i);
        render_segments_into_no_std(body, scope, output)?;
        scope.pop_loop_binding();
    }

    Ok(())
}

/// Render a compiled conditional (if/elif/else chain).
///
/// Evaluates each branch's [`Condition`] in order, rendering the body
/// of the first match. Falls through to `else_body` when no branch
/// matches.
#[cfg(feature = "std")]
#[inline]
fn render_if(
    branches: &[(Condition, alloc::vec::Vec<Segment>)],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
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

/// Render a compiled conditional (`no_std` variant).
#[cfg(not(feature = "std"))]
fn render_if_no_std(
    branches: &[(Condition, alloc::vec::Vec<Segment>)],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    for (condition, body) in branches {
        if eval_condition(condition, scope)? {
            return render_segments_into_no_std(body, scope, output);
        }
    }

    if !else_body.is_empty() {
        render_segments_into_no_std(else_body, scope, output)?;
    }

    Ok(())
}

/// Render a compiled match block.
///
/// Resolves the expression to determine the active enum variant, then
/// renders the body of the matching `{% case %}` arm. Silently produces
/// no output if no arm matches (non-exhaustive matches are caught at
/// compile time by the type system).
#[cfg(feature = "std")]
fn render_match(
    expr: &CompiledPath,
    arms: &[(alloc::vec::Vec<Cow<'static, str>>, alloc::vec::Vec<Segment>)],
    is_option: bool,
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let active_variant = resolve_match_variant(expr, is_option, scope)?;

    for (variants, body) in arms {
        if variants
            .iter()
            .any(|v| v.as_ref() == "_" || active_variant == v.as_ref())
        {
            return render_segments_into(body, scope, base_dir, output);
        }
    }

    // No arm matched — silently skip (non-exhaustive is valid for
    // inline `{% match x case Y %}` single-arm guards).
    Ok(())
}

/// Render a compiled match block (`no_std` variant).
#[cfg(not(feature = "std"))]
fn render_match_no_std(
    expr: &CompiledPath,
    arms: &[(alloc::vec::Vec<Cow<'static, str>>, alloc::vec::Vec<Segment>)],
    is_option: bool,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let active_variant = resolve_match_variant(expr, is_option, scope)?;

    for (variants, body) in arms {
        if variants
            .iter()
            .any(|v| v.as_ref() == "_" || active_variant == v.as_ref())
        {
            return render_segments_into_no_std(body, scope, output);
        }
    }

    Ok(())
}

/// Resolve the active enum/option variant name for a match expression.
///
/// When `is_option` is `true`, uses `Value::None` discriminant check
/// (zero-cost) instead of string-based variant tag lookup.
fn resolve_match_variant<'a>(
    expr: &CompiledPath,
    is_option: bool,
    scope: &'a Scope<'_>,
) -> Result<&'a str, TemplateError> {
    let value = scope.resolve_path(expr)?;

    match value {
        // Absent option value → "None" variant.
        Value::None => Ok("None"),
        // For option<T>: any non-None value is the "Some" branch.
        _ if is_option => Ok("Some"),
        // Unit enum variant stored as plain string.
        Value::Str(s) => Ok(s.as_str()),
        // Struct variant stored as dict with "tag" key.
        Value::Struct(map) => {
            let tag_key = crate::consts::ENUM_TAG_KEY;
            match map.get(tag_key) {
                Some(Value::Str(tag)) => Ok(tag.as_str()),
                _ => Err(TemplateError::syntax(alloc::format!(
                    "match: \'{}\' is a dict without a \'tag\' field",
                    expr.as_str()
                ))),
            }
        }
        _ => Err(TemplateError::syntax(alloc::format!(
            "match: \'{}\' is not an enum value (got {})",
            expr.as_str(),
            value.type_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Value comparison helpers (used by eval_condition)
// ---------------------------------------------------------------------------

/// Compare an `i64` against an `f64` without any `as`-based integer↔float casts.
///
/// Decomposes the `f64` into its integer and fractional parts using IEEE 754 bit
/// manipulation, then compares using only integer arithmetic.
fn cmp_int_float(i: i64, f: f64) -> Option<core::cmp::Ordering> {
    if f.is_nan() {
        return None;
    }
    if f.is_infinite() {
        return if f.is_sign_positive() {
            Some(core::cmp::Ordering::Less)
        } else {
            Some(core::cmp::Ordering::Greater)
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
        core::cmp::Ordering::Less => Some(core::cmp::Ordering::Less),
        core::cmp::Ordering::Greater => Some(core::cmp::Ordering::Greater),
        core::cmp::Ordering::Equal => {
            if !f_has_frac {
                Some(core::cmp::Ordering::Equal)
            } else if f_negative {
                Some(core::cmp::Ordering::Greater)
            } else {
                Some(core::cmp::Ordering::Less)
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
fn partial_cmp_values(a: &Value, b: &Value) -> Option<core::cmp::Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.partial_cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::Float(y)) => cmp_int_float(*x, *y),
        (Value::Float(x), Value::Int(y)) => cmp_int_float(*y, *x).map(core::cmp::Ordering::reverse),
        _ => None,
    }
}

/// Evaluate a pre-parsed condition without any string scanning.
pub(super) fn eval_condition(
    condition: &Condition,
    scope: &Scope<'_>,
) -> Result<bool, TemplateError> {
    match condition {
        Condition::Truthy(operand) => {
            let value = operand.resolve(scope)?;
            Ok(value.is_truthy())
        }
        Condition::Comparison { left, op, right } => {
            let left_val = left.resolve(scope)?;
            let right_val = right.resolve(scope)?;
            let result = match op {
                ComparisonOp::Eq => *left_val == *right_val,
                ComparisonOp::Ne => *left_val != *right_val,
                ComparisonOp::Le => partial_cmp_values(&left_val, &right_val)
                    .is_some_and(core::cmp::Ordering::is_le),
                ComparisonOp::Ge => partial_cmp_values(&left_val, &right_val)
                    .is_some_and(core::cmp::Ordering::is_ge),
                ComparisonOp::Lt => partial_cmp_values(&left_val, &right_val)
                    .is_some_and(core::cmp::Ordering::is_lt),
                ComparisonOp::Gt => partial_cmp_values(&left_val, &right_val)
                    .is_some_and(core::cmp::Ordering::is_gt),
            };
            Ok(result)
        }
    }
}

/// Render a compiled include directive.
///
/// Includes still load files at runtime (since the included file might
/// change), but the host template's structure is pre-compiled.
#[cfg(feature = "std")]
fn render_include(
    inc: &CompiledInclude,
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // Build an IncludeDirective from the compiled data, borrowing from the
    // owned strings.
    let with_vars: alloc::vec::Vec<(&str, &str)> = inc
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

/// Render a compiled include directive under `no_std`.
///
/// Supports three resolution strategies (in order):
///
/// 1. **Pre-compiled inline AST** (`inline_compiled`) — e.g. from `{% tmpl %}`.
/// 2. **Inline template** — defined via `{% tmpl name %}` in the current file.
/// 3. **`Value::Tmpl` parameter** — a template passed as a typed parameter.
///
/// Filesystem-based includes are not available under `no_std` and produce
/// a descriptive error.
#[cfg(not(feature = "std"))]
fn render_include_no_std(
    inc: &CompiledInclude,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    scope.enter_include()?;

    let result = render_include_no_std_inner(inc, scope, output);
    scope.exit_include();

    result.map_err(|e| match e {
        TemplateError::Syntax(ref syn) if syn.message.contains(&*inc.path) => e,
        TemplateError::IncludeNotFound(_) => e,
        TemplateError::Syntax(syn) => {
            TemplateError::syntax(alloc::format!("in include '{}': {}", inc.path, syn.message))
        }
        TemplateError::UndefinedVariable(name) => TemplateError::syntax(alloc::format!(
            "in include '{}': undefined variable '{name}'",
            inc.path
        )),
        other => other,
    })
}

/// Inner include resolution for `no_std`.
#[cfg(not(feature = "std"))]
fn render_include_no_std_inner(
    inc: &CompiledInclude,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    // Build an IncludeDirective from the compiled data.
    let with_vars: alloc::vec::Vec<(&str, &str)> = inc
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

    // 0. Pre-compiled inline AST.
    if let Some(compiled) = &inc.inline_compiled {
        scope.push_consts(
            (*compiled.consts).clone(),
            (*compiled.imported_consts).clone(),
        );
        let result = validate_and_render_no_std(
            &compiled.segments,
            &compiled.declarations,
            &directive,
            scope,
            output,
        );
        scope.pop_consts();
        return result;
    }

    // 1. Inline templates from the current scope.
    if let Some(compiled) = scope.get_inline_template(directive.path).cloned() {
        scope.push_consts(
            (*compiled.consts).clone(),
            (*compiled.imported_consts).clone(),
        );
        let result = validate_and_render_no_std(
            &compiled.segments,
            &compiled.declarations,
            &directive,
            scope,
            output,
        );
        scope.pop_consts();
        return result;
    }

    // 2. Value::Tmpl parameter.
    if let Ok(Value::Tmpl(tmpl)) = scope.resolve_path_str(directive.path) {
        let tmpl = tmpl.clone();
        scope.push_inline_templates(tmpl.inline_templates().clone());
        scope.push_consts((*tmpl.consts()).clone(), (*tmpl.imported_consts()).clone());

        let result = validate_and_render_no_std(
            tmpl.segments(),
            tmpl.declarations(),
            &directive,
            scope,
            output,
        );

        scope.pop_consts();
        scope.pop_inline_templates();
        return result;
    }

    // 3. No match — filesystem includes not available under no_std.
    Err(TemplateError::IncludeNotFound(alloc::format!(
        "cannot resolve '{}': filesystem includes require the `std` feature",
        directive.path
    )))
}

/// Common validation + render path for `no_std` includes.
#[cfg(not(feature = "std"))]
fn validate_and_render_no_std(
    segments: &[Segment],
    declarations: &[crate::types::VarDecl],
    directive: &parser::IncludeDirective<'_>,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    use crate::include_core::{
        build_overrides, inject_defaults_into_layer, validate_include_contract,
        validate_include_types,
    };

    validate_include_contract(declarations, directive)?;
    let overrides = build_overrides(directive, scope)?;
    validate_include_types(declarations, &overrides, directive)?;

    if let Some((binding, list_expr)) = &directive.for_each {
        // Iterated include.
        let list_value = crate::parser::eval_expr(list_expr.trim(), scope)?;
        let Value::List(items) = list_value else {
            return Err(TemplateError::syntax(alloc::format!(
                "'{list_expr}' is not a list"
            )));
        };

        for (i, item) in items.iter().enumerate() {
            {
                let layer = scope.push_layer();
                layer.insert(binding.to_string(), item.clone());
                for (k, v) in &overrides {
                    layer.insert(k.clone(), v.clone());
                }
                inject_defaults_into_layer(layer, declarations, &overrides);
            }
            register_loop_meta(scope, binding, i);
            render_segments_into_no_std(segments, scope, output)?;
            scope.pop_layer();
        }
    } else {
        // Simple include.
        let has_defaults = declarations.iter().any(|d| d.default_value.is_some());
        let needs_layer = !directive.with_vars.is_empty() || has_defaults;
        if needs_layer {
            let layer = scope.push_layer();
            for (k, v) in &overrides {
                layer.insert(k.clone(), v.clone());
            }
            inject_defaults_into_layer(layer, declarations, &overrides);
        }
        render_segments_into_no_std(segments, scope, output)?;
        if needs_layer {
            scope.pop_layer();
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests for numeric comparison helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use core::cmp::Ordering;

    use super::*;

    // -- cmp_int_float: NaN --

    #[test]
    fn cmp_int_float_nan_returns_none() {
        assert_eq!(cmp_int_float(0, f64::NAN), None);
        assert_eq!(cmp_int_float(i64::MAX, f64::NAN), None);
        assert_eq!(cmp_int_float(i64::MIN, f64::NAN), None);
    }

    // -- cmp_int_float: infinity --

    #[test]
    fn cmp_int_float_positive_infinity() {
        // Any integer is less than +∞.
        assert_eq!(cmp_int_float(0, f64::INFINITY), Some(Ordering::Less));
        assert_eq!(cmp_int_float(i64::MAX, f64::INFINITY), Some(Ordering::Less));
        assert_eq!(cmp_int_float(i64::MIN, f64::INFINITY), Some(Ordering::Less));
    }

    #[test]
    fn cmp_int_float_negative_infinity() {
        // Any integer is greater than -∞.
        assert_eq!(cmp_int_float(0, f64::NEG_INFINITY), Some(Ordering::Greater));
        assert_eq!(
            cmp_int_float(i64::MIN, f64::NEG_INFINITY),
            Some(Ordering::Greater)
        );
    }

    // -- cmp_int_float: exact equality --

    #[test]
    fn cmp_int_float_exact_zero() {
        assert_eq!(cmp_int_float(0, 0.0), Some(Ordering::Equal));
        assert_eq!(cmp_int_float(0, -0.0), Some(Ordering::Equal));
    }

    #[test]
    fn cmp_int_float_exact_integer_values() {
        assert_eq!(cmp_int_float(1, 1.0), Some(Ordering::Equal));
        assert_eq!(cmp_int_float(-1, -1.0), Some(Ordering::Equal));
        assert_eq!(cmp_int_float(100, 100.0), Some(Ordering::Equal));
    }

    // -- cmp_int_float: fractional values --

    #[test]
    fn cmp_int_float_integer_less_than_float_with_fraction() {
        // 1 < 1.5
        assert_eq!(cmp_int_float(1, 1.5), Some(Ordering::Less));
    }

    #[test]
    fn cmp_int_float_integer_greater_than_float_with_fraction() {
        // 2 > 1.5
        assert_eq!(cmp_int_float(2, 1.5), Some(Ordering::Greater));
    }

    #[test]
    fn cmp_int_float_negative_fraction() {
        // -2 < -1.5 (i.e., -2 is more negative)
        assert_eq!(cmp_int_float(-2, -1.5), Some(Ordering::Less));
        // -1 > -1.5
        assert_eq!(cmp_int_float(-1, -1.5), Some(Ordering::Greater));
    }

    // -- cmp_int_float: extreme values --

    #[test]
    fn cmp_int_float_i64_max() {
        // i64::MAX (2^63-1) cannot be represented exactly in f64.
        // i64::MAX as f64 rounds to 2^63, so i64::MAX < (i64::MAX as f64).
        // Using the numeric constant directly to avoid an i64→f64 cast lint.
        let f: f64 = 9_223_372_036_854_775_808.0; // 2^63 (rounded i64::MAX)
        assert_eq!(cmp_int_float(i64::MAX, f), Some(Ordering::Less));
    }

    #[test]
    fn cmp_int_float_i64_min() {
        // i64::MIN (-2^63) CAN be represented exactly in f64.
        // Using the numeric constant directly to avoid an i64→f64 cast lint.
        let f: f64 = -9_223_372_036_854_775_808.0; // i64::MIN
        assert_eq!(cmp_int_float(i64::MIN, f), Some(Ordering::Equal));
    }

    // -- cmp_int_float: subnormals --

    #[test]
    fn cmp_int_float_subnormal() {
        // Subnormal numbers are very close to zero but not zero.
        let subnormal = f64::MIN_POSITIVE / 2.0;
        assert!(subnormal > 0.0 && subnormal < f64::MIN_POSITIVE);
        // 0 < subnormal (subnormal has a fractional part, int part = 0)
        assert_eq!(cmp_int_float(0, subnormal), Some(Ordering::Less));
        // 1 > subnormal
        assert_eq!(cmp_int_float(1, subnormal), Some(Ordering::Greater));
    }

    // -- decompose_f64 --

    #[test]
    fn decompose_f64_zero() {
        let (int_part, has_frac, negative) = decompose_f64(0.0);
        assert_eq!(int_part, 0);
        assert!(!has_frac);
        assert!(!negative);
    }

    #[test]
    fn decompose_f64_negative_zero() {
        let (int_part, has_frac, negative) = decompose_f64(-0.0);
        assert_eq!(int_part, 0);
        assert!(!has_frac);
        assert!(negative);
    }

    #[test]
    fn decompose_f64_positive_integer() {
        let (int_part, has_frac, negative) = decompose_f64(42.0);
        assert_eq!(int_part, 42);
        assert!(!has_frac);
        assert!(!negative);
    }

    #[test]
    fn decompose_f64_negative_with_fraction() {
        let (int_part, has_frac, negative) = decompose_f64(-2.78);
        assert_eq!(int_part, 2);
        assert!(has_frac);
        assert!(negative);
    }

    #[test]
    fn decompose_f64_subnormal() {
        let subnormal = f64::MIN_POSITIVE / 2.0;
        let (int_part, has_frac, negative) = decompose_f64(subnormal);
        assert_eq!(int_part, 0);
        assert!(has_frac); // subnormals are tiny fractions
        assert!(!negative);
    }

    #[test]
    fn decompose_f64_exact_float_int_boundary() {
        // 2^52 is the largest integer where all integers up to it are
        // exactly representable in f64.
        // Using the numeric constant directly to avoid a u64→f64 cast lint.
        let boundary: f64 = 4_503_599_627_370_496.0; // 2^52
        let (int_part, has_frac, negative) = decompose_f64(boundary);
        assert_eq!(int_part, 1_u64 << 52);
        assert!(!has_frac);
        assert!(!negative);
    }

    #[test]
    fn decompose_f64_value_less_than_one() {
        let (int_part, has_frac, negative) = decompose_f64(0.5);
        assert_eq!(int_part, 0);
        assert!(has_frac);
        assert!(!negative);
    }
}
