/**
 * Zero-allocation fast-path parameter validator for `Template.render()`.
 *
 * Pre-resolves `VarType` aliases and enum variant lookup tables at compile
 * time so valid JS parameter objects can be verified in-place without
 * boxing into `Value` wrappers, allocating `Map`s/`WeakSet`s, or formatting
 * error path strings. Any invalid or non-standard input (missing/extra params,
 * type mismatches, cyclic objects, boxed `Value`s, or higher-order `TmplRef`s)
 * returns `false` and delegates to `buildContext` as the single source of truth
 * for conversion and error diagnostics.
 *
 * @module
 */

import { type VarDecl, type VarType } from "../frontmatter.js";
import { type Node } from "../parser.js";
import {
  ENUM_TAG_KEY,
  FN_KIND,
  NODE_EXPR,
  NODE_FOR,
  NODE_IF,
  NODE_INCLUDE,
  NODE_MATCH,
  NODE_PANIC,
  OPTION_SOME,
  TYPE_ALIAS,
  TYPE_BOOL,
  TYPE_ENUM,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  TYPE_NONE,
  TYPE_OPTION,
  TYPE_SCALAR_LIST,
  TYPE_STR,
  TYPE_STRUCT,
  TYPE_TMPL,
  TYPE_UNTYPED_LIST,
} from "../consts.js";

const MAX_FAST_VALIDATION_DEPTH = 32;
const LEGACY_DICT_TYPE = "dict";
const FIELD_NAME_TYPE = "type";
const FIELD_NAME_VALUE = "value";

export interface CompiledFieldCheck {
  readonly name: string;
  readonly spec: CompiledTypeSpec;
}

export type CompiledTypeSpec =
  | { readonly kind: typeof TYPE_STR }
  | { readonly kind: typeof TYPE_BOOL }
  | { readonly kind: typeof TYPE_INT }
  | { readonly kind: typeof TYPE_FLOAT }
  | { readonly kind: typeof TYPE_UNTYPED_LIST }
  | {
      readonly kind: typeof TYPE_SCALAR_LIST;
      readonly elementType: CompiledTypeSpec;
    }
  | {
      readonly kind: typeof TYPE_LIST;
      readonly fields: readonly CompiledFieldCheck[];
      readonly needsBoxedCheck: boolean;
    }
  | {
      readonly kind: typeof TYPE_STRUCT;
      readonly fields: readonly CompiledFieldCheck[];
      readonly needsBoxedCheck: boolean;
    }
  | {
      readonly kind: typeof TYPE_OPTION;
      readonly innerType: CompiledTypeSpec;
    }
  | {
      readonly kind: typeof TYPE_ENUM;
      readonly unitVariants: ReadonlySet<string>;
      readonly structVariants: ReadonlyMap<
        string,
        readonly CompiledFieldCheck[]
      >;
    }
  | { readonly kind: typeof TYPE_TMPL }
  | { readonly kind: typeof TYPE_ALIAS };

export interface CompiledParamCheck {
  readonly name: string;
  readonly hasDefault: boolean;
  readonly spec: CompiledTypeSpec;
}

/** Pre-compile parameter declarations into alias-resolved fast check specs. */
export function buildParamChecks(
  params: readonly VarDecl[],
  typeAliases: ReadonlyMap<string, VarType>,
): readonly CompiledParamCheck[] {
  return params.map((decl) => ({
    name: decl.name,
    hasDefault: decl.defaultValue !== undefined,
    spec: compileTypeSpec(decl.varType, typeAliases, new Set()),
  }));
}

function compileTypeSpec(
  varType: VarType,
  typeAliases: ReadonlyMap<string, VarType>,
  seenAliases: Set<string>,
): CompiledTypeSpec {
  switch (varType.kind) {
    case TYPE_STR:
    case TYPE_BOOL:
    case TYPE_INT:
    case TYPE_FLOAT:
    case TYPE_UNTYPED_LIST:
    case TYPE_TMPL:
      return { kind: varType.kind };

    case TYPE_ALIAS: {
      if (seenAliases.has(varType.name)) {
        return { kind: TYPE_ALIAS };
      }
      const resolved = typeAliases.get(varType.name);
      if (resolved === undefined) {
        return { kind: TYPE_ALIAS };
      }
      const nextSeen = new Set(seenAliases);
      nextSeen.add(varType.name);
      return compileTypeSpec(resolved, typeAliases, nextSeen);
    }

    case TYPE_OPTION:
      return {
        kind: TYPE_OPTION,
        innerType: compileTypeSpec(varType.innerType, typeAliases, seenAliases),
      };

    case TYPE_SCALAR_LIST:
      return {
        kind: TYPE_SCALAR_LIST,
        elementType: compileTypeSpec(
          varType.elementType,
          typeAliases,
          seenAliases,
        ),
      };

    case TYPE_LIST: {
      const fields = varType.fields.map((f) => ({
        name: f.name,
        spec: compileTypeSpec(f.varType, typeAliases, seenAliases),
      }));
      const hasTypeField = fields.some((f) => f.name === FIELD_NAME_TYPE);
      return {
        kind: TYPE_LIST,
        fields,
        needsBoxedCheck:
          !hasTypeField &&
          fields.every(
            (f) => f.name === FIELD_NAME_TYPE || f.name === FIELD_NAME_VALUE,
          ),
      };
    }

    case TYPE_STRUCT: {
      const fields = varType.fields.map((f) => ({
        name: f.name,
        spec: compileTypeSpec(f.varType, typeAliases, seenAliases),
      }));
      const hasTypeField = fields.some((f) => f.name === FIELD_NAME_TYPE);
      return {
        kind: TYPE_STRUCT,
        fields,
        needsBoxedCheck:
          !hasTypeField &&
          fields.every(
            (f) => f.name === FIELD_NAME_TYPE || f.name === FIELD_NAME_VALUE,
          ),
      };
    }

    case TYPE_ENUM: {
      if (varType.isOption) {
        const someVariant = varType.variants.find(
          (v) => v.name === OPTION_SOME,
        );
        const someField = someVariant?.fields[0];
        if (someVariant?.fields.length === 1 && someField !== undefined) {
          return {
            kind: TYPE_OPTION,
            innerType: compileTypeSpec(
              someField.varType,
              typeAliases,
              seenAliases,
            ),
          };
        }
      }
      const unitVariants = new Set<string>();
      const structVariants = new Map<string, readonly CompiledFieldCheck[]>();
      for (const variant of varType.variants) {
        // Unit variants can also be passed as `{ __kind__: "Variant" }`
        const compiledFields = variant.fields.map((f) => ({
          name: f.name,
          spec: compileTypeSpec(f.varType, typeAliases, seenAliases),
        }));
        structVariants.set(variant.name, compiledFields);
        if (variant.fields.length === 0) {
          unitVariants.add(variant.name);
        }
      }
      return {
        kind: TYPE_ENUM,
        unitVariants,
        structVariants,
      };
    }
  }
}

function isBoxedValueObject(val: object): boolean {
  if (!(FIELD_NAME_TYPE in val)) return false;
  const t = val.type;
  return (
    t === TYPE_STR ||
    t === TYPE_BOOL ||
    t === TYPE_INT ||
    t === TYPE_FLOAT ||
    t === TYPE_LIST ||
    t === TYPE_STRUCT ||
    t === TYPE_TMPL ||
    t === TYPE_NONE ||
    t === LEGACY_DICT_TYPE
  );
}

function isValidStructFields(
  obj: Record<string, unknown>,
  fields: readonly CompiledFieldCheck[],
  depth: number,
): boolean {
  if (Object.getPrototypeOf(obj) !== Object.prototype) {
    return false;
  }
  for (const field of fields) {
    const fv = obj[field.name];
    if (fv === undefined && !Object.hasOwn(obj, field.name)) {
      return false;
    }
    const k = field.spec.kind;
    if (k === TYPE_STR) {
      if (typeof fv !== "string") return false;
      continue;
    }
    if (k === TYPE_INT) {
      if (typeof fv !== "number" || !Number.isInteger(fv)) return false;
      continue;
    }
    if (k === TYPE_BOOL) {
      if (typeof fv !== "boolean") return false;
      continue;
    }
    if (k === TYPE_FLOAT) {
      if (typeof fv !== "number") return false;
      continue;
    }
    if (!isValidFastValue(fv, field.spec, depth)) {
      return false;
    }
  }
  return true;
}

function isValidFastValue(
  val: unknown,
  spec: CompiledTypeSpec,
  depth: number,
): boolean {
  if (depth > MAX_FAST_VALIDATION_DEPTH) {
    return false;
  }
  switch (spec.kind) {
    case TYPE_STR:
      return typeof val === "string";
    case TYPE_BOOL:
      return typeof val === "boolean";
    case TYPE_INT:
      return typeof val === "number" && Number.isInteger(val);
    case TYPE_FLOAT:
      return typeof val === "number";
    case TYPE_UNTYPED_LIST:
      return Array.isArray(val);
    case TYPE_OPTION:
      if (val === null || val === undefined) return true;
      return isValidFastValue(val, spec.innerType, depth + 1);
    case TYPE_SCALAR_LIST: {
      if (!Array.isArray(val)) return false;
      const items: readonly unknown[] = val;
      for (const item of items) {
        if (!isValidFastValue(item, spec.elementType, depth + 1)) {
          return false;
        }
      }
      return true;
    }
    case TYPE_LIST: {
      if (!Array.isArray(val)) return false;
      const items: readonly unknown[] = val;
      const nextDepth = depth + 1;
      for (const item of items) {
        if (
          typeof item !== "object" ||
          item === null ||
          (spec.needsBoxedCheck && isBoxedValueObject(item)) ||
          !isValidStructFields(
            item as Record<string, unknown>,
            spec.fields,
            nextDepth,
          )
        ) {
          return false;
        }
      }
      return true;
    }
    case TYPE_STRUCT:
      if (
        typeof val !== "object" ||
        val === null ||
        (spec.needsBoxedCheck && isBoxedValueObject(val))
      ) {
        return false;
      }
      return isValidStructFields(
        val as Record<string, unknown>,
        spec.fields,
        depth + 1,
      );
    case TYPE_ENUM: {
      if (typeof val === "string") {
        return spec.unitVariants.has(val);
      }
      if (
        typeof val !== "object" ||
        val === null ||
        Array.isArray(val) ||
        isBoxedValueObject(val)
      ) {
        return false;
      }
      const obj = val as Record<string, unknown>;
      const tag = obj[ENUM_TAG_KEY];
      if (typeof tag !== "string") {
        return false;
      }
      const vFields = spec.structVariants.get(tag);
      if (vFields === undefined) {
        return false;
      }
      return isValidStructFields(obj, vFields, depth + 1);
    }
    case TYPE_TMPL:
    case TYPE_ALIAS:
      return false;
  }
}

/**
 * Returns `true` if `params` passes all missing-param, extra-param, and
 * type-checking rules for direct JS rendering.
 */
export function isValidFastParams(
  params: Record<string, unknown>,
  paramChecks: readonly CompiledParamCheck[],
  _declaredNames: ReadonlySet<string>,
  allowExtra: boolean,
): boolean {
  if (Object.getPrototypeOf(params) !== Object.prototype) {
    return false;
  }
  let presentDeclaredCount = 0;
  for (const check of paramChecks) {
    const val = params[check.name];
    if (val === undefined && !Object.hasOwn(params, check.name)) {
      if (!check.hasDefault) {
        return false;
      }
      continue;
    }
    presentDeclaredCount++;
    const k = check.spec.kind;
    if (k === TYPE_STR) {
      if (typeof val !== "string") return false;
      continue;
    }
    if (k === TYPE_INT) {
      if (typeof val !== "number" || !Number.isInteger(val)) return false;
      continue;
    }
    if (k === TYPE_BOOL) {
      if (typeof val !== "boolean") return false;
      continue;
    }
    if (k === TYPE_FLOAT) {
      if (typeof val !== "number") return false;
      continue;
    }
    if (!isValidFastValue(val, check.spec, 0)) {
      return false;
    }
  }

  if (!allowExtra) {
    let totalKeys = 0;
    for (const _key in params) {
      totalKeys++;
    }
    if (totalKeys !== presentDeclaredCount) {
      return false;
    }
  }

  return true;
}

/**
 * Returns `true` when `nodes` can be rendered directly after `isValidFastParams`
 * without going through `Scope` option-narrowing or `{% include %}` resolution.
 */
export function canUseDirectValidatedRender(
  nodes: readonly Node[],
  optionParams: ReadonlySet<string>,
): boolean {
  const hasOpts = optionParams.size > 0;
  return !nodesRequireSlowRender(nodes, hasOpts);
}

function nodesRequireSlowRender(
  nodes: readonly Node[],
  hasOpts: boolean,
): boolean {
  for (const node of nodes) {
    switch (node.kind) {
      case NODE_INCLUDE:
        return true;
      case NODE_EXPR:
        if (hasOpts && node.expr.includes(`${FN_KIND}(`)) {
          return true;
        }
        break;
      case NODE_FOR:
        if (
          nodesRequireSlowRender(node.body, hasOpts) ||
          (node.elseBody !== undefined &&
            nodesRequireSlowRender(node.elseBody, hasOpts))
        ) {
          return true;
        }
        break;
      case NODE_IF:
        for (const branch of node.branches) {
          if (
            (hasOpts && branch.condition.includes(`${FN_KIND}(`)) ||
            nodesRequireSlowRender(branch.body, hasOpts)
          ) {
            return true;
          }
        }
        if (
          node.elseBody !== undefined &&
          nodesRequireSlowRender(node.elseBody, hasOpts)
        ) {
          return true;
        }
        break;
      case NODE_MATCH:
        if (hasOpts) return true;
        for (const arm of node.arms) {
          if (nodesRequireSlowRender(arm.body, hasOpts)) return true;
        }
        if (
          node.elseArm !== undefined &&
          nodesRequireSlowRender(node.elseArm, hasOpts)
        ) {
          return true;
        }
        if (
          node.inlineGuard !== undefined &&
          nodesRequireSlowRender(node.inlineGuard.body, hasOpts)
        ) {
          return true;
        }
        break;
      case NODE_PANIC:
        if (nodesRequireSlowRender(node.body, hasOpts)) return true;
        break;
      default:
        break;
    }
  }
  return false;
}
