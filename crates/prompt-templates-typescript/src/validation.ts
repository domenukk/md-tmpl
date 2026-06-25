/**
 * Validation rules for parsed frontmatter.
 *
 * Checks collision rules between params, type aliases, and imports,
 * matching the Rust crate's `validation.rs` logic.
 *
 * Rules checked:
 * - Reserved keywords cannot be used as param, const, or type alias names.
 * - Duplicate param names, const names, and type alias names are rejected.
 * - A parameter and a constant cannot share the same name.
 * - PascalCase param/const name vs type alias collision (with type-match exception).
 * - Type alias name cannot shadow an import stem.
 * - Param/const PascalCase name cannot shadow an import stem.
 * - Type aliases cannot shadow built-in type names.
 * - Unused type aliases are rejected unless `allow_unused: true`.
 *
 * @module
 */

import { TemplateSyntaxError } from "./errors.js";
import type { Frontmatter, VarDecl, VarType } from "./frontmatter.js";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Reserved keywords that cannot be used as parameter, constant, or type alias names. */
const RESERVED_NAMES: ReadonlySet<string> = new Set([
  "str",
  "bool",
  "int",
  "float",
  "list",
  "struct",
  "enum",
  "tmpl",
  "option",
  "params",
]);

/** Built-in type names. A type alias cannot shadow any of these. */
const BUILTIN_TYPE_NAMES: ReadonlySet<string> = new Set([
  "str",
  "bool",
  "int",
  "float",
  "list",
  "struct",
  "enum",
  "tmpl",
  "option",
]);

// ---------------------------------------------------------------------------
// PascalCase conversion
// ---------------------------------------------------------------------------

/**
 * Convert a `snake_case`, `kebab-case`, or other string to `PascalCase`.
 *
 * Splits on `_` and `-`, capitalises the first character of each segment,
 * and preserves the remaining characters.
 *
 * @example
 * ```ts
 * toPascalCase("code_review") // → "CodeReview"
 * toPascalCase("task-report")  // → "TaskReport"
 * ```
 */
export function toPascalCase(s: string): string {
  return s
    .split(/[_-]/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}

// ---------------------------------------------------------------------------
// VarType equality (structural)
// ---------------------------------------------------------------------------

/** Deep structural equality check for two VarType values. */
function varTypeEquals(a: VarType, b: VarType): boolean {
  if (a.kind !== b.kind) return false;

  switch (a.kind) {
    case "str":
    case "bool":
    case "int":
    case "float":
    case "untyped_list":
      return true;
    case "alias":
      return (b as typeof a).name === a.name;
    case "scalar_list":
      return varTypeEquals(a.elementType, (b as typeof a).elementType);
    case "list":
    case "struct": {
      const bFields = (b as typeof a).fields;
      if (a.fields.length !== bFields.length) return false;
      return a.fields.every((f, i) => {
        const bf = bFields[i]!;
        return f.name === bf.name && varTypeEquals(f.varType, bf.varType);
      });
    }
    case "enum": {
      const bVariants = (b as typeof a).variants;
      if (a.variants.length !== bVariants.length) return false;
      return a.variants.every((v, i) => {
        const bv = bVariants[i]!;
        if (v.name !== bv.name || v.fields.length !== bv.fields.length)
          return false;
        return v.fields.every((f, j) => {
          const bf = bv.fields[j]!;
          return f.name === bf.name && varTypeEquals(f.varType, bf.varType);
        });
      });
    }
    case "option":
      return varTypeEquals(a.innerType, (b as typeof a).innerType);
    default: {
      const _exhaustive: never = a;
      throw new Error(
        `unexpected VarType kind: ${(_exhaustive as VarType).kind}`,
      );
    }
  }
}

/**
 * Check if a VarType references a specific type alias.
 * Matches by structural equality OR by alias name reference.
 * Recursively descends into list, struct, and enum fields.
 */
function varTypeReferencesAlias(
  ty: VarType,
  aliasType: VarType,
  aliasName?: string,
): boolean {
  if (varTypeEquals(ty, aliasType)) return true;
  // An alias reference (kind: "alias") matches if the name is the same
  if (ty.kind === "alias" && aliasName && ty.name === aliasName) return true;

  switch (ty.kind) {
    case "scalar_list":
      return varTypeReferencesAlias(ty.elementType, aliasType, aliasName);
    case "list":
    case "struct":
      return ty.fields.some((f) =>
        varTypeReferencesAlias(f.varType, aliasType, aliasName),
      );
    case "enum":
      return ty.variants.some((v) =>
        v.fields.some((f) =>
          varTypeReferencesAlias(f.varType, aliasType, aliasName),
        ),
      );
    default:
      return false;
  }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/**
 * Validate frontmatter collision and naming rules.
 *
 * Throws `TemplateSyntaxError` for any violation.
 *
 * @param fm - Parsed frontmatter to validate.
 * @throws {TemplateSyntaxError} On any naming collision or rule violation.
 */
export function validateFrontmatter(fm: Frontmatter): void {
  // ── Reserved keyword check ──────────────────────────────────────────
  for (const decl of fm.params) {
    if (RESERVED_NAMES.has(decl.name)) {
      throw new TemplateSyntaxError(
        `reserved keyword used as name: '${decl.name}'`,
      );
    }
  }
  for (const decl of fm.consts) {
    if (RESERVED_NAMES.has(decl.name)) {
      throw new TemplateSyntaxError(
        `reserved keyword used as name: '${decl.name}'`,
      );
    }
  }
  for (const aliasName of fm.typeAliases.keys()) {
    if (RESERVED_NAMES.has(aliasName)) {
      throw new TemplateSyntaxError(
        `reserved keyword used as name: '${aliasName}'`,
      );
    }
  }

  // ── Duplicate name check ────────────────────────────────────────────
  {
    const seenParams = new Set<string>();
    for (const decl of fm.params) {
      if (seenParams.has(decl.name)) {
        throw new TemplateSyntaxError(
          `duplicate parameter name: '${decl.name}'`,
        );
      }
      seenParams.add(decl.name);
    }
  }
  {
    const seenConsts = new Set<string>();
    for (const decl of fm.consts) {
      if (seenConsts.has(decl.name)) {
        throw new TemplateSyntaxError(
          `duplicate constant name: '${decl.name}'`,
        );
      }
      seenConsts.add(decl.name);
    }
  }
  // Note: duplicate type aliases are caught during parsing in frontmatter.ts
  // since Map.set overwrites. We check here for consistency.
  // (The Rust crate checks during parsing too, but we can't easily detect
  // duplicates from a Map after parsing. We rely on parse-time checks.)

  // ── Built-in shadowing check ────────────────────────────────────────
  for (const aliasName of fm.typeAliases.keys()) {
    if (BUILTIN_TYPE_NAMES.has(aliasName.toLowerCase())) {
      throw new TemplateSyntaxError(
        `type alias shadows built-in type name: '${aliasName}'`,
      );
    }
  }

  // ── Param ↔ const conflict (exact match) ────────────────────────────
  for (const param of fm.params) {
    for (const cst of fm.consts) {
      if (param.name === cst.name) {
        throw new TemplateSyntaxError(
          `parameter name conflicts with constant name: '${param.name}' is declared as both a param and a constant`,
        );
      }
    }
  }

  // ── R1: PascalCase param/const vs type alias collision ──────────────
  const allDecls: readonly VarDecl[] = [...fm.params, ...fm.consts];
  for (const decl of allDecls) {
    const declPascal = toPascalCase(decl.name);
    for (const [aliasName, aliasType] of fm.typeAliases) {
      if (declPascal === aliasName) {
        // Exception: if the declaration's type exactly matches the alias type
        // OR if the declaration uses this alias by name, allow it.
        if (varTypeEquals(decl.varType, aliasType)) {
          continue;
        }
        if (decl.varType.kind === "alias" && decl.varType.name === aliasName) {
          continue;
        }
        // Enum type aliases are auto-injected as constants, so a user-defined
        // constant with the same name simply takes priority — not a conflict.
        if (aliasType.kind === "enum") {
          continue;
        }
        const label = fm.consts.some((c) => c.name === decl.name)
          ? "constant"
          : "param";
        throw new TemplateSyntaxError(
          `type alias name conflicts with parameter name (PascalCase collision): ${label} '${decl.name}' (PascalCase: '${declPascal}') conflicts with type alias '${aliasName}'`,
        );
      }
    }
  }

  // ── R2: Type alias shadows import stem ──────────────────────────────
  for (const imp of fm.imports) {
    for (const aliasName of fm.typeAliases.keys()) {
      if (aliasName === imp.stem) {
        throw new TemplateSyntaxError(
          `type alias shadows import alias: '${aliasName}' shadows '${imp.stem}'`,
        );
      }
    }
  }

  // ── R2b: Param/const PascalCase name shadows import stem ────────────
  for (const imp of fm.imports) {
    for (const decl of allDecls) {
      const declPascal = toPascalCase(decl.name);
      if (declPascal === imp.stem) {
        const label = fm.consts.some((c) => c.name === decl.name)
          ? "constant"
          : "param";
        throw new TemplateSyntaxError(
          `parameter name (PascalCase) shadows import alias: ${label} '${decl.name}' (PascalCase: '${declPascal}') shadows import '${imp.stem}'`,
        );
      }
    }
  }

  // ── R4: Unused type aliases ─────────────────────────────────────────
  if (
    !fm.allowUnused &&
    fm.typeAliases.size > 0 &&
    (fm.params.length > 0 || fm.consts.length > 0)
  ) {
    for (const [aliasName, aliasType] of fm.typeAliases) {
      // Enum types are always used — they're auto-injected as constants.
      if (aliasType.kind === "enum") continue;
      const isUsed = allDecls.some((d) =>
        varTypeReferencesAlias(d.varType, aliasType, aliasName),
      );
      if (!isUsed) {
        throw new TemplateSyntaxError(`unused type alias: '${aliasName}'`);
      }
    }
  }
}

/**
 * Validate body-level naming collisions (rules 9-11).
 *
 * These checks require knowledge of inline template names and for-loop
 * bindings, which are only available after parsing the template body.
 *
 * @param fm - Parsed frontmatter.
 * @param inlineTemplateNames - Names of inline templates defined in the body.
 * @param forBindings - For-loop binding names used in the body.
 */
export function validateBodyCollisions(
  fm: Frontmatter,
  inlineTemplateNames: ReadonlySet<string>,
  forBindings: ReadonlySet<string>,
): void {
  // ── Rule 9: Import stem ↔ inline template name collision ──────────
  for (const imp of fm.imports) {
    if (inlineTemplateNames.has(imp.stem)) {
      throw new TemplateSyntaxError(
        `inline template name conflicts with import stem: '${imp.stem}'`,
      );
    }
  }

  // ── Rule 10: Param/const ↔ inline template name collision ─────────
  for (const decl of fm.params) {
    if (inlineTemplateNames.has(decl.name)) {
      throw new TemplateSyntaxError(
        `inline template name conflicts with parameter name: '${decl.name}'`,
      );
    }
  }
  for (const decl of fm.consts) {
    if (inlineTemplateNames.has(decl.name)) {
      throw new TemplateSyntaxError(
        `inline template name conflicts with constant name: '${decl.name}'`,
      );
    }
  }

  // ── Rule 11: For-loop binding shadowing ───────────────────────────
  const declaredNames = new Set<string>([
    ...fm.params.map((d) => d.name),
    ...fm.consts.map((d) => d.name),
    ...fm.imports.map((i) => i.stem),
  ]);
  for (const binding of forBindings) {
    if (declaredNames.has(binding)) {
      throw new TemplateSyntaxError(
        `for-loop binding '${binding}' shadows a declared name (param, const, or import stem)`,
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Compile-time displayability check (with flow-sensitive narrowing)
// ---------------------------------------------------------------------------

/** Scalar types that can appear in {{ expr }} interpolations. */
function isDisplayableType(ty: VarType): boolean {
  // Alias types can't be resolved here (we'd need the full type alias map).
  // Conservatively allow them — the resolved type may be scalar.
  if (ty.kind === "alias") return true;
  return (
    ty.kind === "str" ||
    ty.kind === "int" ||
    ty.kind === "float" ||
    ty.kind === "bool"
  );
}

/** Built-in functions that always return a scalar (displayable) value. */
const SCALAR_FUNCTIONS = new Set(["len", "idx", "kind", "has", "str"]);

/** Human-readable label for a VarType. */
function varTypeLabel(ty: VarType): string {
  switch (ty.kind) {
    case "list":
    case "scalar_list":
    case "untyped_list":
      return "list";
    case "struct":
      return "struct";
    case "enum":
      return "enum";
    case "option":
      return "option";
    default:
      return ty.kind;
  }
}

/** Hint message for non-displayable types. */
function displayHint(ty: VarType): string {
  switch (ty.kind) {
    case "list":
    case "scalar_list":
    case "untyped_list":
      return "use {% for %} to iterate, or | join()";
    case "struct":
      return "access fields with dot notation, e.g. {{ x.field }}";
    case "enum":
      return "use kind(x) for the variant name, or {% match %}";
    case "option":
      return "use {% if has(x) %} to unwrap, or {% match %}";
    default:
      return "only str, int, float, bool can be displayed";
  }
}

// ---------------------------------------------------------------------------
// Type environment for flow-sensitive narrowing
// ---------------------------------------------------------------------------

/**
 * Immutable type environment that tracks declarations and narrowings.
 * Each scope level can override types for specific variable paths.
 */
class TypeEnv {
  private readonly decls: readonly VarDecl[];
  private readonly narrowings: ReadonlyMap<string, VarType>;

  constructor(
    decls: readonly VarDecl[],
    narrowings?: ReadonlyMap<string, VarType>,
  ) {
    this.decls = decls;
    this.narrowings = narrowings ?? new Map();
  }

  /** Create a new env with an additional narrowing. */
  withNarrowing(path: string, ty: VarType): TypeEnv {
    const next = new Map(this.narrowings);
    next.set(path, ty);
    return new TypeEnv(this.decls, next);
  }

  /** Create a new env with an additional variable binding (e.g. for-loop). */
  withBinding(name: string, ty: VarType): TypeEnv {
    const nextDecls = [...this.decls, { name, varType: ty }];
    return new TypeEnv(nextDecls, this.narrowings);
  }

  /**
   * Resolve the type of a dotted path expression.
   *
   * Returns `undefined` if the path cannot be resolved (unknown variable,
   * unresolvable field). Only returns a concrete VarType when the full path
   * can be statically typed.
   */
  resolveExprType(expr: string): VarType | undefined {
    // If filters are applied, skip — filters may transform the type.
    if (expr.indexOf("|") >= 0) return undefined;

    const pathStr = expr.trim();

    // Skip string/numeric literals
    if (
      pathStr.startsWith('"') ||
      pathStr.startsWith("'") ||
      /^\d/.test(pathStr)
    ) {
      return undefined;
    }

    // Skip built-in functions — they return scalars
    const funcMatch = pathStr.match(/^(\w+)\s*\(/);
    if (funcMatch && SCALAR_FUNCTIONS.has(funcMatch[1]!)) {
      return undefined;
    }

    // Check narrowings first (full path match)
    const narrowed = this.narrowings.get(pathStr);
    if (narrowed !== undefined) return narrowed;

    // Split dotted path: "user.address.city" → ["user", "address", "city"]
    const parts = pathStr.split(".");
    const root = parts[0]!;

    // Check if a prefix is narrowed (e.g. "x" narrowed, resolving "x.field")
    const rootNarrowed = this.narrowings.get(root);
    let currentType: VarType;
    if (rootNarrowed !== undefined) {
      currentType = rootNarrowed;
    } else {
      const rootDecl = this.decls.find((d) => d.name === root);
      if (!rootDecl) return undefined;
      currentType = rootDecl.varType;
    }

    // Walk remaining path segments
    for (let i = 1; i < parts.length; i++) {
      const field = parts[i]!;
      const resolved = resolveFieldType(currentType, field);
      if (resolved === undefined) return undefined;
      currentType = resolved;
    }

    return currentType;
  }
}

/**
 * Resolve a field access on a type. Returns the field's type,
 * or undefined if the type doesn't support field access.
 */
function resolveFieldType(ty: VarType, field: string): VarType | undefined {
  switch (ty.kind) {
    case "struct":
    case "list": {
      const fieldDecl = ty.fields.find((f) => f.name === field);
      return fieldDecl?.varType;
    }
    case "enum": {
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
    case "option": {
      // Transparent access through option: option<struct<x = str>>.x → str
      return resolveFieldType(ty.innerType, field);
    }
    default:
      return undefined;
  }
}

// ---------------------------------------------------------------------------
// Flow-sensitive AST walker
// ---------------------------------------------------------------------------

/**
 * Extract has() narrowing from a condition string.
 *
 * If the condition is `has(path)`, and `path` resolves to `option<T>` in the
 * current environment, returns `[path, T]` — the narrowed type.
 */
function extractHasNarrowing(
  condition: string,
  env: TypeEnv,
): [string, VarType] | undefined {
  const trimmed = condition.trim();
  const match = /^has\(\s*([^)]+?)\s*\)$/.exec(trimmed);
  if (!match) return undefined;

  const path = match[1]!;
  const ty = env.resolveExprType(path);
  if (!ty) return undefined;

  if (ty.kind === "option") {
    return [path, ty.innerType];
  }

  // Legacy enum-based option
  if (ty.kind === "enum" && ty.isOption) {
    const someVariant = ty.variants.find((v) => v.name === "Some");
    if (someVariant && someVariant.fields.length === 1) {
      return [path, someVariant.fields[0]!.varType];
    }
  }

  return undefined;
}

/**
 * Walk AST nodes with flow-sensitive narrowing, collecting displayability errors.
 *
 * This is the core of the compile-time type checker for the TS backend.
 * It mirrors the Rust `walk_segments` + `validate_compiled_path` logic.
 */
function walkNodesWithNarrowing(
  nodes: readonly import("./parser.js").Node[],
  env: TypeEnv,
  errors: string[],
): void {
  for (const node of nodes) {
    switch (node.kind) {
      case "expr": {
        const resolvedType = env.resolveExprType(node.expr);
        if (resolvedType === undefined) continue;
        if (!isDisplayableType(resolvedType)) {
          const hint = displayHint(resolvedType);
          errors.push(
            `'${node.expr.trim()}': cannot display value of type ${varTypeLabel(resolvedType)} — ${hint}`,
          );
        }
        break;
      }

      case "if": {
        for (const branch of node.branches) {
          // Check for has() narrowing
          const narrowing = extractHasNarrowing(branch.condition, env);
          if (narrowing) {
            const [path, narrowedType] = narrowing;
            const narrowedEnv = env.withNarrowing(path, narrowedType);
            walkNodesWithNarrowing(branch.body, narrowedEnv, errors);
          } else {
            walkNodesWithNarrowing(branch.body, env, errors);
          }
        }
        if (node.elseBody) {
          walkNodesWithNarrowing(node.elseBody, env, errors);
        }
        break;
      }

      case "match": {
        const exprPath = node.expr.trim();
        const exprType = env.resolveExprType(exprPath);

        if (exprType?.kind === "enum") {
          // Narrow each arm to only the matched variant(s)
          for (const arm of node.arms) {
            const matchedVariants = exprType.variants.filter((v) =>
              arm.variants.includes(v.name),
            );
            if (matchedVariants.length > 0) {
              const narrowedType: VarType = {
                kind: "enum",
                variants: matchedVariants,
              };
              const narrowedEnv = env.withNarrowing(exprPath, narrowedType);
              walkNodesWithNarrowing(arm.body, narrowedEnv, errors);
            } else if (arm.variants.length === 1 && arm.variants[0] === "_") {
              // Default arm — no narrowing
              walkNodesWithNarrowing(arm.body, env, errors);
            } else {
              walkNodesWithNarrowing(arm.body, env, errors);
            }
          }
        } else if (exprType?.kind === "option") {
          for (const arm of node.arms) {
            if (arm.variants.includes("Some")) {
              // Narrow option to inner type
              const narrowedEnv = env.withNarrowing(
                exprPath,
                exprType.innerType,
              );
              walkNodesWithNarrowing(arm.body, narrowedEnv, errors);
            } else {
              walkNodesWithNarrowing(arm.body, env, errors);
            }
          }
        } else {
          // Can't resolve match expression type — walk arms without narrowing
          for (const arm of node.arms) {
            walkNodesWithNarrowing(arm.body, env, errors);
          }
        }

        if (node.elseArm) {
          walkNodesWithNarrowing(node.elseArm, env, errors);
        }

        // Inline guard
        if (node.inlineGuard) {
          if (exprType?.kind === "enum") {
            const matchedVariants = exprType.variants.filter(
              (v) => v.name === node.inlineGuard!.variant,
            );
            if (matchedVariants.length > 0) {
              const narrowedType: VarType = {
                kind: "enum",
                variants: matchedVariants,
              };
              const narrowedEnv = env.withNarrowing(exprPath, narrowedType);
              walkNodesWithNarrowing(
                node.inlineGuard.body,
                narrowedEnv,
                errors,
              );
            } else {
              walkNodesWithNarrowing(node.inlineGuard.body, env, errors);
            }
          } else {
            walkNodesWithNarrowing(node.inlineGuard.body, env, errors);
          }
        }
        break;
      }

      case "for": {
        // Resolve the iterator expression type to determine element type
        const iterType = env.resolveExprType(node.iterExpr);
        if (iterType) {
          let elementType: VarType | undefined;
          if (iterType.kind === "list") {
            // list<name = str, ...> → struct<name = str, ...>
            elementType = { kind: "struct", fields: iterType.fields };
          } else if (iterType.kind === "scalar_list") {
            elementType = iterType.elementType;
          } else if (iterType.kind === "untyped_list") {
            // Can't determine element type
            elementType = undefined;
          }
          if (elementType) {
            const forEnv = env.withBinding(node.binding, elementType);
            walkNodesWithNarrowing(node.body, forEnv, errors);
          } else {
            walkNodesWithNarrowing(node.body, env, errors);
          }
        } else {
          walkNodesWithNarrowing(node.body, env, errors);
        }

        if (node.elseBody) {
          walkNodesWithNarrowing(node.elseBody, env, errors);
        }
        break;
      }

      default:
        // text, comment, raw, include, tmpl — no expressions to check
        break;
    }
  }
}

/**
 * Validate that all `{{ expr }}` interpolations resolve to displayable
 * (scalar) types, with proper flow-sensitive narrowing through:
 *
 * - `{% if has(x) %}` — narrows `option<T>` to `T` in the true branch
 * - `{% match x %}{% case V %}` — narrows enum to matched variant
 * - `{% for item in list %}` — introduces element binding
 *
 * This is a compile-time check — called during `fromSource()`.
 *
 * @param nodes - Parsed AST nodes from the template body.
 * @param declarations - Parameter declarations from frontmatter.
 * @throws {TemplateSyntaxError} If an expression resolves to a non-displayable type.
 */
export function validateDisplayability(
  nodes: readonly import("./parser.js").Node[],
  declarations: readonly VarDecl[],
): void {
  const env = new TypeEnv(declarations);
  const errors: string[] = [];

  walkNodesWithNarrowing(nodes, env, errors);

  if (errors.length > 0) {
    throw new TemplateSyntaxError(errors[0]!);
  }
}
