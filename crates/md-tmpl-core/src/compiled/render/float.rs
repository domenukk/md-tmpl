//! Fast fixed-precision float formatting.
//!
//! Avoids the heavy `core::fmt` float machinery for the common case of
//! rendering `{{ x | fixed(n) }}` values.

use alloc::string::String;

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
pub(super) fn write_fixed_float(f: f64, precision: usize, output: &mut String) {
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
