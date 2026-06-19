/**
 * Template value types.
 *
 * `Value` is a discriminated union representing every type the template
 * engine operates on. It mirrors the Rust `Value` enum.
 *
 * @module
 */

/** Internal enum tag key used to distinguish enum variants. */
export const ENUM_TAG_KEY = "__kind__";

/** String value. */
export interface StrValue {
  readonly type: "str";
  readonly value: string;
}

/** Boolean value. */
export interface BoolValue {
  readonly type: "bool";
  readonly value: boolean;
}

/** 64-bit integer (represented as JS `number`, safe up to 2^53). */
export interface IntValue {
  readonly type: "int";
  readonly value: number;
}

/** 64-bit float. */
export interface FloatValue {
  readonly type: "float";
  readonly value: number;
}

/** Ordered list of values. */
export interface ListValue {
  readonly type: "list";
  readonly items: readonly Value[];
}

/** Struct value (string-keyed map of fields). */
export interface DictValue {
  readonly type: "dict";
  readonly fields: ReadonlyMap<string, Value>;
}

/** Discriminated union of all template value types. */
export type Value =
  | StrValue
  | BoolValue
  | IntValue
  | FloatValue
  | ListValue
  | DictValue;

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

/** Create a string value. */
export function str(value: string): StrValue {
  return { type: "str", value };
}

/** Create a boolean value. */
export function bool(value: boolean): BoolValue {
  return { type: "bool", value };
}

/** Create an integer value. */
export function int(value: number): IntValue {
  return { type: "int", value: Math.trunc(value) };
}

/** Create a float value. */
export function float(value: number): FloatValue {
  return { type: "float", value };
}

/** Create a list value. */
export function list(items: readonly Value[]): ListValue {
  return { type: "list", items };
}

/** Create a dict value from key-value pairs. */
export function dict(entries: Iterable<readonly [string, Value]>): DictValue {
  return { type: "dict", fields: new Map(entries) };
}

// ---------------------------------------------------------------------------
// Namespaced constructors (preferred — avoids shadowing JS builtins)
// ---------------------------------------------------------------------------

/**
 * Namespaced Value constructors.
 *
 * Use `V.str("hello")`, `V.int(42)`, etc. instead of the bare `str()`, `int()`
 * functions that shadow JavaScript builtins.
 *
 * @example
 * ```ts
 * import { V } from "prompt-templates";
 * const name = V.str("Alice");
 * const count = V.int(42);
 * const items = V.list([name, count]);
 * ```
 */
export const V = {
  str,
  bool,
  int,
  float,
  list,
  dict,
  typeName,
  isTruthy,
  display,
  getField,
  fromJs,
} as const;

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/** Returns the type name as used in frontmatter declarations. */
export function typeName(v: Value): string {
  // The internal discriminator is "dict" but the user-facing name is "struct".
  if (v.type === "dict") return "struct";
  return v.type;
}

/** Returns `true` if the value is considered "truthy". */
export function isTruthy(v: Value): boolean {
  switch (v.type) {
    case "str":
      return v.value.length > 0;
    case "bool":
      return v.value;
    case "int":
      return v.value !== 0;
    case "float":
      return v.value !== 0.0;
    case "list":
      return v.items.length > 0;
    case "dict":
      return v.fields.size > 0;
  }
}

/** Display a value as a string (for `{{ expr }}` output). */
export function display(v: Value): string {
  switch (v.type) {
    case "str":
      return v.value;
    case "bool":
      return v.value ? "true" : "false";
    case "int":
      return String(v.value);
    case "float":
      return String(v.value);
    case "list":
      return `[<list of ${v.items.length}>]`;
    case "dict":
      return `{<struct of ${v.fields.size}>}`;
  }
}

/** Access a field on a Dict value. Hides the internal `__kind__` key. */
export function getField(v: Value, key: string): Value | undefined {
  if (v.type !== "dict") return undefined;
  if (key === ENUM_TAG_KEY) return undefined;
  return v.fields.get(key);
}

// ---------------------------------------------------------------------------
// Conversion from plain JS values → typed Value
// ---------------------------------------------------------------------------

/**
 * Convert a plain JavaScript value to a typed `Value`.
 *
 * - `string` → `StrValue`
 * - `boolean` → `BoolValue`
 * - `number` → `IntValue` if integer, else `FloatValue`
 * - `Array` → `ListValue` (recursively converted)
 * - plain object → `DictValue` (recursively converted)
 * - objects with `_prompt_template_tag` → enum variant `DictValue`
 *
 * Throws `TypeError` for unconvertible values (functions, symbols, etc.).
 */
export function fromJs(value: unknown): Value {
  // null / undefined → empty string.  This is intentional for template
  // ergonomics: missing or optional dict fields resolve to the empty
  // string during rendering rather than throwing, which matches the
  // Rust backend's behavior and avoids surprising errors when a caller
  // omits an optional parameter.
  if (value === null || value === undefined) {
    return str("");
  }
  if (typeof value === "string") {
    return str(value);
  }
  if (typeof value === "boolean") {
    return bool(value);
  }
  if (typeof value === "number") {
    if (Number.isInteger(value)) {
      return int(value);
    }
    return float(value);
  }
  if (Array.isArray(value)) {
    return list(value.map(fromJs));
  }
  if (typeof value === "object") {
    const obj = value as Record<string, unknown>;

    // Check for variant protocol (like Python's _prompt_template_tag)
    if (typeof obj["_prompt_template_tag"] === "string") {
      const tag = obj["_prompt_template_tag"] as string;
      const fieldsObj =
        typeof obj["_prompt_template_fields"] === "object" &&
        obj["_prompt_template_fields"] !== null
          ? (obj["_prompt_template_fields"] as Record<string, unknown>)
          : {};
      const entries: [string, Value][] = [[ENUM_TAG_KEY, str(tag)]];
      for (const [k, v] of Object.entries(fieldsObj)) {
        entries.push([k, fromJs(v)]);
      }
      return dict(entries);
    }

    // Plain object → dict
    const entries: [string, Value][] = [];
    for (const [k, v] of Object.entries(obj)) {
      entries.push([k, fromJs(v)]);
    }
    return dict(entries);
  }
  throw new TypeError(`cannot convert ${typeof value} to template Value`);
}
