/**
 * Direct expression resolution, function calls, and filters.
 *
 * @module
 */

import { TemplateSyntaxError, UnknownFilterError } from "../errors.js";
import {
  ENUM_TAG_KEY,
  EXPR_START,
  OPTION_NONE,
  OPTION_SOME,
} from "../consts.js";
import { DirectScope } from "./scope.js";
import { directDisplay } from "./display.js";
import { interpolateDirectString } from "./condition.js";

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
    const inner = expr.slice(1, -1);
    if (inner.includes(EXPR_START)) {
      return interpolateDirectString(inner, scope);
    }
    return inner;
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
      if (
        arg !== null &&
        typeof arg === "object" &&
        ENUM_TAG_KEY in (arg as object)
      ) {
        return (arg as Record<string, unknown>)[ENUM_TAG_KEY];
      }
      if (typeof arg === "string") return arg;
      // For transparent option values that are not enums, the kind is "Some"
      return OPTION_SOME;
    }
    case "has": {
      const arg = resolveDirectExpr(argStr, scope);
      if (arg === null || arg === undefined) return false;
      if (typeof arg === "string" && arg === OPTION_NONE) return false;
      if (typeof arg === "object" && arg !== null && !Array.isArray(arg)) {
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

/** Apply a filter to a direct JS value. */
export function applyDirectFilter(
  value: unknown,
  filterName: string,
  filterArgs: string[],
): unknown {
  const strVal = typeof value === "string" ? value : String(value ?? "");
  const numVal = typeof value === "number" ? value : Number(value);

  switch (filterName) {
    case "upper":
      return strVal.toUpperCase();
    case "lower":
      return strVal.toLowerCase();
    case "trim":
      return strVal.trim();
    case "fixed": {
      const digits = filterArgs.length > 0 ? parseInt(filterArgs[0]!, 10) : 2;
      return (typeof value === "number" ? value : numVal).toFixed(digits);
    }
    case "join": {
      const sep = filterArgs.length > 0 ? filterArgs[0]! : ", ";
      if (Array.isArray(value)) {
        return value.map((v) => directDisplay(v)).join(sep);
      }
      return strVal;
    }
    case "limit": {
      const max = filterArgs.length > 0 ? parseInt(filterArgs[0]!, 10) : 100;
      if (Array.isArray(value)) {
        return value.slice(0, max);
      }
      return strVal.length > max ? `${strVal.slice(0, max)}…` : strVal;
    }
    case "add": {
      const n = filterArgs.length > 0 ? parseInt(filterArgs[0]!, 10) : 0;
      return (typeof value === "number" ? value : numVal) + n;
    }
    case "sub": {
      const n = filterArgs.length > 0 ? parseInt(filterArgs[0]!, 10) : 0;
      return (typeof value === "number" ? value : numVal) - n;
    }
    default:
      throw new UnknownFilterError(filterName);
  }
}

/** Parse a filter expression like "fixed(2)" into [name, args]. */
export function parseDirectFilter(filterStr: string): [string, string[]] {
  const parenIdx = filterStr.indexOf("(");
  if (parenIdx < 0) return [filterStr, []];

  const name = filterStr.slice(0, parenIdx).trim();
  const argsStr = filterStr.slice(parenIdx + 1, filterStr.length - 1).trim();
  if (argsStr.length === 0) return [name, []];

  // Strip quotes from arguments
  const args = argsStr.split(",").map((a) => {
    const trimmed = a.trim();
    if (
      (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
      (trimmed.startsWith("'") && trimmed.endsWith("'"))
    ) {
      return trimmed.slice(1, -1);
    }
    return trimmed;
  });
  return [name, args];
}

/** Split by pipe, respecting parentheses. Uses slice instead of char-by-char concatenation. */
export function splitDirectPipes(expr: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < expr.length; i++) {
    const ch = expr.charCodeAt(i);
    if (ch === 40 /* ( */) depth++;
    else if (ch === 41 /* ) */) depth--;
    else if (ch === 124 /* | */ && depth === 0) {
      parts.push(expr.slice(start, i));
      start = i + 1;
    }
  }
  if (start < expr.length) {
    parts.push(expr.slice(start));
  }
  return parts;
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
  const pathPart = parts[0]!.trim();

  let value = resolveDirectExpr(pathPart, scope);

  // Apply filter chain
  for (let i = 1; i < parts.length; i++) {
    const [filterName, filterArgs] = parseDirectFilter(parts[i]!.trim());
    value = applyDirectFilter(value, filterName, filterArgs);
  }

  return value;
}
