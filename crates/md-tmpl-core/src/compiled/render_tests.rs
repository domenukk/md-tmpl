use core::cmp::Ordering;

use super::{
    condition::{cmp_int_float, decompose_f64},
    float::write_fixed_float,
};

// -- cmp_int_float: NaN --

#[test]
fn cmp_int_float_nan_returns_none() {
    assert_eq!(cmp_int_float(0, f64::NAN), None);
    assert_eq!(cmp_int_float(i64::MAX, f64::NAN), None);
    assert_eq!(cmp_int_float(i64::MIN, f64::NAN), None);
}

// -- cmp_int_float: infinity --

#[test]
fn cmp_int_float_positive_infinity() {
    // Any integer is less than +∞.
    assert_eq!(cmp_int_float(0, f64::INFINITY), Some(Ordering::Less));
    assert_eq!(cmp_int_float(i64::MAX, f64::INFINITY), Some(Ordering::Less));
    assert_eq!(cmp_int_float(i64::MIN, f64::INFINITY), Some(Ordering::Less));
}

#[test]
fn cmp_int_float_negative_infinity() {
    // Any integer is greater than -∞.
    assert_eq!(cmp_int_float(0, f64::NEG_INFINITY), Some(Ordering::Greater));
    assert_eq!(
        cmp_int_float(i64::MIN, f64::NEG_INFINITY),
        Some(Ordering::Greater)
    );
}

// -- cmp_int_float: exact equality --

#[test]
fn cmp_int_float_exact_zero() {
    assert_eq!(cmp_int_float(0, 0.0), Some(Ordering::Equal));
    assert_eq!(cmp_int_float(0, -0.0), Some(Ordering::Equal));
}

#[test]
fn cmp_int_float_exact_integer_values() {
    assert_eq!(cmp_int_float(1, 1.0), Some(Ordering::Equal));
    assert_eq!(cmp_int_float(-1, -1.0), Some(Ordering::Equal));
    assert_eq!(cmp_int_float(100, 100.0), Some(Ordering::Equal));
}

// -- cmp_int_float: fractional values --

#[test]
fn cmp_int_float_integer_less_than_float_with_fraction() {
    // 1 < 1.5
    assert_eq!(cmp_int_float(1, 1.5), Some(Ordering::Less));
}

#[test]
fn cmp_int_float_integer_greater_than_float_with_fraction() {
    // 2 > 1.5
    assert_eq!(cmp_int_float(2, 1.5), Some(Ordering::Greater));
}

#[test]
fn cmp_int_float_negative_fraction() {
    // -2 < -1.5 (i.e., -2 is more negative)
    assert_eq!(cmp_int_float(-2, -1.5), Some(Ordering::Less));
    // -1 > -1.5
    assert_eq!(cmp_int_float(-1, -1.5), Some(Ordering::Greater));
}

// -- cmp_int_float: extreme values --

#[test]
fn cmp_int_float_i64_max() {
    // i64::MAX (2^63-1) cannot be represented exactly in f64.
    // i64::MAX as f64 rounds to 2^63, so i64::MAX < (i64::MAX as f64).
    // Using the numeric constant directly to avoid an i64→f64 cast lint.
    let f: f64 = 9_223_372_036_854_775_808.0; // 2^63 (rounded i64::MAX)
    assert_eq!(cmp_int_float(i64::MAX, f), Some(Ordering::Less));
}

#[test]
fn cmp_int_float_i64_min() {
    // i64::MIN (-2^63) CAN be represented exactly in f64.
    // Using the numeric constant directly to avoid an i64→f64 cast lint.
    let f: f64 = -9_223_372_036_854_775_808.0; // i64::MIN
    assert_eq!(cmp_int_float(i64::MIN, f), Some(Ordering::Equal));
}

// -- cmp_int_float: subnormals --

#[test]
fn cmp_int_float_subnormal() {
    // Subnormal numbers are very close to zero but not zero.
    let subnormal = f64::MIN_POSITIVE / 2.0;
    assert!(subnormal > 0.0 && subnormal < f64::MIN_POSITIVE);
    // 0 < subnormal (subnormal has a fractional part, int part = 0)
    assert_eq!(cmp_int_float(0, subnormal), Some(Ordering::Less));
    // 1 > subnormal
    assert_eq!(cmp_int_float(1, subnormal), Some(Ordering::Greater));
}

// -- decompose_f64 --

#[test]
fn decompose_f64_zero() {
    let (int_part, has_frac, negative) = decompose_f64(0.0);
    assert_eq!(int_part, 0);
    assert!(!has_frac);
    assert!(!negative);
}

#[test]
fn decompose_f64_negative_zero() {
    let (int_part, has_frac, negative) = decompose_f64(-0.0);
    assert_eq!(int_part, 0);
    assert!(!has_frac);
    assert!(negative);
}

#[test]
fn decompose_f64_positive_integer() {
    let (int_part, has_frac, negative) = decompose_f64(42.0);
    assert_eq!(int_part, 42);
    assert!(!has_frac);
    assert!(!negative);
}

#[test]
fn decompose_f64_negative_with_fraction() {
    let (int_part, has_frac, negative) = decompose_f64(-2.78);
    assert_eq!(int_part, 2);
    assert!(has_frac);
    assert!(negative);
}

#[test]
fn decompose_f64_subnormal() {
    let subnormal = f64::MIN_POSITIVE / 2.0;
    let (int_part, has_frac, negative) = decompose_f64(subnormal);
    assert_eq!(int_part, 0);
    assert!(has_frac); // subnormals are tiny fractions
    assert!(!negative);
}

#[test]
fn decompose_f64_exact_float_int_boundary() {
    // 2^52 is the largest integer where all integers up to it are
    // exactly representable in f64.
    // Using the numeric constant directly to avoid a u64→f64 cast lint.
    let boundary: f64 = 4_503_599_627_370_496.0; // 2^52
    let (int_part, has_frac, negative) = decompose_f64(boundary);
    assert_eq!(int_part, 1_u64 << 52);
    assert!(!has_frac);
    assert!(!negative);
}

#[test]
fn decompose_f64_value_less_than_one() {
    let (int_part, has_frac, negative) = decompose_f64(0.5);
    assert_eq!(int_part, 0);
    assert!(has_frac);
    assert!(!negative);
}

// ---------------------------------------------------------------------------
// write_fixed_float tests
// ---------------------------------------------------------------------------

/// Helper: format a float with the fast path and return the string.
fn fixed(f: f64, precision: usize) -> String {
    let mut out = String::new();
    write_fixed_float(f, precision, &mut out);
    out
}

/// Helper: format with std for reference.
fn fixed_std(f: f64, precision: usize) -> String {
    format!("{f:.precision$}")
}

#[test]
fn write_fixed_float_basic_positive() {
    assert_eq!(fixed(98.7, 1), "98.7");
    assert_eq!(fixed(45.2, 1), "45.2");
    assert_eq!(fixed(12.3, 1), "12.3");
    assert_eq!(fixed(3.14258, 2), "3.14");
}

#[test]
fn write_fixed_float_rounding() {
    assert_eq!(fixed(1.25, 1), "1.3"); // rounds up
    assert_eq!(fixed(1.35, 1), "1.4"); // rounds up
    assert_eq!(fixed(1.45, 1), "1.5"); // rounds up
    assert_eq!(fixed(2.999, 2), "3.00"); // rounds to 3.00
}

#[test]
fn write_fixed_float_zero_precision() {
    assert_eq!(fixed(3.7, 0), "4"); // rounds to 4
    assert_eq!(fixed(3.2, 0), "3"); // rounds to 3
    assert_eq!(fixed(0.0, 0), "0");
}

#[test]
fn write_fixed_float_negative() {
    assert_eq!(fixed(-1.5, 1), "-1.5");
    assert_eq!(fixed(-0.5, 2), "-0.50");
    assert_eq!(fixed(-99.999, 2), "-100.00");
}

#[test]
fn write_fixed_float_zero() {
    assert_eq!(fixed(0.0, 1), "0.0");
    assert_eq!(fixed(0.0, 3), "0.000");
    assert_eq!(fixed(-0.0, 2), "0.00"); // negative zero → no minus
}

#[test]
fn write_fixed_float_leading_zeros_in_fraction() {
    assert_eq!(fixed(1.001, 3), "1.001");
    assert_eq!(fixed(1.01, 3), "1.010");
    assert_eq!(fixed(1.0001, 4), "1.0001");
}

#[test]
fn write_fixed_float_matches_std() {
    // Verify our fast path matches std for common precision values.
    let values = [0.0, 1.0, -1.0, 3.14258, 98.7, 45.2, 12.3, 0.001, 999.999];
    for &v in &values {
        for p in 0..=6 {
            assert_eq!(fixed(v, p), fixed_std(v, p), "mismatch for fixed({v}, {p})");
        }
    }
}

// ---------------------------------------------------------------------------
// try_fast_path_fixed_filter tests — integers through the template pipeline
// ---------------------------------------------------------------------------

/// Build a template `{{ val | fixed(n) }}` and render it with a given i64.
fn render_fixed_int(int_val: i64, precision: usize) -> String {
    use crate::{Template, ctx};
    let src =
        alloc::format!("---\nparams:\n  - val = int\n---\n{{{{ val | fixed({precision}) }}}}");
    let tmpl = Template::from_source(&src).expect("compile");
    tmpl.render_ctx(&ctx! { val: int_val }).expect("render")
}

/// Reference: what `apply_filter_typed(Fixed, ...)` produces for an integer.
fn fixed_int_general(int_val: i64, precision: usize) -> String {
    if precision == 0 {
        alloc::format!("{int_val}")
    } else {
        let mut s = alloc::format!("{int_val}.");
        for _ in 0..precision {
            s.push('0');
        }
        s
    }
}

#[test]
fn fast_path_fixed_int_small_values() {
    assert_eq!(render_fixed_int(0, 1), "0.0");
    assert_eq!(render_fixed_int(42, 2), "42.00");
    assert_eq!(render_fixed_int(-7, 1), "-7.0");
    assert_eq!(render_fixed_int(1, 0), "1");
    assert_eq!(render_fixed_int(0, 0), "0");
}

#[test]
fn fast_path_fixed_int_large_values_no_precision_loss() {
    // 2^53 + 1: would round if cast to f64 (the original bug).
    let big = 9_007_199_254_740_993_i64;
    assert_eq!(render_fixed_int(big, 1), fixed_int_general(big, 1));
    assert_eq!(render_fixed_int(big, 0), fixed_int_general(big, 0));

    // i64::MAX
    assert_eq!(
        render_fixed_int(i64::MAX, 2),
        fixed_int_general(i64::MAX, 2)
    );

    // i64::MIN
    assert_eq!(
        render_fixed_int(i64::MIN, 1),
        fixed_int_general(i64::MIN, 1)
    );
}

#[test]
fn fast_path_fixed_int_matches_general_path() {
    // Sweep representative values and precisions to check parity.
    let values = [
        0,
        1,
        -1,
        100,
        -100,
        i64::MAX,
        i64::MIN,
        9_007_199_254_740_993,
    ];
    for &v in &values {
        for p in 0..=4 {
            assert_eq!(
                render_fixed_int(v, p),
                fixed_int_general(v, p),
                "mismatch for fixed({v}, {p})"
            );
        }
    }
}
