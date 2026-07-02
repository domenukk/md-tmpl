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
import type { VarDecl } from "./frontmatter.js";
import {
  TemplateSyntaxError,
  TemplatePanicError,
  TemplateError,
} from "./errors.js";
import { valueToJs } from "./value.js";
import {
  OPTION_SOME,
  OPTION_NONE,
  MATCH_DEFAULT,
  KW_RAW,
  EXPR_START,
  EXPR_END,
  NODE_TEXT,
  NODE_EXPR,
  NODE_COMMENT,
  NODE_FOR,
  NODE_IF,
  NODE_MATCH,
  NODE_PANIC,
  NODE_INCLUDE,
  NODE_TMPL,
  OP_AND,
  OP_OR,
  OP_NOT,
  OP_EQ,
  OP_NE,
  OP_LT,
  OP_GT,
  OP_LE,
  OP_GE,
  PAREN_OPEN,
  PAREN_CLOSE,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  DOT,
  VARIANT_SEP,
  LIT_TRUE,
  LIT_FALSE,
  ENUM_TAG_KEY,
  KW_MATCH,
  KW_CASE,
  KW_IN,
} from "./consts.js";

export interface DirectRenderOptions {
  inlineTemplates?: Map<
    string,
    {
      declarations: readonly VarDecl[];
      nodes: readonly Node[];
      consts: Map<string, unknown>;
    }
  >;
  templateLoader?: (
    path: string,
    basePath?: string,
  ) =>
    | [
        readonly Node[],
        ReadonlyMap<string, unknown>,
        readonly VarDecl[],
        string?,
      ]
    | undefined;
  maxIncludeDepth?: number;
  currentBasePath?: string;
}

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
  if (typeof value === "boolean") return value ? LIT_TRUE : LIT_FALSE;
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
    if (key === ENUM_TAG_KEY) return undefined;
    current = (current as Record<string, unknown>)[key];
    start = end + 1;
  }
  return current;
}

/** Resolve an expression in the direct scope. */
function resolveDirectExpr(expr: string, scope: DirectScope): unknown {
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
        if (obj._md_tmpl_tag === OPTION_NONE) return false;
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

/** Evaluate a condition expression directly using recursive descent. */
function evaluateDirectCondition(cond: string, scope: DirectScope): boolean {
  const trimmed = cond.trim();
  if (trimmed.length === 0) {
    throw new TemplateSyntaxError("empty condition expression");
  }
  const tokens = tokenizeDirectCondition(trimmed);
  const ctx: DirectParseCtx = { tokens, pos: 0, scope };
  const result = parseDirectOrExpr(ctx);
  if (ctx.pos < ctx.tokens.length) {
    const remaining = ctx.tokens[ctx.pos]!;
    throw new TemplateSyntaxError(
      `unexpected token '${remaining.value}' in condition`,
    );
  }
  return result;
}

// ---------------------------------------------------------------------------
// Direct condition tokenizer (same grammar as evaluator.ts)
// ---------------------------------------------------------------------------

const enum DirectTokKind {
  And = "&&",
  Or = "||",
  Not = "!",
  LParen = "(",
  RParen = ")",
  Eq = "==",
  Ne = "!=",
  Le = "<=",
  Ge = ">=",
  Lt = "<",
  Gt = ">",
  In = "in",
  Pipe = "|",
  Match = "match",
  Case = "case",

  Ident = "IDENT",
  StrLit = "STR",
  NumLit = "NUM",
  BoolLit = "BOOL",
}

interface DirectToken {
  kind: DirectTokKind;
  value: string;
}

function tokenizeDirectCondition(input: string): DirectToken[] {
  const tokens: DirectToken[] = [];
  let i = 0;
  const len = input.length;

  while (i < len) {
    if (input.charCodeAt(i) === 32 || input.charCodeAt(i) === 9) {
      i++;
      continue;
    }

    if (i + 1 < len) {
      const two = input.slice(i, i + 2);
      if (two === OP_AND) {
        tokens.push({ kind: DirectTokKind.And, value: two });
        i += 2;
        continue;
      }
      if (two === OP_OR) {
        tokens.push({ kind: DirectTokKind.Or, value: two });
        i += 2;
        continue;
      }
      if (two === OP_EQ) {
        tokens.push({ kind: DirectTokKind.Eq, value: two });
        i += 2;
        continue;
      }
      if (two === OP_NE) {
        tokens.push({ kind: DirectTokKind.Ne, value: two });
        i += 2;
        continue;
      }
      if (two === OP_LE) {
        tokens.push({ kind: DirectTokKind.Le, value: two });
        i += 2;
        continue;
      }
      if (two === OP_GE) {
        tokens.push({ kind: DirectTokKind.Ge, value: two });
        i += 2;
        continue;
      }
    }

    const ch = input[i]!;
    if (ch === OP_NOT) {
      tokens.push({ kind: DirectTokKind.Not, value: ch });
      i++;
      continue;
    }
    if (ch === PAREN_OPEN) {
      tokens.push({ kind: DirectTokKind.LParen, value: ch });
      i++;
      continue;
    }
    if (ch === PAREN_CLOSE) {
      tokens.push({ kind: DirectTokKind.RParen, value: ch });
      i++;
      continue;
    }
    if (ch === OP_LT) {
      tokens.push({ kind: DirectTokKind.Lt, value: ch });
      i++;
      continue;
    }
    if (ch === OP_GT) {
      tokens.push({ kind: DirectTokKind.Gt, value: ch });
      i++;
      continue;
    }
    if (ch === VARIANT_SEP) {
      tokens.push({ kind: DirectTokKind.Pipe, value: ch });
      i++;
      continue;
    }

    if (ch === QUOTE_DOUBLE || ch === QUOTE_SINGLE) {
      const quote = ch;
      let j = i + 1;
      while (j < len && input[j] !== quote) j++;
      tokens.push({ kind: DirectTokKind.StrLit, value: input.slice(i, j + 1) });
      i = j + 1;
      continue;
    }

    if (
      (ch >= "0" && ch <= "9") ||
      (ch === "-" &&
        i + 1 < len &&
        input[i + 1]! >= "0" &&
        input[i + 1]! <= "9" &&
        (tokens.length === 0 ||
          isDirectOperatorToken(tokens[tokens.length - 1]!)))
    ) {
      let j = i;
      if (ch === "-") j++;
      while (
        j < len &&
        ((input[j]! >= "0" && input[j]! <= "9") || input[j] === DOT)
      )
        j++;
      tokens.push({ kind: DirectTokKind.NumLit, value: input.slice(i, j) });
      i = j;
      continue;
    }

    if (isDirectIdentStart(ch)) {
      let j = i;
      while (j < len && isDirectIdentChar(input[j]!)) j++;
      const word = input.slice(i, j);

      if (word === LIT_TRUE || word === LIT_FALSE) {
        tokens.push({ kind: DirectTokKind.BoolLit, value: word });
        i = j;
        continue;
      }
      if (word === KW_IN) {
        tokens.push({ kind: DirectTokKind.In, value: word });
        i = j;
        continue;
      }
      if (word === KW_MATCH) {
        tokens.push({ kind: DirectTokKind.Match, value: word });
        i = j;
        continue;
      }
      if (word === KW_CASE) {
        tokens.push({ kind: DirectTokKind.Case, value: word });
        i = j;
        continue;
      }

      while (j < len && input[j] === DOT) {
        j++;
        while (j < len && isDirectIdentChar(input[j]!)) j++;
      }
      if (j < len && input[j] === PAREN_OPEN) {
        let depth = 1;
        j++;
        while (j < len && depth > 0) {
          if (input[j] === PAREN_OPEN) depth++;
          else if (input[j] === PAREN_CLOSE) depth--;
          j++;
        }
      }
      tokens.push({ kind: DirectTokKind.Ident, value: input.slice(i, j) });
      i = j;
      continue;
    }

    throw new TemplateSyntaxError(`unexpected character '${ch}' in condition`);
  }

  return tokens;
}

function isDirectIdentStart(ch: string): boolean {
  return (ch >= "a" && ch <= "z") || (ch >= "A" && ch <= "Z") || ch === "_";
}
function isDirectIdentChar(ch: string): boolean {
  return isDirectIdentStart(ch) || (ch >= "0" && ch <= "9");
}
function isDirectOperatorToken(t: DirectToken): boolean {
  return (
    t.kind !== DirectTokKind.Ident &&
    t.kind !== DirectTokKind.StrLit &&
    t.kind !== DirectTokKind.NumLit &&
    t.kind !== DirectTokKind.BoolLit &&
    t.kind !== DirectTokKind.RParen
  );
}

// ---------------------------------------------------------------------------
// Direct recursive descent parser/evaluator
// ---------------------------------------------------------------------------

interface DirectParseCtx {
  tokens: DirectToken[];
  pos: number;
  scope: DirectScope;
}

function peekDirect(ctx: DirectParseCtx): DirectToken | undefined {
  return ctx.tokens[ctx.pos];
}
function advanceDirect(ctx: DirectParseCtx): DirectToken {
  const t = ctx.tokens[ctx.pos];
  if (!t)
    throw new TemplateSyntaxError("unexpected end of condition expression");
  ctx.pos++;
  return t;
}

function isDirectComparisonOp(kind: DirectTokKind): boolean {
  return (
    kind === DirectTokKind.Eq ||
    kind === DirectTokKind.Ne ||
    kind === DirectTokKind.Le ||
    kind === DirectTokKind.Ge ||
    kind === DirectTokKind.Lt ||
    kind === DirectTokKind.Gt ||
    kind === DirectTokKind.In
  );
}

/** OrExpr := AndExpr ( "||" AndExpr )* */
function parseDirectOrExpr(ctx: DirectParseCtx): boolean {
  let result = parseDirectAndExpr(ctx);
  while (peekDirect(ctx)?.kind === DirectTokKind.Or) {
    advanceDirect(ctx);
    if (!result) {
      result = parseDirectAndExpr(ctx);
    } else {
      skipDirectAndExpr(ctx);
    }
  }
  return result;
}

/** AndExpr := UnaryExpr ( "&&" UnaryExpr )* */
function parseDirectAndExpr(ctx: DirectParseCtx): boolean {
  let result = parseDirectUnaryExpr(ctx);
  while (peekDirect(ctx)?.kind === DirectTokKind.And) {
    advanceDirect(ctx);
    if (result) {
      result = parseDirectUnaryExpr(ctx);
    } else {
      skipDirectUnaryExpr(ctx);
    }
  }
  return result;
}

function parseDirectUnaryExpr(ctx: DirectParseCtx): boolean {
  const t = peekDirect(ctx);
  if (t?.kind === DirectTokKind.Not) {
    advanceDirect(ctx);
    return !parseDirectUnaryExpr(ctx);
  }

  return parseDirectAtom(ctx);
}

function parseDirectAtom(ctx: DirectParseCtx): boolean {
  const t = peekDirect(ctx);
  if (!t)
    throw new TemplateSyntaxError("unexpected end of condition expression");

  if (t.kind === DirectTokKind.LParen) {
    advanceDirect(ctx);
    if (peekDirect(ctx)?.kind === DirectTokKind.RParen) {
      throw new TemplateSyntaxError(
        "empty parentheses '()' in condition expression",
      );
    }
    const result = parseDirectOrExpr(ctx);
    if (peekDirect(ctx)?.kind !== DirectTokKind.RParen) {
      throw new TemplateSyntaxError(
        "unclosed parenthesis ')' expected in condition",
      );
    }
    advanceDirect(ctx);
    return result;
  }

  if (t.kind === DirectTokKind.Match) {
    return parseDirectMatchCondition(ctx);
  }

  const operand = parseDirectOperand(ctx);
  const next = peekDirect(ctx);
  if (next && isDirectComparisonOp(next.kind)) {
    const op = advanceDirect(ctx);
    const rightOperand = parseDirectOperand(ctx);
    const left = resolveDirectOperandValue(operand, ctx.scope);
    const right = resolveDirectOperandValue(rightOperand, ctx.scope);
    return evaluateDirectComparison(left, right, op.kind);
  }

  return directIsTruthy(resolveDirectOperandValue(operand, ctx.scope));
}

function parseDirectMatchCondition(ctx: DirectParseCtx): boolean {
  advanceDirect(ctx); // match
  const targetOperand = parseDirectOperand(ctx);
  const targetVal = resolveDirectOperandValue(targetOperand, ctx.scope);

  if (peekDirect(ctx)?.kind !== DirectTokKind.Case) {
    throw new TemplateSyntaxError(
      "expected 'case' keyword after 'match' expression",
    );
  }
  advanceDirect(ctx); // case

  const variants: string[] = [];
  const firstVar = peekDirect(ctx);
  if (!firstVar || firstVar.kind !== DirectTokKind.Ident) {
    throw new TemplateSyntaxError("expected variant name after 'case'");
  }
  variants.push(advanceDirect(ctx).value);
  while (peekDirect(ctx)?.kind === DirectTokKind.Pipe) {
    advanceDirect(ctx);
    const nextVar = peekDirect(ctx);
    if (!nextVar || nextVar.kind !== DirectTokKind.Ident) {
      throw new TemplateSyntaxError("expected variant name after '|'");
    }
    variants.push(advanceDirect(ctx).value);
  }

  const isOpt = variants.some((v) => v === OPTION_SOME || v === OPTION_NONE);
  const variantName = getDirectVariantName(targetVal, isOpt);
  return variants.includes(variantName);
}

function parseDirectOperand(ctx: DirectParseCtx): DirectToken {
  const t = peekDirect(ctx);
  if (!t)
    throw new TemplateSyntaxError("unexpected end of condition expression");
  if (
    t.kind === DirectTokKind.Ident ||
    t.kind === DirectTokKind.StrLit ||
    t.kind === DirectTokKind.NumLit ||
    t.kind === DirectTokKind.BoolLit
  ) {
    return advanceDirect(ctx);
  }
  throw new TemplateSyntaxError(
    `expected expression, got '${t.value}' in condition`,
  );
}

function resolveDirectOperandValue(
  operand: DirectToken,
  scope: DirectScope,
): unknown {
  switch (operand.kind) {
    case DirectTokKind.StrLit: {
      const inner = operand.value.slice(1, -1);
      if (inner.includes(EXPR_START)) {
        return interpolateDirectString(inner, scope);
      }
      return inner;
    }
    case DirectTokKind.BoolLit:
      return operand.value === LIT_TRUE;
    case DirectTokKind.NumLit:
      return Number(operand.value);
    case DirectTokKind.Ident:
      return evaluateDirectExpr(operand.value, scope);
    default:
      return undefined;
  }
}

/**
 * Interpolate `{{ expr }}` references inside a string literal for the direct renderer.
 */
function interpolateDirectString(input: string, scope: DirectScope): string {
  let result = "";
  let remaining = input;
  while (true) {
    const startIdx = remaining.indexOf(EXPR_START);
    if (startIdx === -1) break;
    result += remaining.slice(0, startIdx);
    const afterStart = remaining.slice(startIdx + EXPR_START.length);
    const endIdx = afterStart.indexOf(EXPR_END);
    if (endIdx === -1) {
      throw new TemplateSyntaxError(
        `unclosed '${EXPR_START}' in interpolated string`,
      );
    }
    const expr = afterStart.slice(0, endIdx).trim();
    if (expr === "") {
      throw new TemplateSyntaxError(
        `empty expression '${EXPR_START}${EXPR_END}' in interpolated string`,
      );
    }
    const val = evaluateDirectExpr(expr, scope);
    result += directDisplay(val);
    remaining = afterStart.slice(endIdx + EXPR_END.length);
  }
  result += remaining;
  return result;
}

function evaluateDirectComparison(
  left: unknown,
  right: unknown,
  op: DirectTokKind,
): boolean {
  if (op === DirectTokKind.In) {
    if (Array.isArray(right)) {
      if (Array.isArray(left)) {
        return (left as unknown[]).every((sub) =>
          (right as unknown[]).some((item) => item === sub),
        );
      }
      return (right as unknown[]).some((item) => item === left);
    }
    if (typeof right === "string") {
      if (typeof left !== "string") {
        throw new TemplateSyntaxError(
          `left operand of 'in' must be string when right is string`,
        );
      }
      return right.includes(left);
    }
    throw new TemplateSyntaxError(
      `right operand of 'in' must be list or string`,
    );
  }

  switch (op) {
    case DirectTokKind.Eq:
      return left === right;
    case DirectTokKind.Ne:
      return left !== right;
    case DirectTokKind.Lt:
      return (left as number) < (right as number);
    case DirectTokKind.Gt:
      return (left as number) > (right as number);
    case DirectTokKind.Le:
      return (left as number) <= (right as number);
    case DirectTokKind.Ge:
      return (left as number) >= (right as number);
    default:
      return false;
  }
}

// ---------------------------------------------------------------------------
// Direct skip helpers for short-circuit
// ---------------------------------------------------------------------------

function skipDirectUnaryExpr(ctx: DirectParseCtx): void {
  const t = peekDirect(ctx);
  if (t?.kind === DirectTokKind.Not) {
    advanceDirect(ctx);
    skipDirectUnaryExpr(ctx);
    return;
  }
  skipDirectAtom(ctx);
}

function skipDirectAtom(ctx: DirectParseCtx): void {
  const t = peekDirect(ctx);
  if (!t) return;
  if (t.kind === DirectTokKind.LParen) {
    advanceDirect(ctx);
    skipDirectOrExpr(ctx);
    if (peekDirect(ctx)?.kind === DirectTokKind.RParen) advanceDirect(ctx);
    return;
  }
  if (t.kind === DirectTokKind.Match) {
    advanceDirect(ctx);
    skipDirectOperand(ctx);
    if (peekDirect(ctx)?.kind === DirectTokKind.Case) {
      advanceDirect(ctx);
      skipDirectOperand(ctx);
      while (peekDirect(ctx)?.kind === DirectTokKind.Pipe) {
        advanceDirect(ctx);
        skipDirectOperand(ctx);
      }
    }
    return;
  }
  skipDirectOperand(ctx);
  const next = peekDirect(ctx);
  if (next && isDirectComparisonOp(next.kind)) {
    advanceDirect(ctx);
    skipDirectOperand(ctx);
  }
}

function skipDirectOrExpr(ctx: DirectParseCtx): void {
  skipDirectAndExpr(ctx);
  while (peekDirect(ctx)?.kind === DirectTokKind.Or) {
    advanceDirect(ctx);
    skipDirectAndExpr(ctx);
  }
}

function skipDirectAndExpr(ctx: DirectParseCtx): void {
  skipDirectUnaryExpr(ctx);
  while (peekDirect(ctx)?.kind === DirectTokKind.And) {
    advanceDirect(ctx);
    skipDirectUnaryExpr(ctx);
  }
}

function skipDirectOperand(ctx: DirectParseCtx): void {
  const t = peekDirect(ctx);
  if (
    t &&
    (t.kind === DirectTokKind.Ident ||
      t.kind === DirectTokKind.StrLit ||
      t.kind === DirectTokKind.NumLit ||
      t.kind === DirectTokKind.BoolLit)
  ) {
    advanceDirect(ctx);
  }
}

// ---------------------------------------------------------------------------
// Direct render — the main entry point
// ---------------------------------------------------------------------------

/** Get the variant name from a JS value (enum dispatch). */
function getDirectVariantName(value: unknown, isOption: boolean): string {
  // Transparent option: null/undefined is "None", anything else is "Some"
  if (value === null || value === undefined) return OPTION_NONE;
  if (isOption) return OPTION_SOME;
  if (typeof value === "string") return value;
  if (value !== null && typeof value === "object") {
    // Check __kind__ protocol
    const obj = value as Record<string, unknown>;
    if (typeof obj[ENUM_TAG_KEY] === "string")
      return obj[ENUM_TAG_KEY] as string;
    // Check _md_tmpl_tag protocol
    if (typeof obj._md_tmpl_tag === "string") {
      return obj._md_tmpl_tag;
    }
  }
  // Non-None, non-enum values in option match context → "Some"
  return OPTION_SOME;
}

/** Returns true if the match arms/guard use option-style variant names. */
function isOptionMatch(node: {
  arms: { variants: string[] }[];
  inlineGuard?: { variant: string };
}): boolean {
  if (node.inlineGuard) {
    return (
      node.inlineGuard.variant === OPTION_SOME ||
      node.inlineGuard.variant === OPTION_NONE
    );
  }
  return node.arms.some((arm) =>
    arm.variants.some((v) => v === OPTION_SOME || v === OPTION_NONE),
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
  options?: DirectRenderOptions,
): string {
  const scope = new DirectScope(params, constJsValues);
  return renderDirectNodes(nodes, scope, options);
}

/** Render nodes with a direct scope. */
function renderDirectNodes(
  nodes: readonly Node[],
  scope: DirectScope,
  options?: DirectRenderOptions,
): string {
  const parts: string[] = [];

  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i]!;
    switch (node.kind) {
      case NODE_TEXT:
        parts.push(node.text);
        break;

      case NODE_EXPR: {
        if (node.trimBefore && parts.length > 0) {
          const last = parts[parts.length - 1]!;
          parts[parts.length - 1] = last.replace(/\s+$/, "");
        }
        const val = evaluateDirectExpr(node.expr, scope);
        parts.push(directDisplay(val));
        if (node.trimAfter) {
          // Trim leading whitespace from the next text node without
          // mutating the AST (which would corrupt subsequent renders).
          if (i + 1 < nodes.length && nodes[i + 1]!.kind === NODE_TEXT) {
            const next = nodes[i + 1]! as { kind: "text"; text: string };
            parts.push(next.text.replace(/^\s+/, ""));
            i++; // skip the next node — we already handled it
          }
        }
        break;
      }

      case NODE_COMMENT:
        break;

      case NODE_FOR: {
        const listVal = evaluateDirectExpr(node.iterExpr, scope);
        if (!Array.isArray(listVal)) {
          throw new TemplateSyntaxError(
            `for loop requires a list, got ${typeof listVal}`,
          );
        }
        if (listVal.length === 0 && node.elseBody) {
          parts.push(renderDirectNodes(node.elseBody, scope, options));
        } else {
          for (let idx = 0; idx < listVal.length; idx++) {
            const item = listVal[idx];
            const layer = scope.pushLayer();
            layer.set(node.binding, item);
            scope.setLoopIndex(node.binding, idx);
            parts.push(renderDirectNodes(node.body, scope, options));
            scope.popLayer();
          }
        }
        break;
      }

      case NODE_IF: {
        let matched = false;
        for (const branch of node.branches) {
          if (evaluateDirectCondition(branch.condition, scope)) {
            parts.push(renderDirectNodes(branch.body, scope, options));
            matched = true;
            break;
          }
        }
        if (!matched && node.elseBody) {
          parts.push(renderDirectNodes(node.elseBody, scope, options));
        }
        break;
      }

      case NODE_MATCH: {
        const optMatch = isOptionMatch(node);
        if (node.inlineGuard) {
          const val = evaluateDirectExpr(node.expr, scope);
          const variantName = getDirectVariantName(val, optMatch);
          if (variantName === node.inlineGuard.variant) {
            parts.push(
              renderDirectNodes(node.inlineGuard.body, scope, options),
            );
          }
        } else {
          const val = evaluateDirectExpr(node.expr, scope);
          const variantName = getDirectVariantName(val, optMatch);

          let matched = false;
          for (const arm of node.arms) {
            if (
              arm.variants.includes(variantName) ||
              arm.variants.includes(MATCH_DEFAULT)
            ) {
              // If the arm has a guard, evaluate it
              if (arm.guard && !evaluateDirectCondition(arm.guard, scope)) {
                continue;
              }
              parts.push(renderDirectNodes(arm.body, scope, options));
              matched = true;
              break;
            }
          }
          if (!matched && node.elseArm) {
            parts.push(renderDirectNodes(node.elseArm, scope, options));
          }
        }
        break;
      }

      case KW_RAW:
        parts.push(node.text);
        break;

      case NODE_PANIC: {
        const msg = renderDirectNodes(node.body, scope, options);
        throw new TemplatePanicError(msg);
      }

      case NODE_INCLUDE: {
        const maxDepth = options?.maxIncludeDepth ?? 16;
        if (maxDepth <= 0) {
          throw new TemplateError(
            `maximum include depth exceeded when including '${node.path ?? node.name}'`,
          );
        }
        let includedNodes: readonly Node[];
        let loadedConsts = new Map<string, unknown>();
        let decls: readonly VarDecl[];
        let childOpts: DirectRenderOptions | undefined = options;

        if (node.path !== undefined) {
          if (!options?.templateLoader) {
            throw new TemplateError(
              `cannot resolve '{% include "${node.path}" %}': file includes require a base directory (compile with fromFile or baseDir option)`,
            );
          }
          const loaded = options.templateLoader(
            node.path,
            options.currentBasePath,
          );
          if (!loaded) {
            throw new TemplateError(
              `cannot resolve '{% include "${node.path}" %}': file not found or load failed`,
            );
          }
          const [lNodes, lConsts, lDecls, lBase] = loaded;
          includedNodes = lNodes;
          decls = lDecls;
          for (const [k, v] of lConsts) {
            loadedConsts.set(k, v);
          }
          childOpts = {
            ...options,
            currentBasePath: lBase ?? options.currentBasePath,
            maxIncludeDepth: maxDepth - 1,
          };
        } else {
          const inline = options?.inlineTemplates?.get(node.name);
          if (!inline) {
            throw new TemplateError(
              `undefined inline template '${node.name}' (available: ${Array.from(options?.inlineTemplates?.keys() ?? []).join(", ")})`,
            );
          }
          includedNodes = inline.nodes;
          decls = inline.declarations;
          for (const [k, v] of inline.consts) {
            loadedConsts.set(k, v);
          }
          childOpts = {
            ...options,
            maxIncludeDepth: maxDepth - 1,
          };
        }

        if (node.forBinding && node.forExpr) {
          const listVal = evaluateDirectExpr(node.forExpr, scope);
          if (!Array.isArray(listVal)) {
            throw new TemplateSyntaxError(
              `include ... for ... in requires list, got ${typeof listVal}`,
            );
          }
          const results: string[] = [];
          for (const item of listVal) {
            const iterMap = new Map<string, unknown>();
            iterMap.set(node.forBinding, item);
            for (const [targetKey, sourceExpr] of node.withMappings) {
              iterMap.set(targetKey, evaluateDirectExpr(sourceExpr, scope));
            }
            for (const decl of decls) {
              if (!iterMap.has(decl.name) && decl.defaultValue !== undefined) {
                iterMap.set(decl.name, valueToJs(decl.defaultValue));
              }
            }
            results.push(
              renderDirect(includedNodes, iterMap, loadedConsts, childOpts),
            );
          }
          parts.push(results.join(""));
          break;
        }

        const childMap = new Map<string, unknown>();
        for (const [targetKey, sourceExpr] of node.withMappings) {
          childMap.set(targetKey, evaluateDirectExpr(sourceExpr, scope));
        }
        for (const decl of decls) {
          if (!childMap.has(decl.name) && decl.defaultValue !== undefined) {
            childMap.set(decl.name, valueToJs(decl.defaultValue));
          }
        }
        parts.push(
          renderDirect(includedNodes, childMap, loadedConsts, childOpts),
        );
        break;
      }

      case NODE_TMPL:
        break;
    }
  }

  return parts.join("");
}
