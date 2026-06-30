/**
 * Direct renderer — renders templates directly from JS values.
 *
 * This module bypasses the `Value` intermediate representation entirely.
 * Instead of `fromJs() → Value → display()`, it works directly with
 * plain JS objects, arrays, strings, etc.
 *
 * This gives a significant speedup for `renderUnchecked()` because:
 * - No object allocations for Value wrappers
 * - No `new Map()` for each dict
 * - No `Object.entries()` scanning
 * - Direct property access instead of `Map.get()`
 *
 * @module
 */

import type { Node } from "./parser.js";
import { TemplateSyntaxError } from "./errors.js";

// ---------------------------------------------------------------------------
// Direct scope — resolves variables from plain JS values
// ---------------------------------------------------------------------------

/** A lightweight scope for direct rendering. */
class DirectScope {
  private readonly layers: Map<string, unknown>[] = [];
  private readonly consts: ReadonlyMap<string, unknown>;
  private readonly loopMeta = new Map<string, { index: number }>();
  private lastLoopBinding: string | undefined;

  constructor(
    topLevel: ReadonlyMap<string, unknown>,
    consts: ReadonlyMap<string, unknown>,
  ) {
    this.layers.push(new Map(topLevel));
    this.consts = consts;
  }

  resolve(name: string): unknown {
    // Check layers top-down
    for (let i = this.layers.length - 1; i >= 0; i--) {
      const layer = this.layers[i]!;
      if (layer.has(name)) return layer.get(name);
    }
    // Check consts — return raw value (display conversion happens at render)
    if (this.consts.has(name)) {
      return this.consts.get(name);
    }
    return undefined;
  }

  pushLayer(): Map<string, unknown> {
    const layer = new Map<string, unknown>();
    this.layers.push(layer);
    return layer;
  }

  popLayer(): void {
    this.layers.pop();
  }

  setLoopIndex(binding: string, index: number): void {
    this.loopMeta.set(binding, { index });
    this.lastLoopBinding = binding;
  }

  getLoopIndex(binding: string): number | undefined {
    return this.loopMeta.get(binding)?.index;
  }

  getLastLoopBinding(): string | undefined {
    return this.lastLoopBinding;
  }
}

// ---------------------------------------------------------------------------
// Direct display — convert any JS value to string
// ---------------------------------------------------------------------------

/** Convert a JS value to its display string. */
function directDisplay(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") return String(value);
  if (Array.isArray(value)) {
    throw new Error(
      "cannot display list value directly — iterate with '{% for item in list %}' instead",
    );
  }
  if (typeof value === "object") {
    throw new Error(
      "cannot display struct value directly — access individual fields (e.g. '{{ value.field }}') instead",
    );
  }
  return String(value);
}

/** Check if a JS value is truthy (template semantics). */
function directIsTruthy(value: unknown): boolean {
  if (value === null || value === undefined) return false;
  if (typeof value === "string") return value.length > 0;
  if (typeof value === "boolean") return value;
  if (typeof value === "number") return value !== 0;
  if (Array.isArray(value)) return value.length > 0;
  if (typeof value === "object") return Object.keys(value).length > 0;
  return Boolean(value);
}

// ---------------------------------------------------------------------------
// Direct expression resolution
// ---------------------------------------------------------------------------

/** Resolve a dotted path (e.g., "task.title") from a JS value. */
function resolveDirectPath(
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
    if (key === "__kind__") return undefined;
    current = (current as Record<string, unknown>)[key];
    start = end + 1;
  }
  return current;
}

/** Resolve an expression in the direct scope. */
function resolveDirectExpr(expr: string, scope: DirectScope): unknown {
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
function resolveDirectFunction(expr: string, scope: DirectScope): unknown {
  const parenIdx = expr.indexOf("(");
  if (parenIdx < 0) return undefined;

  const funcName = expr.slice(0, parenIdx).trim();
  const argStr = expr.slice(parenIdx + 1, expr.length - 1).trim();

  switch (funcName) {
    case "len": {
      const arg = resolveDirectExpr(argStr, scope);
      if (typeof arg === "string") return arg.length;
      if (Array.isArray(arg)) return arg.length;
      if (arg !== null && arg !== undefined && typeof arg === "object")
        return Object.keys(arg as object).length;
      throw new TemplateSyntaxError(
        `len() requires a list, string, or struct, got ${typeof arg}`,
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
      if (arg === null || arg === undefined) return "None";
      if (
        arg !== null &&
        typeof arg === "object" &&
        "__kind__" in (arg as object)
      ) {
        return (arg as Record<string, unknown>).__kind__;
      }
      if (typeof arg === "string") return arg;
      // For transparent option values that are not enums, the kind is "Some"
      return "Some";
    }
    case "has": {
      const arg = resolveDirectExpr(argStr, scope);
      if (arg === null || arg === undefined) return false;
      if (typeof arg === "string" && arg === "None") return false;
      if (typeof arg === "object" && arg !== null && !Array.isArray(arg)) {
        const obj = arg as Record<string, unknown>;
        if (obj.__kind__ === "None") return false;
        if (obj._prompt_template_tag === "None") return false;
      }
      return true;
    }
    default:
      return undefined;
  }
}

/** Find the most recent loop binding for bare `idx()` calls. */
function findLoopBinding(scope: DirectScope): string | undefined {
  return scope.getLastLoopBinding();
}

// ---------------------------------------------------------------------------
// Direct filters
// ---------------------------------------------------------------------------

/** Apply a filter to a direct JS value. */
function applyDirectFilter(
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
      return value;
  }
}

/** Parse a filter expression like "fixed(2)" into [name, args]. */
function parseDirectFilter(filterStr: string): [string, string[]] {
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
function splitDirectPipes(expr: string): string[] {
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
function evaluateDirectExpr(expr: string, scope: DirectScope): unknown {
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

// ---------------------------------------------------------------------------
// Direct condition evaluation
// ---------------------------------------------------------------------------

/** Evaluate a condition expression directly. */
function evaluateDirectCondition(cond: string, scope: DirectScope): boolean {
  const trimmed = cond.trim();

  // Comparison: "level == 1", "name != 'foo'"
  const ops = ["==", "!=", ">=", "<=", ">", "<"] as const;
  for (const op of ops) {
    const idx = trimmed.indexOf(op);
    if (idx > 0) {
      const lhs = evaluateDirectExpr(trimmed.slice(0, idx), scope);
      const rhsStr = trimmed.slice(idx + op.length).trim();
      const rhs =
        parseDirectLiteral(rhsStr) ?? evaluateDirectExpr(rhsStr, scope);

      // eslint-disable-next-line eqeqeq
      switch (op) {
        case "==":
          return lhs === rhs;
        case "!=":
          return lhs !== rhs;
        case ">=":
          return (lhs as number) >= (rhs as number);
        case "<=":
          return (lhs as number) <= (rhs as number);
        case ">":
          return (lhs as number) > (rhs as number);
        case "<":
          return (lhs as number) < (rhs as number);
      }
    }
  }

  // Simple truthiness
  return directIsTruthy(evaluateDirectExpr(trimmed, scope));
}

/** Parse a literal value (number or quoted string). */
function parseDirectLiteral(s: string): unknown {
  if (s === "true") return true;
  if (s === "false") return false;
  if (/^-?\d+$/.test(s)) return parseInt(s, 10);
  if (/^-?\d+\.\d+$/.test(s)) return parseFloat(s);
  if (
    (s.startsWith('"') && s.endsWith('"')) ||
    (s.startsWith("'") && s.endsWith("'"))
  ) {
    return s.slice(1, -1);
  }
  return undefined;
}

// ---------------------------------------------------------------------------
// Direct render — the main entry point
// ---------------------------------------------------------------------------

/** Get the variant name from a JS value (enum dispatch). */
function getDirectVariantName(value: unknown, isOption: boolean): string {
  // Transparent option: null/undefined is "None", anything else is "Some"
  if (value === null || value === undefined) return "None";
  if (isOption) return "Some";
  if (typeof value === "string") return value;
  if (value !== null && typeof value === "object") {
    // Check __kind__ protocol
    const obj = value as Record<string, unknown>;
    if (typeof obj.__kind__ === "string") return obj.__kind__;
    // Check _prompt_template_tag protocol
    if (typeof obj._prompt_template_tag === "string") {
      return obj._prompt_template_tag;
    }
  }
  // Non-None, non-enum values in option match context → "Some"
  return "Some";
}

/** Returns true if the match arms/guard use option-style variant names. */
function isOptionMatch(node: {
  arms: { variants: string[] }[];
  inlineGuard?: { variant: string };
}): boolean {
  if (node.inlineGuard) {
    return (
      node.inlineGuard.variant === "Some" || node.inlineGuard.variant === "None"
    );
  }
  return node.arms.some((arm) =>
    arm.variants.some((v) => v === "Some" || v === "None"),
  );
}

/**
 * Render AST nodes directly from JS values — no Value conversion.
 *
 * **Limitation:** `{% include %}` and `{% tmpl %}` nodes are silently
 * skipped in the direct renderer because resolving them would require
 * a template-loader / inline-template map.  This is acceptable because
 * `renderDirect` is only used by `renderUnchecked()`, where the caller
 * has explicitly opted out of full validation.
 *
 * @param nodes - Parsed AST nodes.
 * @param params - Plain JS params object.
 * @param constJsValues - Pre-converted constant values (as JS).
 */
export function renderDirect(
  nodes: readonly Node[],
  params: ReadonlyMap<string, unknown>,
  constJsValues: ReadonlyMap<string, unknown>,
): string {
  const scope = new DirectScope(params, constJsValues);
  return renderDirectNodes(nodes, scope);
}

/** Render nodes with a direct scope. */
function renderDirectNodes(nodes: readonly Node[], scope: DirectScope): string {
  const parts: string[] = [];

  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i]!;
    switch (node.kind) {
      case "text":
        parts.push(node.text);
        break;

      case "expr": {
        if (node.trimBefore && parts.length > 0) {
          const last = parts[parts.length - 1]!;
          parts[parts.length - 1] = last.replace(/\s+$/, "");
        }
        const val = evaluateDirectExpr(node.expr, scope);
        parts.push(directDisplay(val));
        if (node.trimAfter) {
          // Trim leading whitespace from the next text node without
          // mutating the AST (which would corrupt subsequent renders).
          if (i + 1 < nodes.length && nodes[i + 1]!.kind === "text") {
            const next = nodes[i + 1]! as { kind: "text"; text: string };
            parts.push(next.text.replace(/^\s+/, ""));
            i++; // skip the next node — we already handled it
          }
        }
        break;
      }

      case "comment":
        break;

      case "for": {
        const listVal = evaluateDirectExpr(node.iterExpr, scope);
        if (!Array.isArray(listVal)) {
          throw new TemplateSyntaxError(
            `for loop requires a list, got ${typeof listVal}`,
          );
        }
        if (listVal.length === 0 && node.elseBody) {
          parts.push(renderDirectNodes(node.elseBody, scope));
        } else {
          for (let idx = 0; idx < listVal.length; idx++) {
            const item = listVal[idx];
            const layer = scope.pushLayer();
            layer.set(node.binding, item);
            scope.setLoopIndex(node.binding, idx);
            parts.push(renderDirectNodes(node.body, scope));
            scope.popLayer();
          }
        }
        break;
      }

      case "if": {
        let matched = false;
        for (const branch of node.branches) {
          if (evaluateDirectCondition(branch.condition, scope)) {
            parts.push(renderDirectNodes(branch.body, scope));
            matched = true;
            break;
          }
        }
        if (!matched && node.elseBody) {
          parts.push(renderDirectNodes(node.elseBody, scope));
        }
        break;
      }

      case "match": {
        const optMatch = isOptionMatch(node);
        if (node.inlineGuard) {
          const val = evaluateDirectExpr(node.expr, scope);
          const variantName = getDirectVariantName(val, optMatch);
          if (variantName === node.inlineGuard.variant) {
            parts.push(renderDirectNodes(node.inlineGuard.body, scope));
          }
        } else {
          const val = evaluateDirectExpr(node.expr, scope);
          const variantName = getDirectVariantName(val, optMatch);

          let matched = false;
          for (const arm of node.arms) {
            if (
              arm.variants.includes(variantName) ||
              arm.variants.includes("_")
            ) {
              parts.push(renderDirectNodes(arm.body, scope));
              matched = true;
              break;
            }
          }
          if (!matched && node.elseArm) {
            parts.push(renderDirectNodes(node.elseArm, scope));
          }
        }
        break;
      }

      case "raw":
        parts.push(node.text);
        break;

      // Include/tmpl nodes are intentionally skipped in the direct
      // renderer — resolving them requires a template loader which is
      // not available in the unchecked fast path.  See renderDirect JSDoc.
      case "include":
      case "tmpl":
        break;
    }
  }

  return parts.join("");
}
