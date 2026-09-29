//! Built-in expression filters.

mod security;
mod security_ext;
mod truncate;

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

pub use security::{
    DEFAULT_QUARANTINE_TAG, ROLE_TOKEN_DELIMITERS, TOKEN_DELIMITERS, escape_json_str,
    escape_xml_str, fence_str, has_control_tokens, has_quarantine_tag_breakout,
    has_role_control_tokens, has_untrusted_breakout, is_quarantined_str, quarantine_str,
    quarantine_untrusted_str, sanitize_quarantine_payload, sanitize_role_tokens_str,
    sanitize_tokens_str, sanitize_untrusted_str,
};
use security::{
    apply_escape_json, apply_escape_xml, apply_fence, apply_quarantine, apply_sanitize_tokens,
    sanitize_tokens_with_args,
};
pub(crate) use security::{
    fence_into, is_ncname_continue_byte, is_ncname_start_byte, quarantine_into,
    quarantine_untrusted_into,
};
pub use security_ext::{
    DEFAULT_SANITIZE_NOTICE, DEFAULT_SANITIZE_TAG, SANITIZE_NOTICE_TAG_PLACEHOLDER,
    TOOL_OUTPUT_QUARANTINE_CLOSE_TAG, TOOL_OUTPUT_QUARANTINE_OPEN_TAG, TOOL_OUTPUT_QUARANTINE_TAG,
    UNTRUSTED_QUARANTINE_CLOSE_TAG, UNTRUSTED_QUARANTINE_OPEN_TAG, UNTRUSTED_TOOL_OUTPUT_TAG,
    XML_DECODE_PAIRS, decode_xml_str, escape_xml_attr_str, escape_xml_body_str,
    format_sanitize_notice, is_sanitized_block_str, quarantine_untrusted_idempotent_str,
    sanitize_block_idempotent_str, sanitize_block_into, sanitize_block_str, sanitize_str,
    strip_outer_quarantine_tag, unquarantine_str, unsanitize_block_str,
};
use security_ext::{apply_sanitize_with_mode, sanitize_cow_with_mode, sanitize_into_with_mode};
pub(crate) use security_ext::{parse_sanitize_filter_args, parse_sanitize_filter_mode};
pub(crate) use truncate::apply_truncate;
pub use truncate::{
    DEFAULT_LIST_TRUNCATE_MARKER, DEFAULT_TRUNCATE_MARKER, DEFAULT_TRUNCATE_MARKER_COMPACT,
    TRUNCATE_PLACEHOLDER_COUNT, TRUNCATE_PLACEHOLDER_SKIPPED, ceil_char_boundary,
    floor_char_boundary, next_char_boundary, prev_char_boundary, truncate_middle_list,
    truncate_middle_str,
};

use crate::{
    compiled::{FilterKind, ParsedFilter},
    error::TemplateError,
    value::Value,
};

/// Apply a pre-parsed filter (including pre-resolved [`SanitizeFilterMode`]).
///
/// # Errors
///
/// Returns an error if a required argument is missing or the value type is
/// incompatible with the filter.
pub(crate) fn apply_filter_parsed(
    filter: &ParsedFilter,
    value: &Value,
) -> Result<Value, TemplateError> {
    if filter.kind == FilterKind::Sanitize {
        return apply_sanitize_with_mode(
            value,
            filter.sanitize_mode.as_ref(),
            filter.args.as_deref(),
        );
    }
    apply_filter_typed(filter.kind, value, filter.args.as_deref())
}

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
        FilterKind::EscapeXml => apply_escape_xml(value),
        FilterKind::EscapeJson => apply_escape_json(value),
        FilterKind::ToJson => apply_tojson(value, args),
        FilterKind::SanitizeTokens => apply_sanitize_tokens(value, args),
        FilterKind::Fence => apply_fence(value, args),
        FilterKind::Quarantine => apply_quarantine(value, args),
        FilterKind::Sanitize => apply_sanitize_with_mode(value, None, args),
        FilterKind::Truncate => apply_truncate(value, args),
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
    let kind = crate::compiled::parse_filter_kind(filter_name)?;
    apply_filter_typed(kind, value, args)
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
        Value::Str(s) => {
            if s.bytes().any(|b| b.is_ascii_lowercase() || !b.is_ascii()) {
                Ok(Value::Str(s.to_uppercase()))
            } else {
                Ok(value.clone())
            }
        }
        _ => Err(TemplateError::syntax("'upper' requires a string")),
    }
}

/// Convert a string value to lowercase.
fn apply_lower(value: &Value) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => {
            if s.bytes().any(|b| b.is_ascii_uppercase() || !b.is_ascii()) {
                Ok(Value::Str(s.to_lowercase()))
            } else {
                Ok(value.clone())
            }
        }
        _ => Err(TemplateError::syntax("'lower' requires a string")),
    }
}

/// Trim leading and trailing whitespace from a string.
fn apply_trim(value: &Value) -> Result<Value, TemplateError> {
    match value {
        Value::Str(s) => {
            let trimmed = s.trim();
            if trimmed.len() == s.len() {
                Ok(value.clone())
            } else {
                Ok(Value::Str(trimmed.to_string()))
            }
        }
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
            let mut buf = itoa::Buffer::new();
            let int_str = buf.format(*i);
            if precision == 0 {
                Ok(Value::Str(int_str.to_string()))
            } else {
                let zeros = "0".repeat(precision);
                Ok(Value::Str(format!("{int_str}.{zeros}")))
            }
        }
        _ => Err(TemplateError::syntax("'fixed' requires a number")),
    }
}

/// Strip surrounding single or double quotes from a filter argument and apply
/// md-tmpl's uniform escape unescaping to the inner content.
pub(super) fn strip_quotes(s: &str) -> Cow<'_, str> {
    match crate::consts::strip_string_literal(s) {
        Some(inner) => Cow::Owned(crate::consts::unescape_string_literal(inner)),
        None => Cow::Borrowed(s),
    }
}

/// Join list items into a single string with a separator.
fn apply_join(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let separator = strip_quotes(args.unwrap_or(""));
    match value {
        Value::List(items) => {
            let mut buf = String::new();
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    buf.push_str(&separator);
                }
                match v {
                    Value::Str(s) => buf.push_str(s),
                    Value::Int(_) | Value::Float(_) | Value::Bool(_) => {
                        use core::fmt::Write;
                        write!(buf, "{v}").expect("fmt::Write to String is infallible");
                    }
                    Value::None => {}
                    Value::Struct(_) => {
                        return Err(TemplateError::syntax(
                            "cannot display struct value directly \
                             — access individual fields (e.g. '{{ value.field }}') instead",
                        ));
                    }
                    Value::List(_) => {
                        return Err(TemplateError::syntax(
                            "cannot display nested list value directly \
                             — use {% for %} to iterate instead",
                        ));
                    }
                    Value::Tmpl(_) => {
                        return Err(TemplateError::syntax(
                            "cannot display template value directly \
                             — use {% include %} to render instead",
                        ));
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
            Ok(Value::List(Arc::new(taken)))
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

/// Convert `i64` to `f64` via exact 32-bit decomposition (`hi * 2^32 + lo`).
#[inline]
#[must_use]
pub fn i64_to_f64(i: i64) -> f64 {
    let bytes = i.to_le_bytes();
    let lo = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let hi = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    f64::from(hi) * 4_294_967_296.0 + f64::from(lo)
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

/// Serialize any [`Value`] into a valid JSON string, with optional indentation.
pub(crate) fn apply_tojson(value: &Value, args: Option<&str>) -> Result<Value, TemplateError> {
    let indent = match args {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                let n = trimmed.parse::<usize>().map_err(|_parse_err| {
                    TemplateError::syntax(alloc::format!(
                        "'tojson' indent argument must be a non-negative integer, got '{trimmed}'"
                    ))
                })?;
                Some(n)
            }
        }
        None => None,
    };
    let mut out = String::new();
    serialize_value_to_json(value, &mut out, indent, 0)?;
    Ok(Value::Str(out))
}

fn write_indent(out: &mut String, spaces: usize, depth: usize) {
    let total = spaces.saturating_mul(depth);
    for _ in 0..total {
        out.push(' ');
    }
}

fn serialize_value_to_json(
    val: &Value,
    out: &mut String,
    indent: Option<usize>,
    depth: usize,
) -> Result<(), TemplateError> {
    use core::fmt::Write;
    match val {
        Value::Str(s) => {
            out.push('"');
            escape_json_into(s, out);
            out.push('"');
        }
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(i) => {
            let mut buf = itoa::Buffer::new();
            out.push_str(buf.format(*i));
        }
        Value::Float(f) => {
            if f.is_nan() || f.is_infinite() {
                out.push_str("null");
            } else if *f == 0.0 {
                out.push('0');
            } else {
                write!(out, "{f}").expect("fmt to String is infallible");
            }
        }
        Value::None => out.push_str("null"),
        Value::List(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            let next_depth = depth + 1;
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                if let Some(sp) = indent {
                    out.push('\n');
                    write_indent(out, sp, next_depth);
                }
                serialize_value_to_json(item, out, indent, next_depth)?;
            }
            if let Some(sp) = indent {
                out.push('\n');
                write_indent(out, sp, depth);
            }
            out.push(']');
        }
        Value::Struct(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            out.push('{');
            let next_depth = depth + 1;
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            for (idx, key) in keys.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                if let Some(sp) = indent {
                    out.push('\n');
                    write_indent(out, sp, next_depth);
                }
                out.push('"');
                escape_json_into(key, out);
                if indent.is_some() {
                    out.push_str("\": ");
                } else {
                    out.push_str("\":");
                }
                if let Some(val) = map.get(*key) {
                    serialize_value_to_json(val, out, indent, next_depth)?;
                } else {
                    out.push_str("null");
                }
            }
            if let Some(sp) = indent {
                out.push('\n');
                write_indent(out, sp, depth);
            }
            out.push('}');
        }
        Value::Tmpl(_) => {
            return Err(TemplateError::syntax("cannot serialize template to JSON"));
        }
    }
    Ok(())
}

fn escape_json_into(s: &str, out: &mut String) {
    use core::fmt::Write;
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => {
                write!(out, "\\u{:04x}", c as u32).expect("fmt to String is infallible");
            }
            c => out.push(c),
        }
    }
}

const fn is_str_filter_kind(kind: FilterKind) -> bool {
    matches!(
        kind,
        FilterKind::Upper
            | FilterKind::Lower
            | FilterKind::Trim
            | FilterKind::EscapeXml
            | FilterKind::EscapeJson
            | FilterKind::SanitizeTokens
            | FilterKind::Fence
            | FilterKind::Quarantine
            | FilterKind::Sanitize
            | FilterKind::Truncate
    )
}

fn apply_str_filter_cow<'a>(
    filter: &ParsedFilter,
    input: Cow<'a, str>,
) -> Result<Cow<'a, str>, TemplateError> {
    let args = filter.args.as_deref();
    match filter.kind {
        FilterKind::Trim => match input {
            Cow::Borrowed(b) => Ok(Cow::Borrowed(b.trim())),
            Cow::Owned(o) => {
                let trimmed = o.trim();
                if trimmed.len() == o.len() {
                    Ok(Cow::Owned(o))
                } else {
                    Ok(Cow::Owned(trimmed.to_string()))
                }
            }
        },
        FilterKind::Upper => {
            if input
                .bytes()
                .any(|b| b.is_ascii_lowercase() || !b.is_ascii())
            {
                Ok(Cow::Owned(input.to_uppercase()))
            } else {
                Ok(input)
            }
        }
        FilterKind::Lower => {
            if input
                .bytes()
                .any(|b| b.is_ascii_uppercase() || !b.is_ascii())
            {
                Ok(Cow::Owned(input.to_lowercase()))
            } else {
                Ok(input)
            }
        }
        FilterKind::EscapeXml => Ok(match input {
            Cow::Borrowed(b) => escape_xml_str(b),
            Cow::Owned(o) => match escape_xml_str(&o) {
                Cow::Borrowed(_) => Cow::Owned(o),
                Cow::Owned(escaped) => Cow::Owned(escaped),
            },
        }),
        FilterKind::EscapeJson => Ok(match input {
            Cow::Borrowed(b) => escape_json_str(b),
            Cow::Owned(o) => match escape_json_str(&o) {
                Cow::Borrowed(_) => Cow::Owned(o),
                Cow::Owned(escaped) => Cow::Owned(escaped),
            },
        }),
        FilterKind::SanitizeTokens => Ok(match input {
            Cow::Borrowed(b) => sanitize_tokens_with_args(b, args)?,
            Cow::Owned(o) => match sanitize_tokens_with_args(&o, args)? {
                Cow::Borrowed(_) => Cow::Owned(o),
                Cow::Owned(sanitized) => Cow::Owned(sanitized),
            },
        }),
        FilterKind::Fence => Ok(Cow::Owned(fence_str(&input, args)?)),
        FilterKind::Quarantine => Ok(Cow::Owned(quarantine_str(&input, args)?)),
        FilterKind::Sanitize => sanitize_cow_with_mode(input, filter.sanitize_mode.as_ref(), args),
        FilterKind::Truncate => {
            let args_str = args
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    TemplateError::syntax("'truncate' requires at least a limit argument")
                })?;
            let (limit_str, marker_arg) = if let Some(comma_pos) = args_str.find(',') {
                (
                    args_str[..comma_pos].trim(),
                    Some(strip_quotes(args_str[comma_pos + 1..].trim())),
                )
            } else {
                (args_str, None)
            };
            let limit: usize = limit_str.parse().map_err(|e| {
                TemplateError::syntax(format!("'truncate' limit must be an integer: {e}"))
            })?;
            let res = truncate_middle_str(&input, limit, marker_arg.as_deref());
            Ok(match res {
                Cow::Borrowed(_) => input,
                Cow::Owned(o) => Cow::Owned(o),
            })
        }
        FilterKind::Fixed
        | FilterKind::Join
        | FilterKind::Limit
        | FilterKind::Add
        | FilterKind::Sub
        | FilterKind::ToJson => Err(TemplateError::syntax("unexpected non-string filter")),
    }
}

/// Fast path for rendering a string value through a chain of string filters directly into `output`
/// without cloning `Value::Str` or allocating intermediate wrapper strings.
pub(crate) fn try_apply_str_filters_into(
    s: &str,
    filters: &[crate::compiled::ParsedFilter],
    output: &mut String,
) -> Result<bool, TemplateError> {
    let Some((last, prefix)) = filters.split_last() else {
        return Ok(false);
    };
    if !filters.iter().all(|f| is_str_filter_kind(f.kind)) {
        return Ok(false);
    }
    let (fused_untrusted, active_prefix) = match prefix.split_last() {
        Some((prev, head))
            if last.kind == FilterKind::Quarantine
                && prev.kind == FilterKind::SanitizeTokens
                && prev.args.is_none() =>
        {
            (true, head)
        }
        _ => (false, prefix),
    };
    let mut cow = Cow::Borrowed(s);
    for f in active_prefix {
        cow = apply_str_filter_cow(f, cow)?;
    }
    let last_args = last.args.as_deref();
    match last.kind {
        FilterKind::Quarantine if fused_untrusted => {
            quarantine_untrusted_into(&cow, last_args, output)?;
        }
        FilterKind::Quarantine => quarantine_into(&cow, last_args, output)?,
        FilterKind::Sanitize => {
            sanitize_into_with_mode(&cow, last.sanitize_mode.as_ref(), last_args, output)?;
        }
        FilterKind::Fence => fence_into(&cow, last_args, output)?,
        FilterKind::Upper => crate::compiled::render::write_upper(&cow, output),
        FilterKind::Lower => crate::compiled::render::write_lower(&cow, output),
        FilterKind::Trim => output.push_str(cow.trim()),
        _ => {
            let final_cow = apply_str_filter_cow(last, cow)?;
            output.push_str(&final_cow);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
