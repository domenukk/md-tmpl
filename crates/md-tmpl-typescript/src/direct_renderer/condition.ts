/**
 * Recursive-descent parser/evaluator for direct-renderer conditions,
 * including match/case dispatch helpers.
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import {
  ENUM_TAG_KEY,
  EXPR_END,
  EXPR_START,
  LIT_TRUE,
  OPTION_NONE,
  OPTION_SOME,
} from "../consts.js";
import { DirectScope } from "./scope.js";
import { directDisplay, directIsTruthy } from "./display.js";
import { evaluateDirectExpr } from "./expr.js";
import {
  DirectTokKind,
  type DirectToken,
  tokenizeDirectCondition,
} from "./tokenizer.js";

// ---------------------------------------------------------------------------
// Direct condition evaluation
// ---------------------------------------------------------------------------

/** Evaluate a condition expression directly using recursive descent. */
export function evaluateDirectCondition(
  cond: string,
  scope: DirectScope,
): boolean {
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
// Direct recursive descent parser/evaluator
// ---------------------------------------------------------------------------

export interface DirectParseCtx {
  tokens: DirectToken[];
  pos: number;
  scope: DirectScope;
}

export function peekDirect(ctx: DirectParseCtx): DirectToken | undefined {
  return ctx.tokens[ctx.pos];
}
export function advanceDirect(ctx: DirectParseCtx): DirectToken {
  const t = ctx.tokens[ctx.pos];
  if (!t)
    throw new TemplateSyntaxError("unexpected end of condition expression");
  ctx.pos++;
  return t;
}

export function isDirectComparisonOp(kind: DirectTokKind): boolean {
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
export function parseDirectOrExpr(ctx: DirectParseCtx): boolean {
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
export function parseDirectAndExpr(ctx: DirectParseCtx): boolean {
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

export function parseDirectUnaryExpr(ctx: DirectParseCtx): boolean {
  const t = peekDirect(ctx);
  if (t?.kind === DirectTokKind.Not) {
    advanceDirect(ctx);
    return !parseDirectUnaryExpr(ctx);
  }

  return parseDirectAtom(ctx);
}

export function parseDirectAtom(ctx: DirectParseCtx): boolean {
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

export function parseDirectMatchCondition(ctx: DirectParseCtx): boolean {
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

export function parseDirectOperand(ctx: DirectParseCtx): DirectToken {
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

export function resolveDirectOperandValue(
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
export function interpolateDirectString(
  input: string,
  scope: DirectScope,
): string {
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

export function evaluateDirectComparison(
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
    case DirectTokKind.Gt:
    case DirectTokKind.Le:
    case DirectTokKind.Ge: {
      if (typeof left !== "number" || typeof right !== "number") {
        throw new TemplateSyntaxError(
          `numeric comparison requires number operands, got ${typeof left} and ${typeof right}`,
        );
      }
      if (op === DirectTokKind.Lt) return left < right;
      if (op === DirectTokKind.Gt) return left > right;
      if (op === DirectTokKind.Le) return left <= right;
      return left >= right;
    }
    default:
      throw new TemplateSyntaxError(`unknown comparison operator '${op}'`);
  }
}

// ---------------------------------------------------------------------------
// Direct skip helpers for short-circuit
// ---------------------------------------------------------------------------

export function skipDirectUnaryExpr(ctx: DirectParseCtx): void {
  const t = peekDirect(ctx);
  if (t?.kind === DirectTokKind.Not) {
    advanceDirect(ctx);
    skipDirectUnaryExpr(ctx);
    return;
  }
  skipDirectAtom(ctx);
}

export function skipDirectAtom(ctx: DirectParseCtx): void {
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

export function skipDirectOrExpr(ctx: DirectParseCtx): void {
  skipDirectAndExpr(ctx);
  while (peekDirect(ctx)?.kind === DirectTokKind.Or) {
    advanceDirect(ctx);
    skipDirectAndExpr(ctx);
  }
}

export function skipDirectAndExpr(ctx: DirectParseCtx): void {
  skipDirectUnaryExpr(ctx);
  while (peekDirect(ctx)?.kind === DirectTokKind.And) {
    advanceDirect(ctx);
    skipDirectUnaryExpr(ctx);
  }
}

export function skipDirectOperand(ctx: DirectParseCtx): void {
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
/** Get the variant name from a JS value (enum dispatch). */
export function getDirectVariantName(
  value: unknown,
  isOption: boolean,
): string {
  // Transparent option: null/undefined is "None", anything else is "Some"
  if (value === null || value === undefined) return OPTION_NONE;
  if (isOption) return OPTION_SOME;
  if (typeof value === "string") return value;
  if (value !== null && typeof value === "object") {
    // Check __kind__ discriminant
    const obj = value as Record<string, unknown>;
    if (typeof obj[ENUM_TAG_KEY] === "string")
      return obj[ENUM_TAG_KEY] as string;
  }
  // Scalar types: convert to string for label comparison.
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  // Non-None, non-enum values in option match context → "Some"
  return OPTION_SOME;
}

/** Returns true if the match arms/guard use option-style variant names. */
export function isOptionMatch(node: {
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
