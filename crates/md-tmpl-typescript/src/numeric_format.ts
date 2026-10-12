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
const F64_MANTISSA_BITS = 52n;
const F64_EXPONENT_MASK = 0x7ffn;
const F64_EXPONENT_BIAS = 1075n;
const F64_SUBNORMAL_EXPONENT = -1074n;
const F64_IMPLICIT_BIT = 1n << F64_MANTISSA_BITS;
const F64_MANTISSA_MASK = F64_IMPLICIT_BIT - 1n;

const BIGINT_ZERO = 0n;
const BIGINT_ONE = 1n;
const BIGINT_TEN = 10n;

/** Bounds within which `Number#toString` never emits exponential notation. */
const POSITIONAL_FLOAT_MIN = 1e-6;
const POSITIONAL_FLOAT_MAX = 1e21;

/** Reusable typed-array views for zero-allocation IEEE-754 binary64 bit extraction. */
const F64_SCRATCH_ARRAY = new Float64Array(1);
const U32_SCRATCH_ARRAY = new Uint32Array(F64_SCRATCH_ARRAY.buffer);
const U64_SCRATCH_ARRAY = new BigUint64Array(F64_SCRATCH_ARRAY.buffer);

/** Pre-computed powers of 10 (`10n ** BigInt(p)`) for common precisions `0..=20`. */
const POW10_TABLE: readonly bigint[] = Array.from(
  { length: 21 },
  (_, i) => BIGINT_TEN ** BigInt(i),
);
const POW10_NUM_TABLE = [1, 10, 100, 1000, 10000, 100000, 1000000] as const;
const POW2_NUM_TABLE: readonly number[] = Array.from(
  { length: 54 },
  (_, i) => 2 ** i,
);
const ZERO_PAD_TABLE: readonly string[] = Array.from({ length: 21 }, (_, i) =>
  ZERO_DIGIT.repeat(i),
);
const MAX_FAST_FIXED_PREC = 6;
const MAX_FAST_FIXED_ABS = 67108864;
const LIMB_26_BASE = 67108864;
const LIMB_26_HALF = 33554432;
const LIMB_26_MASK = 0x3ffffff;
const F64_HI_MANTISSA_MASK = 0xfffff;
const F64_HI_IMPLICIT_BIT = 0x100000;
const F64_EXPONENT_BIAS_NUM = 1075;

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
  const abs = Math.abs(x);
  if (abs >= POSITIONAL_FLOAT_MIN && abs < POSITIONAL_FLOAT_MAX) {
    return String(x);
  }
  if (!Number.isFinite(x)) {
    return formatNonFinite(x);
  }
  const sign = isSignNegative(x) ? MINUS_SIGN : "";
  const repr = String(abs);
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
  F64_SCRATCH_ARRAY[0] = x;
  const bits = U64_SCRATCH_ARRAY[0] ?? BIGINT_ZERO;
  const biasedExp = (bits >> F64_MANTISSA_BITS) & F64_EXPONENT_MASK;
  const fraction = bits & F64_MANTISSA_MASK;
  if (biasedExp === BIGINT_ZERO) {
    return [fraction, F64_SUBNORMAL_EXPONENT];
  }
  return [fraction | F64_IMPLICIT_BIT, biasedExp - F64_EXPONENT_BIAS];
}

/**
 * Format a float with exactly `precision` fractional digits, byte-identical to
 * Rust's `write_fixed_float`: the exact binary value is rounded half away from
 * zero, a minus sign is written iff the value is `< 0` (so `-0` prints without
 * a sign), and non-finite values print as `NaN` / `inf` / `-inf`.
 */
export function formatFixedFloat(x: number, precision: number): string {
  if (x === 0) {
    return precision === 0
      ? ZERO_DIGIT
      : `${ZERO_DIGIT}${DECIMAL_POINT}${ZERO_PAD_TABLE[precision] ?? ZERO_DIGIT.repeat(precision)}`;
  }
  const abs = x < 0 ? -x : x;
  if (
    precision <= MAX_FAST_FIXED_PREC &&
    abs >= POSITIONAL_FLOAT_MIN &&
    abs < MAX_FAST_FIXED_ABS
  ) {
    const scale = POW10_NUM_TABLE[precision] ?? 1;
    const prod = abs * scale;
    const fl = Math.floor(prod);
    const frac = prod - fl;
    let scaled: number | undefined;
    if (frac < 0.499) {
      scaled = fl;
    } else if (frac > 0.501) {
      scaled = fl + 1;
    } else {
      F64_SCRATCH_ARRAY[0] = abs;
      const lo = U32_SCRATCH_ARRAY[0] ?? 0;
      const hi = U32_SCRATCH_ARRAY[1] ?? 0;
      const biasedExp = (hi >>> 20) & 0x7ff;
      if (biasedExp > 0 && biasedExp < 0x7ff) {
        const shift = F64_EXPONENT_BIAS_NUM - biasedExp;
        if (shift >= 26 && shift <= 78) {
          const mLo = lo & LIMB_26_MASK;
          const mHi =
            ((hi & F64_HI_MANTISSA_MASK) | F64_HI_IMPLICIT_BIT) * 64 +
            (lo >>> 26);
          const pLo = mLo * scale;
          const pLoCarry = Math.floor(pLo / LIMB_26_BASE);
          const pHi = mHi * scale + pLoCarry;
          const s = shift - 26;
          if (s === 0) {
            scaled =
              pHi + (pLo - pLoCarry * LIMB_26_BASE >= LIMB_26_HALF ? 1 : 0);
          } else {
            const half = POW2_NUM_TABLE[s - 1] ?? 1;
            const div = POW2_NUM_TABLE[s] ?? 2;
            scaled = Math.floor((pHi + half) / div);
          }
        }
      }
    }
    if (scaled !== undefined) {
      const sign = x < 0 ? MINUS_SIGN : "";
      if (precision === 0) {
        return `${sign}${String(scaled)}`;
      }
      if (precision === 1) {
        const intPart = Math.floor(scaled / 10);
        const fracPart = scaled - intPart * 10;
        return `${sign}${String(intPart)}${DECIMAL_POINT}${String(fracPart)}`;
      }
      const raw = String(scaled);
      if (raw.length > precision) {
        const pt = raw.length - precision;
        return `${sign}${raw.slice(0, pt)}${DECIMAL_POINT}${raw.slice(pt)}`;
      }
      return `${sign}${ZERO_DIGIT}${DECIMAL_POINT}${ZERO_PAD_TABLE[precision - raw.length] ?? ""}${raw}`;
    }
  }
  if (!Number.isFinite(x)) {
    return formatNonFinite(x);
  }
  const sign = x < 0 ? MINUS_SIGN : "";
  const [mantissa, exponent] = decomposeFinite(abs);
  const scale = POW10_TABLE[precision] ?? BIGINT_TEN ** BigInt(precision);
  const product = mantissa * scale;
  let scaled: bigint;
  if (exponent >= BIGINT_ZERO) {
    scaled = product << exponent;
  } else {
    const shift = -exponent;
    scaled = (product + (BIGINT_ONE << (shift - BIGINT_ONE))) >> shift;
  }
  if (precision === 0) {
    return `${sign}${scaled.toString()}`;
  }
  const rawDigits = scaled.toString();
  if (rawDigits.length > precision) {
    const pointPos = rawDigits.length - precision;
    return `${sign}${rawDigits.slice(0, pointPos)}${DECIMAL_POINT}${rawDigits.slice(pointPos)}`;
  }
  return `${sign}${ZERO_DIGIT}${DECIMAL_POINT}${ZERO_DIGIT.repeat(precision - rawDigits.length)}${rawDigits}`;
}

/** Format an integer with `precision` zero fractional digits, matching Rust's `write_fixed_int`. */
export function formatFixedInt(n: number, precision: number): string {
  const intText = formatIntDisplay(n);
  if (precision === 0) {
    return intText;
  }
  return `${intText}${DECIMAL_POINT}${ZERO_PAD_TABLE[precision] ?? ZERO_DIGIT.repeat(precision)}`;
}
