/**
 * Direct display and truthiness helpers for plain JS values.
 *
 * @module
 */

import { LIT_FALSE, LIT_TRUE } from "../consts.js";

// ---------------------------------------------------------------------------
// Direct display — convert any JS value to string
// ---------------------------------------------------------------------------

/** Convert a JS value to its display string. */
export function directDisplay(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "boolean") return value ? LIT_TRUE : LIT_FALSE;
  if (typeof value === "number") return String(value);
  if (Array.isArray(value)) {
    throw new Error(
      "cannot display list value directly — iterate with '{% for item in list %}' instead",
    );
  }
  if (typeof value === "object") {
    throw new Error(
      "cannot display struct value directly — access individual fields (e.g. '{{ value.field }}') instead",
    );
  }
  // Objects and arrays are handled (thrown) above, so at runtime `value` is a
  // bigint, symbol, or function here. Each of these has a well-defined native
  // `toString`, so we can coerce through it directly — this preserves the exact
  // JS default string coercion while avoiding a base-to-string on `{}`.
  return (
    value as bigint | symbol | ((...args: unknown[]) => unknown)
  ).toString();
}

/** Check if a JS value is truthy (template semantics). */
export function directIsTruthy(value: unknown): boolean {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.length > 0;
  if (typeof value === "boolean") return value;
  if (typeof value === "number") return value !== 0;
  if (Array.isArray(value)) return value.length > 0;
  if (typeof value === "object") return Object.keys(value).length > 0;
  return Boolean(value);
}
