/**
 * Direct expression compilation, resolution, function calls, and filters.
 *
 * Expressions and filter chains are compiled once per AST node at template
 * construction time (`precompileDirectNodes`), matching the Rust core's
 * `CompiledExpr` / `ParsedFilter` pipeline without global runtime caches.
 *
 * @module
 */

import {
  TemplateSyntaxError,
  UndefinedVariableError,
  UnknownFilterError,
} from "../errors.js";
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
import { ENUM_VARIANTS_KEY, fromJs } from "../value.js";
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
  FIXED_ARG_LABEL,
  LIMIT_ARG_LABEL,
  TRUNCATE_ARG_LABEL,
  parseI64,
  parseF64,
  parseUsize,
  parseUsizeFilterArg,
  saturateI64,
} from "../filter_args.js";
import { formatFixedFloat, formatFixedInt } from "../numeric_format.js";
import { truncateMiddleItems } from "../filter_truncate.js";
import { parseJsonIndent, serializeJson } from "../filter_json.js";
import { splitPipes } from "../evaluator.js";
import { DirectScope } from "./scope.js";
import { directDisplay } from "./display.js";
import { interpolateDirectString } from "./condition.js";

const DIRECT_NUM_LITERAL_RE = /^-?[0-9]+(?:\.[0-9]+)?$/;
const INVALID_KEYWORD_NOT = "not";
const SAFE_FAST_ARITH_BOUND = 1e15;

function isValidDirectIdent(s: string): boolean {
  const len = s.length;
  if (len === 0) return false;
  const first = s.charCodeAt(0);
  const validStart =
    (first >= 65 && first <= 90) ||
    (first >= 97 && first <= 122) ||
    first === 95;
  if (!validStart) {
    return false;
  }
  for (let i = 1; i < len; i++) {
    const c = s.charCodeAt(i);
    const validCont =
      (c >= 65 && c <= 90) ||
      (c >= 97 && c <= 122) ||
      (c >= 48 && c <= 57) ||
      c === 95 ||
      c === 45;
    if (!validCont) {
      return false;
    }
  }
  return true;
}

// ---------------------------------------------------------------------------
// Pre-compiled direct expression & filter AST
// ---------------------------------------------------------------------------

export type CompiledFilterStep =
  | { readonly kind: "upper" }
  | { readonly kind: "lower" }
  | { readonly kind: "trim" }
  | { readonly kind: "escape_xml" }
  | { readonly kind: "escape_json" }
  | {
      readonly kind: "add_int";
      readonly deltaNum: number;
      readonly deltaBig: bigint;
    }
  | {
      readonly kind: "sub_int";
      readonly deltaNum: number;
      readonly deltaBig: bigint;
    }
  | { readonly kind: "add_float"; readonly delta: number }
  | { readonly kind: "sub_float"; readonly delta: number }
  | { readonly kind: "fixed"; readonly precision: number }
  | { readonly kind: "join"; readonly sep: string }
  | { readonly kind: "limit"; readonly limit: number }
  | {
      readonly kind: "quarantine_untrusted";
      readonly rawArg: string | undefined;
    }
  | {
      readonly kind: "generic";
      readonly filterName: string;
      readonly rawArg: string | undefined;
    };

export type CompiledDirectExpr =
  | { readonly kind: "var"; readonly name: string }
  | { readonly kind: "invalid_var"; readonly name: string }
  | {
      readonly kind: "prop1";
      readonly root: string;
      readonly prop: string;
      readonly fullPath: string;
    }
  | {
      readonly kind: "path";
      readonly root: string;
      readonly segments: readonly string[];
      readonly fullPath: string;
      readonly stemKey: string;
      readonly stemSegments: readonly string[];
    }
  | { readonly kind: "loop_prop"; readonly prop: string }
  | { readonly kind: "lit"; readonly value: unknown }
  | { readonly kind: "interp_str"; readonly inner: string }
  | { readonly kind: "fn_idx"; readonly binding: string }
  | { readonly kind: "fn_len"; readonly arg: CompiledDirectExpr }
  | { readonly kind: "fn_kind"; readonly arg: CompiledDirectExpr }
  | { readonly kind: "fn_kinds"; readonly arg: CompiledDirectExpr }
  | { readonly kind: "fn_has"; readonly arg: CompiledDirectExpr }
  | { readonly kind: "fn_raw"; readonly expr: string }
  | {
      readonly kind: "filter1";
      readonly base: CompiledDirectExpr;
      readonly step: CompiledFilterStep;
    }
  | {
      readonly kind: "filtered";
      readonly base: CompiledDirectExpr;
      readonly steps: readonly CompiledFilterStep[];
    };

function compileFilterStep(
  filterName: string,
  rawArg: string | undefined,
): CompiledFilterStep {
  switch (filterName) {
    case "upper":
      return { kind: "upper" };
    case "lower":
      return { kind: "lower" };
    case "trim":
      return { kind: "trim" };
    case "escape_xml":
    case "xml":
      return { kind: "escape_xml" };
    case "escape_json":
      return { kind: "escape_json" };
    case "join":
      return {
        kind: "join",
        sep: rawArg !== undefined ? stripQuotes(rawArg) : "",
      };
    case FILTER_LIMIT: {
      if (rawArg !== undefined) {
        const parsed = parseUsize(rawArg);
        if (parsed.ok) {
          return { kind: "limit", limit: parsed.value };
        }
      }
      return { kind: "generic", filterName, rawArg };
    }
    case FILTER_FIXED: {
      if (rawArg !== undefined) {
        const parsed = parseUsize(rawArg);
        if (parsed.ok) {
          return { kind: "fixed", precision: parsed.value };
        }
      }
      return { kind: "generic", filterName, rawArg };
    }
    case FILTER_ADD:
    case FILTER_SUB: {
      if (rawArg !== undefined) {
        const asInt = parseI64(rawArg);
        if (asInt.ok) {
          return {
            kind: filterName === FILTER_ADD ? "add_int" : "sub_int",
            deltaNum: Number(asInt.value),
            deltaBig: asInt.value,
          };
        }
        const asFloat = parseF64(rawArg);
        if (asFloat.ok) {
          return {
            kind: filterName === FILTER_ADD ? "add_float" : "sub_float",
            delta: asFloat.value,
          };
        }
      }
      return { kind: "generic", filterName, rawArg };
    }
    default:
      return { kind: "generic", filterName, rawArg };
  }
}

function compileBaseDirectExpr(trimmed: string): CompiledDirectExpr {
  const first = trimmed.charCodeAt(0);
  if (
    (first === 34 /* '"' */ || first === 39) /* "'" */ &&
    trimmed.charCodeAt(trimmed.length - 1) === first &&
    trimmed.length >= 2
  ) {
    const inner = unescapeStringLiteral(trimmed.slice(1, -1));
    if (inner.includes(EXPR_START)) {
      return { kind: "interp_str", inner };
    }
    return { kind: "lit", value: inner };
  }

  if (trimmed === LIT_TRUE) return { kind: "lit", value: true };
  if (trimmed === LIT_FALSE) return { kind: "lit", value: false };
  if (
    ((first >= 48 && first <= 57) || first === 45) /* '-' */ &&
    DIRECT_NUM_LITERAL_RE.test(trimmed)
  ) {
    return { kind: "lit", value: Number(trimmed) };
  }

  if (trimmed.charCodeAt(trimmed.length - 1) === 41 /* ')' */) {
    const parenIdx = trimmed.indexOf("(");
    if (parenIdx > 0) {
      const funcName = trimmed.slice(0, parenIdx).trim();
      const argStr = trimmed.slice(parenIdx + 1, trimmed.length - 1).trim();
      switch (funcName) {
        case "idx":
          return { kind: "fn_idx", binding: argStr };
        case "len":
          return { kind: "fn_len", arg: compileBaseDirectExpr(argStr) };
        case "kind":
          return { kind: "fn_kind", arg: compileBaseDirectExpr(argStr) };
        case "kinds":
          return { kind: "fn_kinds", arg: compileBaseDirectExpr(argStr) };
        case "has":
          return { kind: "fn_has", arg: compileBaseDirectExpr(argStr) };
        default:
          return { kind: "fn_raw", expr: trimmed };
      }
    }
  }

  let path = trimmed;
  if (path.startsWith(PREFIX_CONSTS_DOT)) {
    path = path.slice(PREFIX_CONSTS_DOT.length).trim();
  } else if (path.startsWith(PREFIX_OPTS_DOT)) {
    path = path.slice(PREFIX_OPTS_DOT.length).trim();
  } else if (path.startsWith(PREFIX_OPTIONS_DOT)) {
    path = path.slice(PREFIX_OPTIONS_DOT.length).trim();
  } else if (path.startsWith(PREFIX_PARAMS_DOT)) {
    path = path.slice(PREFIX_PARAMS_DOT.length).trim();
  }

  if (path.startsWith("loop.")) {
    return { kind: "loop_prop", prop: path.slice(5).trim() };
  }

  const dotIdx = path.indexOf(".");
  if (dotIdx >= 0) {
    const root = path.slice(0, dotIdx);
    if (!isValidDirectIdent(root)) {
      return { kind: "invalid_var", name: path };
    }
    const secondDot = path.indexOf(".", dotIdx + 1);
    if (secondDot === -1) {
      const prop = path.slice(dotIdx + 1);
      if (!isValidDirectIdent(prop)) {
        return { kind: "invalid_var", name: path };
      }
      if (prop !== ENUM_TAG_KEY) {
        return { kind: "prop1", root, prop, fullPath: path };
      }
    }
    const allParts = path.split(".");
    return {
      kind: "path",
      root,
      segments: allParts.slice(1),
      fullPath: path,
      stemKey: secondDot !== -1 ? path.slice(0, secondDot) : "",
      stemSegments: secondDot !== -1 ? allParts.slice(2) : [],
    };
  }

  if (path === INVALID_KEYWORD_NOT || !isValidDirectIdent(path)) {
    return { kind: "invalid_var", name: path };
  }

  return { kind: "var", name: path };
}

/** Compile an expression string (with optional filter chain) into an execution plan. */
export function compileDirectExpr(expr: string): CompiledDirectExpr {
  const trimmed =
    expr.charCodeAt(0) === 32 || expr.charCodeAt(expr.length - 1) === 32
      ? expr.trim()
      : expr;

  if (!trimmed.includes("|")) {
    return compileBaseDirectExpr(trimmed);
  }

  const parts = splitPipes(trimmed);
  const base = compileBaseDirectExpr((parts[0] ?? "").trim());
  const steps: CompiledFilterStep[] = [];
  for (let i = 1; i < parts.length; i++) {
    const part = parts[i];
    if (part === undefined) continue;
    const [filterName, rawArg] = parseFilter(part.trim());
    if (
      filterName === "sanitize_tokens" &&
      rawArg === undefined &&
      i + 1 < parts.length
    ) {
      const nextPart = parts[i + 1];
      if (nextPart !== undefined) {
        const [nextName, nextArg] = parseFilter(nextPart.trim());
        if (nextName === "quarantine") {
          steps.push({ kind: "quarantine_untrusted", rawArg: nextArg });
          i++;
          continue;
        }
      }
    }
    steps.push(compileFilterStep(filterName, rawArg));
  }

  if (steps.length === 1 && steps[0] !== undefined) {
    return { kind: "filter1", base, step: steps[0] };
  }
  return { kind: "filtered", base, steps };
}

function evalSegments(root: unknown, segments: readonly string[]): unknown {
  let current = root;
  for (const key of segments) {
    if (
      current === null ||
      current === undefined ||
      typeof current !== "object" ||
      Array.isArray(current) ||
      key === ENUM_TAG_KEY
    ) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

export function evalCompiledFilterStep(
  value: unknown,
  step: CompiledFilterStep,
): unknown {
  switch (step.kind) {
    case "upper":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'upper' requires a string");
      }
      return value.toUpperCase();
    case "lower":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'lower' requires a string");
      }
      return value.toLowerCase();
    case "trim":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'trim' requires a string");
      }
      return value.trim();
    case "escape_xml":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'escape_xml' requires a string");
      }
      return escapeXmlString(value);
    case "escape_json":
      if (typeof value !== "string") {
        throw new TemplateSyntaxError("'escape_json' requires a string");
      }
      return escapeJsonString(value);
    case "join":
      if (!Array.isArray(value)) {
        throw new TemplateSyntaxError("'join' requires a list");
      }
      return value.map((v) => directDisplay(v)).join(step.sep);
    case "limit":
      if (!Array.isArray(value)) {
        throw new TemplateSyntaxError("'limit' requires a list");
      }
      return value.slice(0, step.limit);
    case "fixed":
      if (typeof value !== "number") {
        throw new TemplateSyntaxError("'fixed' requires a number");
      }
      return Number.isInteger(value)
        ? formatFixedInt(value, step.precision)
        : formatFixedFloat(value, step.precision);
    case "add_int":
      if (typeof value !== "number") {
        throw new TemplateSyntaxError("'add' requires a number");
      }
      if (Number.isInteger(value)) {
        if (
          Math.abs(value) <= SAFE_FAST_ARITH_BOUND &&
          Math.abs(step.deltaNum) <= SAFE_FAST_ARITH_BOUND
        ) {
          return value + step.deltaNum;
        }
        return Math.trunc(Number(saturateI64(BigInt(value) + step.deltaBig)));
      }
      return value + step.deltaNum;
    case "sub_int":
      if (typeof value !== "number") {
        throw new TemplateSyntaxError("'sub' requires a number");
      }
      if (Number.isInteger(value)) {
        if (
          Math.abs(value) <= SAFE_FAST_ARITH_BOUND &&
          Math.abs(step.deltaNum) <= SAFE_FAST_ARITH_BOUND
        ) {
          return value - step.deltaNum;
        }
        return Math.trunc(Number(saturateI64(BigInt(value) - step.deltaBig)));
      }
      return value - step.deltaNum;
    case "add_float":
      if (typeof value !== "number") {
        throw new TemplateSyntaxError("'add' requires a number");
      }
      return value + step.delta;
    case "sub_float":
      if (typeof value !== "number") {
        throw new TemplateSyntaxError("'sub' requires a number");
      }
      return value - step.delta;
    case "quarantine_untrusted":
      if (typeof value === "string") {
        return quarantineUntrustedString(value, step.rawArg);
      }
      return applyDirectFilter(
        applyDirectFilter(value, "sanitize_tokens", undefined),
        "quarantine",
        step.rawArg,
      );
    case "generic":
      return applyDirectFilter(value, step.filterName, step.rawArg);
  }
}

export function evalCompiledDirectExpr(
  compiled: CompiledDirectExpr,
  scope: DirectScope,
): unknown {
  switch (compiled.kind) {
    case "var":
      return scope.resolve(compiled.name);

    case "invalid_var":
      throw new UndefinedVariableError(compiled.name);

    case "prop1":
      return scope.resolveProp1(
        compiled.root,
        compiled.prop,
        compiled.fullPath,
      );

    case "path": {
      const exactConst = scope.resolveConst(compiled.fullPath);
      if (exactConst !== undefined) return exactConst;
      if (compiled.stemKey.length > 0) {
        const stemVal = scope.resolveConst(compiled.stemKey);
        if (stemVal !== undefined) {
          return evalSegments(stemVal, compiled.stemSegments);
        }
      }
      const rootVal = scope.resolve(compiled.root);
      if (rootVal === undefined) return undefined;
      return evalSegments(rootVal, compiled.segments);
    }

    case "loop_prop": {
      const meta = scope.topLoop();
      if (!meta) {
        throw new TemplateSyntaxError(
          "loop metadata is only available inside a for loop",
        );
      }
      switch (compiled.prop) {
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
          throw new TemplateSyntaxError(
            `field '${compiled.prop}' not found on loop`,
          );
      }
    }

    case "lit":
      return compiled.value;

    case "interp_str":
      return interpolateDirectString(compiled.inner, scope);

    case "fn_idx": {
      const binding = compiled.binding || scope.getLastLoopBinding();
      if (binding) {
        const idx = scope.getLoopIndex(binding);
        if (idx !== undefined) return idx;
      }
      throw new TemplateSyntaxError(
        `idx() requires an active loop binding${compiled.binding ? ` for '${compiled.binding}'` : ""}`,
      );
    }

    case "fn_len": {
      const arg = evalCompiledDirectExpr(compiled.arg, scope);
      if (typeof arg === "string") return arg.length;
      if (Array.isArray(arg)) return arg.length;
      throw new TemplateSyntaxError(
        `len() requires a list or string, got ${typeof arg}`,
      );
    }

    case "fn_kind": {
      const arg = evalCompiledDirectExpr(compiled.arg, scope);
      if (arg === null || arg === undefined) return OPTION_NONE;
      if (typeof arg === "object" && ENUM_TAG_KEY in arg) {
        return (arg as Record<string, unknown>)[ENUM_TAG_KEY];
      }
      if (typeof arg === "string") return arg;
      return OPTION_SOME;
    }

    case "fn_kinds": {
      const arg = evalCompiledDirectExpr(compiled.arg, scope);
      if (arg !== null && typeof arg === "object" && !Array.isArray(arg)) {
        const variants = (arg as Record<string, unknown>)[ENUM_VARIANTS_KEY];
        if (Array.isArray(variants)) return variants;
      }
      throw new TemplateSyntaxError(
        `kinds() requires an enum type namespace, got ${typeof arg}`,
      );
    }

    case "fn_has": {
      const arg = evalCompiledDirectExpr(compiled.arg, scope);
      if (arg === null || arg === undefined) return false;
      if (typeof arg === "object" && !Array.isArray(arg)) {
        const obj = arg as Record<string, unknown>;
        if (obj[ENUM_TAG_KEY] === OPTION_NONE) return false;
      }
      return true;
    }

    case "fn_raw":
      return resolveDirectFunction(compiled.expr, scope);

    case "filter1":
      return evalCompiledFilterStep(
        evalCompiledDirectExpr(compiled.base, scope),
        compiled.step,
      );

    case "filtered": {
      let value = evalCompiledDirectExpr(compiled.base, scope);
      for (const step of compiled.steps) {
        value = evalCompiledFilterStep(value, step);
      }
      return value;
    }
  }
}

// ---------------------------------------------------------------------------
// Direct expression resolution (public compatibility API)
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
    if (key === ENUM_TAG_KEY) return undefined;
    current = (current as Record<string, unknown>)[key];
    start = end + 1;
  }
  return current;
}

/** Resolve an expression in the direct scope. */
export function resolveDirectExpr(expr: string, scope: DirectScope): unknown {
  return evalCompiledDirectExpr(compileDirectExpr(expr), scope);
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
      if (typeof value === "number" && rawArg !== undefined) {
        const precision = parseUsizeFilterArg(
          rawArg,
          FILTER_FIXED,
          FIXED_ARG_LABEL,
        );
        return Number.isInteger(value)
          ? formatFixedInt(value, precision)
          : formatFixedFloat(value, precision);
      }
      return directNumericFilter(value, filterName, rawArg);
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
  return evalCompiledDirectExpr(compileDirectExpr(expr), scope);
}

export function directToJson(value: unknown, indent?: number): string {
  if (typeof value === "function") {
    throw new TemplateSyntaxError("cannot serialize template to JSON");
  }
  return serializeJson(fromJs(value), indent);
}
