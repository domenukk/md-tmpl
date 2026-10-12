/**
 * `json` / `tojson` filter: deterministic JSON serialization that is
 * byte-identical to the Rust core (`filter::serialize_value_to_json`).
 *
 * - Struct keys are sorted by Unicode code point (Rust `String` byte order).
 * - Floats use Rust `Display` digits (never exponent notation); `NaN` / `±inf`
 *   serialize as `null`; zero serializes as `0`.
 * - Strings escape `"`, `\`, control characters, `U+2028`, and `U+2029`
 *   (`/` is not escaped, unlike the `escape_json` filter).
 * - The optional indent is any `usize`, with no upper cap.
 * - Enum values keep their `__kind__` discriminant key.
 *
 * @module
 */

import {
  TYPE_BOOL,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  TYPE_NONE,
  TYPE_STR,
  TYPE_STRUCT,
  TYPE_TMPL,
} from "./consts.js";
import { TemplateSyntaxError } from "./errors.js";
import { escapeJsonBody } from "./json_escape.js";
import { formatFloatDisplay, formatIntDisplay } from "./numeric_format.js";
import { type Value, str } from "./value.js";
import { parseUsize } from "./filter_args.js";

const JSON_NULL = "null";
const JSON_TRUE = "true";
const JSON_FALSE = "false";
const JSON_ZERO = "0";
const JSON_QUOTE = '"';
const JSON_ITEM_SEP = ",";
const JSON_KEY_SEP_COMPACT = ":";
const JSON_KEY_SEP_PRETTY = ": ";
const JSON_NEWLINE = "\n";
const JSON_INDENT_UNIT = " ";
const JSON_EMPTY_LIST = "[]";
const JSON_EMPTY_OBJECT = "{}";
const JSON_LIST_OPEN = "[";
const JSON_LIST_CLOSE = "]";
const JSON_OBJECT_OPEN = "{";
const JSON_OBJECT_CLOSE = "}";
const ERR_SERIALIZE_TEMPLATE = "cannot serialize template to JSON";

/** UTF-16 surrogate range, remapped above `U+E000..U+FFFF` for code point ordering. */
const SURROGATE_MIN = 0xd800;
const SURROGATE_MAX = 0xdfff;
const SURROGATE_SHIFT_UP = 0x2000;
const BMP_HIGH_SHIFT_DOWN = 0x800;

function codePointOrderKey(unit: number): number {
  if (unit < SURROGATE_MIN) return unit;
  return unit <= SURROGATE_MAX
    ? unit + SURROGATE_SHIFT_UP
    : unit - BMP_HIGH_SHIFT_DOWN;
}

/** Compare strings by Unicode code point (equivalent to Rust's UTF-8 byte order). */
export function compareCodePoints(a: string, b: string): number {
  const len = Math.min(a.length, b.length);
  for (let i = 0; i < len; i++) {
    const ca = a.charCodeAt(i);
    const cb = b.charCodeAt(i);
    if (ca !== cb) {
      return codePointOrderKey(ca) - codePointOrderKey(cb);
    }
  }
  return a.length - b.length;
}

function pushIndent(out: string[], spaces: number, depth: number): void {
  out.push(JSON_INDENT_UNIT.repeat(spaces * depth));
}

function serializeInto(
  value: Value,
  out: string[],
  indent: number | undefined,
  depth: number,
): void {
  switch (value.type) {
    case TYPE_STR:
      out.push(JSON_QUOTE, escapeJsonBody(value.value, false), JSON_QUOTE);
      return;
    case TYPE_BOOL:
      out.push(value.value ? JSON_TRUE : JSON_FALSE);
      return;
    case TYPE_INT:
      out.push(formatIntDisplay(value.value));
      return;
    case TYPE_FLOAT:
      if (!Number.isFinite(value.value)) {
        out.push(JSON_NULL);
      } else if (value.value === 0) {
        out.push(JSON_ZERO);
      } else {
        out.push(formatFloatDisplay(value.value));
      }
      return;
    case TYPE_NONE:
      out.push(JSON_NULL);
      return;
    case TYPE_LIST: {
      if (value.items.length === 0) {
        out.push(JSON_EMPTY_LIST);
        return;
      }
      out.push(JSON_LIST_OPEN);
      value.items.forEach((item, idx) => {
        if (idx > 0) out.push(JSON_ITEM_SEP);
        if (indent !== undefined) {
          out.push(JSON_NEWLINE);
          pushIndent(out, indent, depth + 1);
        }
        serializeInto(item, out, indent, depth + 1);
      });
      if (indent !== undefined) {
        out.push(JSON_NEWLINE);
        pushIndent(out, indent, depth);
      }
      out.push(JSON_LIST_CLOSE);
      return;
    }
    case TYPE_STRUCT: {
      if (value.fields.size === 0) {
        out.push(JSON_EMPTY_OBJECT);
        return;
      }
      out.push(JSON_OBJECT_OPEN);
      const keys = Array.from(value.fields.keys()).sort(compareCodePoints);
      keys.forEach((key, idx) => {
        if (idx > 0) out.push(JSON_ITEM_SEP);
        if (indent !== undefined) {
          out.push(JSON_NEWLINE);
          pushIndent(out, indent, depth + 1);
        }
        out.push(
          JSON_QUOTE,
          escapeJsonBody(key, false),
          JSON_QUOTE,
          indent !== undefined ? JSON_KEY_SEP_PRETTY : JSON_KEY_SEP_COMPACT,
        );
        const field = value.fields.get(key);
        if (field === undefined) {
          out.push(JSON_NULL);
        } else {
          serializeInto(field, out, indent, depth + 1);
        }
      });
      if (indent !== undefined) {
        out.push(JSON_NEWLINE);
        pushIndent(out, indent, depth);
      }
      out.push(JSON_OBJECT_CLOSE);
      return;
    }
    case TYPE_TMPL:
      throw new TemplateSyntaxError(ERR_SERIALIZE_TEMPLATE);
  }
}

/** Serialize a template value to JSON exactly like the Rust core. */
export function serializeJson(
  value: Value,
  indent: number | undefined,
): string {
  const out: string[] = [];
  serializeInto(value, out, indent, 0);
  return out.join("");
}

/** Parse the optional `json(N)` indent argument (any `usize`). */
export function parseJsonIndent(args: string | undefined): number | undefined {
  if (args === undefined) {
    return undefined;
  }
  const trimmed = args.trim();
  if (trimmed.length === 0) {
    return undefined;
  }
  const outcome = parseUsize(trimmed);
  if (!outcome.ok) {
    throw new TemplateSyntaxError(
      `'tojson' indent argument must be a non-negative integer, got '${trimmed}'`,
    );
  }
  return outcome.value;
}

/** Apply the `json` / `tojson` / `to_json` filter. */
export function applyToJson(value: Value, args?: string): Value {
  return str(serializeJson(value, parseJsonIndent(args)));
}
