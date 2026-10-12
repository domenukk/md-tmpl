//! Exact fixed-precision float formatting for `{{ x | fixed(n) }}`.
//!
//! Semantics (shared byte-for-byte with the TypeScript backend):
//! - the **exact** decimal value of the binary `f64` is rounded half away from zero;
//! - a minus sign is written iff the value is `< 0` (so `-0.0` prints without a sign,
//!   while `-0.001` with precision 2 prints `-0.00`);
//! - `NaN`, `inf`, and `-inf` print as `NaN`, `inf`, and `-inf`.
//!
//! The common case uses exact `u128` integer math; extreme magnitudes or precisions
//! fall back to rounding the exact decimal expansion produced by `core::fmt`.

use alloc::string::String;
use core::fmt::Write;

/// Maximum precision handled by the exact `u128` fast path (`10^38 < 2^128`).
const MAX_FAST_FIXED_PRECISION: usize = 18;
/// Number of explicit mantissa bits in an IEEE-754 binary64.
const F64_MANTISSA_BITS: u32 = 52;
/// Mask of the biased exponent field after shifting out the mantissa.
const F64_EXPONENT_MASK: u64 = 0x7ff;
/// Exponent bias plus mantissa width: `value = mantissa * 2^(biased - 1075)`.
const F64_EXPONENT_OFFSET: i32 = 1075;
/// Exponent of the least significant mantissa bit of subnormal values.
const F64_SUBNORMAL_EXPONENT: i32 = -1074;
/// Maximum number of fractional decimal digits in the exact expansion of any `f64`.
const F64_MAX_EXACT_FRAC_DIGITS: usize = 1074;
/// Bit width of the `u128` fast-path accumulator.
const U128_BITS: u32 = 128;
/// Decimal radix.
const DECIMAL_RADIX: u128 = 10;
/// First digit (`'5'`) that rounds a truncated decimal expansion away from zero.
const ROUND_UP_DIGIT: u8 = b'5';

/// Decompose a finite, non-negative `f64` into `(mantissa, exponent)` such that
/// `value == mantissa * 2^exponent`.
fn decompose(abs: f64) -> (u64, i32) {
    let bits = abs.to_bits();
    let fraction = bits & ((1u64 << F64_MANTISSA_BITS) - 1);
    let biased = (bits >> F64_MANTISSA_BITS) & F64_EXPONENT_MASK;
    if biased == 0 {
        (fraction, F64_SUBNORMAL_EXPONENT)
    } else {
        let biased = i32::try_from(biased).expect("11-bit exponent fits i32");
        (
            fraction | (1u64 << F64_MANTISSA_BITS),
            biased - F64_EXPONENT_OFFSET,
        )
    }
}

/// Compute `round_half_away(abs * 10^precision)` exactly with `u128` math, or `None`
/// when the intermediate value does not fit.
fn exact_scaled_u128(abs: f64, precision: u32) -> Option<u128> {
    let scale = DECIMAL_RADIX.checked_pow(precision)?;
    let (mantissa, exponent) = decompose(abs);
    let product = u128::from(mantissa).checked_mul(scale)?;
    if exponent >= 0 {
        let shift = exponent.unsigned_abs();
        if shift >= U128_BITS || product.leading_zeros() < shift {
            return None;
        }
        return Some(product << shift);
    }
    let shift = exponent.unsigned_abs();
    if shift >= U128_BITS {
        // `product < 2^113 <= 2^(shift - 1)`: strictly below one half, rounds to zero.
        return Some(0);
    }
    let quotient = product >> shift;
    let remainder = product & ((1u128 << shift) - 1);
    let half = 1u128 << (shift - 1);
    Some(if remainder >= half {
        quotient + 1
    } else {
        quotient
    })
}

/// Write `scaled / 10^precision` with exactly `precision` fractional digits.
fn write_scaled(scaled: u128, precision: usize, output: &mut String) {
    let mut buf = itoa::Buffer::new();
    let digits = buf.format(scaled);
    if precision == 0 {
        output.push_str(digits);
        return;
    }
    if digits.len() > precision {
        let (int_part, frac_part) = digits.split_at(digits.len() - precision);
        output.push_str(int_part);
        output.push('.');
        output.push_str(frac_part);
    } else {
        output.push_str("0.");
        for _ in digits.len()..precision {
            output.push('0');
        }
        output.push_str(digits);
    }
}

/// Round the exact decimal expansion of `abs` half away from zero to `precision`
/// fractional digits (used when the `u128` fast path cannot represent the value).
fn write_rounded_exact_expansion(abs: f64, precision: usize, output: &mut String) {
    let mut exact = String::new();
    write!(exact, "{abs:.F64_MAX_EXACT_FRAC_DIGITS$}")
        .expect("fmt::Write for String is infallible");
    let (int_part, frac_part) = exact
        .split_once('.')
        .expect("formatting with non-zero precision always emits a decimal point");
    let frac_bytes = frac_part.as_bytes();
    let mut digits: alloc::vec::Vec<u8> = int_part.bytes().collect();
    let kept_frac = &frac_bytes[..precision.min(frac_bytes.len())];
    digits.extend_from_slice(kept_frac);
    digits.resize(int_part.len() + precision, b'0');
    if frac_bytes
        .get(precision)
        .is_some_and(|&d| d >= ROUND_UP_DIGIT)
    {
        let mut idx = digits.len();
        loop {
            if idx == 0 {
                digits.insert(0, b'1');
                break;
            }
            idx -= 1;
            if digits[idx] == b'9' {
                digits[idx] = b'0';
            } else {
                digits[idx] += 1;
                break;
            }
        }
    }
    let int_len = digits.len() - precision;
    let text = core::str::from_utf8(&digits).expect("decimal digits are ASCII");
    output.push_str(&text[..int_len]);
    if precision > 0 {
        output.push('.');
        output.push_str(&text[int_len..]);
    }
}

/// Write `f` with exactly `precision` fractional digits (see the module docs for the
/// rounding and sign rules).
#[inline]
pub fn write_fixed_float(f: f64, precision: usize, output: &mut String) {
    if !f.is_finite() {
        write!(output, "{f:.precision$}").expect("fmt::Write for String is infallible");
        return;
    }
    if f < 0.0 {
        output.push('-');
    }
    let abs = f.abs();
    let fast = if precision <= MAX_FAST_FIXED_PRECISION {
        let exp = u32::try_from(precision).expect("precision <= MAX_FAST_FIXED_PRECISION fits u32");
        exact_scaled_u128(abs, exp)
    } else {
        None
    };
    match fast {
        Some(scaled) => write_scaled(scaled, precision, output),
        None => write_rounded_exact_expansion(abs, precision, output),
    }
}
