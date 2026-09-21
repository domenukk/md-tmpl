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
import { unescapeStringLiteral } from "./consts.js";

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

/** Strip surrounding quotes from a filter argument and unescape its content. */
function stripQuotes(s: string): string {
  if (s.length >= 2) {
    if (
      (s.startsWith('"') && s.endsWith('"')) ||
      (s.startsWith("'") && s.endsWith("'"))
    ) {
      return unescapeStringLiteral(s.slice(1, -1));
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
    case "escape_xml":
    case "xml":
      return applyEscapeXml(value);
    case "escape_json":
    case "json":
      return applyEscapeJson(value);
    case "sanitize_tokens":
      return applySanitizeTokens(value);
    case "fence":
      return applyFence(value, args);
    case "quarantine":
      return applyQuarantine(value, args);
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
  if (Number.isNaN(precision)) {
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
  if (Number.isNaN(limit)) {
    throw new TemplateSyntaxError(
      `'limit' argument must be an integer: ${args}`,
    );
  }
  if (value.type === "list") {
    return list(value.items.slice(0, limit));
  }
  throw new TemplateSyntaxError("'limit' requires a list");
}

function parseNumArg(arg: string | undefined, filterName: string): number {
  if (arg === undefined) {
    throw new TemplateSyntaxError(`'${filterName}' requires a number argument`);
  }
  const n = Number(arg);
  if (Number.isNaN(n)) {
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

// ---------------------------------------------------------------------------
// Security filters
// ---------------------------------------------------------------------------

/** Predefined XML entity escaping with illegal control character removal. */
export function escapeXmlString(s: string): string {
  let out = "";
  for (const ch of s) {
    switch (ch) {
      case "&":
        out += "&amp;";
        break;
      case "<":
        out += "&lt;";
        break;
      case ">":
        out += "&gt;";
        break;
      case '"':
        out += "&quot;";
        break;
      case "'":
        out += "&apos;";
        break;
      default: {
        const code = ch.charCodeAt(0);
        // Strip XML 1.0 illegal control characters (U+0000..U+0008, U+000B, U+000C, U+000E..U+001F)
        if (
          ch.length === 1 &&
          ((code >= 0x00 && code <= 0x08) ||
            code === 0x0b ||
            code === 0x0c ||
            (code >= 0x0e && code <= 0x1f))
        ) {
          // Strip illegal control character
        } else {
          out += ch;
        }
        break;
      }
    }
  }
  return out;
}

function applyEscapeXml(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'escape_xml' requires a string");
  }
  return str(escapeXmlString(value.value));
}

/**
 * Escape JSON string body characters.
 *
 * Escapes quotes, backslashes, control characters, U+2028/U+2029, and forward
 * slashes (`/` to `\/` for HTML script tag safety). Caller supplies quotes.
 */
export function escapeJsonString(s: string): string {
  let out = "";
  for (const ch of s) {
    switch (ch) {
      case "\\":
        out += "\\\\";
        break;
      case '"':
        out += '\\"';
        break;
      case "/":
        out += "\\/";
        break;
      case "\n":
        out += "\\n";
        break;
      case "\r":
        out += "\\r";
        break;
      case "\t":
        out += "\\t";
        break;
      case "\b":
        out += "\\b";
        break;
      case "\f":
        out += "\\f";
        break;
      case "\u2028":
        out += "\\u2028";
        break;
      case "\u2029":
        out += "\\u2029";
        break;
      default: {
        const code = ch.charCodeAt(0);
        if (ch.length === 1 && code < 0x20) {
          out += `\\u${code.toString(16).padStart(4, "0")}`;
        } else {
          out += ch;
        }
        break;
      }
    }
  }
  return out;
}

function applyEscapeJson(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'escape_json' requires a string");
  }
  return str(escapeJsonString(value.value));
}

/** Delimiter pairs for LLM control token neutralization. */
export const TOKEN_DELIMITERS: readonly [string, string][] = [
  ["<|im_start|>", "&lt;|im_start|&gt;"],
  ["<|im_end|>", "&lt;|im_end|&gt;"],
  ["<|endoftext|>", "&lt;|endoftext|&gt;"],
  ["<|start_header_id|>", "&lt;|start_header_id|&gt;"],
  ["<|end_header_id|>", "&lt;|end_header_id|&gt;"],
  ["<|eot_id|>", "&lt;|eot_id|&gt;"],
  ["<tool_call>", "&lt;tool_call&gt;"],
  ["</tool_call>", "&lt;/tool_call&gt;"],
  ["<tool_response>", "&lt;tool_response&gt;"],
  ["</tool_response>", "&lt;/tool_response&gt;"],
  ["[INST]", "&#91;INST&#93;"],
  ["[/INST]", "&#91;/INST&#93;"],
  ["<<SYS>>", "&lt;&lt;SYS&gt;&gt;"],
  ["<</SYS>>", "&lt;&lt;/SYS&gt;&gt;"],
  ["<start_of_turn>", "&lt;start_of_turn&gt;"],
  ["<end_of_turn>", "&lt;end_of_turn&gt;"],
  ["<think>", "&lt;think&gt;"],
  ["</think>", "&lt;/think&gt;"],
  ["<untrusted_tool_output>", "&lt;untrusted_tool_output&gt;"],
  ["</untrusted_tool_output>", "&lt;/untrusted_tool_output&gt;"],
  ["<untrusted_content>", "&lt;untrusted_content&gt;"],
  ["</untrusted_content>", "&lt;/untrusted_content&gt;"],
  ["<｜begin▁of▁sentence｜>", "&lt;｜begin▁of▁sentence｜&gt;"],
  ["<｜end▁of▁sentence｜>", "&lt;｜end▁of▁sentence｜&gt;"],
  ["<｜User｜>", "&lt;｜User｜&gt;"],
  ["<｜Assistant｜>", "&lt;｜Assistant｜&gt;"],
  ["<｜tool▁calls▁begin｜>", "&lt;｜tool▁calls▁begin｜&gt;"],
  ["<|user|>", "&lt;|user|&gt;"],
  ["<|assistant|>", "&lt;|assistant|&gt;"],
  ["<|system|>", "&lt;|system|&gt;"],
  ["<|end|>", "&lt;|end|&gt;"],
  ["<|START_OF_TURN_TOKEN|>", "&lt;|START_OF_TURN_TOKEN|&gt;"],
  ["<|END_OF_TURN_TOKEN|>", "&lt;|END_OF_TURN_TOKEN|&gt;"],
  ["[TOOL_CALLS]", "&#91;TOOL_CALLS&#93;"],
  ["[AVAILABLE_TOOLS]", "&#91;AVAILABLE_TOOLS&#93;"],
  ["[/TOOL_CALLS]", "&#91;/TOOL_CALLS&#93;"],
  ["[/AVAILABLE_TOOLS]", "&#91;/AVAILABLE_TOOLS&#93;"],
  ["\n\nHuman:", "\n\nHuman&#58;"],
  ["\n\nAssistant:", "\n\nAssistant&#58;"],
  ["<role>", "&lt;role&gt;"],
  ["</role>", "&lt;/role&gt;"],
  ["<|role_end|>", "&lt;|role_end|&gt;"],
  ["<|channel|>", "&lt;|channel|&gt;"],
  ["<|message|>", "&lt;|message|&gt;"],
];

/** Replace known LLM and chat turn delimiters with safe entities. */
export function sanitizeTokensString(s: string): string {
  let out = s;
  for (const [token, replacement] of TOKEN_DELIMITERS) {
    if (out.includes(token)) {
      out = out.replaceAll(token, replacement);
    }
  }
  return out;
}

function applySanitizeTokens(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'sanitize_tokens' requires a string");
  }
  return str(sanitizeTokensString(value.value));
}

const MIN_FENCE_BACKTICKS = 3;

/** Wrap a string in markdown code fences with adaptive backtick counts. */
export function fenceString(s: string, langArg?: string): string {
  const lang = langArg !== undefined ? stripQuotes(langArg) : "";
  if (/[`\s]/.test(lang)) {
    throw new TemplateSyntaxError(
      `'fence' language must not contain backticks or whitespace: '${lang}'`,
    );
  }
  let maxBackticks = 0;
  let currentBackticks = 0;
  for (let i = 0; i < s.length; i++) {
    if (s.charCodeAt(i) === 96 /* '`' */) {
      currentBackticks++;
      if (currentBackticks > maxBackticks) {
        maxBackticks = currentBackticks;
      }
    } else {
      currentBackticks = 0;
    }
  }
  const fenceLen = Math.max(MIN_FENCE_BACKTICKS, maxBackticks + 1);
  const fenceTicks = "`".repeat(fenceLen);
  const trailingNl = s.endsWith("\n") ? "" : "\n";
  return `${fenceTicks}${lang}\n${s}${trailingNl}${fenceTicks}`;
}

function applyFence(value: Value, args: string | undefined): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'fence' requires a string");
  }
  return str(fenceString(value.value, args));
}

const DEFAULT_QUARANTINE_TAG = "untrusted_content";

function isValidNcname(name: string): boolean {
  return /^[a-zA-Z_][a-zA-Z0-9._-]*$/.test(name);
}

/** Sanitize embedded open/close quarantine tags inside payload. */
export function sanitizeQuarantinePayload(s: string, tagName: string): string {
  let out = "";
  const tagLower = tagName.toLowerCase();
  let i = 0;
  while (i < s.length) {
    if (s.charCodeAt(i) === 60 /* '<' */) {
      if (i + 1 < s.length && s.charCodeAt(i + 1) === 47 /* '/' */) {
        const afterSlash = i + 2;
        if (
          s.slice(afterSlash, afterSlash + tagName.length).toLowerCase() ===
          tagLower
        ) {
          const afterTag = afterSlash + tagName.length;
          const nextChar = s[afterTag] ?? "";
          const isNcnameChar =
            afterTag < s.length && /[a-zA-Z0-9._-]/.test(nextChar);
          if (!isNcnameChar) {
            let j = afterTag;
            while (j < s.length && /[\s]/.test(s[j] ?? "")) {
              j++;
            }
            if (j < s.length && s.charCodeAt(j) === 62 /* '>' */) {
              out += `&lt;/${s.slice(afterSlash, j)}&gt;`;
              i = j + 1;
              continue;
            }
          }
        }
      } else {
        const afterLt = i + 1;
        if (
          s.slice(afterLt, afterLt + tagName.length).toLowerCase() === tagLower
        ) {
          const afterTag = afterLt + tagName.length;
          const nextChar = s[afterTag] ?? "";
          const isNcnameChar =
            afterTag < s.length && /[a-zA-Z0-9._-]/.test(nextChar);
          if (!isNcnameChar) {
            let j = afterTag;
            while (
              j < s.length &&
              s.charCodeAt(j) !== 62 /* '>' */ &&
              s.charCodeAt(j) !== 60 /* '<' */
            ) {
              j++;
            }
            if (j < s.length && s.charCodeAt(j) === 62 /* '>' */) {
              out += `&lt;${s.slice(afterLt, j)}&gt;`;
              i = j + 1;
              continue;
            }
          }
        }
      }
    }
    out += s[i] ?? "";
    i++;
  }
  return out;
}

/** Wrap untrusted content in XML boundary tags. */
export function quarantineString(s: string, tagArg?: string): string {
  const tag =
    tagArg !== undefined ? stripQuotes(tagArg) : DEFAULT_QUARANTINE_TAG;
  const tagName = tag.length === 0 ? DEFAULT_QUARANTINE_TAG : tag;
  if (!isValidNcname(tagName)) {
    throw new TemplateSyntaxError(
      `'quarantine' tag name must be a valid XML NCName: '${tag}'`,
    );
  }
  const sanitized = sanitizeQuarantinePayload(s, tagName);
  const trailingNl = sanitized.endsWith("\n") ? "" : "\n";
  return `<${tagName}>\n${sanitized}${trailingNl}</${tagName}>`;
}

function applyQuarantine(value: Value, args: string | undefined): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'quarantine' requires a string");
  }
  return str(quarantineString(value.value, args));
}
