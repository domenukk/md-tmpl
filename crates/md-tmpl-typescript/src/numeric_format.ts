/**
 * Numeric formatting that is byte-identical to the Rust core.
 *
 * - {@link formatFloatDisplay} mirrors Rust's `Display` for `f64` (shortest
 *   round-trip digits, never exponent notation, `inf` / `-inf` / `NaN`).
 * - {@link formatIntDisplay} prints integers positionally (no `1e+21`).
 * - {@link formatFixedFloat} mirrors Rust's `write_fixed_float`: exact
 *   decimal expansion of the binary value, rounded half away from zero.
 *
 * @module
 */

/** Rust `Display` text for `f64::NAN`. */
const FLOAT_NAN_TEXT = "NaN";
/** Rust `Display` text for `f64::INFINITY`. */
const FLOAT_INF_TEXT = "inf";
/** Sign prefix for negative numbers. */
const MINUS_SIGN = "-";
/** Decimal separator. */
const DECIMAL_POINT = ".";
/** Exponent marker in JavaScript's `Number#toString` output. */
const EXPONENT_MARKER = "e";
/** Zero digit used for padding. */
const ZERO_DIGIT = "0";

/** IEEE-754 binary64 layout. */
const F64_BYTES = 8;
const F64_MANTISSA_BITS = 52n;
const F64_EXPONENT_MASK = 0x7ffn;
const F64_EXPONENT_BIAS = 1075n;
const F64_SUBNORMAL_EXPONENT = -1074n;
const F64_IMPLICIT_BIT = 1n << F64_MANTISSA_BITS;
const F64_MANTISSA_MASK = F64_IMPLICIT_BIT - 1n;

const BIGINT_ZERO = 0n;
const BIGINT_ONE = 1n;
const BIGINT_TWO = 2n;
const BIGINT_FIVE = 5n;
const BIGINT_TEN = 10n;

/** Returns `true` if `x` has its sign bit set (including `-0`). */
function isSignNegative(x: number): boolean {
  return x < 0 || Object.is(x, -0);
}

/** Format a non-finite float exactly like Rust's `Display` (`NaN`, `inf`, `-inf`). */
function formatNonFinite(x: number): string {
  if (Number.isNaN(x)) {
    return FLOAT_NAN_TEXT;
  }
  return x > 0 ? FLOAT_INF_TEXT : `${MINUS_SIGN}${FLOAT_INF_TEXT}`;
}

/**
 * Format a float exactly like Rust's `Display` for `f64`: the shortest digits
 * that round-trip, always in positional notation (`1e21` → `"1000000000000000000000"`,
 * `1e-7` → `"0.0000001"`, `1.0` → `"1"`, `-0.0` → `"-0"`).
 */
export function formatFloatDisplay(x: number): string {
  if (!Number.isFinite(x)) {
    return formatNonFinite(x);
  }
  const sign = isSignNegative(x) ? MINUS_SIGN : "";
  const repr = String(Math.abs(x));
  const expIdx = repr.indexOf(EXPONENT_MARKER);
  if (expIdx === -1) {
    return `${sign}${repr}`;
  }
  const mantissa = repr.slice(0, expIdx);
  const exponent = Number(repr.slice(expIdx + 1));
  const pointIdx = mantissa.indexOf(DECIMAL_POINT);
  const intDigits = pointIdx === -1 ? mantissa : mantissa.slice(0, pointIdx);
  const fracDigits = pointIdx === -1 ? "" : mantissa.slice(pointIdx + 1);
  const digits = `${intDigits}${fracDigits}`;
  const pointPos = intDigits.length + exponent;
  if (pointPos <= 0) {
    return `${sign}${ZERO_DIGIT}${DECIMAL_POINT}${ZERO_DIGIT.repeat(-pointPos)}${digits}`;
  }
  if (pointPos >= digits.length) {
    return `${sign}${digits}${ZERO_DIGIT.repeat(pointPos - digits.length)}`;
  }
  return `${sign}${digits.slice(0, pointPos)}${DECIMAL_POINT}${digits.slice(pointPos)}`;
}

/** Format an integer value positionally (never `1e+21`), matching Rust's `i64` display. */
export function formatIntDisplay(n: number): string {
  return Number.isSafeInteger(n) ? String(n) : BigInt(n).toString();
}

/** Decompose a finite, non-negative float into `[mantissa, exponent]` with `x = mantissa * 2^exponent`. */
function decomposeFinite(x: number): readonly [bigint, bigint] {
  const view = new DataView(new ArrayBuffer(F64_BYTES));
  view.setFloat64(0, x);
  const bits = view.getBigUint64(0);
  const biasedExp = (bits >> F64_MANTISSA_BITS) & F64_EXPONENT_MASK;
  const fraction = bits & F64_MANTISSA_MASK;
  if (biasedExp === BIGINT_ZERO) {
    return [fraction, F64_SUBNORMAL_EXPONENT];
  }
  return [fraction | F64_IMPLICIT_BIT, biasedExp - F64_EXPONENT_BIAS];
}

/** Round `numerator / denominator` (both non-negative) to the nearest integer, ties away from zero. */
function divRoundHalfAway(numerator: bigint, denominator: bigint): bigint {
  const quotient = numerator / denominator;
  const twiceRemainder = (numerator % denominator) * BIGINT_TWO;
  return twiceRemainder >= denominator ? quotient + BIGINT_ONE : quotient;
}

/**
 * Format a float with exactly `precision` fractional digits, byte-identical to
 * Rust's `write_fixed_float`: the exact binary value is rounded half away from
 * zero, a minus sign is written iff the value is `< 0` (so `-0` prints without
 * a sign), and non-finite values print as `NaN` / `inf` / `-inf`.
 */
export function formatFixedFloat(x: number, precision: number): string {
  if (!Number.isFinite(x)) {
    return formatNonFinite(x);
  }
  const sign = x < 0 ? MINUS_SIGN : "";
  const [mantissa, exponent] = decomposeFinite(Math.abs(x));
  const scale = BIGINT_TEN ** BigInt(precision);
  let scaled: bigint;
  if (exponent >= BIGINT_ZERO) {
    scaled = (mantissa << exponent) * scale;
  } else {
    const fracDigits = -exponent;
    const exactDecimal = mantissa * BIGINT_FIVE ** fracDigits;
    scaled = divRoundHalfAway(exactDecimal * scale, BIGINT_TEN ** fracDigits);
  }
  if (precision === 0) {
    return `${sign}${scaled.toString()}`;
  }
  const digits = scaled.toString().padStart(precision + 1, ZERO_DIGIT);
  const pointPos = digits.length - precision;
  return `${sign}${digits.slice(0, pointPos)}${DECIMAL_POINT}${digits.slice(pointPos)}`;
}

/** Format an integer with `precision` zero fractional digits, matching Rust's `write_fixed_int`. */
export function formatFixedInt(n: number, precision: number): string {
  const intText = formatIntDisplay(n);
  if (precision === 0) {
    return intText;
  }
  return `${intText}${DECIMAL_POINT}${ZERO_DIGIT.repeat(precision)}`;
}
