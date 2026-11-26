/**
 * Direct plain-JS JSON serializer for `direct_renderer` (`| tojson` / `| json` / `| to_json`).
 *
 * Produces byte-identical JSON to `serializeJson(fromJs(value), indent)` without boxing
 * plain JavaScript objects or arrays into `Map`-backed `Value` trees.
 *
 * @module
 */

import { TemplateError, TemplateSyntaxError } from "../errors.js";
import { compareCodePoints, serializeJson } from "../filter_json.js";
import { escapeJsonBody } from "../json_escape.js";
import { formatFloatDisplay, formatIntDisplay } from "../numeric_format.js";
import { type TmplRef, isBoxedValue } from "../value.js";

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
const MAX_JSON_DEPTH = 256;
const ERR_SERIALIZE_TEMPLATE = "cannot serialize template to JSON";

function pushIndent(out: string[], spaces: number, depth: number): void {
  out.push(JSON_INDENT_UNIT.repeat(spaces * depth));
}

function serializeJsInto(
  value: unknown,
  out: string[],
  indent: number | undefined,
  depth: number,
  stack: object[],
): void {
  if (depth > MAX_JSON_DEPTH) {
    throw new TemplateError(
      "maximum recursion depth exceeded in template parameter",
    );
  }
  if (value === null || value === undefined) {
    out.push(JSON_NULL);
    return;
  }
  if (typeof value === "string") {
    out.push(JSON_QUOTE, escapeJsonBody(value, false), JSON_QUOTE);
    return;
  }
  if (typeof value === "boolean") {
    out.push(value ? JSON_TRUE : JSON_FALSE);
    return;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      out.push(JSON_NULL);
    } else if (Number.isInteger(value)) {
      out.push(formatIntDisplay(value));
    } else if (value === 0) {
      out.push(JSON_ZERO);
    } else {
      out.push(formatFloatDisplay(value));
    }
    return;
  }
  if (typeof value === "function") {
    throw new TemplateSyntaxError(ERR_SERIALIZE_TEMPLATE);
  }
  if (typeof value === "object") {
    if (typeof (value as TmplRef).renderForInclude === "function") {
      throw new TemplateSyntaxError(ERR_SERIALIZE_TEMPLATE);
    }
    if (isBoxedValue(value)) {
      out.push(serializeJson(value, indent));
      return;
    }
    if (stack.includes(value)) {
      throw new TemplateError("cyclic object detected in template parameter");
    }
    stack.push(value);
    try {
      if (Array.isArray(value)) {
        if (value.length === 0) {
          out.push(JSON_EMPTY_LIST);
          return;
        }
        out.push(JSON_LIST_OPEN);
        for (let idx = 0; idx < value.length; idx++) {
          if (idx > 0) out.push(JSON_ITEM_SEP);
          if (indent !== undefined) {
            out.push(JSON_NEWLINE);
            pushIndent(out, indent, depth + 1);
          }
          serializeJsInto(value[idx], out, indent, depth + 1, stack);
        }
        if (indent !== undefined) {
          out.push(JSON_NEWLINE);
          pushIndent(out, indent, depth);
        }
        out.push(JSON_LIST_CLOSE);
        return;
      }

      const obj = value as Record<string, unknown>;
      const keys = Object.keys(obj);
      if (keys.length === 0) {
        out.push(JSON_EMPTY_OBJECT);
        return;
      }
      keys.sort(compareCodePoints);
      out.push(JSON_OBJECT_OPEN);
      for (let idx = 0; idx < keys.length; idx++) {
        const key = keys[idx];
        if (key === undefined) continue;
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
        serializeJsInto(obj[key], out, indent, depth + 1, stack);
      }
      if (indent !== undefined) {
        out.push(JSON_NEWLINE);
        pushIndent(out, indent, depth);
      }
      out.push(JSON_OBJECT_CLOSE);
      return;
    } finally {
      stack.pop();
    }
  }
  throw new TypeError(`cannot convert ${typeof value} to template Value`);
}

/** Serialize any plain JS or boxed `Value` directly to deterministic JSON. */
export function directToJson(value: unknown, indent?: number): string {
  const out: string[] = [];
  serializeJsInto(value, out, indent, 0, []);
  return out.join("");
}
