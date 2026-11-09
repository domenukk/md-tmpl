/**
 * Host-side quarantine / sanitize envelope helpers composed from the core
 * sanitizers (mirrors the Rust `md_tmpl` exports `strip_outer_quarantine_tag`,
 * `unquarantine_str`, `quarantine_untrusted_idempotent_str`, and
 * `sanitize_block_idempotent_str`).
 *
 * @module
 */

import { FILTER_QUARANTINE } from "./consts.js";
import {
  DEFAULT_QUARANTINE_TAG,
  hasUntrustedBreakout,
  isQuarantinedString,
  quarantineUntrustedString,
  stripQuotes,
  validateTagSpecForFilter,
} from "./filters.js";
import {
  envelopeBody,
  stripEnvelopeNewlines,
  stripOuterQuarantineTag,
} from "./sanitize_runtime.js";

export {
  sanitizeBlockIdempotentString,
  stripOuterQuarantineTag,
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
