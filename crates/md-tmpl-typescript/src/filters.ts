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
  FILTER_SANITIZE,
  FILTER_SANITIZE_ENCLOSED,
  FILTER_UNSAFE_TRUST,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  stripQuotes,
} from "./consts.js";
import { applySanitizeFilterStr } from "./sanitize_runtime.js";
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
  isUnconditionalInvisibleCodePoint,
  stripInvisibleStr,
} from "./invisible.js";
import {
  type CompiledEnclosedSpec,
  type CompiledQuarantineSpec,
  type ParsedTagSpec,
  DEFAULT_QUARANTINE_TAG,
  INLINE_HEADING_SENTINEL,
  ROLE_TOKEN_DELIMITERS,
  TOKEN_DELIMITERS,
  compileEnclosedSpec,
  compileFenceLang,
  compileQuarantineSpec,
  compileSanitizeTokensTags,
  compileValidatedTagSpec,
  fencePreparsed,
  fenceString,
  hasControlTokens,
  hasQuarantineTagBreakout,
  hasRoleControlTokens,
  hasUntrustedBreakout,
  hasUntrustedBreakoutPreparsed,
  isQuarantinedString,
  quarantinePreparsed,
  quarantineString,
  quarantineUntrustedPreparsed,
  quarantineUntrustedString,
  sanitizeEnclosedPreparsed,
  sanitizeEnclosedString,
  sanitizeQuarantinePayload,
  sanitizeRoleTokensString,
  sanitizeTokensString,
  sanitizeTokensWithArgs,
  sanitizeUntrustedPreparsed,
  sanitizeUntrustedString,
  validateTagSpecForFilter,
} from "./filter_security.js";

export {
  type CompiledEnclosedSpec,
  type CompiledQuarantineSpec,
  type ParsedTagSpec,
  DEFAULT_QUARANTINE_TAG,
  INLINE_HEADING_SENTINEL,
  ROLE_TOKEN_DELIMITERS,
  TOKEN_DELIMITERS,
  compileEnclosedSpec,
  compileFenceLang,
  compileQuarantineSpec,
  compileSanitizeTokensTags,
  compileValidatedTagSpec,
  fencePreparsed,
  fenceString,
  hasControlTokens,
  hasInvisibleChars,
  hasQuarantineTagBreakout,
  hasRoleControlTokens,
  hasUntrustedBreakout,
  hasUntrustedBreakoutPreparsed,
  isQuarantinedString,
  isUnconditionalInvisibleCodePoint,
  quarantinePreparsed,
  quarantineString,
  quarantineUntrustedPreparsed,
  quarantineUntrustedString,
  sanitizeEnclosedPreparsed,
  sanitizeEnclosedString,
  sanitizeQuarantinePayload,
  sanitizeRoleTokensString,
  sanitizeTokensString,
  sanitizeTokensWithArgs,
  sanitizeUntrustedPreparsed,
  sanitizeUntrustedString,
  stripInvisibleStr,
  validateTagSpecForFilter,
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

export { stripQuotes };

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
  const out = value.value.toUpperCase();
  return out === value.value ? value : str(out);
}

function applyLower(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'lower' requires a string");
  }
  const out = value.value.toLowerCase();
  return out === value.value ? value : str(out);
}

function applyTrim(value: Value): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'trim' requires a string");
  }
  const out = value.value.trim();
  return out === value.value ? value : str(out);
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

import { isIllegalXmlCodePoint, replaceEachXmlCodePoint } from "./xml_codec.js";

/** Predefined XML entity escaping with illegal control/noncharacter removal in a single pass. */
export function escapeXmlString(s: string): string {
  return replaceEachXmlCodePoint(s, (cp) => {
    switch (cp) {
      case 38 /* '&' */:
        return "&amp;";
      case 60 /* '<' */:
        return "&lt;";
      case 62 /* '>' */:
        return "&gt;";
      case 34 /* '"' */:
        return "&quot;";
      case 39 /* "'" */:
        return "&apos;";
      default:
        return isIllegalXmlCodePoint(cp) ? "" : undefined;
    }
  });
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

function applySanitizeTokens(value: Value, args: string | undefined): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'sanitize_tokens' requires a string");
  }
  return str(sanitizeTokensWithArgs(value.value, args));
}

function applyFence(value: Value, args: string | undefined): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'fence' requires a string");
  }
  return str(fenceString(value.value, args));
}

function applyQuarantine(value: Value, args: string | undefined): Value {
  if (value.type !== "str") {
    throw new TemplateSyntaxError("'quarantine' requires a string");
  }
  return str(quarantineString(value.value, args));
}
