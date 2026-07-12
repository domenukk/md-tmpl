//! Rendering pre-compiled template segments.
//!
//! Walks the compiled segment tree and produces output text by
//! evaluating expressions, loops, conditionals, and includes at
//! render time.

#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{borrow::Cow, string::String, sync::Arc};

use super::{ComparisonOp, CompiledInclude, Condition, MatchArm, ParsedFilter, Segment};
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

/// Maximum precision for the fast integer-math path.
/// Above this, f64 loses precision so we fall back to std formatting.
const MAX_FAST_FIXED_PRECISION: usize = 18;

/// Pre-computed powers of 10 for precision 0..=18.
const POW10: [f64; 19] = {
    let mut table = [1.0; 19];
    let mut i = 1;
    while i < 19 {
        table[i] = table[i - 1] * 10.0;
        i += 1;
    }
    table
};

/// Write a float with fixed precision into `output`, avoiding the heavy
/// `std::fmt::float_to_decimal_common_exact` machinery.
///
/// For precision ≤ 18, this uses multiply-round-truncate + `itoa`, which
/// is ~3× faster than `write!("{f:.precision$}")`.
#[inline]
fn write_fixed_float(f: f64, precision: usize, output: &mut String) {
    /// Convert a known-positive, bounded f64 to u64.
    ///
    /// Callers guarantee `v` is in `[0, u64::MAX as f64]`.
    // NOLINT: caller guarantees v is non-negative and within u64 range; truncation/sign-loss is intentional
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn positive_f64_to_u64(v: f64) -> u64 {
        debug_assert!(v >= 0.0 && v.is_finite());
        v as u64
    }

    if precision > MAX_FAST_FIXED_PRECISION || !f.is_finite() {
        // Fallback for extreme precision or NaN/Inf.
        use core::fmt::Write;
        write!(output, "{f:.precision$}").expect("fmt::Write for String is infallible");
        return;
    }

    let is_neg = f.is_sign_negative() && f != 0.0;
    let abs = f.abs();

    // Multiply by 10^precision and round to nearest integer.
    let scale = POW10[precision];
    let scaled = positive_f64_to_u64(abs * scale + 0.5);

    if precision == 0 {
        if is_neg {
            output.push('-');
        }
        let mut buf = itoa::Buffer::new();
        output.push_str(buf.format(scaled));
        return;
    }

    // Split into integer and fractional parts.
    let divisor = positive_f64_to_u64(scale);
    let int_part = scaled / divisor;
    let frac_part = scaled % divisor;

    if is_neg {
        output.push('-');
    }

    let mut buf = itoa::Buffer::new();
    output.push_str(buf.format(int_part));
    output.push('.');

    // Pad fractional part with leading zeros.
    let mut frac_buf = itoa::Buffer::new();
    let frac_str = frac_buf.format(frac_part);
    let pad = precision - frac_str.len();
    for _ in 0..pad {
        output.push('0');
    }
    output.push_str(frac_str);
}

/// Estimate the total static text size for `String::with_capacity`.
#[must_use]
pub fn estimate_output_capacity(segments: &[Segment]) -> usize {
    let mut size: usize = 0;
    for seg in segments {
        match seg {
            Segment::Static(s) | Segment::Raw(s) => size += s.len(),
            Segment::Expr { .. } | Segment::Include(_) | Segment::Panic(_) => {
                size += ESTIMATED_EXPR_SIZE;
            }
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
                for arm in arms {
                    max_arm = max_arm.max(estimate_output_capacity(&arm.body));
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
                list_expr,
                body,
                else_body,
            } => {
                render_for_loop(
                    binding.as_ref(),
                    list_expr,
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
            Segment::Panic(segments) => {
                let msg = render_segments(segments, scope, base_dir)?;
                return Err(TemplateError::panic(msg));
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
                list_expr,
                body,
                else_body,
            } => {
                render_for_loop_no_std(
                    binding.as_ref(),
                    list_expr,
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
            Segment::Panic(segments) => {
                let mut msg = String::new();
                render_segments_into_no_std(segments, scope, &mut msg)?;
                return Err(TemplateError::panic(msg));
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
        Value::Bool(true) => output.push_str(crate::consts::LIT_TRUE),
        Value::Bool(false) => output.push_str(crate::consts::LIT_FALSE),
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
fn eval_compiled_expr_into(
    expr: &CompiledExpr,
    filters: &[ParsedFilter],
    scope: &Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    if try_fast_path_fixed_filter(expr, filters, scope, output)? {
        return Ok(());
    }

    if filters.is_empty() {
        if let CompiledExpr::Path(path) = expr {
            let val = scope.resolve_path(path)?;
            return render_value_into(val, output);
        }
        if let CompiledExpr::Kind(path) = expr {
            let val = scope.resolve_path(path)?;
            if scope.is_option_path(path.as_str()) {
                match val {
                    Value::None => output.push_str(crate::consts::OPTION_NONE),
                    _ => output.push_str(crate::consts::OPTION_SOME),
                }
                return Ok(());
            }
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
                Value::None => {
                    output.push_str(crate::consts::OPTION_NONE);
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
    }

    let value = eval_compiled_expr_val(expr, scope)?;
    apply_filters_and_render(value, filters, output)
}

/// Fast path: single `fixed(n)` filter on a numeric value — write directly
/// into `output` without allocating an intermediate String or Value.
#[inline]
fn try_fast_path_fixed_filter(
    expr: &CompiledExpr,
    filters: &[ParsedFilter],
    scope: &Scope<'_>,
    output: &mut String,
) -> Result<bool, TemplateError> {
    if filters.len() == 1 && filters[0].kind == super::FilterKind::Fixed {
        if let CompiledExpr::Path(path) = expr {
            if let Some(precision) = filters[0].parsed_num {
                let val = scope.resolve_path(path)?;
                match val {
                    Value::Float(f) => {
                        write_fixed_float(*f, precision, output);
                        return Ok(true);
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
                        return Ok(true);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(false)
}

fn eval_compiled_expr_val<'a>(
    expr: &'a CompiledExpr,
    scope: &'a Scope<'_>,
) -> Result<Cow<'a, Value>, TemplateError> {
    match expr {
        CompiledExpr::Path(path) => scope.resolve_path(path).map(Cow::Borrowed),
        CompiledExpr::Idx(binding) => {
            let meta = scope.get_loop_meta(binding).ok_or_else(|| {
                TemplateError::syntax(alloc::format!(
                    "idx() requires active loop binding '{binding}'"
                ))
            })?;
            Ok(Cow::Owned(Value::Int(meta.index)))
        }
        CompiledExpr::Len(path) => {
            let val = scope.resolve_path(path)?;
            let count = match val {
                Value::List(l) => i64::try_from(l.len()).expect("collection length fits i64"),
                Value::Str(s) => i64::try_from(s.len()).expect("string length fits i64"),
                _ => {
                    return Err(TemplateError::syntax("len() requires a list or string"));
                }
            };
            Ok(Cow::Owned(Value::Int(count)))
        }
        CompiledExpr::Kind(path) => {
            let val = scope.resolve_path(path)?;
            if scope.is_option_path(path.as_str()) {
                return match val {
                    Value::None => Ok(Cow::Owned(Value::Str(crate::consts::OPTION_NONE.into()))),
                    _ => Ok(Cow::Owned(Value::Str(crate::consts::OPTION_SOME.into()))),
                };
            }
            match val {
                Value::Struct(d) => {
                    if let Some(Value::Str(k)) = d.get(crate::consts::ENUM_TAG_KEY) {
                        Ok(Cow::Owned(Value::Str(k.clone())))
                    } else {
                        Err(TemplateError::syntax(
                            "kind() requires an enum value (dict with variant tag)",
                        ))
                    }
                }
                Value::Str(s) => Ok(Cow::Owned(Value::Str(s.clone()))),
                Value::None => Ok(Cow::Owned(Value::Str(crate::consts::OPTION_NONE.into()))),
                _ => Err(TemplateError::syntax(alloc::format!(
                    "kind() requires an enum value, got {}",
                    val.type_name()
                ))),
            }
        }
        CompiledExpr::Kinds(path) => {
            let val = scope.resolve_path(path)?;
            match val {
                Value::Struct(d) => {
                    if let Some(list_val) = d.get(crate::consts::ENUM_VARIANTS_KEY) {
                        Ok(Cow::Borrowed(list_val))
                    } else {
                        Err(TemplateError::syntax(
                            "kinds() requires an enum type namespace",
                        ))
                    }
                }
                _ => Err(TemplateError::syntax(alloc::format!(
                    "kinds() requires an enum type namespace, got {}",
                    val.type_name()
                ))),
            }
        }
        CompiledExpr::Has(path) => {
            let val = scope.resolve_path(path)?;
            Ok(Cow::Owned(Value::Bool(Scope::is_option_some(val))))
        }
    }
}

/// Apply filters to a resolved value and render the result.
fn apply_filters_and_render(
    value: Cow<'_, Value>,
    filters: &[ParsedFilter],
    output: &mut String,
) -> Result<(), TemplateError> {
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
#[inline]
pub(crate) fn register_loop_meta(scope: &mut Scope<'_>, binding: &str, i: usize) {
    let index = i64::try_from(i).expect("loop index exceeds i64::MAX");
    scope.set_loop_meta(binding, crate::scope::LoopMeta { index });
}

/// Render a compiled for-loop.
#[cfg(feature = "std")]
#[inline]
fn render_for_loop(
    binding: &str,
    list_expr: &CompiledExpr,
    body: &[Segment],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let list_ref = eval_compiled_expr_val(list_expr, scope)?;
    let items = if let Value::List(items) = &*list_ref {
        Arc::clone(items)
    } else {
        let expr_str = match list_expr {
            CompiledExpr::Path(p)
            | CompiledExpr::Len(p)
            | CompiledExpr::Kind(p)
            | CompiledExpr::Kinds(p)
            | CompiledExpr::Has(p) => p.as_str(),
            CompiledExpr::Idx(b) => b.as_ref(),
        };
        return Err(TemplateError::syntax(alloc::format!(
            "'{expr_str}' is not a list"
        )));
    };

    if items.is_empty() && !else_body.is_empty() {
        return render_segments_into(else_body, scope, base_dir, output);
    }

    for (i, item) in items.iter().enumerate() {
        scope.push_loop_binding(binding, item);
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
    list_expr: &CompiledExpr,
    body: &[Segment],
    else_body: &[Segment],
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let list_ref = eval_compiled_expr_val(list_expr, scope)?;
    let items = match &*list_ref {
        Value::List(items) => Arc::clone(items),
        _ => {
            let expr_str = match list_expr {
                CompiledExpr::Path(p)
                | CompiledExpr::Len(p)
                | CompiledExpr::Kind(p)
                | CompiledExpr::Kinds(p)
                | CompiledExpr::Has(p) => p.as_str(),
                CompiledExpr::Idx(b) => b.as_ref(),
            };
            return Err(TemplateError::syntax(alloc::format!(
                "'{}' is not a list",
                expr_str
            )));
        }
    };

    if items.is_empty() && !else_body.is_empty() {
        return render_segments_into_no_std(else_body, scope, output);
    }

    for (i, item) in items.iter().enumerate() {
        scope.push_loop_binding(binding, item);
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
    arms: &[MatchArm],
    is_option: bool,
    scope: &mut Scope<'_>,
    base_dir: Option<&std::path::Path>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let active_variant = resolve_match_variant(expr, is_option, scope)?;

    for arm in arms {
        let variant_matches = arm_matches(&active_variant, &arm.variants, scope);
        if variant_matches {
            // Evaluate guard if present.
            if let Some(ref guard) = arm.guard {
                if !eval_condition(guard, scope)? {
                    continue;
                }
            }
            return render_segments_into(&arm.body, scope, base_dir, output);
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
    arms: &[MatchArm],
    is_option: bool,
    scope: &mut Scope<'_>,
    output: &mut String,
) -> Result<(), TemplateError> {
    let active_variant = resolve_match_variant(expr, is_option, scope)?;

    for arm in arms {
        let variant_matches = arm_matches(&active_variant, &arm.variants, scope);
        if variant_matches {
            if let Some(ref guard) = arm.guard {
                if !eval_condition(guard, scope)? {
                    continue;
                }
            }
            return render_segments_into_no_std(&arm.body, scope, output);
        }
    }

    Ok(())
}

/// Check if any arm variant matches the active variant.
///
/// Matching rules:
/// - `_` (`MATCH_DEFAULT`): always matches
/// - Quoted label (e.g. `"Active"`): strip quotes, compare literally
/// - Unquoted label matching an enum variant: compare literally
/// - Unquoted label (param-ref on str match): resolve the param value
///   from scope and compare the resolved value against `active_variant`
fn arm_matches(active_variant: &str, variants: &[Cow<'_, str>], scope: &Scope<'_>) -> bool {
    for v in variants {
        let label = v.as_ref();
        if label == crate::consts::MATCH_DEFAULT {
            return true;
        }
        // Quoted string literal: strip quotes, interpolate if needed, and compare.
        if let Some(inner) = crate::consts::strip_string_literal(label) {
            if inner.contains(crate::consts::EXPR_START) {
                // Contains {{ expr }} — compile and render the interpolated string.
                // NOLINT: compile/render failure means the label doesn't match — fall through to literal comparison
                if let Ok(segments) = crate::compiled::compile_body(inner) {
                    // NOLINT: render failure means the interpolated label is unresolvable — not a match
                    if let Ok(rendered) = render_interpolated_str(&segments, scope) {
                        if active_variant == rendered.as_str() {
                            return true;
                        }
                    }
                }
            } else if active_variant == inner {
                return true;
            }
            continue;
        }
        // Direct comparison (enum variant name).
        if active_variant == label {
            return true;
        }
        // Param-ref: resolve label as a variable and compare its string value.
        if let Some(Value::Str(s)) = scope.resolve(label) {
            if active_variant == s.as_str() {
                return true;
            }
        }
    }
    false
}

/// Resolve the active variant name for a match expression.
///
/// - **enum**: returns the variant tag (unit or struct variant).
/// - **option**: returns `"Some"` or `"None"`.
/// - **str**: returns the string value itself.
/// - **int/bool/float**: returns the value formatted as a string.
fn resolve_match_variant<'a>(
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
        Condition::Not(inner) => {
            let result = eval_condition(inner, scope)?;
            Ok(!result)
        }
        Condition::And(left, right) => {
            // Short-circuit: if left is false, don't evaluate right.
            if !eval_condition(left, scope)? {
                return Ok(false);
            }
            eval_condition(right, scope)
        }
        Condition::Or(left, right) => {
            // Short-circuit: if left is true, don't evaluate right.
            if eval_condition(left, scope)? {
                return Ok(true);
            }
            eval_condition(right, scope)
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
                ComparisonOp::In => match &*right_val {
                    Value::List(right_items) => match &*left_val {
                        Value::List(left_items) => {
                            left_items.iter().all(|l| right_items.contains(l))
                        }
                        scalar => right_items.contains(scalar),
                    },
                    Value::Str(right_str) => match &*left_val {
                        Value::Str(left_str) => right_str.contains(left_str.as_str()),
                        Value::List(left_items) => left_items.iter().all(|l| {
                            if let Value::Str(s) = l {
                                right_str.contains(s.as_str())
                            } else {
                                false
                            }
                        }),
                        _ => false,
                    },
                    _ => false,
                },
            };
            Ok(result)
        }
        Condition::MatchVariant {
            expr,
            variants,
            is_option,
        } => {
            let active_variant = resolve_match_variant(expr, *is_option, scope)?;
            Ok(variants.iter().any(|v| {
                let label = v.as_ref();
                label == crate::consts::MATCH_DEFAULT
                    || active_variant == label
                    || crate::consts::strip_string_literal(label)
                        .is_some_and(|inner| active_variant == inner)
            }))
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
    // NOLINT: resolution failure means path is not a tmpl() param — fall through to filesystem
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

    scope.push_declarations(declarations);

    let res = if let Some((binding, list_expr)) = &directive.for_each {
        // Iterated include.
        let list_value = crate::parser::eval_expr(list_expr.trim(), scope)?;
        let Value::List(items) = list_value else {
            scope.pop_declarations(declarations);
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
        Ok(())
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
        let r = render_segments_into_no_std(segments, scope, output);
        if needs_layer {
            scope.pop_layer();
        }
        r
    };
    scope.pop_declarations(declarations);
    res
}

// ---------------------------------------------------------------------------
// String interpolation helper
// ---------------------------------------------------------------------------

/// Render interpolated string segments (from `{{ expr }}` inside quoted strings)
/// against an **immutable** scope.
///
/// This is a lightweight renderer that handles only `Static`/`Raw` and `Expr`
/// segments — the only segment types produced by `compile_body` on a simple
/// interpolated string literal (no control flow tags allowed inside string
/// literals). Any other segment type is a compile-time logic error and will
/// produce a syntax error.
pub(crate) fn render_interpolated_str(
    segments: &[Segment],
    scope: &Scope<'_>,
) -> Result<String, TemplateError> {
    let mut output = String::new();
    for segment in segments {
        match segment {
            Segment::Static(text) | Segment::Raw(text) => output.push_str(text),
            Segment::Expr { expr, filters } => {
                eval_compiled_expr_into(expr, filters, scope, &mut output)?;
            }
            Segment::Comment(_) => {}
            _ => {
                return Err(TemplateError::syntax(
                    "control-flow tags are not allowed inside interpolated strings",
                ));
            }
        }
    }
    Ok(output)
}

// ---------------------------------------------------------------------------
// Tests for numeric comparison helpers
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "std"))]
#[path = "render_tests.rs"]
mod render_tests;
