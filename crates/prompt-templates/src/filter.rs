//! Built-in expression filters.

use crate::{compiled::FilterKind, error::TemplateError, value::Value};

/// Apply a filter by its strongly-typed [`FilterKind`].
///
/// Used by the compiled rendering path to avoid runtime string matching.
///
/// # Errors
///
/// Returns an error if a required argument is missing or the value type is
/// incompatible with the filter.
pub(crate) fn apply_filter_typed(
    kind: FilterKind,
    value: &Value,
    args: Option<&str>,
) -> Result<Value, TemplateError> {
    match kind {
        FilterKind::Upper => apply_upper(value),
        FilterKind::Lower => apply_lower(value),
        FilterKind::Trim => apply_trim(value),
        FilterKind::Fixed => apply_fixed(value, args),
        FilterKind::Default => apply_default(value, args),
        FilterKind::Length => apply_length(value),
        FilterKind::Join => apply_join(value, args),
        FilterKind::Limit => apply_limit(value, args),
        FilterKind::Gt => apply_gt(value, args),
    }
}

/// Apply a named filter to a value.
///
/// This is the string-based dispatch used only in tests. Production code
/// uses [`apply_filter_typed`] with pre-resolved [`FilterKind`]s.
///
/// # Errors
///
/// Returns an error if the filter name is unknown, a required argument is
/// missing, or the value type is incompatible with the filter.
#[cfg(test)]
pub fn apply_filter(
    value: &Value,
    filter_name: &str,
    args: Option<&str>,
) -> Result<Value, TemplateError> {
    use crate::consts::{
        FILTER_DEFAULT, FILTER_FIXED, FILTER_GT, FILTER_JOIN, FILTER_LENGTH, FILTER_LIMIT,
        FILTER_LOWER, FILTER_TRIM, FILTER_UPPER,
    };
    match filter_name {
        FILTER_UPPER => apply_upper(value),
        FILTER_LOWER => apply_lower(value),
        FILTER_TRIM => apply_trim(value),
        FILTER_FIXED => apply_fixed(value, args),
        FILTER_DEFAULT => apply_default(value, args),
        FILTER_LENGTH => apply_length(value),
        FILTER_JOIN => apply_join(value, args),
        FILTER_LIMIT => apply_limit(value, args),
        FILTER_GT => apply_gt(value, args),
        _ => Err(TemplateError::UnknownFilter(filter_name.to_string())),
    }
}

/// Parse a filter expression like `fixed(2)` into (name, optional args).
#[must_use]
pub(crate) fn parse_filter(filter: &str) -> (&str, Option<&str>) {
    let filter = filter.trim();
    if let Some(paren_start) = filter.find('(') {
        let name = filter[..paren_start].trim();
        let args = filter[paren_start + 1..]
            .strip_suffix(')')
            .unwrap_or("")
            .trim();
        let args = if args.is_empty() { None } else { Some(args) };
        (name, args)
    } else {
        (filter, None)
    }
}

// ---------------------------------------------------------------------------
// Individual filter implementations
// ---------------------------------------------------------------------------

/// Convert a string value to uppercase.
fn apply_upper(value: &Value) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => Ok(Value::Str(s.to_uppercase())),
        _ => Err(TemplateError::syntax("'upper' requires a string")),
    }
}

/// Convert a string value to lowercase.
fn apply_lower(value: &Value) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => Ok(Value::Str(s.to_lowercase())),
        _ => Err(TemplateError::syntax("'lower' requires a string")),
    }
}

/// Trim leading and trailing whitespace from a string.
fn apply_trim(value: &Value) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => Ok(Value::Str(s.trim().to_string())),
        _ => Err(TemplateError::syntax("'trim' requires a string")),
    }
}

/// Format a number with fixed-point decimal precision.
fn apply_fixed(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let precision: usize = args
        .ok_or_else(|| TemplateError::syntax("'fixed' requires precision arg"))?
        .parse()
        .map_err(|e| TemplateError::syntax(format!("'fixed' precision must be an integer: {e}")))?;
    match value {
        Value::Float(f) => Ok(Value::Str(format!("{f:.precision$}"))),
        Value::Int(i) => {
            if precision == 0 {
                Ok(Value::Str(format!("{i}")))
            } else {
                let zeros = "0".repeat(precision);
                Ok(Value::Str(format!("{i}.{zeros}")))
            }
        }
        _ => Err(TemplateError::syntax("'fixed' requires a number")),
    }
}

/// Strip surrounding single or double quotes from a filter argument.
fn strip_quotes(s: &str) -> &str {
    s.trim_matches('"').trim_matches('\'')
}

/// Return the value if truthy, otherwise fall back to the provided default.
///
/// # Errors
///
/// Returns an error if no fallback argument is provided.
fn apply_default(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let fallback =
        args.ok_or_else(|| TemplateError::syntax("'default' requires a fallback argument"))?;
    let fallback = strip_quotes(fallback);
    if value.is_truthy() {
        Ok(value.clone())
    } else {
        Ok(Value::Str(fallback.to_string()))
    }
}

/// Return the length of a list, string, or dict.
fn apply_length(value: &Value) -> Result<Value, TemplateError> {
    match value {
        // `.len()` cannot exceed `isize::MAX`, which always fits in `i64`.
        Value::List(v) => Ok(Value::Int(
            i64::try_from(v.len()).expect("len <= isize::MAX < i64::MAX"),
        )),
        Value::Str(s) => Ok(Value::Int(
            i64::try_from(s.len()).expect("len <= isize::MAX < i64::MAX"),
        )),
        Value::Dict(m) => Ok(Value::Int(
            i64::try_from(m.len()).expect("len <= isize::MAX < i64::MAX"),
        )),
        _ => Err(TemplateError::syntax(
            "'length' requires a list, string, or dict",
        )),
    }
}

/// Join list items into a single string with a separator.
fn apply_join(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let separator = strip_quotes(args.unwrap_or(""));
    match value {
        Value::List(items) => {
            // Write directly into a single buffer, avoiding an intermediate
            // Vec<String> of Display'd items.
            let mut buf = String::new();
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    buf.push_str(separator);
                }
                match v {
                    Value::Str(s) => buf.push_str(s),
                    other => {
                        use std::fmt::Write;
                        write!(buf, "{other}").expect("fmt::Write to String is infallible");
                    }
                }
            }
            Ok(Value::Str(buf))
        }
        _ => Err(TemplateError::syntax("'join' requires a list")),
    }
}

/// Limit a list to a maximum number of elements.
fn apply_limit(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let limit: usize = args
        .ok_or_else(|| TemplateError::syntax("'limit' requires a limit argument"))?
        .parse()
        .map_err(|e| TemplateError::syntax(format!("'limit' argument must be an integer: {e}")))?;
    match value {
        Value::List(items) => {
            let taken = items.iter().take(limit).cloned().collect::<Vec<Value>>();
            Ok(Value::List(taken))
        }
        _ => Err(TemplateError::syntax("'limit' requires a list")),
    }
}

/// Check if a number is greater than a threshold.
fn apply_gt(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let threshold: f64 = args
        .ok_or_else(|| TemplateError::syntax("'gt' requires a comparison argument"))?
        .parse()
        .map_err(|e| TemplateError::syntax(format!("'gt' argument must be a number: {e}")))?;
    match value {
        Value::Int(i) => Ok(Value::Bool(cmp_int_f64_gt(*i, threshold))),
        Value::Float(f) => Ok(Value::Bool(*f > threshold)),
        _ => Err(TemplateError::syntax("'gt' requires a number")),
    }
}

/// Compare `i > threshold` without precision loss for large `i64` values.
///
/// Decomposes the threshold into integer and fractional parts using
/// IEEE 754 bit manipulation, then compares using only integer arithmetic.
fn cmp_int_f64_gt(i: i64, threshold: f64) -> bool {
    if threshold.is_nan() {
        return false;
    }
    if threshold.is_infinite() {
        return threshold.is_sign_negative();
    }

    let bits = threshold.to_bits();
    let negative = (bits >> 63) != 0;
    let raw_exp = (bits >> 52) & 0x7FF; // u64
    let mantissa = bits & 0x000F_FFFF_FFFF_FFFF;

    // Zero.
    if raw_exp == 0 && mantissa == 0 {
        return i > 0;
    }
    // Subnormal or |threshold| < 1.
    if raw_exp < 1023 {
        return if negative { true } else { i > 0 };
    }

    let exp = raw_exp - 1023; // >= 0, u64
    let full_mantissa = (1_u64 << 52) | mantissa;

    let (int_abs, has_frac) = if exp >= 52 {
        let shift = exp - 52;
        if shift >= 64 {
            return negative;
        }
        (full_mantissa << shift, false)
    } else {
        let shift = 52 - exp;
        let int_part = full_mantissa >> shift;
        let frac_mask = (1_u64 << shift) - 1;
        (int_part, (full_mantissa & frac_mask) != 0)
    };

    let threshold_int: i128 = if negative {
        -i128::from(int_abs)
    } else {
        i128::from(int_abs)
    };

    let i_wide = i128::from(i);

    match i_wide.cmp(&threshold_int) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            if has_frac {
                negative
            } else {
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_filter --

    #[test]
    fn parse_filter_no_args() {
        assert_eq!(parse_filter("upper"), ("upper", None));
        assert_eq!(parse_filter("  lower  "), ("lower", None));
    }

    #[test]
    fn parse_filter_with_args() {
        assert_eq!(parse_filter("fixed(2)"), ("fixed", Some("2")));
        assert_eq!(
            parse_filter("default(\"fallback\")"),
            ("default", Some("\"fallback\""))
        );
    }

    #[test]
    fn parse_filter_empty_args() {
        assert_eq!(parse_filter("trim()"), ("trim", None));
    }

    // -- upper --

    #[test]
    fn upper_converts_string() {
        let result = apply_filter(&Value::Str("hello".into()), "upper", None).unwrap();
        assert_eq!(result, Value::Str("HELLO".into()));
    }

    #[test]
    fn upper_rejects_non_string() {
        let err = apply_filter(&Value::Int(1), "upper", None).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- lower --

    #[test]
    fn lower_converts_string() {
        let result = apply_filter(&Value::Str("WORLD".into()), "lower", None).unwrap();
        assert_eq!(result, Value::Str("world".into()));
    }

    #[test]
    fn lower_rejects_non_string() {
        let err = apply_filter(&Value::Bool(true), "lower", None).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- trim --

    #[test]
    fn trim_strips_whitespace() {
        let result = apply_filter(&Value::Str("  spaced  ".into()), "trim", None).unwrap();
        assert_eq!(result, Value::Str("spaced".into()));
    }

    #[test]
    fn trim_no_op_on_clean_string() {
        let result = apply_filter(&Value::Str("clean".into()), "trim", None).unwrap();
        assert_eq!(result, Value::Str("clean".into()));
    }

    #[test]
    fn trim_rejects_non_string() {
        let err = apply_filter(&Value::Float(1.0), "trim", None).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- fixed --

    #[test]
    fn fixed_formats_float() {
        let result = apply_filter(&Value::Float(3.56789), "fixed", Some("2")).unwrap();
        assert_eq!(result, Value::Str("3.57".into()));
    }

    #[test]
    fn fixed_formats_int_as_float() {
        let result = apply_filter(&Value::Int(42), "fixed", Some("3")).unwrap();
        assert_eq!(result, Value::Str("42.000".into()));
    }

    #[test]
    fn fixed_missing_precision_errors() {
        let err = apply_filter(&Value::Float(1.0), "fixed", None).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    #[test]
    fn fixed_invalid_precision_errors() {
        let err = apply_filter(&Value::Float(1.0), "fixed", Some("abc")).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    #[test]
    fn fixed_rejects_non_number() {
        let err = apply_filter(&Value::Str("x".into()), "fixed", Some("2")).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- default --

    #[test]
    fn default_returns_value_when_truthy() {
        let result = apply_filter(
            &Value::Str("present".into()),
            "default",
            Some("\"fallback\""),
        )
        .unwrap();
        assert_eq!(result, Value::Str("present".into()));
    }

    #[test]
    fn default_returns_fallback_when_falsy() {
        let result =
            apply_filter(&Value::Str(String::new()), "default", Some("\"fallback\"")).unwrap();
        assert_eq!(result, Value::Str("fallback".into()));
    }

    #[test]
    fn default_no_args_errors() {
        let err = apply_filter(&Value::Bool(false), "default", None)
            .expect_err("'default' without fallback argument should fail");
        assert!(
            err.to_string().contains("fallback"),
            "should mention missing fallback argument: {err}"
        );
    }

    #[test]
    fn default_with_single_quoted_fallback() {
        let result = apply_filter(&Value::Int(0), "default", Some("'none'")).unwrap();
        assert_eq!(result, Value::Str("none".into()));
    }

    // -- length --

    #[test]
    fn length_of_list() {
        let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = apply_filter(&list, "length", None).unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn length_of_string() {
        let result = apply_filter(&Value::Str("hello".into()), "length", None).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn length_of_dict() {
        use std::collections::HashMap;
        let dict = Value::Dict(HashMap::from([
            ("a".into(), Value::Int(1)),
            ("b".into(), Value::Int(2)),
        ]));
        let result = apply_filter(&dict, "length", None).unwrap();
        assert_eq!(result, Value::Int(2));
    }

    #[test]
    fn length_of_empty_list() {
        let result = apply_filter(&Value::List(vec![]), "length", None).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn length_rejects_non_collection() {
        let err = apply_filter(&Value::Int(42), "length", None).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- join --

    #[test]
    fn join_strings_with_separator() {
        let list = Value::List(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
            Value::Str("c".into()),
        ]);
        let result = apply_filter(&list, "join", Some("\", \"")).unwrap();
        assert_eq!(result, Value::Str("a, b, c".into()));
    }

    #[test]
    fn join_without_separator() {
        let list = Value::List(vec![Value::Str("x".into()), Value::Str("y".into())]);
        let result = apply_filter(&list, "join", None).unwrap();
        assert_eq!(result, Value::Str("xy".into()));
    }

    #[test]
    fn join_converts_non_strings() {
        let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = apply_filter(&list, "join", Some("\"-\"")).unwrap();
        assert_eq!(result, Value::Str("1-2-3".into()));
    }

    #[test]
    fn join_empty_list() {
        let result = apply_filter(&Value::List(vec![]), "join", Some("\",\"")).unwrap();
        assert_eq!(result, Value::Str(String::new()));
    }

    #[test]
    fn join_rejects_non_list() {
        let err = apply_filter(&Value::Str("x".into()), "join", Some("\",\"")).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- limit --

    #[test]
    fn limit_takes_elements() {
        let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = apply_filter(&list, "limit", Some("2")).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1), Value::Int(2)]));
    }

    #[test]
    fn limit_keeps_all_if_large() {
        let list = Value::List(vec![Value::Int(1)]);
        let result = apply_filter(&list, "limit", Some("5")).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1)]));
    }

    #[test]
    fn limit_rejects_non_list() {
        let err = apply_filter(&Value::Str("x".into()), "limit", Some("2")).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- gt --

    #[test]
    fn gt_compares_int() {
        assert_eq!(
            apply_filter(&Value::Int(15), "gt", Some("10")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            apply_filter(&Value::Int(5), "gt", Some("10")).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn gt_compares_float() {
        assert_eq!(
            apply_filter(&Value::Float(15.5), "gt", Some("10")).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            apply_filter(&Value::Float(5.5), "gt", Some("10")).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn gt_rejects_non_number() {
        let err = apply_filter(&Value::Str("x".into()), "gt", Some("10")).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- unknown filter --

    #[test]
    fn unknown_filter_errors() {
        let err = apply_filter(&Value::Str("x".into()), "nonexistent", None).unwrap_err();
        assert!(matches!(err, TemplateError::UnknownFilter(ref name) if name == "nonexistent"));
    }
}
