/**
 * Built-in expression filters.
 *
 * Filters transform values in pipe chains: `{{ expr | filter | filter }}`.
 * Each filter is a pure function: `(value, args?) → value`.
 *
 * @module
 */

import { type Value, str, list, display } from "./value.js";
import { TemplateSyntaxError, UnknownFilterError } from "./errors.js";
import {
  FILTER_ADD,
  FILTER_FIXED,
  FILTER_LIMIT,
  FILTER_SUB,
  FILTER_QUARANTINE,
  FILTER_SANITIZE,
  FILTER_SANITIZE_ENCLOSED,
  FILTER_SANITIZE_TOKENS,
  FILTER_UNSAFE_TRUST,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  unescapeStringLiteral,
} from "./consts.js";
import { applySanitizeFilterStr } from "./sanitize_pass.js";
import {
  ArithFilter,
  FIXED_ARG_LABEL,
  LIMIT_ARG_LABEL,
  applyArith,
  applyFixed,
  parseArithArg,
  parseUsizeFilterArg,
} from "./filter_args.js";
import { applyToJson } from "./filter_json.js";
import { escapeJsonBody } from "./json_escape.js";
import {
  hasInvisibleChars,
  isInvisibleCandidateUnit,
  isUnconditionalInvisibleCodePoint,
  stripInvisibleStr,
} from "./invisible.js";

export {
  hasInvisibleChars,
  isUnconditionalInvisibleCodePoint,
  stripInvisibleStr,
};

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
    case FILTER_FIXED:
      return applyFixedFilter(value, args);
    case "join":
      return applyJoin(value, args);
    case FILTER_LIMIT:
      return applyLimit(value, args);
    case "truncate":
    case "truncate_middle":
      return applyTruncate(value, args);
    case FILTER_ADD:
    case FILTER_SUB:
      return applyArithFilter(value, filterName, args);
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
    case FILTER_SANITIZE_ENCLOSED:
      if (value.type !== "str") {
        throw new TemplateSyntaxError(`'${FILTER_SANITIZE}' requires a string`);
      }
      return str(sanitizeEnclosedString(value.value, args));
    case FILTER_UNSAFE_TRUST:
      if (args !== undefined) {
        throw new TemplateSyntaxError(
          `'${FILTER_UNSAFE_TRUST}' does not take arguments`,
        );
      }
      if (value.type !== "str") {
        throw new TemplateSyntaxError(
          `'${FILTER_UNSAFE_TRUST}' requires a string`,
        );
      }
      return value;
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

function applyFixedFilter(value: Value, args: string | undefined): Value {
  if (value.type !== TYPE_FLOAT && value.type !== TYPE_INT) {
    if (args === undefined) {
      throw new TemplateSyntaxError("'fixed' requires precision arg");
    }
    parseUsizeFilterArg(args, FILTER_FIXED, FIXED_ARG_LABEL);
    throw new TemplateSyntaxError("'fixed' requires a number");
  }
  return str(applyFixed(value, args));
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
  const limit = parseUsizeFilterArg(args, FILTER_LIMIT, LIMIT_ARG_LABEL);
  if (value.type === TYPE_LIST) {
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

function applyArithFilter(
  value: Value,
  filter: ArithFilter,
  args: string | undefined,
): Value {
  const arg = parseArithArg(args, filter);
  if (value.type !== TYPE_INT && value.type !== TYPE_FLOAT) {
    throw new TemplateSyntaxError(`'${filter}' requires a number`);
  }
  return applyArith(value, filter, arg);
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
 * slashes (`/` to `\\/` for HTML script tag safety). Caller supplies quotes.
 */
export function escapeJsonString(s: string): string {
  return escapeJsonBody(s, true);
}

function applyEscapeJson(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'escape_json' requires a string");
  }
  return str(escapeJsonString(value.value));
}

import {
  ESCAPED_GT,
  ESCAPED_LT,
  ROLE_TOKEN_DELIMITERS,
  TOKEN_DELIMITERS,
  isAsciiWhitespaceCode,
  matchControlTokenAt,
} from "./token_delimiters.js";

export { ROLE_TOKEN_DELIMITERS, TOKEN_DELIMITERS };

export const DEFAULT_QUARANTINE_TAG = "untrusted_content";

function toAsciiLowerCode(c: number): number {
  return c >= 65 && c <= 90 ? c + 32 : c;
}

/** ASCII-only lowercasing (Rust `to_ascii_lowercase`): non-ASCII code units are kept as-is. */
function toAsciiLowerString(s: string): string {
  return s.replace(/[A-Z]/g, (ch) =>
    String.fromCharCode(ch.charCodeAt(0) | 32),
  );
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

export const INLINE_HEADING_SENTINEL = "#heading";

function isHeadingLineBreakCode(code: number): boolean {
  return code <= 0x85
    ? code === 0x0a ||
        code === 0x0d ||
        code === 0x0b ||
        code === 0x0c ||
        code === 0x85
    : code === 0x2028 || code === 0x2029;
}

function collapseHeadingLineBreaks(s: string): string {
  let out = "";
  let last = 0;
  for (let i = 0; i < s.length; i++) {
    if (isHeadingLineBreakCode(s.charCodeAt(i))) {
      out += `${s.slice(last, i)} `;
      last = i + 1;
    }
  }
  return last === 0 ? s : out + s.slice(last);
}

/**
 * Validate a single or comma-separated list of XML `NCName` boundary tags for
 * `filterName`, returning `[primaryTag, spec]` (mirrors Rust
 * `validate_tag_spec_for_filter`). The internal `#heading` sentinel is rejected.
 */
export function validateTagSpecForFilter(
  spec: string,
  filterName: string,
): [string, string] {
  if (spec.length === 0) {
    throw new TemplateSyntaxError(
      `'${filterName}' tag name must be a valid XML NCName: ''`,
    );
  }
  const parts = spec.split(",").map((p) => p.trim());
  for (const part of parts) {
    if (!isValidNcname(part)) {
      throw new TemplateSyntaxError(
        `'${filterName}' tag name must be a valid XML NCName: '${spec}'`,
      );
    }
  }
  return [parts[0] ?? DEFAULT_QUARANTINE_TAG, spec];
}

function validateQuarantineTagSpec(spec: string): [string, string] {
  return validateTagSpecForFilter(spec, FILTER_QUARANTINE);
}

export interface ParsedTagSpec {
  readonly firstCharMask: number;
  readonly tagsLower: readonly string[];
}

/**
 * Bounded memo for parsed tag specs. Specs come from compiled templates (a small,
 * fixed set per process), so caching avoids re-splitting and lower-casing the same
 * spec on every render; the cap keeps adversarial host input from growing it.
 */
const TAG_SPEC_CACHE = new Map<string, ParsedTagSpec>();
const TAG_SPEC_CACHE_MAX = 256;

function parseTagSpec(tagSpec: string | undefined): ParsedTagSpec | undefined {
  if (tagSpec === undefined) {
    return undefined;
  }
  const cached = TAG_SPEC_CACHE.get(tagSpec);
  if (cached !== undefined) {
    return cached;
  }
  const parsed = parseTagSpecUncached(tagSpec);
  if (TAG_SPEC_CACHE.size >= TAG_SPEC_CACHE_MAX) {
    TAG_SPEC_CACHE.clear();
  }
  TAG_SPEC_CACHE.set(tagSpec, parsed);
  return parsed;
}

function parseTagSpecUncached(tagSpec: string): ParsedTagSpec {
  let firstCharMask = 0;
  const tagsLower: string[] = [];
  if (!tagSpec.includes(",")) {
    const trimmed = toAsciiLowerString(tagSpec.trim());
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
    const trimmed = toAsciiLowerString(raw.trim());
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

function hasBreakoutClean(
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

function hasBreakoutWithRules(
  s: string,
  rules: readonly [string, string][],
  parsedTags: ParsedTagSpec | undefined,
): boolean {
  let checkedInvisible = false;
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
    } else if (!checkedInvisible && isInvisibleCandidateUnit(c)) {
      checkedInvisible = true;
      const stripped = stripInvisibleStr(s);
      if (stripped !== s) {
        return (
          rules.length > 0 || hasBreakoutClean(stripped, rules, parsedTags)
        );
      }
    }
  }
  return false;
}

function sanitizeWithRulesClean(
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
                : sanitizeWithRulesClean(inner, rules, undefined);
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
            const innerPipe = sanitizeWithRulesClean(
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

function sanitizeWithRules(
  s: string,
  rules: readonly [string, string][],
  parsedTags: ParsedTagSpec | undefined,
): string {
  let out: string | undefined;
  let lastCopied = 0;
  let checkedInvisible = false;
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
    } else if (!checkedInvisible && isInvisibleCandidateUnit(c)) {
      checkedInvisible = true;
      const stripped = stripInvisibleStr(s);
      if (stripped !== s) {
        return sanitizeWithRulesClean(stripped, rules, parsedTags);
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
  const [, validSpec] = validateTagSpecForFilter(
    stripQuotes(tagArg),
    FILTER_SANITIZE_TOKENS,
  );
  return sanitizeUntrustedString(s, validSpec);
}

/**
 * Apply the compiler-generated `sanitize#enclosed("tags")` filter: in-place
 * sanitization of control tokens plus the statically enclosing XML tags, with
 * line breaks collapsed when the tags include the `#heading` sentinel (mirrors
 * Rust `SanitizeFilterMode::Inline { enclosing_tags }`).
 */
/** Pre-decoded `sanitize#enclosed` execution plan (zero runtime Map lookups). */
export interface CompiledEnclosedSpec {
  readonly parsedTags: ParsedTagSpec | undefined;
  readonly heading: boolean;
}

/** Pre-decode a `sanitize#enclosed` filter argument once at template compile time. */
export function compileEnclosedSpec(
  tagArg: string | undefined,
): CompiledEnclosedSpec {
  if (tagArg === undefined) {
    return { parsedTags: undefined, heading: false };
  }
  const spec = enclosedSpec(tagArg);
  return {
    parsedTags: spec.tags.length === 0 ? undefined : parseTagSpec(spec.tags),
    heading: spec.heading,
  };
}

/** Fast path for `direct_renderer` using a compile-time pre-decoded `CompiledEnclosedSpec`. */
export function sanitizeEnclosedPreparsed(
  s: string,
  parsedTags: ParsedTagSpec | undefined,
  heading: boolean,
): string {
  const sanitized = sanitizeWithRules(s, TOKEN_DELIMITERS, parsedTags);
  return heading ? collapseHeadingLineBreaks(sanitized) : sanitized;
}

export function sanitizeEnclosedString(s: string, tagArg?: string): string {
  if (tagArg === undefined) {
    return sanitizeTokensString(s);
  }
  const spec = enclosedSpec(tagArg);
  const sanitized =
    spec.tags.length === 0
      ? sanitizeTokensString(s)
      : sanitizeUntrustedString(s, spec.tags);
  return spec.heading ? collapseHeadingLineBreaks(sanitized) : sanitized;
}

/** Decoded `sanitize#enclosed` argument: unquoted XML tag list + heading flag. */
interface EnclosedSpec {
  readonly tags: string;
  readonly heading: boolean;
}

/** Bounded memo for decoded `sanitize#enclosed` arguments (see `TAG_SPEC_CACHE`). */
const ENCLOSED_SPEC_CACHE = new Map<string, EnclosedSpec>();

function enclosedSpec(tagArg: string): EnclosedSpec {
  const cached = ENCLOSED_SPEC_CACHE.get(tagArg);
  if (cached !== undefined) {
    return cached;
  }
  const raw = stripQuotes(tagArg);
  if (raw === INLINE_HEADING_SENTINEL) {
    const spec: EnclosedSpec = { tags: "", heading: true };
    ENCLOSED_SPEC_CACHE.set(tagArg, spec);
    return spec;
  }
  let heading = false;
  const xmlTags: string[] = [];
  for (const part of raw.split(",")) {
    const trimmed = part.trim();
    if (trimmed === INLINE_HEADING_SENTINEL) {
      heading = true;
    } else if (trimmed.length > 0) {
      xmlTags.push(trimmed);
    }
  }
  const spec: EnclosedSpec = {
    tags: xmlTags.join(","),
    heading,
  };
  if (ENCLOSED_SPEC_CACHE.size >= TAG_SPEC_CACHE_MAX) {
    ENCLOSED_SPEC_CACHE.clear();
  }
  ENCLOSED_SPEC_CACHE.set(tagArg, spec);
  return spec;
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
