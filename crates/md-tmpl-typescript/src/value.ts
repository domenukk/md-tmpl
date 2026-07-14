/**
 * Template value types.
 *
 * `Value` is a discriminated union representing every type the template
 * engine operates on. It mirrors the Rust `Value` enum.
 *
 * @module
 */

import {
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_TMPL,
  TYPE_NONE,
  OPTION_SOME,
  OPTION_NONE,
  ENUM_TAG_KEY,
  ENUM_VARIANTS_KEY,
  LIT_TRUE,
  LIT_FALSE,
} from "./consts.js";

export {
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_TMPL,
  TYPE_NONE,
  OPTION_SOME,
  OPTION_NONE,
  ENUM_TAG_KEY,
  ENUM_VARIANTS_KEY,
};
import { TemplateError } from "./errors.js";

/** String value. */
export interface StrValue {
  readonly type: typeof TYPE_STR;
  readonly value: string;
}

/** Boolean value. */
export interface BoolValue {
  readonly type: typeof TYPE_BOOL;
  readonly value: boolean;
}

/** 64-bit integer (represented as JS `number`, safe up to 2^53). */
export interface IntValue {
  readonly type: typeof TYPE_INT;
  readonly value: number;
}

/** 64-bit float. */
export interface FloatValue {
  readonly type: typeof TYPE_FLOAT;
  readonly value: number;
}

/** Ordered list of values. */
export interface ListValue {
  readonly type: typeof TYPE_LIST;
  readonly items: readonly Value[];
}

/** Struct value (string-keyed map of fields). */
export interface StructValue {
  readonly type: typeof TYPE_STRUCT;
  readonly fields: ReadonlyMap<string, Value>;
}

/** Deprecated alias for StructValue */
export type DictValue = StructValue;

/** None value — represents an absent option value. */
export interface NoneValue {
  readonly type: typeof TYPE_NONE;
}

/** Template value — wraps a parsed template for higher-order composition. */
export interface TmplValue {
  readonly type: typeof TYPE_TMPL;
  readonly ref: TmplRef;
}

/** Interface for higher-order template references (avoids circular deps). */
export interface TmplRef {
  declarations(): ReadonlyArray<readonly [string, string]>;
  rawDeclarations(): ReadonlyArray<{
    name: string;
    varType: unknown;
    defaultValue?: Value;
  }>;
  renderForInclude(
    params: ReadonlyMap<string, Value>,
    parentConsts: ReadonlyMap<string, Value>,
    parentOptionParams: ReadonlySet<string>,
    maxDepth: number,
    templateLoader?: unknown,
    basePath?: string,
  ): string;
}

/** Discriminated union of all template value types. */
export type Value =
  | StrValue
  | BoolValue
  | IntValue
  | FloatValue
  | ListValue
  | StructValue
  | NoneValue
  | TmplValue;

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

/** Create a string value. */
export function str(value: string): StrValue {
  return { type: TYPE_STR, value };
}

/** Create a boolean value. */
export function bool(value: boolean): BoolValue {
  return { type: TYPE_BOOL, value };
}

/** Create an integer value. */
export function int(value: number): IntValue {
  return { type: TYPE_INT, value: Math.trunc(value) };
}

/** Create a float value. */
export function float(value: number): FloatValue {
  return { type: TYPE_FLOAT, value };
}

/** Create a list value. */
export function list(items: readonly Value[]): ListValue {
  return { type: TYPE_LIST, items };
}

/** Create a struct value from key-value pairs. */
export function structVal(
  entries: Iterable<readonly [string, Value]>,
): StructValue {
  return { type: TYPE_STRUCT, fields: new Map(entries) };
}

/** Deprecated alias for structVal */
export const dict = structVal;

/** Singleton None value. */
export const NONE: NoneValue = { type: TYPE_NONE };

/** Create a template value wrapping a TmplRef. */
export function tmplVal(ref: TmplRef): TmplValue {
  return { type: TYPE_TMPL, ref };
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
 * import { V } from "md-tmpl";
 * const name = V.str("Alice");
 * const count = V.int(42);
 * const items = V.list([name, count]);
 * ```
 */
// V constructor namespace is defined at the bottom of the file after all functions.

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/** Returns the type name as used in frontmatter declarations. */
export function typeName(v: Value): string {
  if (v.type === TYPE_NONE) return TYPE_NONE;
  return v.type;
}

/** Returns `true` if the value is considered "truthy". */
export function isTruthy(v: Value): boolean {
  switch (v.type) {
    case TYPE_NONE:
      return false;
    case TYPE_STR:
      return v.value.length > 0;
    case TYPE_BOOL:
      return v.value;
    case TYPE_INT:
      return v.value !== 0;
    case TYPE_FLOAT:
      return v.value !== 0.0;
    case TYPE_LIST:
      return v.items.length > 0;
    case TYPE_STRUCT:
      return v.fields.size > 0;
    case TYPE_TMPL:
      return true;
  }
}

/** Display a value as a string (for `{{ expr }}` output). */
export function display(v: Value): string {
  switch (v.type) {
    case TYPE_NONE:
      return "";
    case TYPE_STR:
      return v.value;
    case TYPE_BOOL:
      return v.value ? LIT_TRUE : LIT_FALSE;
    case TYPE_INT:
      return String(v.value);
    case TYPE_FLOAT:
      return String(v.value);
    case TYPE_LIST:
      throw new Error(
        "cannot display list value directly — iterate with '{% for item in list %}' instead",
      );
    case TYPE_STRUCT: {
      const kind = v.fields.get(ENUM_TAG_KEY);
      if (kind !== undefined) {
        throw new Error(
          "cannot display enum value directly — use '{% match expr %}' to handle variants",
        );
      }
      throw new Error(
        "cannot display struct value directly — access individual fields (e.g. '{{ value.field }}') instead",
      );
    }
    case TYPE_TMPL:
      throw new Error(
        "cannot display template value directly — use '{% include name %}' to render it",
      );
  }
}

/** Access a field on a Struct value. Hides the internal `__kind__` key. */
export function getField(v: Value, key: string): Value | undefined {
  if (v.type !== TYPE_STRUCT) return undefined;
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
 * - objects with a `__kind__` tag → enum variant `StructValue`
 *
 * Throws `TypeError` for unconvertible values (functions, symbols, etc.).
 */
export function fromJs(
  value: unknown,
  seen: WeakSet<object> = new WeakSet(),
  depth = 0,
): Value {
  if (depth > 256) {
    throw new TemplateError(
      "maximum recursion depth exceeded in template parameter",
    );
  }
  // null / undefined → NONE (absent value).  This enables transparent
  // option(T) representation: null/undefined values map to the NONE
  // sentinel, which displays as empty string and is falsy.
  if (value === null || value === undefined) {
    return NONE;
  }
  if (value && typeof value === "object" && "type" in (value as object)) {
    const t = (value as { type: string }).type;
    if (
      t === TYPE_STR ||
      t === TYPE_BOOL ||
      t === TYPE_INT ||
      t === TYPE_FLOAT ||
      t === TYPE_LIST ||
      t === TYPE_STRUCT ||
      t === TYPE_TMPL ||
      t === TYPE_NONE ||
      t === "dict"
    ) {
      return value as Value;
    }
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
  if (typeof value === "object" || typeof value === "function") {
    if (typeof (value as TmplRef).renderForInclude === "function") {
      return tmplVal(value as TmplRef);
    }
    if (seen.has(value as object)) {
      throw new TemplateError("cyclic object detected in template parameter");
    }
    seen.add(value as object);
    try {
      if (Array.isArray(value)) {
        return list(value.map((v) => fromJs(v, seen, depth + 1)));
      }
      const obj = value as Record<string, unknown>;

      // Plain object → struct.
      //
      // Variant instances use the `__kind__` discriminant as an ordinary
      // (enumerable) property with a non-enumerable `toString`, so they are
      // converted here like any other struct: the resulting `Value` carries
      // an `ENUM_TAG_KEY` field with the variant tag.
      const entries: [string, Value][] = [];
      for (const [k, v] of Object.entries(obj)) {
        entries.push([k, fromJs(v, seen, depth + 1)]);
      }
      return structVal(entries);
    } finally {
      seen.delete(value as object);
    }
  }
  throw new TypeError(`cannot convert ${typeof value} to template Value`);
}

/** Convert a Value back to plain JS for defaults/consts output. */
export function valueToJs(v: Value): unknown {
  switch (v.type) {
    case TYPE_STR:
      return v.value;
    case TYPE_BOOL:
      return v.value;
    case TYPE_INT:
      return v.value;
    case TYPE_FLOAT:
      return v.value;
    case TYPE_LIST:
      return v.items.map(valueToJs);
    case TYPE_STRUCT: {
      const obj: Record<string, unknown> = {};
      for (const [k, val] of v.fields) {
        obj[k] = valueToJs(val);
      }
      return obj;
    }
    case TYPE_TMPL:
      return v.ref;
    case TYPE_NONE:
      return null;
  }
}

export const V = {
  str,
  bool,
  int,
  float,
  list,
  structVal,
  dict,
  tmplVal,
  NONE,
  typeName,
  isTruthy,
  display,
  getField,
  fromJs,
  valueToJs,
} as const;
