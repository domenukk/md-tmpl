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

/// Pre-computed integer powers of 10 for precision 0..=18.
const POW10_U64: [u64; 19] = {
    let mut table = [1u64; 19];
    let mut i = 1;
    while i < 19 {
        table[i] = table[i - 1] * 10;
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
pub fn write_fixed_float(f: f64, precision: usize, output: &mut String) {
    /// Convert a known-positive, finite `f64` to `u64` via IEEE-754 bit extraction.
    #[inline]
    fn positive_f64_to_u64(v: f64) -> u64 {
        debug_assert!(v >= 0.0 && v.is_finite());
        let bits = v.to_bits();
        if (bits >> 63) != 0 || v.is_nan() {
            return 0;
        }
        let biased_exp = ((bits >> 52) & 0x7ff) as u32;
        if biased_exp < 1023 {
            return 0;
        }
        let exp = biased_exp - 1023;
        if exp >= 64 {
            return u64::MAX;
        }
        let mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
        if exp >= 52 {
            mantissa << (exp - 52)
        } else {
            mantissa >> (52 - exp)
        }
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

    // Split into integer and fractional parts using precomputed u64 power of 10.
    let divisor = POW10_U64[precision];
    let int_part = scaled / divisor;
    let frac_part = scaled % divisor;

    if is_neg {
        output.push('-');
    }

    let mut buf = itoa::Buffer::new();
    output.push_str(buf.format(int_part));
    output.push('.');

    let frac_str = buf.format(frac_part);
    if precision > frac_str.len() {
        for _ in 0..(precision - frac_str.len()) {
            output.push('0');
        }
    }
    output.push_str(frac_str);
}
