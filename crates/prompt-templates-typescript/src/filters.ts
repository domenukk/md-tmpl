/**
 * Built-in expression filters.
 *
 * Filters transform values in pipe chains: `{{ expr | filter | filter }}`.
 * Each filter is a pure function: `(value, args?) → value`.
 *
 * @module
 */

import { type Value, str, int, float, list, display } from "./value.js";
import { TemplateSyntaxError, UnknownFilterError } from "./errors.js";

/** Parse a filter expression like `fixed(2)` into `[name, args?]`. */
export function parseFilter(filter: string): [string, string | undefined] {
  const trimmed = filter.trim();
  const parenIdx = trimmed.indexOf("(");
  if (parenIdx === -1) {
    return [trimmed, undefined];
  }
  const name = trimmed.slice(0, parenIdx).trim();
  let args = trimmed.slice(parenIdx + 1);
  if (args.endsWith(")")) {
    args = args.slice(0, -1);
  }
  args = args.trim();
  return [name, args.length === 0 ? undefined : args];
}

/** Strip surrounding quotes from a filter argument. */
function stripQuotes(s: string): string {
  if (s.length >= 2) {
    if (
      (s.startsWith('"') && s.endsWith('"')) ||
      (s.startsWith("'") && s.endsWith("'"))
    ) {
      return s.slice(1, -1);
    }
  }
  return s;
}

/** Apply a named filter to a value. */
export function applyFilter(
  value: Value,
  filterName: string,
  args: string | undefined,
): Value {
  switch (filterName) {
    case "upper":
      return applyUpper(value);
    case "lower":
      return applyLower(value);
    case "trim":
      return applyTrim(value);
    case "fixed":
      return applyFixed(value, args);
    case "join":
      return applyJoin(value, args);
    case "limit":
      return applyLimit(value, args);
    case "add":
      return applyAdd(value, args);
    case "sub":
      return applySub(value, args);
    default:
      throw new UnknownFilterError(filterName);
  }
}

function applyUpper(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'upper' requires a string");
  }
  return str(value.value.toUpperCase());
}

function applyLower(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'lower' requires a string");
  }
  return str(value.value.toLowerCase());
}

function applyTrim(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'trim' requires a string");
  }
  return str(value.value.trim());
}

function applyFixed(value: Value, args: string | undefined): Value {
  if (args === undefined) {
    throw new TemplateSyntaxError("'fixed' requires precision arg");
  }
  const precision = parseInt(args, 10);
  if (isNaN(precision)) {
    throw new TemplateSyntaxError(
      `'fixed' precision must be an integer: ${args}`,
    );
  }
  if (value.type === "float") {
    return str(value.value.toFixed(precision));
  }
  if (value.type === "int") {
    if (precision === 0) {
      return str(String(value.value));
    }
    return str(value.value.toFixed(precision));
  }
  throw new TemplateSyntaxError("'fixed' requires a number");
}

function applyJoin(value: Value, args: string | undefined): Value {
  const separator = args !== undefined ? stripQuotes(args) : "";
  if (value.type !== "list") {
    throw new TemplateSyntaxError("'join' requires a list");
  }
  const parts = value.items.map(display);
  return str(parts.join(separator));
}

function applyLimit(value: Value, args: string | undefined): Value {
  if (args === undefined) {
    throw new TemplateSyntaxError("'limit' requires a limit argument");
  }
  const limit = parseInt(args, 10);
  if (isNaN(limit)) {
    throw new TemplateSyntaxError(
      `'limit' argument must be an integer: ${args}`,
    );
  }
  if (value.type !== "list") {
    throw new TemplateSyntaxError("'limit' requires a list");
  }
  return list(value.items.slice(0, limit));
}

function parseNumArg(arg: string | undefined, filterName: string): number {
  if (arg === undefined) {
    throw new TemplateSyntaxError(`'${filterName}' requires a number argument`);
  }
  const n = Number(arg);
  if (isNaN(n)) {
    throw new TemplateSyntaxError(
      `'${filterName}' argument must be a number: ${arg}`,
    );
  }
  return n;
}

function applyAdd(value: Value, args: string | undefined): Value {
  const operand = parseNumArg(args, "add");
  if (value.type === "int") {
    const result = value.value + operand;
    return Number.isInteger(result) && Number.isInteger(operand)
      ? int(result)
      : float(result);
  }
  if (value.type === "float") {
    return float(value.value + operand);
  }
  throw new TemplateSyntaxError("'add' requires a number");
}

function applySub(value: Value, args: string | undefined): Value {
  const operand = parseNumArg(args, "sub");
  if (value.type === "int") {
    const result = value.value - operand;
    return Number.isInteger(result) && Number.isInteger(operand)
      ? int(result)
      : float(result);
  }
  if (value.type === "float") {
    return float(value.value - operand);
  }
  throw new TemplateSyntaxError("'sub' requires a number");
}
