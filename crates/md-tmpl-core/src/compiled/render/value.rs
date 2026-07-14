//! Rendering a resolved [`Value`] into an output buffer.

use alloc::string::String;

use crate::{error::TemplateError, value::Value};

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
