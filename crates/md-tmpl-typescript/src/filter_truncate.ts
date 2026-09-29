/**
 * Middle-out truncation filter for strings and lists.
 *
 * Provides `| truncate(limit)` and `| truncate(limit, marker)` to truncate excess content
 * from the middle (retaining both head and tail context) while inserting an informative
 * omission marker with `{skipped}` placeholder replacement.
 *
 * @module
 */

import { type Value, str, list } from "./value.js";
import { TemplateSyntaxError } from "./errors.js";
import { unescapeStringLiteral } from "./consts.js";

/** Default multi-line string truncation marker. */
export const DEFAULT_TRUNCATE_MARKER =
  "\n[TRUNCATED: {skipped} bytes omitted]\n";

/** Default single-line string truncation marker. */
export const DEFAULT_TRUNCATE_MARKER_COMPACT =
  " [TRUNCATED: {skipped} bytes omitted] ";

/** Default list truncation marker. */
export const DEFAULT_LIST_TRUNCATE_MARKER =
  "[TRUNCATED: {skipped} items omitted]";

/** Placeholder replaced with the skipped byte or item count in truncation markers. */
export const TRUNCATE_PLACEHOLDER_SKIPPED = "{skipped}";

/** Alias placeholder replaced with the skipped byte or item count in truncation markers. */
export const TRUNCATE_PLACEHOLDER_COUNT = "{count}";

const UTF8_ENCODER = new TextEncoder();
const UTF8_DECODER = new TextDecoder();
const NEWLINE_BYTE = 0x0a;

/** Strip surrounding quotes from a filter argument and unescape its content. */
function stripQuotesLocal(s: string): string {
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

function substituteMarker(template: string, count: number): string {
  const countStr = String(count);
  return template
    .replaceAll(TRUNCATE_PLACEHOLDER_SKIPPED, countStr)
    .replaceAll(TRUNCATE_PLACEHOLDER_COUNT, countStr);
}

/** Return true if `idx` is on a valid UTF-8 codepoint boundary in `bytes`. */
export function isUtf8CharBoundary(bytes: Uint8Array, idx: number): boolean {
  if (idx <= 0 || idx >= bytes.length) return true;
  return ((bytes[idx] ?? 0) & 0xc0) !== 0x80;
}

/** Return the largest valid UTF-8 character boundary `<= maxBytes` in `bytes`. */
export function floorCharBoundary(bytes: Uint8Array, maxBytes: number): number {
  let idx = Math.min(Math.max(0, maxBytes), bytes.length);
  while (idx > 0 && !isUtf8CharBoundary(bytes, idx)) {
    idx--;
  }
  return idx;
}

/** Return the smallest valid UTF-8 character boundary `>= minBytes` in `bytes`. */
export function ceilCharBoundary(bytes: Uint8Array, minBytes: number): number {
  let idx = Math.min(Math.max(0, minBytes), bytes.length);
  while (idx < bytes.length && !isUtf8CharBoundary(bytes, idx)) {
    idx++;
  }
  return idx;
}

/** Step backwards to the preceding UTF-8 character boundary in `bytes`. */
export function prevCharBoundary(bytes: Uint8Array, idx: number): number {
  if (idx <= 0) return 0;
  let cur = idx - 1;
  while (cur > 0 && !isUtf8CharBoundary(bytes, cur)) {
    cur--;
  }
  return cur;
}

/** Step forwards to the following UTF-8 character boundary in `bytes`. */
export function nextCharBoundary(bytes: Uint8Array, idx: number): number {
  if (idx >= bytes.length) return bytes.length;
  let cur = idx + 1;
  while (cur < bytes.length && !isUtf8CharBoundary(bytes, cur)) {
    cur++;
  }
  return cur;
}

/**
 * Truncate `input` in the middle (head/tail middle-out), keeping at most `limit` UTF-8 bytes.
 *
 * Excess bytes in the middle are replaced with `markerTemplate` (where `{skipped}` is
 * substituted with the exact number of skipped UTF-8 bytes).
 */
export function truncateMiddleString(
  input: string,
  limit: number,
  markerTemplate?: string,
): string {
  if (limit <= 0) return "";
  const inputBytes = UTF8_ENCODER.encode(input);
  const totalLen = inputBytes.length;
  if (totalLen <= limit) return input;

  const hasNewline = input.includes("\n");
  const defaultMarker = hasNewline
    ? DEFAULT_TRUNCATE_MARKER
    : DEFAULT_TRUNCATE_MARKER_COMPACT;
  const markerTmpl = markerTemplate ?? defaultMarker;

  const estOmitted = Math.max(0, totalLen - limit);
  const sampleMarker = substituteMarker(markerTmpl, estOmitted);
  const sampleMarkerBytes = UTF8_ENCODER.encode(sampleMarker);

  if (limit <= sampleMarkerBytes.length) {
    const cut = floorCharBoundary(sampleMarkerBytes, limit);
    return UTF8_DECODER.decode(sampleMarkerBytes.subarray(0, cut));
  }

  const contentBudget = limit - sampleMarkerBytes.length;
  const headBudget = Math.floor(contentBudget / 2);
  const tailBudget = contentBudget - headBudget;

  let headEnd = floorCharBoundary(inputBytes, headBudget);
  let tailStart = ceilCharBoundary(
    inputBytes,
    Math.max(0, totalLen - tailBudget),
  );

  if (hasNewline) {
    for (let i = headEnd - 1; i > 0; i--) {
      if (inputBytes[i] === NEWLINE_BYTE) {
        headEnd = i;
        break;
      }
    }
    for (let i = tailStart; i < totalLen; i++) {
      if (inputBytes[i] === NEWLINE_BYTE) {
        if (i + 1 < totalLen) {
          tailStart = i + 1;
        }
        break;
      }
    }
  }

  if (headEnd > tailStart) {
    headEnd = tailStart;
  }

  const skippedBytes = Math.max(0, tailStart - headEnd);
  let marker = substituteMarker(markerTmpl, skippedBytes);
  let markerByteLen = UTF8_ENCODER.encode(marker).length;

  while (
    headEnd + markerByteLen + (totalLen - tailStart) > limit &&
    headEnd > 0
  ) {
    headEnd = prevCharBoundary(inputBytes, headEnd);
  }
  while (
    headEnd + markerByteLen + (totalLen - tailStart) > limit &&
    tailStart < totalLen
  ) {
    tailStart = nextCharBoundary(inputBytes, tailStart);
  }

  const finalSkipped = Math.max(0, tailStart - headEnd);
  if (finalSkipped !== skippedBytes) {
    marker = substituteMarker(markerTmpl, finalSkipped);
    markerByteLen = UTF8_ENCODER.encode(marker).length;
    while (
      headEnd + markerByteLen + (totalLen - tailStart) > limit &&
      headEnd > 0
    ) {
      headEnd = prevCharBoundary(inputBytes, headEnd);
    }
    while (
      headEnd + markerByteLen + (totalLen - tailStart) > limit &&
      tailStart < totalLen
    ) {
      tailStart = nextCharBoundary(inputBytes, tailStart);
    }
  }

  const headStr = UTF8_DECODER.decode(inputBytes.subarray(0, headEnd));
  const tailStr = UTF8_DECODER.decode(inputBytes.subarray(tailStart));
  return headStr + marker + tailStr;
}

/** Apply the `truncate` / `truncate_middle` filter to a {@link Value}. */
export function applyTruncate(value: Value, args: string | undefined): Value {
  if (args === undefined || args.trim().length === 0) {
    throw new TemplateSyntaxError(
      "'truncate' requires at least a limit argument",
    );
  }
  const commaIdx = args.indexOf(",");
  const limitStr = (commaIdx !== -1 ? args.slice(0, commaIdx) : args).trim();
  const markerArg =
    commaIdx !== -1
      ? stripQuotesLocal(args.slice(commaIdx + 1).trim())
      : undefined;

  const limit = Number(limitStr);
  if (Number.isNaN(limit) || !Number.isInteger(limit) || limit < 0) {
    throw new TemplateSyntaxError(
      `'truncate' limit must be an integer: ${limitStr}`,
    );
  }

  if (value.type === "str") {
    return str(truncateMiddleString(value.value, limit, markerArg));
  }

  if (value.type === "list") {
    const total = value.items.length;
    if (total <= limit) return value;
    if (limit === 0) return list([]);

    const skippedItems = total - limit;
    const headCount = Math.floor(limit / 2);
    const tailCount = limit - headCount;

    let markerItem: Value | undefined;
    if (markerArg === undefined) {
      markerItem = str(
        substituteMarker(DEFAULT_LIST_TRUNCATE_MARKER, skippedItems),
      );
    } else if (markerArg.length > 0) {
      markerItem = str(substituteMarker(markerArg, skippedItems));
    }

    const head = value.items.slice(0, headCount);
    const tail = value.items.slice(total - tailCount);
    const resultItems =
      markerItem !== undefined
        ? [...head, markerItem, ...tail]
        : [...head, ...tail];
    return list(resultItems);
  }

  throw new TemplateSyntaxError("'truncate' requires a string or a list");
}
