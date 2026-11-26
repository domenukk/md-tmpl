/**
 * Pure string-level security filter primitives: token neutralization, XML
 * boundary tag escaping, adaptive backtick fencing, and quarantine framing.
 *
 * Kept as a leaf module (no dependency on `filters.ts`, `sanitize_runtime.ts`,
 * or `value.ts`) so the TypeScript module graph remains strictly acyclic.
 *
 * @module
 */

import { TemplateSyntaxError } from "./errors.js";
import {
  FILTER_QUARANTINE,
  FILTER_SANITIZE_TOKENS,
  stripQuotes,
} from "./consts.js";
import {
  isInvisibleCandidateUnit,
  isUnconditionalInvisibleCodePoint,
  stripInvisibleStr,
} from "./invisible.js";
import {
  ESCAPED_GT,
  ESCAPED_LT,
  ROLE_TOKEN_DELIMITERS,
  TOKEN_DELIMITERS,
  isNcnameContinueCode,
  isNcnameStartCode,
  isTagWhitespaceCode,
  matchControlTokenAt,
  ncnameFirstCharBit,
  skipTagWhitespace,
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
  let pos = skipTagWhitespace(s, i + 1);
  if (pos < s.length && s.charCodeAt(pos) === 47 /* '/' */) {
    pos = skipTagWhitespace(s, pos + 1);
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

function isTriggerUnit(c: number, i: number): boolean {
  return (
    c === 60 /* '<' */ ||
    c === 91 /* '[' */ ||
    c === 10 /* '\n' */ ||
    c === 13 /* '\r' */ ||
    (i === 0 &&
      (c === 72 /* 'H' */ ||
        c === 65 /* 'A' */ ||
        c === 32 /* ' ' */ ||
        c === 9)) /* '\t' */
  );
}

function hasBreakoutClean(
  s: string,
  rules: readonly [string, string][],
  parsedTags: ParsedTagSpec | undefined,
): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (isTriggerUnit(c, i)) {
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
    if (isTriggerUnit(c, i)) {
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

const CHAR_PRINTABLE_MIN = 0x20;
const CHAR_PRINTABLE_MAX = 0x7e;
const CHAR_LT = 60;
const CHAR_LBRACKET = 91;

function isSafePrintableAsciiNonTrigger(c: number): boolean {
  return (
    c >= CHAR_PRINTABLE_MIN &&
    c <= CHAR_PRINTABLE_MAX &&
    c !== CHAR_LT &&
    c !== CHAR_LBRACKET
  );
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
    if (i > 0) {
      while (i < s.length && isSafePrintableAsciiNonTrigger(s.charCodeAt(i))) {
        i++;
      }
      if (i >= s.length) break;
    }
    const c = s.charCodeAt(i);
    if (isTriggerUnit(c, i)) {
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
          } else if (matched.kind === "turn") {
            out = (out ?? "") + s.slice(lastCopied, matched.colonIdx) + "&#58;";
            i = matched.colonIdx + 1;
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
    if (i > 0) {
      while (i < s.length && isSafePrintableAsciiNonTrigger(s.charCodeAt(i))) {
        i++;
      }
      if (i >= s.length) break;
    }
    const c = s.charCodeAt(i);
    if (isTriggerUnit(c, i)) {
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
                ? stripInvisibleStr(inner)
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
          } else if (matched.kind === "turn") {
            out = (out ?? "") + s.slice(lastCopied, matched.colonIdx) + "&#58;";
            i = matched.colonIdx + 1;
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
  const pre = heading ? collapseHeadingLineBreaks(s) : s;
  return sanitizeWithRules(pre, TOKEN_DELIMITERS, parsedTags);
}

/**
 * Apply the compiler-generated `sanitize#enclosed("tags")` filter: in-place
 * sanitization of control tokens plus the statically enclosing XML tags, with
 * line breaks collapsed when the tags include the `#heading` sentinel (mirrors
 * Rust `SanitizeFilterMode::Inline { enclosing_tags }`).
 */
export function sanitizeEnclosedString(s: string, tagArg?: string): string {
  if (tagArg === undefined) {
    return sanitizeTokensString(s);
  }
  const spec = enclosedSpec(tagArg);
  const pre = spec.heading ? collapseHeadingLineBreaks(s) : s;
  return spec.tags.length === 0
    ? sanitizeTokensString(pre)
    : sanitizeUntrustedString(pre, spec.tags);
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

const MIN_FENCE_BACKTICKS = 3;

function hasDisallowedFenceLangChar(lang: string): boolean {
  let i = 0;
  while (i < lang.length) {
    const cp = lang.codePointAt(i);
    if (cp === undefined) break;
    if (
      cp === 96 /* '`' */ ||
      cp <= 0x20 ||
      (cp >= 0x7f && cp <= 0x9f) ||
      isUnconditionalInvisibleCodePoint(cp) ||
      isTagWhitespaceCode(cp)
    ) {
      return true;
    }
    i += cp > 0xffff ? 2 : 1;
  }
  return false;
}

/** Pre-validate a `fence` language argument at compile time, returning `undefined` if invalid. */
export function compileFenceLang(
  langArg: string | undefined,
): string | undefined {
  const lang = langArg !== undefined ? stripQuotes(langArg) : "";
  return hasDisallowedFenceLangChar(lang) ? undefined : lang;
}

/** Fast path for `direct_renderer` using a compile-time validated `lang` string. */
export function fencePreparsed(s: string, lang: string): string {
  const clean = s.includes("\0") ? s.replaceAll("\0", "") : s;
  let maxBackticks = 0;
  let currentBackticks = 0;
  let i = 0;
  while (i < clean.length) {
    const cp = clean.codePointAt(i);
    if (cp === undefined) break;
    const step = cp > 0xffff ? 2 : 1;
    if (cp === 96 /* '`' */) {
      currentBackticks++;
      if (currentBackticks > maxBackticks) {
        maxBackticks = currentBackticks;
      }
    } else if (
      isUnconditionalInvisibleCodePoint(cp) ||
      cp === 0x200d ||
      cp === 0xfe0f
    ) {
      // Do not reset `currentBackticks` on invisible code points so split backtick runs widen the fence.
    } else {
      currentBackticks = 0;
    }
    i += step;
  }
  const fenceLen = Math.max(MIN_FENCE_BACKTICKS, maxBackticks + 1);
  const fenceTicks = "`".repeat(fenceLen);
  const trailingNl = clean.endsWith("\n") ? "" : "\n";
  return `${fenceTicks}${lang}\n${clean}${trailingNl}${fenceTicks}`;
}

/** Wrap a string in markdown code fences with adaptive backtick counts, stripping NUL (`\0`) bytes. */
export function fenceString(s: string, langArg?: string): string {
  const lang = langArg !== undefined ? stripQuotes(langArg) : "";
  if (hasDisallowedFenceLangChar(lang)) {
    throw new TemplateSyntaxError(
      `'fence' language must not contain backticks or whitespace: '${lang}'`,
    );
  }
  return fencePreparsed(s, lang);
}

/** Returns `true` if `s` contains any opening/closing XML tag matching `tagSpec`. */
export function hasQuarantineTagBreakout(s: string, tagSpec: string): boolean {
  if (!s.includes("<")) {
    return false;
  }
  return hasBreakoutWithRules(s, [], parseTagSpec(tagSpec));
}

/** Returns `true` if `s` contains any LLM control token, generic pipe token, or XML tag matching `parsedTags`. */
export function hasUntrustedBreakoutPreparsed(
  s: string,
  parsedTags: ParsedTagSpec | undefined,
): boolean {
  return hasBreakoutWithRules(s, TOKEN_DELIMITERS, parsedTags);
}

/** Returns `true` if `s` contains any LLM control token, generic pipe token, or XML tag matching `tagSpec` in a single pass. */
export function hasUntrustedBreakout(s: string, tagSpec: string): boolean {
  return hasBreakoutWithRules(s, TOKEN_DELIMITERS, parseTagSpec(tagSpec));
}

/** Sanitize embedded open/close quarantine tags inside payload for one or more comma-separated tag names. */
export function sanitizeQuarantinePayload(s: string, tagSpec: string): string {
  return sanitizeWithRules(s, [], parseTagSpec(tagSpec));
}

/** Neutralize all known LLM control tokens, generic pipe tokens, and opening/closing XML boundary tags matching `parsedTags`. */
export function sanitizeUntrustedPreparsed(
  s: string,
  parsedTags: ParsedTagSpec | undefined,
): string {
  return sanitizeWithRules(s, TOKEN_DELIMITERS, parsedTags);
}

/** Neutralize all known LLM control tokens, generic pipe tokens, and opening/closing XML boundary tags matching `tagSpec` in a single pass. */
export function sanitizeUntrustedString(s: string, tagSpec: string): string {
  return sanitizeWithRules(s, TOKEN_DELIMITERS, parseTagSpec(tagSpec));
}

export interface CompiledQuarantineSpec {
  readonly primaryTag: string;
  readonly parsedTags: ParsedTagSpec | undefined;
}

/** Pre-parse a `quarantine` tag argument once at compile time, returning `undefined` if invalid. */
export function compileQuarantineSpec(
  tagArg: string | undefined,
): CompiledQuarantineSpec | undefined {
  const rawSpec =
    tagArg !== undefined ? stripQuotes(tagArg) : DEFAULT_QUARANTINE_TAG;
  try {
    const [primaryTag] = validateQuarantineTagSpec(rawSpec);
    return { primaryTag, parsedTags: parseTagSpec(rawSpec) };
  } catch {
    return undefined;
  }
}

/** Pre-parse a `sanitize_tokens("tags")` argument once at compile time, returning `undefined` if invalid. */
export function compileSanitizeTokensTags(
  tagArg: string,
): ParsedTagSpec | undefined {
  try {
    const [, validSpec] = validateTagSpecForFilter(
      stripQuotes(tagArg),
      FILTER_SANITIZE_TOKENS,
    );
    return parseTagSpec(validSpec);
  } catch {
    return undefined;
  }
}

/** Pre-parse any validated `tagSpec` into a `ParsedTagSpec`. */
export function compileValidatedTagSpec(
  validSpec: string,
): ParsedTagSpec | undefined {
  return parseTagSpec(validSpec);
}

/** Fast path for `direct_renderer` `| quarantine(...)`. */
export function quarantinePreparsed(
  s: string,
  spec: CompiledQuarantineSpec,
): string {
  const sanitized = sanitizeWithRules(s, [], spec.parsedTags);
  const trailingNl = sanitized.endsWith("\n") ? "" : "\n";
  return `<${spec.primaryTag}>\n${sanitized}${trailingNl}</${spec.primaryTag}>`;
}

/** Fast path for `direct_renderer` fused `| sanitize_tokens | quarantine(...)`. */
export function quarantineUntrustedPreparsed(
  s: string,
  spec: CompiledQuarantineSpec,
): string {
  const sanitized = sanitizeWithRules(s, TOKEN_DELIMITERS, spec.parsedTags);
  const trailingNl = sanitized.endsWith("\n") ? "" : "\n";
  return `<${spec.primaryTag}>\n${sanitized}${trailingNl}</${spec.primaryTag}>`;
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
