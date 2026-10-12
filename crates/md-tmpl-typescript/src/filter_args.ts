/**
 * Filter argument parsing and numeric filter semantics that are byte-identical
 * to the Rust core (`str::parse::<usize>()`, `str::parse::<i64>()`,
 * `str::parse::<f64>()`, and `i64` saturating arithmetic), including the exact
 * `core::num` parse error messages.
 *
 * @module
 */

import { FILTER_ADD, FILTER_FIXED, FILTER_SUB, TYPE_INT } from "./consts.js";
import { TemplateSyntaxError } from "./errors.js";
import { formatFixedFloat, formatFixedInt } from "./numeric_format.js";
import { type FloatValue, type IntValue, float, int } from "./value.js";

/** `core::num::ParseIntError` message for an empty input. */
const PARSE_INT_EMPTY = "cannot parse integer from empty string";
/** `core::num::ParseIntError` message for a non-digit character. */
const PARSE_INT_INVALID_DIGIT = "invalid digit found in string";
/** `core::num::ParseIntError` message for a value above the target maximum. */
const PARSE_INT_POS_OVERFLOW = "number too large to fit in target type";
/** `core::num::ParseIntError` message for a value below the target minimum. */
const PARSE_INT_NEG_OVERFLOW = "number too small to fit in target type";
/** `core::num::ParseFloatError` message for an empty input. */
const PARSE_FLOAT_EMPTY = "cannot parse float from empty string";
/** `core::num::ParseFloatError` message for a malformed literal. */
const PARSE_FLOAT_INVALID = "invalid float literal";

/** `usize::MAX` on 64-bit targets. */
const USIZE_MAX = (1n << 64n) - 1n;
/** `i64::MAX`. */
export const I64_MAX = (1n << 63n) - 1n;
/** `i64::MIN`. */
export const I64_MIN = -(1n << 63n);

/** Unsigned decimal integer accepted by Rust's `usize` parser (optional `+`). */
const USIZE_RE = /^\+?[0-9]+$/;
/** Signed decimal integer accepted by Rust's `i64` parser. */
const I64_RE = /^[+-]?[0-9]+$/;
/** Grammar accepted by Rust's `f64` parser (`inf`/`infinity`/`nan` are case-insensitive). */
const F64_RE =
  /^[+-]?(?:inf|infinity|nan|(?:[0-9]+\.?[0-9]*|\.[0-9]+)(?:e[+-]?[0-9]+)?)$/i;
/** Lowercased special `f64` literals. */
const F64_INF_WORDS: ReadonlySet<string> = new Set(["inf", "infinity"]);
const F64_NAN_WORD = "nan";
const MINUS = "-";
const PLUS = "+";

/** Result of a Rust-compatible parse: the value, or the Rust error message. */
export type ParseOutcome<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly reason: string };

function parsed<T>(value: T): ParseOutcome<T> {
  return { ok: true, value };
}

function failed<T>(reason: string): ParseOutcome<T> {
  return { ok: false, reason };
}

/** Parse like Rust's `str::parse::<usize>()`. */
export function parseUsize(raw: string): ParseOutcome<number> {
  if (raw.length === 0) {
    return failed(PARSE_INT_EMPTY);
  }
  if (!USIZE_RE.test(raw)) {
    return failed(PARSE_INT_INVALID_DIGIT);
  }
  const big = BigInt(raw.startsWith(PLUS) ? raw.slice(1) : raw);
  if (big > USIZE_MAX) {
    return failed(PARSE_INT_POS_OVERFLOW);
  }
  return parsed(Number(big));
}

/** Parse like Rust's `str::parse::<i64>()`. */
export function parseI64(raw: string): ParseOutcome<bigint> {
  if (raw.length === 0) {
    return failed(PARSE_INT_EMPTY);
  }
  if (!I64_RE.test(raw)) {
    return failed(PARSE_INT_INVALID_DIGIT);
  }
  const big = BigInt(raw.startsWith(PLUS) ? raw.slice(1) : raw);
  if (big > I64_MAX) {
    return failed(PARSE_INT_POS_OVERFLOW);
  }
  if (big < I64_MIN) {
    return failed(PARSE_INT_NEG_OVERFLOW);
  }
  return parsed(big);
}

/** Parse like Rust's `str::parse::<f64>()`. */
export function parseF64(raw: string): ParseOutcome<number> {
  if (raw.length === 0) {
    return failed(PARSE_FLOAT_EMPTY);
  }
  if (!F64_RE.test(raw)) {
    return failed(PARSE_FLOAT_INVALID);
  }
  const negative = raw.startsWith(MINUS);
  const unsigned = (
    negative || raw.startsWith(PLUS) ? raw.slice(1) : raw
  ).toLowerCase();
  if (F64_INF_WORDS.has(unsigned)) {
    return parsed(negative ? -Infinity : Infinity);
  }
  if (unsigned === F64_NAN_WORD) {
    return parsed(NaN);
  }
  return parsed(Number(raw));
}

/** Clamp an arbitrary-precision integer to the `i64` range (Rust `saturating_*`). */
export function saturateI64(n: bigint): bigint {
  if (n > I64_MAX) return I64_MAX;
  if (n < I64_MIN) return I64_MIN;
  return n;
}

/** Numeric template value, as seen by the numeric filters. */
export type NumericOperand = IntValue | FloatValue;

/** Parsed `add` / `sub` argument: `i64` first, then `f64` (Rust `parse_num_arg`). */
export type ArithArg = bigint | number;

/** Arithmetic numeric filters. */
export type ArithFilter = typeof FILTER_ADD | typeof FILTER_SUB;

/** Parse an `add` / `sub` argument exactly like Rust (`i64`, falling back to `f64`). */
export function parseArithArg(
  raw: string | undefined,
  filter: ArithFilter,
): ArithArg {
  if (raw === undefined) {
    throw new TemplateSyntaxError(`'${filter}' requires a number argument`);
  }
  const asInt = parseI64(raw);
  if (asInt.ok) {
    return asInt.value;
  }
  const asFloat = parseF64(raw);
  if (!asFloat.ok) {
    throw new TemplateSyntaxError(
      `'${filter}' argument must be a number: ${asFloat.reason}`,
    );
  }
  return asFloat.value;
}

/**
 * Apply `add(n)` / `sub(n)` with Rust semantics: `int ± int` saturates at the
 * `i64` bounds and stays `int`; any float operand produces a `float`.
 */
export function applyArith(
  operand: NumericOperand,
  filter: ArithFilter,
  arg: ArithArg,
): NumericOperand {
  if (operand.type === TYPE_INT && typeof arg === "bigint") {
    const lhs = BigInt(operand.value);
    const result = filter === FILTER_ADD ? lhs + arg : lhs - arg;
    return int(Number(saturateI64(result)));
  }
  const rhs = Number(arg);
  return float(
    filter === FILTER_ADD ? operand.value + rhs : operand.value - rhs,
  );
}

/** Parse a required `usize` filter argument, reporting `'<filter>' <what> must be an integer: <reason>`. */
export function parseUsizeFilterArg(
  raw: string,
  filter: string,
  what: string,
): number {
  const outcome = parseUsize(raw);
  if (!outcome.ok) {
    throw new TemplateSyntaxError(
      `'${filter}' ${what} must be an integer: ${outcome.reason}`,
    );
  }
  return outcome.value;
}

/** Argument labels used in `'<filter>' <label> must be an integer` errors. */
export const FIXED_ARG_LABEL = "precision";
export const LIMIT_ARG_LABEL = "argument";
export const TRUNCATE_ARG_LABEL = "limit";

/** Apply `fixed(precision)` with Rust semantics (strict `usize` precision, exact half-to-even rounding). */
export function applyFixed(
  operand: NumericOperand,
  rawArg: string | undefined,
): string {
  if (rawArg === undefined) {
    throw new TemplateSyntaxError(`'${FILTER_FIXED}' requires precision arg`);
  }
  const precision = parseUsizeFilterArg(rawArg, FILTER_FIXED, FIXED_ARG_LABEL);
  return operand.type === TYPE_INT
    ? formatFixedInt(operand.value, precision)
    : formatFixedFloat(operand.value, precision);
}
