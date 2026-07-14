//! Condition evaluation and numeric comparison helpers.

use super::matching::resolve_match_variant;
use crate::{
    compiled::{ComparisonOp, Condition},
    error::TemplateError,
    scope::Scope,
    value::Value,
};

/// Compare an `i64` against an `f64` without any `as`-based integer↔float casts.
///
/// Decomposes the `f64` into its integer and fractional parts using IEEE 754 bit
/// manipulation, then compares using only integer arithmetic.
pub(super) fn cmp_int_float(i: i64, f: f64) -> Option<core::cmp::Ordering> {
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
pub(super) fn decompose_f64(f: f64) -> (u64, bool, bool) {
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
pub(crate) fn eval_condition(
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
