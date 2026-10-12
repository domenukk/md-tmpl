/**
 * Direct expression resolution, function calls, and filters.
 *
 * @module
 */

import { TemplateSyntaxError, UnknownFilterError } from "../errors.js";
import {
  ENUM_TAG_KEY,
  EXPR_START,
  FILTER_ADD,
  FILTER_FIXED,
  FILTER_LIMIT,
  FILTER_SANITIZE_ENCLOSED,
  FILTER_SUB,
  FILTER_TRUNCATE,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_STR,
  LIT_FALSE,
  LIT_TRUE,
  OPTION_NONE,
  OPTION_SOME,
  PREFIX_CONSTS_DOT,
  PREFIX_OPTIONS_DOT,
  PREFIX_OPTS_DOT,
  PREFIX_PARAMS_DOT,
  unescapeStringLiteral,
} from "../consts.js";
import { ENUM_VARIANTS_KEY } from "../value.js";
import {
  escapeXmlString,
  escapeJsonString,
  sanitizeEnclosedString,
  sanitizeTokensWithArgs,
  fenceString,
  quarantineString,
  quarantineUntrustedString,
  truncateMiddleString,
  applyFilter,
  parseFilter,
  stripQuotes,
} from "../filters.js";
import { applySanitizeFilterStr } from "../sanitize_pass.js";
import {
  LIMIT_ARG_LABEL,
  TRUNCATE_ARG_LABEL,
  parseUsizeFilterArg,
} from "../filter_args.js";
import { truncateMiddleItems } from "../filter_truncate.js";
import { parseJsonIndent, serializeJson } from "../filter_json.js";
import { fromJs } from "../value.js";
import { splitPipes } from "../evaluator.js";
import { DirectScope } from "./scope.js";
import { directDisplay } from "./display.js";
import { interpolateDirectString } from "./condition.js";

const DIRECT_NUM_LITERAL_RE = /^-?[0-9]+(?:\.[0-9]+)?$/;

// ---------------------------------------------------------------------------
// Direct expression resolution
// ---------------------------------------------------------------------------

/** Resolve a dotted path (e.g., "task.title") from a JS value. */
export function resolveDirectPath(
  root: unknown,
  path: string,
  startOffset: number,
): unknown {
  let current = root;
  let start = startOffset;
  while (start < path.length) {
    if (current === null || current === undefined) return undefined;
    if (typeof current !== "object" || Array.isArray(current)) return undefined;
    const nextDot = path.indexOf(".", start);
    const end = nextDot === -1 ? path.length : nextDot;
    const key = path.slice(start, end);
    // Skip __kind__ tag (enum protocol)
    if (key === ENUM_TAG_KEY) return undefined;
    current = (current as Record<string, unknown>)[key];
    start = end + 1;
  }
  return current;
}

/** Resolve an expression in the direct scope. */
export function resolveDirectExpr(expr: string, scope: DirectScope): unknown {
  // String literal: "..." or '...' — with optional {{ expr }} interpolation.
  const first = expr.charCodeAt(0);
  if (
    (first === 34 /* '"' */ || first === 39) /* "'" */ &&
    expr.charCodeAt(expr.length - 1) === first
  ) {
    const inner = unescapeStringLiteral(expr.slice(1, -1));
    if (inner.includes(EXPR_START)) {
      return interpolateDirectString(inner, scope);
    }
    return inner;
  }

  // Bare boolean and numeric literals
  if (expr === LIT_TRUE) return true;
  if (expr === LIT_FALSE) return false;
  if (DIRECT_NUM_LITERAL_RE.test(expr)) {
    return Number(expr);
  }

  // Function calls (must end with ')')
  if (expr.charCodeAt(expr.length - 1) === 41 /* ')' */) {
    return resolveDirectFunction(expr, scope);
  }

  // Strip namespace prefixes (`consts.`, `opts.`, `options.`, `params.`)
  let path = expr;
  if (path.startsWith(PREFIX_CONSTS_DOT)) {
    path = path.slice(PREFIX_CONSTS_DOT.length).trim();
  } else if (path.startsWith(PREFIX_OPTS_DOT)) {
    path = path.slice(PREFIX_OPTS_DOT.length).trim();
  } else if (path.startsWith(PREFIX_OPTIONS_DOT)) {
    path = path.slice(PREFIX_OPTIONS_DOT.length).trim();
  } else if (path.startsWith(PREFIX_PARAMS_DOT)) {
    path = path.slice(PREFIX_PARAMS_DOT.length).trim();
  }

  // Loop metadata (loop.<prop>)
  if (path.startsWith("loop.")) {
    const prop = path.slice(5).trim();
    const meta = scope.getLastLoopMeta();
    if (!meta) {
      throw new TemplateSyntaxError(
        "loop metadata is only available inside a for loop",
      );
    }
    switch (prop) {
      case "first":
        return meta.index === 0;
      case "last":
        return meta.len > 0 && meta.index + 1 === meta.len;
      case "index0":
        return meta.index;
      case "index":
        return meta.index + 1;
      case "length":
      case "len":
        return meta.len;
      default:
        throw new TemplateSyntaxError(`field '${prop}' not found on loop`);
    }
  }

  // Dotted path: "task.title" or imported const "child.MAX" / "child.Stage.Design"
  const dotIdx = path.indexOf(".");
  if (dotIdx > 0) {
    const exactConst = scope.resolveConst(path);
    if (exactConst !== undefined) return exactConst;

    const secondDot = path.indexOf(".", dotIdx + 1);
    if (secondDot !== -1) {
      const stemKey = path.slice(0, secondDot);
      const stemVal = scope.resolveConst(stemKey);
      if (stemVal !== undefined) {
        return resolveDirectPath(stemVal, path, secondDot + 1);
      }
    }

    const root = path.slice(0, dotIdx);
    const resolved = scope.resolve(root);
    if (resolved === undefined) return undefined;
    return resolveDirectPath(resolved, path, dotIdx + 1);
  }

  // Simple variable
  return scope.resolve(path);
}

/** Handle built-in function calls. */
export function resolveDirectFunction(
  expr: string,
  scope: DirectScope,
): unknown {
  const parenIdx = expr.indexOf("(");
  if (parenIdx < 0) return undefined;

  const funcName = expr.slice(0, parenIdx).trim();
  const argStr = expr.slice(parenIdx + 1, expr.length - 1).trim();

  switch (funcName) {
    case "len": {
      const arg = resolveDirectExpr(argStr, scope);
      if (typeof arg === "string") return arg.length;
      if (Array.isArray(arg)) return arg.length;
      throw new TemplateSyntaxError(
        `len() requires a list or string, got ${typeof arg}`,
      );
    }
    case "idx": {
      // idx() or idx(binding) — return current loop index
      const binding = argStr || findLoopBinding(scope);
      if (binding) {
        const idx = scope.getLoopIndex(binding);
        if (idx !== undefined) return idx;
      }
      throw new TemplateSyntaxError(
        `idx() requires an active loop binding${argStr ? ` for '${argStr}'` : ""}`,
      );
    }
    case "kind": {
      const arg = resolveDirectExpr(argStr, scope);
      if (arg === null || arg === undefined) return OPTION_NONE;
      if (typeof arg === "object" && ENUM_TAG_KEY in arg) {
        return (arg as Record<string, unknown>)[ENUM_TAG_KEY];
      }
      if (typeof arg === "string") return arg;
      // For transparent option values that are not enums, the kind is "Some"
      return OPTION_SOME;
    }
    case "kinds": {
      const arg = resolveDirectExpr(argStr, scope);
      if (arg !== null && typeof arg === "object" && !Array.isArray(arg)) {
        const variants = (arg as Record<string, unknown>)[ENUM_VARIANTS_KEY];
        if (Array.isArray(variants)) return variants;
      }
      throw new TemplateSyntaxError(
        `kinds() requires an enum type namespace, got ${typeof arg}`,
      );
    }
    case "has": {
      const arg = resolveDirectExpr(argStr, scope);
      if (arg === null || arg === undefined) return false;
      if (typeof arg === "object" && !Array.isArray(arg)) {
        const obj = arg as Record<string, unknown>;
        if (obj[ENUM_TAG_KEY] === OPTION_NONE) return false;
      }
      return true;
    }
    default:
      throw new TemplateSyntaxError(`unknown function '${funcName}'`);
  }
}

/** Find the most recent loop binding for bare `idx()` calls. */
export function findLoopBinding(scope: DirectScope): string | undefined {
  return scope.getLastLoopBinding();
}

// ---------------------------------------------------------------------------
// Direct filters
// ---------------------------------------------------------------------------

/**
 * Apply `fixed` / `add` / `sub` to a raw JS value through the typed filter
 * implementation (integers map to `int`, like `fromJs`), unwrapping the result.
 */
function directNumericFilter(
  value: unknown,
  filterName: string,
  rawArg: string | undefined,
): unknown {
  const result = applyFilter(fromJs(value), filterName, rawArg);
  return result.type === TYPE_STR ||
    result.type === TYPE_INT ||
    result.type === TYPE_FLOAT
    ? result.value
    : result;
}

/** Apply a filter to a direct JS value. */
export function applyDirectFilter(
  value: unknown,
  filterName: string,
  rawArg: string | undefined,
): unknown {
  switch (filterName) {
    case "upper":
      if (typeof value !== "string")
        throw new TemplateSyntaxError("'upper' requires a string");
      return value.toUpperCase();
    case "lower":
      if (typeof value !== "string")
        throw new TemplateSyntaxError("'lower' requires a string");
      return value.toLowerCase();
    case "trim":
      if (typeof value !== "string")
        throw new TemplateSyntaxError("'trim' requires a string");
      return value.trim();
    case FILTER_FIXED:
    case FILTER_ADD:
    case FILTER_SUB:
      return directNumericFilter(value, filterName, rawArg);
    case "join": {
      const sep = rawArg !== undefined ? stripQuotes(rawArg) : "";
      if (!Array.isArray(value)) {
        throw new TemplateSyntaxError("'join' requires a list");
      }
      return value.map((v) => directDisplay(v)).join(sep);
    }
    case FILTER_LIMIT: {
      if (!Array.isArray(value)) {
        return applyFilter(fromJs(value), filterName, rawArg);
      }
      if (rawArg === undefined) {
        throw new TemplateSyntaxError("'limit' requires a limit argument");
      }
      return value.slice(
        0,
        parseUsizeFilterArg(rawArg, FILTER_LIMIT, LIMIT_ARG_LABEL),
      );
    }
    case "escape_xml":
    case "xml":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'escape_xml' requires a string");
      }
      return escapeXmlString(value);
    case "escape_json":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'escape_json' requires a string");
      }
      return escapeJsonString(value);
    case "tojson":
    case "to_json":
    case "json":
      return directToJson(value, parseJsonIndent(rawArg));
    case "sanitize_tokens":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'sanitize_tokens' requires a string");
      }
      return sanitizeTokensWithArgs(value, rawArg);
    case "fence":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'fence' requires a string");
      }
      return fenceString(value, rawArg);
    case "quarantine":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'quarantine' requires a string");
      }
      return quarantineString(value, rawArg);
    case "sanitize":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'sanitize' requires a string");
      }
      return applySanitizeFilterStr(value, rawArg);
    case FILTER_SANITIZE_ENCLOSED:
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'sanitize' requires a string");
      }
      return sanitizeEnclosedString(value, rawArg);
    case "truncate":
    case "truncate_middle": {
      if (rawArg === undefined || rawArg.trim().length === 0) {
        throw new TemplateSyntaxError(
          "'truncate' requires at least a limit argument",
        );
      }
      const commaIdx = rawArg.indexOf(",");
      const limitStr = (
        commaIdx !== -1 ? rawArg.slice(0, commaIdx) : rawArg
      ).trim();
      const markerArg =
        commaIdx !== -1
          ? stripQuotes(rawArg.slice(commaIdx + 1).trim())
          : undefined;
      const limit = parseUsizeFilterArg(
        limitStr,
        FILTER_TRUNCATE,
        TRUNCATE_ARG_LABEL,
      );
      if (typeof value === "string") {
        return truncateMiddleString(value, limit, markerArg);
      }
      if (Array.isArray(value)) {
        return truncateMiddleItems(
          value as unknown[],
          limit,
          markerArg,
          (text): unknown => text,
        );
      }
      throw new TemplateSyntaxError("'truncate' requires a string or a list");
    }
    default:
      throw new UnknownFilterError(filterName);
  }
}

/** Parse a filter expression like "fixed(2)" into [name, args]. */
export function parseDirectFilter(filterStr: string): [string, string[]] {
  const [name, rawArg] = parseFilter(filterStr);
  return [name, rawArg !== undefined ? [rawArg] : []];
}

/** Split by pipe, respecting quotes and parentheses. */
export function splitDirectPipes(expr: string): string[] {
  return splitPipes(expr);
}

/** Evaluate an expression with filters, returning a JS value. */
export function evaluateDirectExpr(expr: string, scope: DirectScope): unknown {
  // Fast path: no pipe means no filters
  const pipeIdx = expr.indexOf("|");
  if (pipeIdx === -1) {
    const trimmed =
      expr.charCodeAt(0) === 32 || expr.charCodeAt(expr.length - 1) === 32
        ? expr.trim()
        : expr;
    return resolveDirectExpr(trimmed, scope);
  }

  const parts = splitDirectPipes(expr.trim());
  const pathPart = (parts[0] ?? "").trim();

  let value = resolveDirectExpr(pathPart, scope);

  // Apply filter chain (fusing adjacent sanitize_tokens | quarantine into 1 pass)
  for (let i = 1; i < parts.length; i++) {
    const part = parts[i];
    if (part === undefined) continue;
    const [filterName, rawArg] = parseFilter(part.trim());
    if (
      filterName === "sanitize_tokens" &&
      rawArg === undefined &&
      typeof value === "string" &&
      i + 1 < parts.length
    ) {
      const nextPart = parts[i + 1];
      if (nextPart !== undefined) {
        const [nextName, nextArg] = parseFilter(nextPart.trim());
        if (nextName === "quarantine") {
          value = quarantineUntrustedString(value, nextArg);
          i++;
          continue;
        }
      }
    }
    value = applyDirectFilter(value, filterName, rawArg);
  }

  return value;
}

export function directToJson(value: unknown, indent?: number): string {
  if (typeof value === "function") {
    throw new TemplateSyntaxError("cannot serialize template to JSON");
  }
  return serializeJson(fromJs(value), indent);
}
