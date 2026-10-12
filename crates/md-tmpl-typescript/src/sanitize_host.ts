/**
 * Host-side quarantine / sanitize envelope helpers composed from the core
 * sanitizers (mirrors the Rust `md_tmpl` exports `strip_outer_quarantine_tag`,
 * `unquarantine_str`, `quarantine_untrusted_idempotent_str`, and
 * `sanitize_block_idempotent_str`).
 *
 * @module
 */

import { DEFAULT_SANITIZE_TAG, FILTER_QUARANTINE } from "./consts.js";
import {
  DEFAULT_QUARANTINE_TAG,
  hasUntrustedBreakout,
  isQuarantinedString,
  quarantineUntrustedString,
  stripQuotes,
  validateTagSpecForFilter,
} from "./filters.js";
import {
  formatSanitizeNotice,
  isSanitizedBlockString,
  sanitizeBlockString,
  validateSanitizeTagSpec,
} from "./sanitize_runtime.js";

/** Canonical XML tag name for `<tool_output_quarantine>` envelopes. */
export const TOOL_OUTPUT_QUARANTINE_TAG = "tool_output_quarantine";
/** Canonical XML tag name for `<untrusted_tool_output>` envelopes. */
export const UNTRUSTED_TOOL_OUTPUT_TAG = "untrusted_tool_output";
/** Canonical opening tag for `<tool_output_quarantine>` framing. */
export const TOOL_OUTPUT_QUARANTINE_OPEN_TAG = `<${TOOL_OUTPUT_QUARANTINE_TAG}>`;
/** Canonical closing tag for `<tool_output_quarantine>` framing. */
export const TOOL_OUTPUT_QUARANTINE_CLOSE_TAG = `</${TOOL_OUTPUT_QUARANTINE_TAG}>`;
/** Canonical opening tag for `<untrusted_tool_output>` framing. */
export const UNTRUSTED_QUARANTINE_OPEN_TAG = `<${UNTRUSTED_TOOL_OUTPUT_TAG}>`;
/** Canonical closing tag for `<untrusted_tool_output>` framing. */
export const UNTRUSTED_QUARANTINE_CLOSE_TAG = `</${UNTRUSTED_TOOL_OUTPUT_TAG}>`;

const ENVELOPE_NEWLINE = "\n";

/** Remove at most one leading and one trailing envelope newline. */
function stripEnvelopeNewlines(body: string): string {
  const withoutLeading = body.startsWith(ENVELOPE_NEWLINE)
    ? body.slice(ENVELOPE_NEWLINE.length)
    : body;
  return withoutLeading.endsWith(ENVELOPE_NEWLINE)
    ? withoutLeading.slice(0, -ENVELOPE_NEWLINE.length)
    : withoutLeading;
}

/** Return the raw body between `<tag>` and `</tag>`, or `undefined` if `s` is not wrapped. */
function envelopeBody(s: string, tagName: string): string | undefined {
  const open = `<${tagName}>`;
  const close = `</${tagName}>`;
  if (
    s.length < open.length + close.length ||
    !s.startsWith(open) ||
    !s.endsWith(close)
  ) {
    return undefined;
  }
  return s.slice(open.length, s.length - close.length);
}

/**
 * Strip outer `<tagName>` and `</tagName>` tags if present, along with at most
 * one leading and one trailing `\n` of envelope framing.
 */
export function stripOuterQuarantineTag(
  s: string,
  tagName: string,
): string | undefined {
  const body = envelopeBody(s, tagName);
  return body === undefined ? undefined : stripEnvelopeNewlines(body);
}

/**
 * Extract the inner payload of a quarantined string if and only if `s` is a
 * valid quarantine envelope with no un-neutralized breakouts.
 */
export function unquarantineString(
  s: string,
  tagArg?: string,
): string | undefined {
  const rawSpec =
    tagArg !== undefined ? stripQuotes(tagArg) : DEFAULT_QUARANTINE_TAG;
  let primaryTag: string;
  let validSpec: string;
  try {
    [primaryTag, validSpec] = validateTagSpecForFilter(
      rawSpec,
      FILTER_QUARANTINE,
    );
  } catch {
    return undefined;
  }
  const body = envelopeBody(s, primaryTag);
  if (body === undefined || hasUntrustedBreakout(body, validSpec)) {
    return undefined;
  }
  return stripEnvelopeNewlines(body);
}

/**
 * Idempotently sanitize and wrap `s` in `<primary_tag>\n...\n</primary_tag>`:
 * an already valid envelope is returned unchanged, and a dirty envelope is
 * unwrapped once and re-wrapped cleanly.
 */
export function quarantineUntrustedIdempotentString(
  s: string,
  tagArg?: string,
): string {
  const rawSpec =
    tagArg !== undefined ? stripQuotes(tagArg) : DEFAULT_QUARANTINE_TAG;
  const [primaryTag, validSpec] = validateTagSpecForFilter(
    rawSpec,
    FILTER_QUARANTINE,
  );
  if (isQuarantinedString(s, validSpec)) {
    return s;
  }
  const inner = stripOuterQuarantineTag(s.trim(), primaryTag) ?? s;
  return quarantineUntrustedString(inner, validSpec);
}

/**
 * Idempotently sanitize and wrap `s` in `<primary_tag>\n{notice}\n{sanitized}\n</primary_tag>`:
 * an already valid block is returned unchanged, and a dirty block is unwrapped
 * once (dropping a leading notice line) and re-wrapped cleanly.
 */
export function sanitizeBlockIdempotentString(
  s: string,
  tagArg?: string,
  customNotice?: string,
): string {
  const rawSpec =
    tagArg !== undefined ? stripQuotes(tagArg) : DEFAULT_SANITIZE_TAG;
  const [primaryTag, validSpec] = validateSanitizeTagSpec(rawSpec);
  if (isSanitizedBlockString(s, validSpec, customNotice)) {
    return s;
  }
  let inner = stripOuterQuarantineTag(s.trim(), primaryTag) ?? s;
  const notice = formatSanitizeNotice(primaryTag, customNotice).replace(
    /\n+$/,
    "",
  );
  if (notice.length > 0 && inner.startsWith(notice)) {
    const afterNotice = inner.slice(notice.length);
    inner = afterNotice.startsWith(ENVELOPE_NEWLINE)
      ? afterNotice.slice(ENVELOPE_NEWLINE.length)
      : afterNotice;
  }
  return sanitizeBlockString(inner, validSpec, customNotice);
}
