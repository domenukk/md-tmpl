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
import {
  FILTER_SANITIZE,
  TYPE_BOOL,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  TYPE_NONE,
  TYPE_STR,
  TYPE_STRUCT,
  TYPE_TMPL,
  unescapeStringLiteral,
} from "./consts.js";
import { applySanitizeFilterStr } from "./sanitize_pass.js";

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
export function stripQuotes(s: string): string {
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
    case "truncate":
    case "truncate_middle":
      return applyTruncate(value, args);
    case "add":
      return applyAdd(value, args);
    case "sub":
      return applySub(value, args);
    case "escape_xml":
    case "xml":
      return applyEscapeXml(value);
    case "escape_json":
      return applyEscapeJson(value);
    case "tojson":
    case "to_json":
    case "json":
      return applyToJson(value, args);
    case "sanitize_tokens":
      return applySanitizeTokens(value, args);
    case "fence":
      return applyFence(value, args);
    case "quarantine":
      return applyQuarantine(value, args);
    case FILTER_SANITIZE:
      if (value.type !== "str") {
        throw new TemplateSyntaxError(`'${FILTER_SANITIZE}' requires a string`);
      }
      return str(applySanitizeFilterStr(value.value, args));
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

import {
  DEFAULT_TRUNCATE_MARKER,
  DEFAULT_TRUNCATE_MARKER_COMPACT,
  DEFAULT_LIST_TRUNCATE_MARKER,
  TRUNCATE_PLACEHOLDER_SKIPPED,
  TRUNCATE_PLACEHOLDER_COUNT,
  truncateMiddleString,
  applyTruncate,
} from "./filter_truncate.js";

export {
  DEFAULT_TRUNCATE_MARKER,
  DEFAULT_TRUNCATE_MARKER_COMPACT,
  DEFAULT_LIST_TRUNCATE_MARKER,
  TRUNCATE_PLACEHOLDER_SKIPPED,
  TRUNCATE_PLACEHOLDER_COUNT,
  truncateMiddleString,
};

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

/** Predefined XML entity escaping with illegal control character removal in a single pass. */
export function escapeXmlString(s: string): string {
  let out: string | undefined;
  let lastCopied = 0;
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    let replacement: string | undefined;
    switch (code) {
      case 38 /* '&' */:
        replacement = "&amp;";
        break;
      case 60 /* '<' */:
        replacement = "&lt;";
        break;
      case 62 /* '>' */:
        replacement = "&gt;";
        break;
      case 34 /* '"' */:
        replacement = "&quot;";
        break;
      case 39 /* "'" */:
        replacement = "&apos;";
        break;
      default:
        if (
          code <= 0x1f &&
          (code <= 0x08 || code === 0x0b || code === 0x0c || code >= 0x0e)
        ) {
          replacement = "";
        }
        break;
    }
    if (replacement !== undefined) {
      out = (out ?? "") + s.slice(lastCopied, i) + replacement;
      lastCopied = i + 1;
    }
  }
  return out !== undefined ? out + s.slice(lastCopied) : s;
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
  let out: string | undefined;
  let lastCopied = 0;
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    let replacement: string | undefined;
    switch (code) {
      case 92 /* '\\' */:
        replacement = "\\\\";
        break;
      case 34 /* '"' */:
        replacement = '\\"';
        break;
      case 47 /* '/' */:
        replacement = "\\/";
        break;
      case 10 /* '\n' */:
        replacement = "\\n";
        break;
      case 13 /* '\r' */:
        replacement = "\\r";
        break;
      case 9 /* '\t' */:
        replacement = "\\t";
        break;
      case 8 /* '\b' */:
        replacement = "\\b";
        break;
      case 12 /* '\f' */:
        replacement = "\\f";
        break;
      case 0x2028:
        replacement = "\\u2028";
        break;
      case 0x2029:
        replacement = "\\u2029";
        break;
      default:
        if (code < 0x20) {
          replacement = `\\u${code.toString(16).padStart(4, "0")}`;
        }
        break;
    }
    if (replacement !== undefined) {
      out = (out ?? "") + s.slice(lastCopied, i) + replacement;
      lastCopied = i + 1;
    }
  }
  return out !== undefined ? out + s.slice(lastCopied) : s;
}

function applyEscapeJson(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'escape_json' requires a string");
  }
  return str(escapeJsonString(value.value));
}

export function valueToJsSorted(v: Value): unknown {
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
      return v.items.map(valueToJsSorted);
    case TYPE_STRUCT: {
      const sortedKeys = Array.from(v.fields.keys()).sort();
      const obj: Record<string, unknown> = {};
      for (const k of sortedKeys) {
        const val = v.fields.get(k);
        if (val !== undefined) {
          obj[k] = valueToJsSorted(val);
        }
      }
      return obj;
    }
    case TYPE_TMPL:
      throw new TemplateSyntaxError("cannot serialize template to JSON");
    case TYPE_NONE:
      return null;
  }
}

export function applyToJson(value: Value, args?: string): Value {
  if (value.type === TYPE_TMPL) {
    throw new TemplateSyntaxError("cannot serialize template to JSON");
  }
  let indent: number | undefined;
  if (args !== undefined && args.trim().length > 0) {
    const parsed = Number(args.trim());
    if (!Number.isInteger(parsed) || parsed < 0) {
      throw new TemplateSyntaxError(
        `'tojson' indent argument must be a non-negative integer, got '${args.trim()}'`,
      );
    }
    indent = parsed;
  }
  const jsVal = valueToJsSorted(value);
  const jsonStr = JSON.stringify(jsVal, null, indent);
  return str(jsonStr);
}

/** Minimum length of any rule in `TOKEN_DELIMITERS` (`"</s>"`, `4` chars). */
const MIN_RULE_LEN = 4;
/** Maximum inner length for generic `<|...|>` / `<｜...｜>` special control tokens. */
const MAX_PIPE_TOKEN_INNER_LEN = 64;
const ESCAPED_LT = "&lt;";
const ESCAPED_GT = "&gt;";
const FULLWIDTH_PIPE_OPEN = "<｜";

const ROLE_DELIMITER_COUNT = 79;

/** Delimiter pairs for LLM control token neutralization across all supported dialects. */
export const TOKEN_DELIMITERS: readonly [string, string][] = [
  // ChatML / OpenAI / Qwen
  ["<|im_start|>", "&lt;|im_start|&gt;"],
  ["<|im_end|>", "&lt;|im_end|&gt;"],
  ["<|endoftext|>", "&lt;|endoftext|&gt;"],
  // Llama 3
  ["<|start_header_id|>", "&lt;|start_header_id|&gt;"],
  ["<|end_header_id|>", "&lt;|end_header_id|&gt;"],
  ["<|eot_id|>", "&lt;|eot_id|&gt;"],
  // Llama 2
  ["[INST]", "&#91;INST&#93;"],
  ["[/INST]", "&#91;/INST&#93;"],
  ["<<SYS>>", "&lt;&lt;SYS&gt;&gt;"],
  ["<</SYS>>", "&lt;&lt;/SYS&gt;&gt;"],
  ["</s>", "&lt;/s&gt;"],
  // Gemma 2 & Gemma 4
  ["<start_of_turn>", "&lt;start_of_turn&gt;"],
  ["<end_of_turn>", "&lt;end_of_turn&gt;"],
  ["<|turn>", "&lt;|turn&gt;"],
  ["<turn|>", "&lt;turn|&gt;"],
  ["<|tool>", "&lt;|tool&gt;"],
  ["<tool|>", "&lt;tool|&gt;"],
  ["<|tool_call>", "&lt;|tool_call&gt;"],
  ["<tool_call|>", "&lt;tool_call|&gt;"],
  ["<|tool_response>", "&lt;|tool_response&gt;"],
  ["<tool_response|>", "&lt;tool_response|&gt;"],
  ["<|thought", "&lt;|thought"],
  ["<thought|>", "&lt;thought|&gt;"],
  ["<|think|>", "&lt;|think|&gt;"],
  ['<|"|>', '&lt;|"|&gt;'],
  ["<bos>", "&lt;bos&gt;"],
  ["<eos>", "&lt;eos&gt;"],
  // T5 / CodeGemma / ChatGLM / GLM-4 / Cohere / Yi / InternLM
  ["<extra_id_", "&lt;extra_id_"],
  ["[gMASK]", "&#91;gMASK&#93;"],
  ["<sop>", "&lt;sop&gt;"],
  ["<eop>", "&lt;eop&gt;"],
  ["<SPECIAL_", "&lt;SPECIAL_"],
  ["<beginning_of_sentence>", "&lt;beginning_of_sentence&gt;"],
  ["<end_of_sentence>", "&lt;end_of_sentence&gt;"],
  // DeepSeek V3
  ["<｜begin▁of▁sentence｜>", "&lt;｜begin▁of▁sentence｜&gt;"],
  ["<｜end▁of▁sentence｜>", "&lt;｜end▁of▁sentence｜&gt;"],
  ["<｜begin▁of▁thought｜>", "&lt;｜begin▁of▁thought｜&gt;"],
  ["<｜end▁of▁thought｜>", "&lt;｜end▁of▁thought｜&gt;"],
  ["<｜User｜>", "&lt;｜User｜&gt;"],
  ["<｜Assistant｜>", "&lt;｜Assistant｜&gt;"],
  ["<｜tool▁calls▁begin｜>", "&lt;｜tool▁calls▁begin｜&gt;"],
  ["<｜tool▁calls▁end｜>", "&lt;｜tool▁calls▁end｜&gt;"],
  ["<｜tool▁call▁begin｜>", "&lt;｜tool▁call▁begin｜&gt;"],
  ["<｜tool▁call▁end｜>", "&lt;｜tool▁call▁end｜&gt;"],
  ["<｜tool▁sep｜>", "&lt;｜tool▁sep｜&gt;"],
  ["<｜tool▁outputs▁begin｜>", "&lt;｜tool▁outputs▁begin｜&gt;"],
  ["<｜tool▁outputs▁end｜>", "&lt;｜tool▁outputs▁end｜&gt;"],
  ["<｜tool▁output▁begin｜>", "&lt;｜tool▁output▁begin｜&gt;"],
  ["<｜tool▁output▁end｜>", "&lt;｜tool▁output▁end｜&gt;"],
  // Phi-3/4, Command-R, Bailing & Harmony
  ["<|user|>", "&lt;|user|&gt;"],
  ["<|assistant|>", "&lt;|assistant|&gt;"],
  ["<|system|>", "&lt;|system|&gt;"],
  ["<|end|>", "&lt;|end|&gt;"],
  ["<|start|>", "&lt;|start|&gt;"],
  ["<|channel|>", "&lt;|channel|&gt;"],
  ["<|message|>", "&lt;|message|&gt;"],
  ["<|call|>", "&lt;|call|&gt;"],
  ["<|return|>", "&lt;|return|&gt;"],
  ["<|constrain|>", "&lt;|constrain|&gt;"],
  ["<|role_end|>", "&lt;|role_end|&gt;"],
  ["<|START_OF_TURN_TOKEN|>", "&lt;|START_OF_TURN_TOKEN|&gt;"],
  ["<|END_OF_TURN_TOKEN|>", "&lt;|END_OF_TURN_TOKEN|&gt;"],
  ["<role>", "&lt;role&gt;"],
  ["</role>", "&lt;/role&gt;"],
  // Mistral & Ministral
  ["[SYSTEM_PROMPT]", "&#91;SYSTEM_PROMPT&#93;"],
  ["[/SYSTEM_PROMPT]", "&#91;/SYSTEM_PROMPT&#93;"],
  ["[TOOL_CALLS]", "&#91;TOOL_CALLS&#93;"],
  ["[AVAILABLE_TOOLS]", "&#91;AVAILABLE_TOOLS&#93;"],
  ["[/TOOL_CALLS]", "&#91;/TOOL_CALLS&#93;"],
  ["[/AVAILABLE_TOOLS]", "&#91;/AVAILABLE_TOOLS&#93;"],
  ["[TOOL_RESULTS]", "&#91;TOOL_RESULTS&#93;"],
  ["[/TOOL_RESULTS]", "&#91;/TOOL_RESULTS&#93;"],
  ["[TOOL_CONTENT]", "&#91;TOOL_CONTENT&#93;"],
  ["[ARGS]", "&#91;ARGS&#93;"],
  ["[/ARGS]", "&#91;/ARGS&#93;"],
  ["[THINK]", "&#91;THINK&#93;"],
  ["[/THINK]", "&#91;/THINK&#93;"],
  // Anthropic
  ["\n\nHuman:", "\n\nHuman&#58;"],
  ["\n\nAssistant:", "\n\nAssistant&#58;"],
  // Tool Calling & Qwen3-Coder / Anthropic XML
  ["<tool_call>", "&lt;tool_call&gt;"],
  ["</tool_call>", "&lt;/tool_call&gt;"],
  ["<tool_response>", "&lt;tool_response&gt;"],
  ["</tool_response>", "&lt;/tool_response&gt;"],
  ["<function=", "&lt;function="],
  ["</function>", "&lt;/function&gt;"],
  ["<function>", "&lt;function&gt;"],
  ["<parameter=", "&lt;parameter="],
  ["</parameter>", "&lt;/parameter&gt;"],
  ["<tools>", "&lt;tools&gt;"],
  ["</tools>", "&lt;/tools&gt;"],
  ["<function_calls>", "&lt;function_calls&gt;"],
  ["</function_calls>", "&lt;/function_calls&gt;"],
  ["<function_results>", "&lt;function_results&gt;"],
  ["</function_results>", "&lt;/function_results&gt;"],
  // Reasoning XML
  ["<think>", "&lt;think&gt;"],
  ["</think>", "&lt;/think&gt;"],
  // Quarantine envelopes
  ["<untrusted_tool_output>", "&lt;untrusted_tool_output&gt;"],
  ["</untrusted_tool_output>", "&lt;/untrusted_tool_output&gt;"],
  ["<tool_output_quarantine>", "&lt;tool_output_quarantine&gt;"],
  ["</tool_output_quarantine>", "&lt;/tool_output_quarantine&gt;"],
  ["<untrusted_content>", "&lt;untrusted_content&gt;"],
  ["</untrusted_content>", "&lt;/untrusted_content&gt;"],
];

/** Subset of `TOKEN_DELIMITERS` covering role/turn headers and special control tokens. */
export const ROLE_TOKEN_DELIMITERS: readonly [string, string][] =
  TOKEN_DELIMITERS.slice(0, ROLE_DELIMITER_COUNT);

const BRACKET_TOKEN_RULES: readonly [string, string][] =
  ROLE_TOKEN_DELIMITERS.filter(([pat]) => pat.charCodeAt(0) === 91 /* '[' */);

function isBracketSecondCode(c1: number): boolean {
  return (
    c1 === 73 /* 'I' */ ||
    c1 === 47 /* '/' */ ||
    c1 === 103 /* 'g' */ ||
    c1 === 83 /* 'S' */ ||
    c1 === 84 /* 'T' */ ||
    c1 === 65 /* 'A' */
  );
}

function isAngleSecondCode(c1: number): boolean {
  return (
    c1 === 124 /* '|' */ ||
    c1 === 0xff5c /* '｜' */ ||
    c1 === 47 /* '/' */ ||
    c1 === 60 /* '<' */ ||
    c1 === 115 /* 's' */ ||
    c1 === 101 /* 'e' */ ||
    c1 === 116 /* 't' */ ||
    c1 === 98 /* 'b' */ ||
    c1 === 83 /* 'S' */ ||
    c1 === 114 /* 'r' */ ||
    c1 === 102 /* 'f' */ ||
    c1 === 112 /* 'p' */ ||
    c1 === 117 /* 'u' */
  );
}

function isAsciiWhitespaceCode(c: number): boolean {
  return (
    c === 32 /* ' ' */ ||
    c === 9 /* '\t' */ ||
    c === 10 /* '\n' */ ||
    c === 12 /* '\f' */ ||
    c === 13 /* '\r' */
  );
}

function isPipeTokenForbiddenCode(code: number): boolean {
  return (
    isAsciiWhitespaceCode(code) ||
    code === 60 /* '<' */ ||
    code === 62 /* '>' */
  );
}

function matchGenericPipeToken(s: string, i: number): number | undefined {
  let openLen: number;
  let closePipeCode: number;
  if (s.startsWith("<|", i)) {
    openLen = 2;
    closePipeCode = 124 /* '|' */;
  } else if (s.startsWith(FULLWIDTH_PIPE_OPEN, i)) {
    openLen = FULLWIDTH_PIPE_OPEN.length;
    closePipeCode = 0xff5c /* '｜' */;
  } else {
    return undefined;
  }
  const start = i + openLen;
  const maxEnd = Math.min(s.length - 1, start + MAX_PIPE_TOKEN_INNER_LEN);
  let utf8Bytes = 0;
  for (let pos = start; pos <= maxEnd; pos++) {
    const code = s.charCodeAt(pos);
    if (code === closePipeCode && s.charCodeAt(pos + 1) === 62 /* '>' */) {
      return utf8Bytes > 0 ? pos + 2 : undefined;
    }
    if (isPipeTokenForbiddenCode(code)) {
      return undefined;
    }
    if (code < 0x80) {
      utf8Bytes += 1;
    } else if (code < 0x800) {
      utf8Bytes += 2;
    } else if (
      code >= 0xd800 &&
      code <= 0xdbff &&
      pos + 1 <= maxEnd &&
      s.charCodeAt(pos + 1) >= 0xdc00 &&
      s.charCodeAt(pos + 1) <= 0xdfff
    ) {
      utf8Bytes += 4;
      pos++;
    } else {
      utf8Bytes += 3;
    }
    if (utf8Bytes > MAX_PIPE_TOKEN_INNER_LEN) {
      return undefined;
    }
  }
  return undefined;
}

type ControlTokenMatch =
  | {
      readonly kind: "rule";
      readonly len: number;
      readonly replacement: string;
    }
  | { readonly kind: "pipe"; readonly endIdx: number };

function matchControlTokenAt(
  s: string,
  i: number,
  rules: readonly [string, string][],
): ControlTokenMatch | undefined {
  if (i + MIN_RULE_LEN > s.length) {
    return undefined;
  }
  const c0 = s.charCodeAt(i);
  const c1 = s.charCodeAt(i + 1);
  if (c0 === 10 /* '\n' */) {
    if (c1 !== 10) {
      return undefined;
    }
    if (s.startsWith("\n\nHuman:", i)) {
      return { kind: "rule", len: 8, replacement: "\n\nHuman&#58;" };
    }
    if (s.startsWith("\n\nAssistant:", i)) {
      return { kind: "rule", len: 12, replacement: "\n\nAssistant&#58;" };
    }
    return undefined;
  }
  if (c0 === 91 /* '[' */) {
    if (!isBracketSecondCode(c1)) {
      return undefined;
    }
    const c2 = s.charCodeAt(i + 2);
    const c3 = s.charCodeAt(i + 3);
    for (const [pat, replacement] of BRACKET_TOKEN_RULES) {
      if (
        pat.charCodeAt(1) === c1 &&
        pat.charCodeAt(2) === c2 &&
        pat.charCodeAt(3) === c3 &&
        s.startsWith(pat, i)
      ) {
        return { kind: "rule", len: pat.length, replacement };
      }
    }
    return undefined;
  }
  if (c0 === 60 /* '<' */) {
    if (!isAngleSecondCode(c1)) {
      return undefined;
    }
    if (c1 === 124 /* '|' */ || c1 === 0xff5c /* '｜' */) {
      const endIdx = matchGenericPipeToken(s, i);
      if (endIdx !== undefined) {
        return { kind: "pipe", endIdx };
      }
    }
    const c2 = s.charCodeAt(i + 2);
    const c3 = s.charCodeAt(i + 3);
    for (const [pat, replacement] of rules) {
      if (
        pat.charCodeAt(0) === 60 &&
        pat.charCodeAt(1) === c1 &&
        pat.charCodeAt(2) === c2 &&
        pat.charCodeAt(3) === c3 &&
        s.startsWith(pat, i)
      ) {
        return { kind: "rule", len: pat.length, replacement };
      }
    }
  }
  return undefined;
}

export const DEFAULT_QUARANTINE_TAG = "untrusted_content";

function toAsciiLowerCode(c: number): number {
  return c >= 65 && c <= 90 ? c + 32 : c;
}

function ncnameFirstCharBit(c: number): number {
  const lower = c | 32;
  if (lower >= 97 && lower <= 122) {
    return 1 << (lower - 97);
  }
  if (c === 95 /* '_' */) {
    return 1 << 26;
  }
  return 0;
}

function isNcnameStartCode(c: number): boolean {
  return ncnameFirstCharBit(c) !== 0;
}

function isNcnameContinueCode(c: number): boolean {
  return (
    isNcnameStartCode(c) ||
    (c >= 48 && c <= 57) ||
    c === 46 /* '.' */ ||
    c === 45 /* '-' */
  );
}

function isValidNcname(name: string): boolean {
  if (name.length === 0 || !isNcnameStartCode(name.charCodeAt(0))) {
    return false;
  }
  for (let i = 1; i < name.length; i++) {
    if (!isNcnameContinueCode(name.charCodeAt(i))) {
      return false;
    }
  }
  return true;
}

function validateQuarantineTagSpec(spec: string): [string, string[]] {
  if (spec.length === 0) {
    throw new TemplateSyntaxError(
      "'quarantine' tag name must be a valid XML NCName: ''",
    );
  }
  const parts = spec.split(",").map((p) => p.trim());
  for (const part of parts) {
    if (!isValidNcname(part)) {
      throw new TemplateSyntaxError(
        `'quarantine' tag name must be a valid XML NCName: '${spec}'`,
      );
    }
  }
  return [parts[0] ?? DEFAULT_QUARANTINE_TAG, parts];
}

interface ParsedTagSpec {
  readonly firstCharMask: number;
  readonly tagsLower: readonly string[];
}

function parseTagSpec(tagSpec: string | undefined): ParsedTagSpec | undefined {
  if (tagSpec === undefined) {
    return undefined;
  }
  let firstCharMask = 0;
  const tagsLower: string[] = [];
  if (!tagSpec.includes(",")) {
    const trimmed = tagSpec.trim().toLowerCase();
    if (trimmed.length > 0) {
      const bit = ncnameFirstCharBit(trimmed.charCodeAt(0));
      if (bit !== 0) {
        firstCharMask = bit;
        tagsLower.push(trimmed);
      }
    }
    return { firstCharMask, tagsLower };
  }
  for (const raw of tagSpec.split(",")) {
    const trimmed = raw.trim().toLowerCase();
    if (trimmed.length === 0) {
      continue;
    }
    const bit = ncnameFirstCharBit(trimmed.charCodeAt(0));
    if (bit === 0) {
      continue;
    }
    firstCharMask |= bit;
    tagsLower.push(trimmed);
  }
  return { firstCharMask, tagsLower };
}

function quarantineTagNameStart(s: string, i: number): number {
  let pos = i + 1;
  while (pos < s.length && isAsciiWhitespaceCode(s.charCodeAt(pos))) {
    pos++;
  }
  if (pos < s.length && s.charCodeAt(pos) === 47 /* '/' */) {
    pos++;
    while (pos < s.length && isAsciiWhitespaceCode(s.charCodeAt(pos))) {
      pos++;
    }
  }
  return pos;
}

function matchesTagIgnoreAsciiCaseAt(
  s: string,
  pos: number,
  tagLower: string,
): boolean {
  for (let k = 1; k < tagLower.length; k++) {
    if (toAsciiLowerCode(s.charCodeAt(pos + k)) !== tagLower.charCodeAt(k)) {
      return false;
    }
  }
  return true;
}

function matchAnyQuarantineTag(
  s: string,
  i: number,
  parsedTags: ParsedTagSpec,
): number | undefined {
  if (parsedTags.firstCharMask === 0) {
    return undefined;
  }
  const pos = quarantineTagNameStart(s, i);
  if (pos >= s.length) {
    return undefined;
  }
  const firstCode = s.charCodeAt(pos);
  if ((parsedTags.firstCharMask & ncnameFirstCharBit(firstCode)) === 0) {
    return undefined;
  }
  const firstLowerCode = toAsciiLowerCode(firstCode);
  for (const tagLower of parsedTags.tagsLower) {
    if (tagLower.charCodeAt(0) !== firstLowerCode) {
      continue;
    }
    const afterTag = pos + tagLower.length;
    if (
      afterTag <= s.length &&
      matchesTagIgnoreAsciiCaseAt(s, pos, tagLower) &&
      !(afterTag < s.length && isNcnameContinueCode(s.charCodeAt(afterTag)))
    ) {
      return afterTag;
    }
  }
  return undefined;
}

function hasBreakoutWithRules(
  s: string,
  rules: readonly [string, string][],
  parsedTags: ParsedTagSpec | undefined,
): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 60 /* '<' */ || c === 91 /* '[' */ || c === 10 /* '\n' */) {
      if (rules.length > 0 && matchControlTokenAt(s, i, rules) !== undefined) {
        return true;
      }
      if (
        c === 60 &&
        parsedTags !== undefined &&
        matchAnyQuarantineTag(s, i, parsedTags) !== undefined
      ) {
        return true;
      }
    }
  }
  return false;
}

function sanitizeWithRules(
  s: string,
  rules: readonly [string, string][],
  parsedTags: ParsedTagSpec | undefined,
): string {
  let out: string | undefined;
  let lastCopied = 0;
  let i = 0;
  while (i < s.length) {
    const c = s.charCodeAt(i);
    if (c === 60 /* '<' */ || c === 91 /* '[' */ || c === 10 /* '\n' */) {
      if (c === 60 && parsedTags !== undefined) {
        const afterTag = matchAnyQuarantineTag(s, i, parsedTags);
        if (afterTag !== undefined) {
          let j = afterTag;
          while (
            j < s.length &&
            s.charCodeAt(j) !== 62 /* '>' */ &&
            s.charCodeAt(j) !== 60 /* '<' */
          ) {
            j++;
          }
          if (j < s.length && s.charCodeAt(j) === 62 /* '>' */) {
            const inner = s.slice(i + 1, j);
            const sanitizedInner =
              rules.length === 0
                ? inner
                : sanitizeWithRules(inner, rules, undefined);
            out =
              (out ?? "") +
              s.slice(lastCopied, i) +
              `${ESCAPED_LT}${sanitizedInner}${ESCAPED_GT}`;
            i = j + 1;
            lastCopied = i;
            continue;
          }
          out = (out ?? "") + s.slice(lastCopied, i) + ESCAPED_LT;
          i++;
          lastCopied = i;
          continue;
        }
      }
      if (rules.length > 0) {
        const matched = matchControlTokenAt(s, i, rules);
        if (matched !== undefined) {
          if (matched.kind === "rule") {
            out = (out ?? "") + s.slice(lastCopied, i) + matched.replacement;
            i += matched.len;
          } else {
            const innerPipe = sanitizeWithRules(
              s.slice(i + 1, matched.endIdx - 1),
              rules,
              undefined,
            );
            out =
              (out ?? "") +
              s.slice(lastCopied, i) +
              `${ESCAPED_LT}${innerPipe}${ESCAPED_GT}`;
            i = matched.endIdx;
          }
          lastCopied = i;
          continue;
        }
      }
    }
    i++;
  }
  return out !== undefined ? out + s.slice(lastCopied) : s;
}

/** Returns `true` if `s` contains any known LLM control token or `<|...|>` / `<｜...｜>` special token. */
export function hasControlTokens(s: string): boolean {
  return hasBreakoutWithRules(s, TOKEN_DELIMITERS, undefined);
}

/** Returns `true` if `s` contains any role/turn control token in `ROLE_TOKEN_DELIMITERS` or generic pipe token. */
export function hasRoleControlTokens(s: string): boolean {
  return hasBreakoutWithRules(s, ROLE_TOKEN_DELIMITERS, undefined);
}

/** Replace known LLM and chat turn delimiters and `<|...|>` / `<｜...｜>` special tokens in a single pass. */
export function sanitizeTokensString(s: string): string {
  return sanitizeWithRules(s, TOKEN_DELIMITERS, undefined);
}

/** Replace role/turn headers (`ROLE_TOKEN_DELIMITERS`) and generic pipe tokens in a single pass while preserving XML tool docs. */
export function sanitizeRoleTokensString(s: string): string {
  return sanitizeWithRules(s, ROLE_TOKEN_DELIMITERS, undefined);
}

/** Apply `sanitize_tokens` (or `sanitize_tokens("tag1,tag2")`) to a string. */
export function sanitizeTokensWithArgs(s: string, tagArg?: string): string {
  if (tagArg === undefined) {
    return sanitizeTokensString(s);
  }
  const rawSpec = stripQuotes(tagArg);
  validateQuarantineTagSpec(rawSpec);
  return sanitizeUntrustedString(s, rawSpec);
}

function applySanitizeTokens(value: Value, args: string | undefined): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'sanitize_tokens' requires a string");
  }
  return str(sanitizeTokensWithArgs(value.value, args));
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

/** Returns `true` if `s` contains any opening/closing XML tag matching `tagSpec`. */
export function hasQuarantineTagBreakout(s: string, tagSpec: string): boolean {
  if (!s.includes("<")) {
    return false;
  }
  return hasBreakoutWithRules(s, [], parseTagSpec(tagSpec));
}

/** Returns `true` if `s` contains any LLM control token, generic pipe token, or XML tag matching `tagSpec` in a single pass. */
export function hasUntrustedBreakout(s: string, tagSpec: string): boolean {
  return hasBreakoutWithRules(s, TOKEN_DELIMITERS, parseTagSpec(tagSpec));
}

/** Sanitize embedded open/close quarantine tags inside payload for one or more comma-separated tag names. */
export function sanitizeQuarantinePayload(s: string, tagSpec: string): string {
  if (!s.includes("<")) {
    return s;
  }
  return sanitizeWithRules(s, [], parseTagSpec(tagSpec));
}

/** Neutralize all known LLM control tokens, generic pipe tokens, and opening/closing XML boundary tags matching `tagSpec` in a single pass. */
export function sanitizeUntrustedString(s: string, tagSpec: string): string {
  return sanitizeWithRules(s, TOKEN_DELIMITERS, parseTagSpec(tagSpec));
}

/** Wrap untrusted content in XML boundary tags (`<primary_tag>\n...\n</primary_tag>`). */
export function quarantineString(s: string, tagArg?: string): string {
  const rawSpec =
    tagArg !== undefined ? stripQuotes(tagArg) : DEFAULT_QUARANTINE_TAG;
  const [primaryTag] = validateQuarantineTagSpec(rawSpec);
  const sanitized = sanitizeQuarantinePayload(s, rawSpec);
  const trailingNl = sanitized.endsWith("\n") ? "" : "\n";
  return `<${primaryTag}>\n${sanitized}${trailingNl}</${primaryTag}>`;
}

/** Sanitize both control tokens and boundary XML tags in a single pass and wrap in `<primary_tag>\n...\n</primary_tag>`. */
export function quarantineUntrustedString(s: string, tagArg?: string): string {
  const rawSpec =
    tagArg !== undefined ? stripQuotes(tagArg) : DEFAULT_QUARANTINE_TAG;
  const [primaryTag] = validateQuarantineTagSpec(rawSpec);
  const sanitized = sanitizeUntrustedString(s, rawSpec);
  const trailingNl = sanitized.endsWith("\n") ? "" : "\n";
  return `<${primaryTag}>\n${sanitized}${trailingNl}</${primaryTag}>`;
}

/** Returns `true` if `s` is wrapped in `<primary_tag>...</primary_tag>` with zero unescaped boundary tags or control tokens. */
export function isQuarantinedString(s: string, tagArg?: string): boolean {
  const rawSpec =
    tagArg !== undefined ? stripQuotes(tagArg) : DEFAULT_QUARANTINE_TAG;
  let primaryTag: string;
  try {
    [primaryTag] = validateQuarantineTagSpec(rawSpec);
  } catch {
    return false;
  }
  const openPrefix = `<${primaryTag}>`;
  const closeSuffix = `</${primaryTag}>`;
  if (!s.startsWith(openPrefix) || !s.endsWith(closeSuffix)) {
    return false;
  }
  const body = s.slice(openPrefix.length, s.length - closeSuffix.length);
  return !hasUntrustedBreakout(body, rawSpec);
}

function applyQuarantine(value: Value, args: string | undefined): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'quarantine' requires a string");
  }
  return str(quarantineString(value.value, args));
}
