/**
 * Compile-time type rules shared with the Rust checker
 * (`compiled/type_check/{paths,conditions,includes}.rs` and
 * `compiled/type_resolve.rs`): field access on dotted paths, built-in
 * argument types, enum comparisons, and include contracts.
 *
 * Diagnostics mirror the Rust backend verbatim so both engines reject the
 * same templates with the same messages.
 */

import type { VarDecl, VarType, VariantDecl } from "./frontmatter.js";
import {
  DOT,
  OPTION_SOME,
  TYPE_ALIAS,
  TYPE_BOOL,
  TYPE_ENUM,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  TYPE_OPTION,
  TYPE_SCALAR_LIST,
  TYPE_STR,
  TYPE_STRUCT,
  TYPE_TMPL,
  TYPE_UNTYPED_LIST,
} from "./consts.js";
import { splitPipes } from "./evaluator.js";
import { closestName } from "./suggest.js";
import type { TypeEnv } from "./type_env.js";

/** A plain dotted variable path such as `user.address.city`. */
const DOTTED_PATH_RE = /^[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)*$/;

/** A built-in call on a single dotted path argument, e.g. `kind(task.state)`. */
const BUILTIN_CALL_RE =
  /^(len|kind|has|kinds)\(\s*([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)*)\s*\)$/;

/** Built-in functions whose argument type is checked at compile time. */
enum Builtin {
  Len = "len",
  Kind = "kind",
  Has = "has",
  Kinds = "kinds",
}

/** Comparison operators recognised in condition leaves (longest first). */
type ComparisonOperator = "==" | "!=" | "<=" | ">=" | "<" | ">";

// ---------------------------------------------------------------------------
// Type formatting (mirrors Rust `impl Display for VarType`)
// ---------------------------------------------------------------------------

function formatFields(fields: readonly VarDecl[]): string {
  return fields
    .map((f) =>
      f.name === ""
        ? formatVarType(f.varType)
        : `${f.name} = ${formatVarType(f.varType)}`,
    )
    .join(", ");
}

function formatVariant(variant: VariantDecl): string {
  return variant.fields.length === 0
    ? variant.name
    : `${variant.name}(${formatFields(variant.fields)})`;
}

/** Inner `T` of an `option(T)` encoded as `enum(Some(val = T), None)`. */
function optionInnerType(ty: VarType): VarType | undefined {
  if (ty.kind !== TYPE_ENUM || ty.isOption !== true) return undefined;
  const some = ty.variants.find((v) => v.name === OPTION_SOME);
  return some?.fields.length === 1 ? some.fields[0]?.varType : undefined;
}

/** Render a type exactly like the Rust core's `Display` impl. */
export function formatVarType(ty: VarType): string {
  switch (ty.kind) {
    case TYPE_STR:
    case TYPE_BOOL:
    case TYPE_INT:
    case TYPE_FLOAT:
      return ty.kind;
    case TYPE_LIST:
      return `${TYPE_LIST}(${formatFields(ty.fields)})`;
    case TYPE_SCALAR_LIST:
      return `${TYPE_LIST}(${formatVarType(ty.elementType)})`;
    case TYPE_UNTYPED_LIST:
      return `${TYPE_LIST}()`;
    case TYPE_STRUCT:
      return `${TYPE_STRUCT}(${formatFields(ty.fields)})`;
    case TYPE_TMPL:
      return `${TYPE_TMPL}(${formatFields(ty.fields)})`;
    case TYPE_ENUM: {
      const someInner = optionInnerType(ty);
      return someInner === undefined
        ? `${TYPE_ENUM}(${ty.variants.map(formatVariant).join(", ")})`
        : `${TYPE_OPTION}(${formatVarType(someInner)})`;
    }
    case TYPE_OPTION:
      return `${TYPE_OPTION}(${formatVarType(ty.innerType)})`;
    case TYPE_ALIAS:
      return ty.name;
  }
}

// ---------------------------------------------------------------------------
// Type predicates
// ---------------------------------------------------------------------------

/** `true` for `option(T)` in either representation. */
export function isOptionType(ty: VarType): boolean {
  return (
    ty.kind === TYPE_OPTION || (ty.kind === TYPE_ENUM && ty.isOption === true)
  );
}

function isListType(ty: VarType): boolean {
  return (
    ty.kind === TYPE_LIST ||
    ty.kind === TYPE_SCALAR_LIST ||
    ty.kind === TYPE_UNTYPED_LIST
  );
}

function isPlainEnum(ty: VarType): boolean {
  return ty.kind === TYPE_ENUM && ty.isOption !== true;
}

// ---------------------------------------------------------------------------
// Field access (mirrors Rust `validate_compiled_path` / `resolve_field`)
// ---------------------------------------------------------------------------

/** Outcome of resolving one field access on a type. */
type FieldResult =
  | { readonly kind: "ok"; readonly type: VarType }
  | { readonly kind: "terminal" }
  | { readonly kind: "error"; readonly reason: string };

function suggestion(field: string, candidates: readonly string[]): string {
  const best = closestName(field, candidates);
  return best === undefined ? "" : ` Did you mean '${best}'?`;
}

function resolveEnumField(
  variants: readonly VariantDecl[],
  field: string,
): FieldResult {
  if (variants.length === 0) return { kind: "terminal" };
  let resolved: VarType | undefined;
  const missingOn: string[] = [];
  for (const variant of variants) {
    const decl = variant.fields.find((f) => f.name === field);
    if (decl === undefined) {
      missingOn.push(variant.name);
    } else {
      resolved = decl.varType;
    }
  }
  if (missingOn.length === 0) {
    return resolved === undefined
      ? { kind: "terminal" }
      : { kind: "ok", type: resolved };
  }
  if (missingOn.length === variants.length) {
    const allFields = [
      ...new Set(variants.flatMap((v) => v.fields.map((f) => f.name))),
    ].sort();
    return {
      kind: "error",
      reason: `field '${field}' does not exist on any variant.${suggestion(field, allFields)} (${variants.map((v) => v.name).join(", ")})`,
    };
  }
  return {
    kind: "error",
    reason:
      variants.length > 1
        ? `field '${field}' is not available on variant(s) ${missingOn.join(", ")} — use {% match %} to narrow the type first`
        : `field '${field}' is not available on variant '${missingOn.join("")}'`,
  };
}

function resolveField(ty: VarType, field: string): FieldResult {
  switch (ty.kind) {
    case TYPE_ENUM:
      return resolveEnumField(ty.variants, field);
    case TYPE_STRUCT: {
      const fields = ty.fields;
      if (fields.length === 0) return { kind: "terminal" };
      for (const f of fields) {
        if (f.name === field) {
          return { kind: "ok", type: f.varType };
        }
      }
      const declared = fields.map((f) => f.name).sort();
      return {
        kind: "error",
        reason: `field '${field}' does not exist on dict.${suggestion(field, declared)} (declared fields: ${declared.join(", ")})`,
      };
    }
    case TYPE_LIST:
    case TYPE_SCALAR_LIST: {
      const elementFields =
        ty.kind === TYPE_LIST ? ty.fields.map((f) => f.name) : [""];
      if (elementFields.length === 0) return { kind: "terminal" };
      return {
        kind: "error",
        reason: `cannot access field '${field}' on list — use {% for %} to iterate (element fields: ${elementFields.join(", ")})`,
      };
    }
    case TYPE_UNTYPED_LIST:
    case TYPE_ALIAS:
      return { kind: "terminal" };
    case TYPE_OPTION:
      return {
        kind: "error",
        reason: `cannot access field '${field}' on ${formatVarType(ty)} — use {% if has(...) %} or {% match %} to unwrap first`,
      };
    case TYPE_STR:
    case TYPE_INT:
    case TYPE_FLOAT:
    case TYPE_BOOL:
    case TYPE_TMPL:
      return {
        kind: "error",
        reason: `cannot access field '${field}' on ${formatVarType(ty)}`,
      };
  }
}

/** The dotted path at the start of an expression, without filters. */
function basePath(expr: string): string {
  if (!expr.includes("|")) return expr.trim();
  return (splitPipes(expr)[0] ?? "").trim();
}

function isAsciiIdentSlice(
  s: string,
  start: number,
  end: number,
  allowLeadingDigit: boolean,
): boolean {
  if (end <= start) return false;
  for (let i = start; i < end; i++) {
    const c = s.charCodeAt(i);
    const isAlpha = (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c === 95;
    const isDigit = c >= 48 && c <= 57;
    if (i === start) {
      if (!isAlpha && !(allowLeadingDigit && isDigit)) return false;
    } else if (!isAlpha && !isDigit) {
      return false;
    }
  }
  return true;
}

/**
 * First field-access error on the dotted path `expr` (filters ignored), or
 * `undefined` when the path is valid, not a plain path, or rooted in a name
 * the environment does not type (undeclared names are reported elsewhere).
 */
export function pathAccessError(
  expr: string,
  env: TypeEnv,
): string | undefined {
  if (!expr.includes(DOT)) return undefined;
  const path = basePath(expr);
  const dotIdx = path.indexOf(DOT);
  if (dotIdx <= 0) return undefined;
  if (
    !path.includes(DOT, dotIdx + 1) &&
    isAsciiIdentSlice(path, 0, dotIdx, false) &&
    isAsciiIdentSlice(path, dotIdx + 1, path.length, true)
  ) {
    const root = path.slice(0, dotIdx);
    const current = env.lookupRoot(root);
    if (current === undefined) return undefined;
    if (env.narrowingOf(path) !== undefined) return undefined;
    const field = path.slice(dotIdx + 1);
    if (current.kind === TYPE_STRUCT && current.fields.length > 0) {
      for (const f of current.fields) {
        if (f.name === field) return undefined;
      }
    }
    if (isOptionType(current)) {
      return `'${path}': cannot access field '${field}' on ${formatVarType(current)} — option values must be checked before access. Try guard: {% if has(${root}) %}{{${path}}}{% /if %} or match: {% match ${root} %}{% case Some %}{{${path}}}{% /match %}`;
    }
    if (
      current.kind === TYPE_ENUM &&
      current.variants.some((v) => v.name === field)
    ) {
      return undefined;
    }
    const result = resolveField(current, field);
    if (result.kind === "error") return `'${path}': ${result.reason}`;
    return undefined;
  }
  if (!DOTTED_PATH_RE.test(path)) return undefined;
  const [root = "", ...fields] = path.split(DOT);
  let current = env.lookupRoot(root);
  if (current === undefined) return undefined;
  let traversed = root;
  for (const field of fields) {
    const parent = traversed;
    traversed = `${traversed}${DOT}${field}`;
    const narrowed = env.narrowingOf(traversed);
    if (narrowed !== undefined) {
      current = narrowed;
      continue;
    }
    if (isOptionType(current)) {
      return `'${traversed}': cannot access field '${field}' on ${formatVarType(current)} — option values must be checked before access. Try guard: {% if has(${parent}) %}{{${traversed}}}{% /if %} or match: {% match ${parent} %}{% case Some %}{{${traversed}}}{% /match %}`;
    }
    if (
      current.kind === TYPE_ENUM &&
      current.variants.some((v) => v.name === field)
    ) {
      continue;
    }
    const result = resolveField(current, field);
    if (result.kind === "error") return `'${traversed}': ${result.reason}`;
    if (result.kind === "terminal") return undefined;
    current = env.resolveAlias(result.type);
  }
  return undefined;
}

// ---------------------------------------------------------------------------
// Built-in argument types (mirrors Rust `resolve_compiled_expr_type`)
// ---------------------------------------------------------------------------

function builtinArgumentError(
  builtin: Builtin,
  arg: string,
  ty: VarType,
): string | undefined {
  const label = formatVarType(ty);
  switch (builtin) {
    case Builtin.Len:
      return isListType(ty) || ty.kind === TYPE_STR
        ? undefined
        : `'len()' requires a list or str type, got ${label} on '${arg}'`;
    case Builtin.Kind:
      return ty.kind === TYPE_ENUM || ty.kind === TYPE_OPTION
        ? undefined
        : `'kind()' requires an enum or option type, got ${label} on '${arg}'`;
    case Builtin.Has:
      return isOptionType(ty)
        ? undefined
        : `'has()' requires an option type, got ${label} on '${arg}'`;
    case Builtin.Kinds:
      return isPlainEnum(ty)
        ? undefined
        : `kinds('${arg}'): expected enum type namespace, got ${label}`;
  }
}

/**
 * Statically known type of a path with aliases followed, or `undefined`
 * when unknown (including aliases the environment cannot resolve).
 */
function resolvedType(path: string, env: TypeEnv): VarType | undefined {
  const ty = env.resolveExprType(path);
  if (ty === undefined) return undefined;
  const resolved = env.resolveAlias(ty);
  return resolved.kind === TYPE_ALIAS ? undefined : resolved;
}

/** Built-in functions keyed by their call name. */
const BUILTINS_BY_NAME: ReadonlyMap<string, Builtin> = new Map(
  Object.values(Builtin).map((builtin) => [builtin, builtin]),
);

/**
 * Combined single-pass check + type resolution for `{{ expr }}`, `{% for %}`,
 * `{% match %}`, and bare truthiness conditions.
 *
 * Returns:
 * - `string`: the compile-time type/field error message, if invalid
 * - `VarType`: the resolved expression type, if valid and statically typed
 * - `undefined`: valid scalar/literal/untyped expression
 */
export function resolveExprWithCheck(
  expr: string,
  env: TypeEnv,
): VarType | string | undefined {
  if (!expr.includes("|")) {
    const trimmed =
      expr.charCodeAt(0) === 32 || expr.charCodeAt(expr.length - 1) === 32
        ? expr.trim()
        : expr;
    const firstCode = trimmed.charCodeAt(0);
    if (
      firstCode === 34 /* '"' */ ||
      firstCode === 39 /* "'" */ ||
      (firstCode >= 48 && firstCode <= 57) ||
      firstCode === 45 /* '-' */
    ) {
      return undefined;
    }
    if (!trimmed.includes("(")) {
      const dotIdx = trimmed.indexOf(DOT);
      if (dotIdx === -1) {
        return env.lookupRoot(trimmed);
      }
      if (
        dotIdx > 0 &&
        !trimmed.includes(DOT, dotIdx + 1) &&
        isAsciiIdentSlice(trimmed, 0, dotIdx, false) &&
        isAsciiIdentSlice(trimmed, dotIdx + 1, trimmed.length, true)
      ) {
        const root = trimmed.slice(0, dotIdx);
        const current = env.lookupRoot(root);
        if (current === undefined) return undefined;
        const narrowed = env.narrowingOf(trimmed);
        if (narrowed !== undefined) return narrowed;
        const field = trimmed.slice(dotIdx + 1);
        if (current.kind === TYPE_STRUCT && current.fields.length > 0) {
          for (const f of current.fields) {
            if (f.name === field) {
              return env.resolveAlias(f.varType);
            }
          }
        }
        const err = pathAccessError(trimmed, env);
        if (err !== undefined) return err;
        return env.resolveExprType(trimmed);
      }
    }
  }
  const err = operandError(expr, env);
  if (err !== undefined) return err;
  return env.resolveExprType(expr);
}

/**
 * Type error for an operand: a field-access error on a dotted path, or an
 * argument-type error for `len()` / `kind()` / `has()` / `kinds()`.
 */
export function operandError(expr: string, env: TypeEnv): string | undefined {
  const operand = basePath(expr);
  if (!operand.includes("(")) {
    return pathAccessError(operand, env);
  }
  const call = BUILTIN_CALL_RE.exec(operand);
  if (call === null) return pathAccessError(operand, env);
  const builtin = BUILTINS_BY_NAME.get(call[1] ?? "");
  const arg = call[2] ?? "";
  if (builtin === undefined) return undefined;
  const pathError = pathAccessError(arg, env);
  if (pathError !== undefined) return pathError;
  const argType = resolvedType(arg, env);
  return argType === undefined
    ? undefined
    : builtinArgumentError(builtin, arg, argType);
}

// ---------------------------------------------------------------------------
// Comparisons (mirrors Rust `validate_condition` for `Comparison`)
// ---------------------------------------------------------------------------

/** A binary comparison split at its top-level operator. */
export interface Comparison {
  readonly left: string;
  readonly operator: ComparisonOperator;
  readonly right: string;
}

/** Split `leaf` at its first top-level comparison operator (outside quotes and parentheses). */
export function splitComparison(leaf: string): Comparison | undefined {
  let depth = 0;
  let quoteCode = 0;
  for (let i = 0; i < leaf.length; i++) {
    const code = leaf.charCodeAt(i);
    if (quoteCode !== 0) {
      if (code === quoteCode && leaf.charCodeAt(i - 1) !== 92 /* '\\' */) {
        quoteCode = 0;
      }
      continue;
    }
    if (code === 34 /* '"' */ || code === 39 /* "'" */) {
      quoteCode = code;
    } else if (code === 40 /* '(' */) {
      depth++;
    } else if (code === 41 /* ')' */) {
      depth--;
    } else if (
      depth === 0 &&
      (code === 61 /* '=' */ ||
        code === 33 /* '!' */ ||
        code === 60 /* '<' */ ||
        code === 62) /* '>' */
    ) {
      const next = leaf.charCodeAt(i + 1);
      let operator: ComparisonOperator | undefined;
      if (code === 61 && next === 61) operator = "==";
      else if (code === 33 && next === 61) operator = "!=";
      else if (code === 60) operator = next === 61 ? "<=" : "<";
      else if (code === 62) operator = next === 61 ? ">=" : ">";
      if (operator !== undefined) {
        return {
          left: leaf.slice(0, i).trim(),
          operator,
          right: leaf.slice(i + operator.length).trim(),
        };
      }
    }
  }
  return undefined;
}

/** First type error in a comparison: enum equality, then operand errors. */
export function comparisonError(
  comparison: Comparison,
  env: TypeEnv,
): string | undefined {
  const leftRes = resolveExprWithCheck(comparison.left, env);
  const rightRes = resolveExprWithCheck(comparison.right, env);
  if (comparison.operator === "==" || comparison.operator === "!=") {
    if (
      typeof leftRes === "object" &&
      isPlainEnum(leftRes) &&
      DOTTED_PATH_RE.test(comparison.left)
    ) {
      return `cannot compare enum '${comparison.left}' with '==' — use {% match %} instead`;
    }
    if (
      typeof rightRes === "object" &&
      isPlainEnum(rightRes) &&
      DOTTED_PATH_RE.test(comparison.right)
    ) {
      return `cannot compare enum '${comparison.right}' with '==' — use {% match %} instead`;
    }
  }
  if (typeof leftRes === "string") return leftRes;
  if (typeof rightRes === "string") return rightRes;
  return (
    operandError(comparison.left, env) ?? operandError(comparison.right, env)
  );
}

// ---------------------------------------------------------------------------
// Include contracts (mirrors Rust `validate_include_contract`)
// ---------------------------------------------------------------------------

/**
 * Error for an include that leaves required (default-less) params of the
 * included template unprovided by `with` mappings or the `for` binding.
 */
export function includeContractError(
  includeName: string,
  declarations: readonly VarDecl[],
  provided: ReadonlySet<string>,
): string | undefined {
  const missing = declarations.filter(
    (d) => d.defaultValue === undefined && !provided.has(d.name),
  );
  if (missing.length === 0) return undefined;
  const descriptions = missing.map(
    (d) => `${d.name}: ${formatVarType(d.varType)}`,
  );
  const hints = missing.map((d) => `${d.name}=${d.name}`);
  return `include '${includeName}': missing required param(s): ${descriptions.join(", ")}. Use 'with ${hints.join(", ")}' to pass them`;
}
