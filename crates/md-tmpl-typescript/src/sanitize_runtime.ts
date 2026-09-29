/**
 * Runtime string framing, escaping, token neutralization, and filter parsing
 * for `| sanitize`, `| sanitize("tag")`, and `| sanitize("tag", "notice")`.
 *
 * @module
 */

import {
  BACKSLASH,
  COMMA,
  DEFAULT_SANITIZE_NOTICE,
  DEFAULT_SANITIZE_TAG,
  FILTER_SANITIZE,
  FM_SANITIZE_NOTICE_PREFIX,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  SANITIZE_NOTICE_TAG_PLACEHOLDER,
} from "./consts.js";
import { TemplateSyntaxError } from "./errors.js";
import {
  hasUntrustedBreakout,
  sanitizeTokensString,
  sanitizeUntrustedString,
  stripQuotes,
} from "./filters.js";

export {
  DEFAULT_SANITIZE_NOTICE,
  DEFAULT_SANITIZE_TAG,
  FILTER_SANITIZE,
  FM_SANITIZE_NOTICE_PREFIX,
  SANITIZE_NOTICE_TAG_PLACEHOLDER,
};

/** Declarative sanitization specification attached to a frontmatter parameter or type field. */
export type SanitizeSpec =
  | { readonly kind: "inline" }
  | {
      readonly kind: "block";
      readonly tag: string;
      readonly notice?: string;
    };

export interface SanitizeFilterArgs {
  readonly tagSpec: string;
  readonly notice?: string;
}

export function isNcnameStartCode(c: number): boolean {
  const lower = c | 32;
  return (lower >= 97 && lower <= 122) || c === 95 /* '_' */;
}

export function isNcnameContinueCode(c: number): boolean {
  return (
    isNcnameStartCode(c) ||
    (c >= 48 && c <= 57) ||
    c === 46 /* '.' */ ||
    c === 45 /* '-' */
  );
}

export function isAsciiWhitespaceCode(c: number): boolean {
  return c === 32 || c === 9 || c === 10 || c === 13;
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

/** Validate a comma-separated XML tag specification for `sanitize`, returning `[primaryTag, validSpec]`. */
export function validateSanitizeTagSpec(spec: string): [string, string] {
  const trimmed = spec.trim();
  if (trimmed.length === 0) {
    throw new TemplateSyntaxError(
      `'${FILTER_SANITIZE}' tag name must be a valid XML NCName: ''`,
    );
  }
  const parts = trimmed.split(COMMA).map((p) => p.trim());
  for (const part of parts) {
    if (!isValidNcname(part)) {
      throw new TemplateSyntaxError(
        `'${FILTER_SANITIZE}' tag name must be a valid XML NCName: '${trimmed}'`,
      );
    }
  }
  return [parts[0] ?? DEFAULT_SANITIZE_TAG, trimmed];
}

/**
 * Format the untrusted-data boundary notice for `primaryTag`, substituting `{tag}` with `primaryTag`.
 *
 * - When `customNotice` is `undefined`, formats `DEFAULT_SANITIZE_NOTICE`.
 * - When `customNotice` is `""`, returns `""` (omits the notice line).
 * - When `customNotice` is a non-empty string, substitutes `{tag}` if present.
 */
export function formatSanitizeNotice(
  primaryTag: string,
  customNotice?: string,
): string {
  if (customNotice === "") {
    return "";
  }
  const template = customNotice ?? DEFAULT_SANITIZE_NOTICE;
  if (template.includes(SANITIZE_NOTICE_TAG_PLACEHOLDER)) {
    return template.replaceAll(SANITIZE_NOTICE_TAG_PLACEHOLDER, primaryTag);
  }
  return template;
}

/**
 * Neutralize all known LLM control tokens (`TOKEN_DELIMITERS`), generic `<|...|>` / `<｜...｜>`
 * pipe tokens, and optional comma-separated XML boundary tags (`tagSpec`) in a single pass.
 */
export function sanitizeString(s: string, tagSpec?: string): string {
  if (tagSpec === undefined || tagSpec === "") {
    return sanitizeTokensString(s);
  }
  const rawSpec = stripQuotes(tagSpec);
  const [, validSpec] = validateSanitizeTagSpec(rawSpec);
  return sanitizeUntrustedString(s, validSpec);
}

/**
 * Sanitize control tokens and boundary XML tags (`tagSpec`, defaulting to `DEFAULT_SANITIZE_TAG`)
 * in a single pass and wrap `s` in `<primary_tag>\n{notice}\n{sanitized}\n</primary_tag>`.
 *
 * Pass `customNotice: undefined` to use `DEFAULT_SANITIZE_NOTICE`, a string to override the
 * notice (with `{tag}` replaced by `primaryTag`), or `""` to omit the notice line.
 */
export function sanitizeBlockString(
  s: string,
  tagSpec?: string,
  customNotice?: string,
): string {
  const rawSpec =
    tagSpec !== undefined ? stripQuotes(tagSpec) : DEFAULT_SANITIZE_TAG;
  const [primaryTag, validSpec] = validateSanitizeTagSpec(rawSpec);
  const notice = formatSanitizeNotice(primaryTag, customNotice).replace(
    /\n+$/,
    "",
  );
  const sanitized = sanitizeUntrustedString(s, validSpec);
  const trailingNl = sanitized.endsWith("\n") ? "" : "\n";
  const noticeLine = notice.length > 0 ? `${notice}\n` : "";
  return `<${primaryTag}>\n${noticeLine}${sanitized}${trailingNl}</${primaryTag}>`;
}

/**
 * Extract the inner payload from a sanitized block if and only if `s` is wrapped in
 * `<primary_tag>...</primary_tag>`, begins with the expected boundary notice (when non-empty),
 * and contains no un-neutralized control tokens or breakout tags from `tagSpec`.
 */
export function unsanitizeBlockString(
  s: string,
  tagSpec?: string,
  customNotice?: string,
): string | undefined {
  const rawSpec =
    tagSpec !== undefined ? stripQuotes(tagSpec) : DEFAULT_SANITIZE_TAG;
  let primaryTag: string;
  let validSpec: string;
  try {
    [primaryTag, validSpec] = validateSanitizeTagSpec(rawSpec);
  } catch {
    return undefined;
  }
  const openPrefix = `<${primaryTag}>`;
  const closeSuffix = `</${primaryTag}>`;
  if (!s.startsWith(openPrefix) || !s.endsWith(closeSuffix)) {
    return undefined;
  }
  let inner = s.slice(openPrefix.length, s.length - closeSuffix.length);
  if (inner.startsWith("\n")) {
    inner = inner.slice(1);
  }
  if (inner.endsWith("\n")) {
    inner = inner.slice(0, -1);
  }
  const notice = formatSanitizeNotice(primaryTag, customNotice).replace(
    /\n+$/,
    "",
  );
  let payload: string;
  if (notice.length === 0) {
    payload = inner;
  } else {
    if (!inner.startsWith(notice)) {
      return undefined;
    }
    const afterNotice = inner.slice(notice.length);
    payload = afterNotice.startsWith("\n") ? afterNotice.slice(1) : afterNotice;
  }
  if (hasUntrustedBreakout(payload, validSpec)) {
    return undefined;
  }
  return payload;
}

/**
 * Returns `true` if `s` is a valid sanitized block wrapped in `<primary_tag>...</primary_tag>`
 * containing the expected boundary notice (when non-empty) and no unescaped tags from `tagSpec`
 * or un-neutralized control tokens.
 */
export function isSanitizedBlockString(
  s: string,
  tagSpec?: string,
  customNotice?: string,
): boolean {
  return unsanitizeBlockString(s, tagSpec, customNotice) !== undefined;
}

function splitFilterArgsComma(raw: string): string[] {
  const parts: string[] = [];
  let start = 0;
  let inSingle = false;
  let inDouble = false;
  let escaped = false;
  for (let i = 0; i < raw.length; i++) {
    const ch = raw[i];
    if (escaped) {
      escaped = false;
      continue;
    }
    if (ch === BACKSLASH && (inSingle || inDouble)) {
      escaped = true;
      continue;
    }
    if (ch === QUOTE_SINGLE && !inDouble) {
      inSingle = !inSingle;
      continue;
    }
    if (ch === QUOTE_DOUBLE && !inSingle) {
      inDouble = !inDouble;
      continue;
    }
    if (ch === COMMA && !inSingle && !inDouble) {
      parts.push(raw.slice(start, i));
      start = i + 1;
    }
  }
  const tail = raw.slice(start);
  if (tail.trim().length > 0 || parts.length > 0) {
    parts.push(tail);
  }
  return parts;
}

/**
 * Parse optional filter arguments for `| sanitize`:
 * - `undefined` -> `undefined` (0 args: inline sanitization)
 * - `"\"tag\""` -> `{ tagSpec, notice: undefined }` (1 arg: block sanitization with default notice)
 * - `"\"tag\", \"notice\""` -> `{ tagSpec, notice }` (2 args: block sanitization with custom notice)
 */
export function parseSanitizeFilterArgs(
  args: string | undefined,
): { readonly tagSpec: string; readonly notice?: string } | undefined {
  if (args === undefined) {
    return undefined;
  }
  const parts = splitFilterArgsComma(args);
  if (parts.length === 0) {
    throw new TemplateSyntaxError(
      `'${FILTER_SANITIZE}' tag name must be a valid XML NCName: ''`,
    );
  }
  if (parts.length === 1) {
    const tagRaw = stripQuotes((parts[0] ?? "").trim());
    const [, validSpec] = validateSanitizeTagSpec(tagRaw);
    return { tagSpec: validSpec };
  }
  if (parts.length === 2) {
    const tagRaw = stripQuotes((parts[0] ?? "").trim());
    const [, validSpec] = validateSanitizeTagSpec(tagRaw);
    const noticeRaw = stripQuotes((parts[1] ?? "").trim());
    return { tagSpec: validSpec, notice: noticeRaw };
  }
  throw new TemplateSyntaxError(
    `'${FILTER_SANITIZE}' accepts at most 2 arguments (tag, optional notice)`,
  );
}

/** Apply `| sanitize`, `| sanitize("tag")`, or `| sanitize("tag", "notice")` to a string. */
export function applySanitizeFilterStr(
  s: string,
  args: string | undefined,
): string {
  const parsed = parseSanitizeFilterArgs(args);
  if (parsed === undefined) {
    return sanitizeTokensString(s);
  }
  return sanitizeBlockString(s, parsed.tagSpec, parsed.notice);
}
