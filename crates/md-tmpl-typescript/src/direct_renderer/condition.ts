/**
 * Recursive-descent parser/evaluator for direct-renderer conditions,
 * compiled once per AST branch at template construction time.
 *
 * @module
 */

import { TemplateSyntaxError, TypeMismatchError } from "../errors.js";
import {
  ENUM_TAG_KEY,
  EXPR_END,
  EXPR_START,
  LIT_TRUE,
  OPTION_NONE,
  OPTION_SOME,
  TYPE_LIST,
  TYPE_STR,
  unescapeStringLiteral,
} from "../consts.js";
import { DirectScope } from "./scope.js";
import { directDisplay, directIsTruthy } from "./display.js";
import {
  type CompiledDirectExpr,
  compileDirectExpr,
  evalCompiledDirectExpr,
  evaluateDirectExpr,
} from "./expr.js";
import {
  DirectTokKind,
  type DirectToken,
  tokenizeDirectCondition,
} from "./tokenizer.js";

type CompiledDirectOperand =
  | { readonly kind: "lit"; readonly value: unknown }
  | { readonly kind: "interp_str"; readonly inner: string }
  | { readonly kind: "expr"; readonly expr: CompiledDirectExpr };

export type CompiledDirectCondition =
  | {
      readonly kind: "truthy_var";
      readonly name: string;
    }
  | {
      readonly kind: "truthy_prop1";
      readonly root: string;
      readonly prop: string;
      readonly fullPath: string;
    }
  | {
      readonly kind: "eq_var_lit";
      readonly name: string;
      readonly right: unknown;
    }
  | {
      readonly kind: "ne_var_lit";
      readonly name: string;
      readonly right: unknown;
    }
  | {
      readonly kind: "eq_prop1_lit";
      readonly root: string;
      readonly prop: string;
      readonly fullPath: string;
      readonly right: unknown;
    }
  | {
      readonly kind: "ne_prop1_lit";
      readonly root: string;
      readonly prop: string;
      readonly fullPath: string;
      readonly right: unknown;
    }
  | {
      readonly kind: "gt_prop1_num";
      readonly root: string;
      readonly prop: string;
      readonly fullPath: string;
      readonly right: number;
    }
  | {
      readonly kind: "lt_prop1_num";
      readonly root: string;
      readonly prop: string;
      readonly fullPath: string;
      readonly right: number;
    }
  | {
      readonly kind: "ge_prop1_num";
      readonly root: string;
      readonly prop: string;
      readonly fullPath: string;
      readonly right: number;
    }
  | {
      readonly kind: "le_prop1_num";
      readonly root: string;
      readonly prop: string;
      readonly fullPath: string;
      readonly right: number;
    }
  | {
      readonly kind: "gt_var_num";
      readonly name: string;
      readonly right: number;
    }
  | {
      readonly kind: "lt_var_num";
      readonly name: string;
      readonly right: number;
    }
  | {
      readonly kind: "ge_var_num";
      readonly name: string;
      readonly right: number;
    }
  | {
      readonly kind: "le_var_num";
      readonly name: string;
      readonly right: number;
    }
  | {
      readonly kind: "eq";
      readonly left: CompiledDirectOperand;
      readonly right: CompiledDirectOperand;
    }
  | {
      readonly kind: "ne";
      readonly left: CompiledDirectOperand;
      readonly right: CompiledDirectOperand;
    }
  | {
      readonly kind: "gt";
      readonly left: CompiledDirectOperand;
      readonly right: CompiledDirectOperand;
    }
  | {
      readonly kind: "lt";
      readonly left: CompiledDirectOperand;
      readonly right: CompiledDirectOperand;
    }
  | {
      readonly kind: "ge";
      readonly left: CompiledDirectOperand;
      readonly right: CompiledDirectOperand;
    }
  | {
      readonly kind: "le";
      readonly left: CompiledDirectOperand;
      readonly right: CompiledDirectOperand;
    }
  | {
      readonly kind: "cmp";
      readonly op: DirectTokKind;
      readonly left: CompiledDirectOperand;
      readonly right: CompiledDirectOperand;
    }
  | { readonly kind: "truthy"; readonly operand: CompiledDirectOperand }
  | { readonly kind: "not_truthy"; readonly operand: CompiledDirectOperand }
  | { readonly kind: "tokens"; readonly tokens: DirectToken[] };

function isOperandToken(t: DirectToken | undefined): t is DirectToken {
  return (
    t !== undefined &&
    (t.kind === DirectTokKind.Ident ||
      t.kind === DirectTokKind.StrLit ||
      t.kind === DirectTokKind.NumLit ||
      t.kind === DirectTokKind.BoolLit)
  );
}

function compileDirectOperand(operand: DirectToken): CompiledDirectOperand {
  switch (operand.kind) {
    case DirectTokKind.StrLit: {
      const inner = unescapeStringLiteral(operand.value.slice(1, -1));
      if (inner.includes(EXPR_START)) {
        return { kind: "interp_str", inner };
      }
      return { kind: "lit", value: inner };
    }
    case DirectTokKind.BoolLit:
      return { kind: "lit", value: operand.value === LIT_TRUE };
    case DirectTokKind.NumLit:
      return { kind: "lit", value: Number(operand.value) };
    case DirectTokKind.Ident:
      return { kind: "expr", expr: compileDirectExpr(operand.value) };
    default:
      return { kind: "lit", value: undefined };
  }
}

function evalCompiledOperand(
  operand: CompiledDirectOperand,
  scope: DirectScope,
): unknown {
  switch (operand.kind) {
    case "lit":
      return operand.value;
    case "interp_str":
      return interpolateDirectString(operand.inner, scope);
    case "expr":
      return evalCompiledDirectExpr(operand.expr, scope);
  }
}

/** Compile a condition expression string into an execution plan. */
export function compileDirectCondition(cond: string): CompiledDirectCondition {
  const trimmed = cond.trim();
  if (trimmed.length === 0) {
    throw new TemplateSyntaxError("empty condition expression");
  }
  const tokens = tokenizeDirectCondition(trimmed);

  if (tokens.length === 1 && isOperandToken(tokens[0])) {
    const operand = compileDirectOperand(tokens[0]);
    if (operand.kind === "expr") {
      if (operand.expr.kind === "prop1") {
        return {
          kind: "truthy_prop1",
          root: operand.expr.root,
          prop: operand.expr.prop,
          fullPath: operand.expr.fullPath,
        };
      }
      if (operand.expr.kind === "var") {
        return {
          kind: "truthy_var",
          name: operand.expr.name,
        };
      }
    }
    return {
      kind: "truthy",
      operand,
    };
  }
  if (
    tokens.length === 2 &&
    tokens[0]?.kind === DirectTokKind.Not &&
    isOperandToken(tokens[1])
  ) {
    return {
      kind: "not_truthy",
      operand: compileDirectOperand(tokens[1]),
    };
  }
  if (
    tokens.length === 3 &&
    isOperandToken(tokens[0]) &&
    tokens[1] !== undefined &&
    isDirectComparisonOp(tokens[1].kind) &&
    isOperandToken(tokens[2])
  ) {
    const left = compileDirectOperand(tokens[0]);
    const right = compileDirectOperand(tokens[2]);
    const op = tokens[1].kind;
    if (left.kind === "expr" && right.kind === "lit") {
      if (left.expr.kind === "prop1") {
        const { root, prop, fullPath } = left.expr;
        if (op === DirectTokKind.Eq) {
          return {
            kind: "eq_prop1_lit",
            root,
            prop,
            fullPath,
            right: right.value,
          };
        }
        if (op === DirectTokKind.Ne) {
          return {
            kind: "ne_prop1_lit",
            root,
            prop,
            fullPath,
            right: right.value,
          };
        }
        if (typeof right.value === "number") {
          if (op === DirectTokKind.Gt) {
            return {
              kind: "gt_prop1_num",
              root,
              prop,
              fullPath,
              right: right.value,
            };
          }
          if (op === DirectTokKind.Lt) {
            return {
              kind: "lt_prop1_num",
              root,
              prop,
              fullPath,
              right: right.value,
            };
          }
          if (op === DirectTokKind.Ge) {
            return {
              kind: "ge_prop1_num",
              root,
              prop,
              fullPath,
              right: right.value,
            };
          }
          if (op === DirectTokKind.Le) {
            return {
              kind: "le_prop1_num",
              root,
              prop,
              fullPath,
              right: right.value,
            };
          }
        }
      } else if (left.expr.kind === "var") {
        const { name } = left.expr;
        if (op === DirectTokKind.Eq) {
          return { kind: "eq_var_lit", name, right: right.value };
        }
        if (op === DirectTokKind.Ne) {
          return { kind: "ne_var_lit", name, right: right.value };
        }
        if (typeof right.value === "number") {
          if (op === DirectTokKind.Gt) {
            return { kind: "gt_var_num", name, right: right.value };
          }
          if (op === DirectTokKind.Lt) {
            return { kind: "lt_var_num", name, right: right.value };
          }
          if (op === DirectTokKind.Ge) {
            return { kind: "ge_var_num", name, right: right.value };
          }
          if (op === DirectTokKind.Le) {
            return { kind: "le_var_num", name, right: right.value };
          }
        }
      }
    }
    switch (op) {
      case DirectTokKind.Eq:
        return { kind: "eq", left, right };
      case DirectTokKind.Ne:
        return { kind: "ne", left, right };
      case DirectTokKind.Gt:
        return { kind: "gt", left, right };
      case DirectTokKind.Lt:
        return { kind: "lt", left, right };
      case DirectTokKind.Ge:
        return { kind: "ge", left, right };
      case DirectTokKind.Le:
        return { kind: "le", left, right };
      default:
        return { kind: "cmp", op, left, right };
    }
  }
  return { kind: "tokens", tokens };
}

// ---------------------------------------------------------------------------
// Direct condition evaluation
// ---------------------------------------------------------------------------

/** Evaluate a pre-compiled condition plan directly against `scope`. */
export function evalCompiledDirectCondition(
  compiled: CompiledDirectCondition,
  scope: DirectScope,
): boolean {
  switch (compiled.kind) {
    case "truthy_prop1": {
      const v = scope.resolveProp1(
        compiled.root,
        compiled.prop,
        compiled.fullPath,
      );
      return typeof v === "boolean" ? v : directIsTruthy(v);
    }
    case "truthy_var": {
      const v = scope.resolve(compiled.name);
      return typeof v === "boolean" ? v : directIsTruthy(v);
    }
    case "eq_var_lit":
      return scope.resolve(compiled.name) === compiled.right;
    case "ne_var_lit":
      return scope.resolve(compiled.name) !== compiled.right;
    case "eq_prop1_lit":
      return (
        scope.resolveProp1(compiled.root, compiled.prop, compiled.fullPath) ===
        compiled.right
      );
    case "ne_prop1_lit":
      return (
        scope.resolveProp1(compiled.root, compiled.prop, compiled.fullPath) !==
        compiled.right
      );
    case "gt_prop1_num": {
      const l = scope.resolveProp1(
        compiled.root,
        compiled.prop,
        compiled.fullPath,
      );
      if (typeof l === "number") return l > compiled.right;
      return evaluateDirectComparison(l, compiled.right, DirectTokKind.Gt);
    }
    case "lt_prop1_num": {
      const l = scope.resolveProp1(
        compiled.root,
        compiled.prop,
        compiled.fullPath,
      );
      if (typeof l === "number") return l < compiled.right;
      return evaluateDirectComparison(l, compiled.right, DirectTokKind.Lt);
    }
    case "ge_prop1_num": {
      const l = scope.resolveProp1(
        compiled.root,
        compiled.prop,
        compiled.fullPath,
      );
      if (typeof l === "number") return l >= compiled.right;
      return evaluateDirectComparison(l, compiled.right, DirectTokKind.Ge);
    }
    case "le_prop1_num": {
      const l = scope.resolveProp1(
        compiled.root,
        compiled.prop,
        compiled.fullPath,
      );
      if (typeof l === "number") return l <= compiled.right;
      return evaluateDirectComparison(l, compiled.right, DirectTokKind.Le);
    }
    case "gt_var_num": {
      const l = scope.resolve(compiled.name);
      if (typeof l === "number") return l > compiled.right;
      return evaluateDirectComparison(l, compiled.right, DirectTokKind.Gt);
    }
    case "lt_var_num": {
      const l = scope.resolve(compiled.name);
      if (typeof l === "number") return l < compiled.right;
      return evaluateDirectComparison(l, compiled.right, DirectTokKind.Lt);
    }
    case "ge_var_num": {
      const l = scope.resolve(compiled.name);
      if (typeof l === "number") return l >= compiled.right;
      return evaluateDirectComparison(l, compiled.right, DirectTokKind.Ge);
    }
    case "le_var_num": {
      const l = scope.resolve(compiled.name);
      if (typeof l === "number") return l <= compiled.right;
      return evaluateDirectComparison(l, compiled.right, DirectTokKind.Le);
    }
    case "eq":
      return (
        evalCompiledOperand(compiled.left, scope) ===
        evalCompiledOperand(compiled.right, scope)
      );
    case "ne":
      return (
        evalCompiledOperand(compiled.left, scope) !==
        evalCompiledOperand(compiled.right, scope)
      );
    case "gt": {
      const l = evalCompiledOperand(compiled.left, scope);
      const r = evalCompiledOperand(compiled.right, scope);
      if (typeof l === "number" && typeof r === "number") return l > r;
      return evaluateDirectComparison(l, r, DirectTokKind.Gt);
    }
    case "lt": {
      const l = evalCompiledOperand(compiled.left, scope);
      const r = evalCompiledOperand(compiled.right, scope);
      if (typeof l === "number" && typeof r === "number") return l < r;
      return evaluateDirectComparison(l, r, DirectTokKind.Lt);
    }
    case "ge": {
      const l = evalCompiledOperand(compiled.left, scope);
      const r = evalCompiledOperand(compiled.right, scope);
      if (typeof l === "number" && typeof r === "number") return l >= r;
      return evaluateDirectComparison(l, r, DirectTokKind.Ge);
    }
    case "le": {
      const l = evalCompiledOperand(compiled.left, scope);
      const r = evalCompiledOperand(compiled.right, scope);
      if (typeof l === "number" && typeof r === "number") return l <= r;
      return evaluateDirectComparison(l, r, DirectTokKind.Le);
    }
    case "cmp":
      return evaluateDirectComparison(
        evalCompiledOperand(compiled.left, scope),
        evalCompiledOperand(compiled.right, scope),
        compiled.op,
      );
    case "truthy":
      return directIsTruthy(evalCompiledOperand(compiled.operand, scope));
    case "not_truthy":
      return !directIsTruthy(evalCompiledOperand(compiled.operand, scope));
    case "tokens": {
      const ctx: DirectParseCtx = { tokens: compiled.tokens, pos: 0, scope };
      const result = parseDirectOrExpr(ctx);
      const remaining = ctx.tokens[ctx.pos];
      if (remaining !== undefined) {
        throw new TemplateSyntaxError(
          `unexpected token '${remaining.value}' in condition`,
        );
      }
      return result;
    }
  }
}

/** Evaluate a condition expression directly. */
export function evaluateDirectCondition(
  cond: string,
  scope: DirectScope,
): boolean {
  return evalCompiledDirectCondition(compileDirectCondition(cond), scope);
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
  if (firstVar?.kind !== DirectTokKind.Ident) {
    throw new TemplateSyntaxError("expected variant name after 'case'");
  }
  variants.push(advanceDirect(ctx).value);
  while (peekDirect(ctx)?.kind === DirectTokKind.Pipe) {
    advanceDirect(ctx);
    const nextVar = peekDirect(ctx);
    if (nextVar?.kind !== DirectTokKind.Ident) {
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
      const inner = unescapeStringLiteral(operand.value.slice(1, -1));
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
  let startIdx = remaining.indexOf(EXPR_START);
  while (startIdx !== -1) {
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
    startIdx = remaining.indexOf(EXPR_START);
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
        throw new TypeMismatchError(
          "left operand of 'in'",
          TYPE_STR,
          typeof left,
        );
      }
      return right.includes(left);
    }
    throw new TypeMismatchError(
      "right operand of 'in'",
      `${TYPE_LIST} or ${TYPE_STR}`,
      typeof right,
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
      if (typeof left === "number" && typeof right === "number") {
        if (op === DirectTokKind.Lt) return left < right;
        if (op === DirectTokKind.Gt) return left > right;
        if (op === DirectTokKind.Le) return left <= right;
        return left >= right;
      }
      if (typeof left === "string" && typeof right === "string") {
        if (op === DirectTokKind.Lt) return left < right;
        if (op === DirectTokKind.Gt) return left > right;
        if (op === DirectTokKind.Le) return left <= right;
        return left >= right;
      }
      throw new TemplateSyntaxError(
        `numeric comparison requires number operands, got ${typeof left} and ${typeof right}`,
      );
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
  if (typeof value === "object") {
    // Check __kind__ discriminant
    const obj = value as Record<string, unknown>;
    if (typeof obj[ENUM_TAG_KEY] === "string") return obj[ENUM_TAG_KEY];
  }
  // Scalar types: convert to string for label comparison.
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  // Non-None, non-enum values in option match context → "Some"
  return OPTION_SOME;
}

export { isOptionMatchNode as isOptionMatch } from "../consts.js";
