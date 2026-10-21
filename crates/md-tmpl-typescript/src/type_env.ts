/**
 * Immutable, flow-sensitive type environment used by the compile-time
 * type checker: declarations, narrowings (`has()`, `{% match %}`), loop
 * bindings, and type aliases.
 */

import type { VarDecl, VarType } from "./frontmatter.js";
import {
  TYPE_STR,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_ENUM,
  TYPE_TMPL,
  TYPE_OPTION,
  TYPE_ALIAS,
  FILTER_SANITIZE,
  FILTER_SANITIZE_TOKENS,
  PIPE,
  DOT,
} from "./consts.js";
import { splitPipes } from "./evaluator.js";

/** Built-in functions that always return a scalar (displayable) value. */
const SCALAR_FUNCTIONS = new Set(["len", "idx", "kind", "has", TYPE_STR]);
const EMPTY_NARROWINGS: ReadonlyMap<string, VarType> = new Map();

/**
 * Immutable type environment that tracks declarations and narrowings.
 * Each scope level can override types for specific variable paths.
 */
export class TypeEnv {
  private readonly decls: readonly VarDecl[];
  private readonly narrowings: ReadonlyMap<string, VarType>;
  private readonly typeAliases?: ReadonlyMap<string, VarType>;

  constructor(
    decls: readonly VarDecl[],
    narrowings?: ReadonlyMap<string, VarType>,
    typeAliases?: ReadonlyMap<string, VarType>,
  ) {
    this.decls = decls;
    this.narrowings = narrowings ?? EMPTY_NARROWINGS;
    this.typeAliases = typeAliases;
  }

  /** Create a new env with an additional narrowing. */
  withNarrowing(path: string, ty: VarType): TypeEnv {
    const next = new Map(this.narrowings);
    next.set(path, ty);
    return new TypeEnv(this.decls, next, this.typeAliases);
  }

  /** Create a new env with an additional variable binding (e.g. for-loop). */
  withBinding(name: string, ty: VarType): TypeEnv {
    const nextDecls = [...this.decls, { name, varType: ty }];
    return new TypeEnv(nextDecls, this.narrowings, this.typeAliases);
  }

  /**
   * Resolve the type of a dotted path expression.
   *
   * Returns `undefined` if the path cannot be resolved (unknown variable,
   * unresolvable field). Only returns a concrete VarType when the full path
   * can be statically typed.
   */
  resolveExprType(expr: string): VarType | undefined {
    let pathStr = expr.trim();
    if (expr.includes(PIPE)) {
      const parts = splitPipes(expr);
      const base = parts[0];
      const filterStrs = parts.slice(1);
      const allTypePreserving =
        filterStrs.length > 0 &&
        filterStrs.every((s) => {
          const trimmed = s.trim();
          return (
            trimmed === FILTER_SANITIZE ||
            trimmed.startsWith("sanitize(") ||
            trimmed === FILTER_SANITIZE_TOKENS ||
            trimmed.startsWith("sanitize_tokens(") ||
            trimmed === "quarantine" ||
            trimmed.startsWith("quarantine(") ||
            trimmed.startsWith("limit(") ||
            trimmed === "limit" ||
            trimmed.startsWith("truncate(") ||
            trimmed === "truncate" ||
            trimmed.startsWith("truncate_middle(") ||
            trimmed === "truncate_middle"
          );
        });
      if (allTypePreserving && base !== undefined) {
        pathStr = base.trim();
      } else {
        return undefined;
      }
    }

    // Skip string/numeric literals
    const firstCode = pathStr.charCodeAt(0);
    if (
      firstCode === 34 /* " */ ||
      firstCode === 39 /* ' */ ||
      (firstCode >= 48 && firstCode <= 57)
    ) {
      return undefined;
    }

    // Skip built-in functions — they return scalars. Match a leading
    // `name(` function-call form and check it against the scalar builtins.
    if (pathStr.includes("(")) {
      const funcMatch = /^([a-zA-Z_][a-zA-Z0-9_]*)\s*\(/.exec(pathStr);
      const funcName = funcMatch?.[1];
      if (funcName !== undefined && SCALAR_FUNCTIONS.has(funcName)) {
        return undefined;
      }
    }

    // Check narrowings first (full path match)
    if (this.narrowings.size > 0) {
      const narrowed = this.narrowings.get(pathStr);
      if (narrowed !== undefined) return narrowed;
    }

    const dotIdx = pathStr.indexOf(DOT);
    if (dotIdx === -1) {
      return this.lookupRoot(pathStr);
    }

    if (!pathStr.includes(DOT, dotIdx + 1)) {
      const root = pathStr.slice(0, dotIdx);
      const rootType = this.lookupRoot(root);
      if (rootType === undefined) return undefined;
      const field = pathStr.slice(dotIdx + 1);
      const resolved = resolveFieldType(rootType, field);
      return resolved === undefined ? undefined : this.resolveAlias(resolved);
    }

    // Split multi-dot path: "user.address.city" → ["user", "address", "city"]
    const parts = pathStr.split(DOT);
    const root = parts[0];
    if (root === undefined) return undefined;

    // Check if a prefix is narrowed (e.g. "x" narrowed, resolving "x.field")
    const rootType = this.lookupRoot(root);
    if (rootType === undefined) return undefined;
    let currentType = rootType;

    // Walk remaining path segments
    for (let i = 1; i < parts.length; i++) {
      const field = parts[i];
      if (field === undefined) continue;
      const resolved = resolveFieldType(currentType, field);
      if (resolved === undefined) return undefined;
      currentType = this.resolveAlias(resolved);
    }

    return currentType;
  }

  /**
   * Type bound to a root name — a narrowing, then a declaration or loop
   * binding, then a type alias (enum namespaces) — with aliases followed.
   */
  lookupRoot(root: string): VarType | undefined {
    if (this.narrowings.size > 0) {
      const narrowed = this.narrowings.get(root);
      if (narrowed !== undefined) return this.resolveAlias(narrowed);
    }
    const decls = this.decls;
    for (let i = decls.length - 1; i >= 0; i--) {
      const d = decls[i];
      if (d?.name === root) {
        return this.resolveAlias(d.varType);
      }
    }
    const alias = this.typeAliases?.get(root);
    return alias === undefined ? undefined : this.resolveAlias(alias);
  }

  /** Narrowed type of a full dotted path (e.g. `task.cat` inside a match). */
  narrowingOf(path: string): VarType | undefined {
    if (this.narrowings.size === 0) return undefined;
    const ty = this.narrowings.get(path);
    return ty === undefined ? undefined : this.resolveAlias(ty);
  }

  /** Follow `types:` alias names to the aliased type. */
  resolveAlias(ty: VarType): VarType {
    let current = ty;
    while (current.kind === TYPE_ALIAS) {
      const aliased = this.typeAliases?.get(current.name);
      if (aliased === undefined) return current;
      current = aliased;
    }
    return current;
  }
}

/**
 * Resolve a field access on a type. Returns the field's type,
 * or undefined if the type doesn't support field access.
 */
export function resolveFieldType(
  ty: VarType,
  field: string,
): VarType | undefined {
  switch (ty.kind) {
    case TYPE_TMPL:
    case TYPE_STRUCT:
    case TYPE_LIST: {
      for (const f of ty.fields) {
        if (f.name === field) return f.varType;
      }
      return undefined;
    }
    case TYPE_ENUM: {
      // A field is accessible if ALL variants have it
      const allHave = ty.variants.every((v) =>
        v.fields.some((f) => f.name === field),
      );
      if (!allHave) return undefined;
      for (const v of ty.variants) {
        const f = v.fields.find((f) => f.name === field);
        if (f) return f.varType;
      }
      return undefined;
    }
    case TYPE_OPTION: {
      // Transparent access through option: option(struct(x = str)).x → str
      return resolveFieldType(ty.innerType, field);
    }
    default:
      return undefined;
  }
}
