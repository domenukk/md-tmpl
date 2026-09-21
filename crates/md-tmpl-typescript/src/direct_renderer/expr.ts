/**
 * Direct expression resolution, function calls, and filters.
 *
 * @module
 */

import { TemplateSyntaxError, UnknownFilterError } from "../errors.js";
import {
  ENUM_TAG_KEY,
  EXPR_START,
  LIT_FALSE,
  LIT_TRUE,
  OPTION_NONE,
  OPTION_SOME,
  unescapeStringLiteral,
} from "../consts.js";
import {
  escapeXmlString,
  escapeJsonString,
  sanitizeTokensString,
  fenceString,
  quarantineString,
  parseFilter,
  stripQuotes,
} from "../filters.js";
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

  // Dotted path: "task.title"
  const dotIdx = expr.indexOf(".");
  if (dotIdx > 0) {
    const root = expr.slice(0, dotIdx);
    const resolved = scope.resolve(root);
    if (resolved === undefined) return undefined;
    return resolveDirectPath(resolved, expr, dotIdx + 1);
  }

  // Simple variable
  return scope.resolve(expr);
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

function parseDirectNumArg(
  arg: string | undefined,
  filterName: string,
): number {
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
    case "fixed": {
      if (rawArg === undefined) {
        throw new TemplateSyntaxError("'fixed' requires precision arg");
      }
      const digits = parseInt(rawArg, 10);
      if (Number.isNaN(digits)) {
        throw new TemplateSyntaxError(
          `'fixed' precision must be an integer: ${rawArg}`,
        );
      }
      if (typeof value !== "number") {
        throw new TemplateSyntaxError("'fixed' requires a number");
      }
      if (digits === 0 && Number.isInteger(value)) {
        return String(value);
      }
      return value.toFixed(digits);
    }
    case "join": {
      const sep = rawArg !== undefined ? stripQuotes(rawArg) : "";
      if (!Array.isArray(value)) {
        throw new TemplateSyntaxError("'join' requires a list");
      }
      return value.map((v) => directDisplay(v)).join(sep);
    }
    case "limit": {
      if (rawArg === undefined) {
        throw new TemplateSyntaxError("'limit' requires a limit argument");
      }
      const max = parseInt(rawArg, 10);
      if (Number.isNaN(max)) {
        throw new TemplateSyntaxError(
          `'limit' argument must be an integer: ${rawArg}`,
        );
      }
      if (!Array.isArray(value)) {
        throw new TemplateSyntaxError("'limit' requires a list");
      }
      return value.slice(0, max);
    }
    case "add": {
      const n = parseDirectNumArg(rawArg, "add");
      if (typeof value !== "number") {
        throw new TemplateSyntaxError("'add' requires a number");
      }
      return value + n;
    }
    case "sub": {
      const n = parseDirectNumArg(rawArg, "sub");
      if (typeof value !== "number") {
        throw new TemplateSyntaxError("'sub' requires a number");
      }
      return value - n;
    }
    case "escape_xml":
    case "xml":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'escape_xml' requires a string");
      }
      return escapeXmlString(value);
    case "escape_json":
    case "json":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'escape_json' requires a string");
      }
      return escapeJsonString(value);
    case "sanitize_tokens":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'sanitize_tokens' requires a string");
      }
      return sanitizeTokensString(value);
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

  // Apply filter chain
  for (const part of parts.slice(1)) {
    const [filterName, rawArg] = parseFilter(part.trim());
    value = applyDirectFilter(value, filterName, rawArg);
  }

  return value;
}
