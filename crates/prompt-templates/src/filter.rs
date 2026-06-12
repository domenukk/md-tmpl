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
        FilterKind::Join => apply_join(value, args),
        FilterKind::Limit => apply_limit(value, args),
        FilterKind::Add => apply_add(value, args),
        FilterKind::Sub => apply_sub(value, args),
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
        FILTER_ADD, FILTER_FIXED, FILTER_JOIN, FILTER_LIMIT, FILTER_LOWER, FILTER_SUB, FILTER_TRIM,
        FILTER_UPPER,
    };
    match filter_name {
        FILTER_UPPER => apply_upper(value),
        FILTER_LOWER => apply_lower(value),
        FILTER_TRIM => apply_trim(value),
        FILTER_FIXED => apply_fixed(value, args),
        FILTER_JOIN => apply_join(value, args),
        FILTER_LIMIT => apply_limit(value, args),
        FILTER_ADD => apply_add(value, args),
        FILTER_SUB => apply_sub(value, args),
        _ => Err(TemplateError::UnknownFilter(filter_name.to_string())),
    }
}

/// Parse a filter expression like `fixed(2)` into (name, optional args).
#[must_use]
pub(crate) fn parse_filter(filter: &str) -> (&str, Option<&str>) {
    let filter = filter.trim();
    if let Some(paren_start) = filter.find(crate::consts::PAREN_OPEN) {
        let name = filter[..paren_start].trim();
        let args = filter[paren_start + 1..]
            .strip_suffix(crate::consts::PAREN_CLOSE)
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
    crate::consts::strip_string_literal(s).unwrap_or(s)
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

/// Parsed numeric argument — either integer or floating-point.
enum NumArg {
    Int(i64),
    Float(f64),
}

/// Parse a filter argument as a number, trying integer first.
fn parse_num_arg(arg: &str, filter_name: &str) -> Result<NumArg, TemplateError> {
    if let Ok(n) = arg.parse::<i64>() {
        return Ok(NumArg::Int(n));
    }
    arg.parse::<f64>().map(NumArg::Float).map_err(|e| {
        TemplateError::syntax(format!("'{filter_name}' argument must be a number: {e}"))
    })
}

/// Convert `i64` to `f64`, using lossless `i32` path when possible.
///
/// For values within `i32` range, `f64::from(i32)` is exact.  For larger
/// values the cast goes through `as f64` which may lose low-order bits;
/// this is acceptable for template arithmetic.
fn i64_to_f64(i: i64) -> f64 {
    if let Ok(small) = i32::try_from(i) {
        f64::from(small)
    } else {
        // Values outside i32 range lose at most 11 bits of precision.
        // This is inherent to f64 and acceptable for template arithmetic.
        i.to_string().parse().expect("i64 always parses as f64")
    }
}

/// Add a number to the value: `{{ x | add(1) }}`.
fn apply_add(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let raw = args.ok_or_else(|| TemplateError::syntax("'add' requires a number argument"))?;
    let operand = parse_num_arg(raw, "add")?;
    match (value, operand) {
        (Value::Int(i), NumArg::Int(n)) => Ok(Value::Int(i.saturating_add(n))),
        (Value::Int(i), NumArg::Float(n)) => Ok(Value::Float(i64_to_f64(*i) + n)),
        (Value::Float(f), NumArg::Int(n)) => Ok(Value::Float(*f + i64_to_f64(n))),
        (Value::Float(f), NumArg::Float(n)) => Ok(Value::Float(*f + n)),
        _ => Err(TemplateError::syntax("'add' requires a number")),
    }
}

/// Subtract a number from the value: `{{ x | sub(1) }}`.
fn apply_sub(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let raw = args.ok_or_else(|| TemplateError::syntax("'sub' requires a number argument"))?;
    let operand = parse_num_arg(raw, "sub")?;
    match (value, operand) {
        (Value::Int(i), NumArg::Int(n)) => Ok(Value::Int(i.saturating_sub(n))),
        (Value::Int(i), NumArg::Float(n)) => Ok(Value::Float(i64_to_f64(*i) - n)),
        (Value::Float(f), NumArg::Int(n)) => Ok(Value::Float(*f - i64_to_f64(n))),
        (Value::Float(f), NumArg::Float(n)) => Ok(Value::Float(*f - n)),
        _ => Err(TemplateError::syntax("'sub' requires a number")),
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

    // -- default (removed — use := in frontmatter) --

    // -- length (removed — use len() function instead) --

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

    // -- add --

    #[test]
    fn add_int() {
        assert_eq!(
            apply_filter(&Value::Int(5), "add", Some("3")).unwrap(),
            Value::Int(8)
        );
    }

    #[test]
    fn add_negative() {
        assert_eq!(
            apply_filter(&Value::Int(5), "add", Some("-2")).unwrap(),
            Value::Int(3)
        );
    }

    #[test]
    fn add_float() {
        assert_eq!(
            apply_filter(&Value::Float(1.5), "add", Some("2.5")).unwrap(),
            Value::Float(4.0)
        );
    }

    #[test]
    fn add_int_with_float_operand() {
        assert_eq!(
            apply_filter(&Value::Int(3), "add", Some("0.5")).unwrap(),
            Value::Float(3.5)
        );
    }

    #[test]
    fn add_rejects_non_number() {
        let err = apply_filter(&Value::Str("x".into()), "add", Some("1")).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    #[test]
    fn add_missing_arg_errors() {
        let err = apply_filter(&Value::Int(1), "add", None).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- sub --

    #[test]
    fn sub_int() {
        assert_eq!(
            apply_filter(&Value::Int(10), "sub", Some("3")).unwrap(),
            Value::Int(7)
        );
    }

    #[test]
    fn sub_float() {
        assert_eq!(
            apply_filter(&Value::Float(5.0), "sub", Some("1.5")).unwrap(),
            Value::Float(3.5)
        );
    }

    #[test]
    fn sub_rejects_non_number() {
        let err = apply_filter(&Value::Str("x".into()), "sub", Some("1")).unwrap_err();
        assert!(matches!(err, TemplateError::Syntax(_)));
    }

    // -- unknown filter --

    #[test]
    fn unknown_filter_errors() {
        let err = apply_filter(&Value::Str("x".into()), "nonexistent", None).unwrap_err();
        assert!(matches!(err, TemplateError::UnknownFilter(ref name) if name == "nonexistent"));
    }
}
