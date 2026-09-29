//! Rendering a resolved [`Value`] into an output buffer.

use alloc::string::String;

use crate::{error::TemplateError, value::Value};

/// Write an integer directly into `output` using `itoa` without heap allocation.
#[inline]
pub fn write_int(i: i64, output: &mut String) {
    let mut buf = itoa::Buffer::new();
    output.push_str(buf.format(i));
}

/// Write a floating-point number directly into `output`, normalizing `-0.0` to `"0"`.
#[inline]
pub fn write_float(f: f64, output: &mut String) {
    use core::fmt::Write;
    // Normalize negative zero: std formats `-0.0` as "-0", but the TS
    // backend (and `String(-0)`) render "0". Emit "0" for both zeros so
    // the two engines stay byte-for-byte identical. `-0.0 == 0.0` is
    // true under IEEE-754, so this catches both signs.
    if f == 0.0 {
        output.push('0');
    } else {
        // SAFETY: `fmt::Write for String` is infallible — it only
        // forwards to `String::push_str` which cannot fail.
        write!(output, "{f}").expect("fmt::Write for String is infallible");
    }
}

/// Write an integer formatted with fixed decimal precision (`i.00...`) directly into `output`.
#[inline]
pub fn write_fixed_int(i: i64, precision: usize, output: &mut String) {
    let mut buf = itoa::Buffer::new();
    output.push_str(buf.format(i));
    if precision > 0 {
        output.push('.');
        for _ in 0..precision {
            output.push('0');
        }
    }
}

/// Write `s` converted to uppercase directly into `output` without intermediate `String` allocation.
#[inline]
pub fn write_upper(s: &str, output: &mut String) {
    if s.bytes().any(|b| b.is_ascii_lowercase() || !b.is_ascii()) {
        output.reserve(s.len());
        for c in s.chars() {
            for u in c.to_uppercase() {
                output.push(u);
            }
        }
    } else {
        output.push_str(s);
    }
}

/// Write `s` converted to lowercase directly into `output` without intermediate `String` allocation.
#[inline]
pub fn write_lower(s: &str, output: &mut String) {
    if s.bytes().any(|b| b.is_ascii_uppercase() || !b.is_ascii()) {
        output.reserve(s.len());
        for c in s.chars() {
            for l in c.to_lowercase() {
                output.push(l);
            }
        }
    } else {
        output.push_str(s);
    }
}

/// Write a rendered [`Value`] directly into an output buffer,
/// avoiding an intermediate `String` allocation.
#[inline]
pub(super) fn render_value_into(val: &Value, output: &mut String) -> Result<(), TemplateError> {
    match val {
        Value::Str(s) => output.push_str(s),
        // Direct push avoids the `write!` → `fmt` machinery.
        Value::Bool(true) => output.push_str(crate::consts::LIT_TRUE),
        Value::Bool(false) => output.push_str(crate::consts::LIT_FALSE),
        // itoa/ryu are ~3x faster than `write!` for number formatting.
        Value::Int(i) => write_int(*i, output),
        // Float formatting via Display — benchmarks show it's faster
        // than ryu+strip_suffix for whole numbers (the common case).
        Value::Float(f) => write_float(*f, output),
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
