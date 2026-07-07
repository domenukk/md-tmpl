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
import {
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_ENUM,
  TYPE_TMPL,
  TYPE_OPTION,
  TYPE_ALIAS,
  TYPE_SCALAR_LIST,
  TYPE_UNTYPED_LIST,
  PIPE,
  QUOTE_DOUBLE,
  QUOTE_SINGLE,
  OPTION_SOME,
  MATCH_DEFAULT,
  DOT,
  OP_IN_SPACED,
  NODE_EXPR,
  NODE_IF,
  NODE_MATCH,
  NODE_FOR,
} from "./consts.js";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Reserved keywords that cannot be used as parameter, constant, or type alias names. */
const RESERVED_NAMES: ReadonlySet<string> = new Set([
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_ENUM,
  TYPE_TMPL,
  TYPE_OPTION,
  "params",
]);

/** Built-in type names. A type alias cannot shadow any of these. */
const BUILTIN_TYPE_NAMES: ReadonlySet<string> = new Set([
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_ENUM,
  TYPE_TMPL,
  TYPE_OPTION,
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
    case TYPE_STR:
    case TYPE_BOOL:
    case TYPE_INT:
    case TYPE_FLOAT:
    case TYPE_UNTYPED_LIST:
      return true;
    case TYPE_ALIAS:
      return (b as typeof a).name === a.name;
    case TYPE_SCALAR_LIST:
      return varTypeEquals(a.elementType, (b as typeof a).elementType);
    case TYPE_LIST:
    case TYPE_STRUCT: {
      const bFields = (b as typeof a).fields;
      if (a.fields.length !== bFields.length) return false;
      return a.fields.every((f, i) => {
        const bf = bFields[i]!;
        return f.name === bf.name && varTypeEquals(f.varType, bf.varType);
      });
    }
    case TYPE_ENUM: {
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
    case TYPE_OPTION:
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
  // An alias reference (kind: TYPE_ALIAS) matches if the name is the same
  if (ty.kind === TYPE_ALIAS && aliasName && ty.name === aliasName) return true;

  switch (ty.kind) {
    case TYPE_SCALAR_LIST:
      return varTypeReferencesAlias(ty.elementType, aliasType, aliasName);
    case TYPE_LIST:
    case TYPE_STRUCT:
      return ty.fields.some((f) =>
        varTypeReferencesAlias(f.varType, aliasType, aliasName),
      );
    case TYPE_ENUM:
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

function throwErr(
  msg: string,
  loc?: { line: number; column?: number; snippet?: string },
): never {
  throw new TemplateSyntaxError(msg, loc?.line, loc?.column, loc?.snippet);
}

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
      throwErr(`reserved keyword used as name: '${decl.name}'`, decl.loc);
    }
  }
  for (const decl of fm.consts) {
    if (RESERVED_NAMES.has(decl.name)) {
      throwErr(`reserved keyword used as name: '${decl.name}'`, decl.loc);
    }
  }
  for (const decl of fm.env) {
    if (RESERVED_NAMES.has(decl.name)) {
      throwErr(`reserved keyword used as name: '${decl.name}'`, decl.loc);
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
        throwErr(`duplicate parameter name: '${decl.name}'`, decl.loc);
      }
      seenParams.add(decl.name);
    }
  }
  {
    const seenConsts = new Set<string>();
    for (const decl of fm.consts) {
      if (seenConsts.has(decl.name)) {
        throwErr(`duplicate constant name: '${decl.name}'`, decl.loc);
      }
      seenConsts.add(decl.name);
    }
  }
  {
    const seenEnv = new Set<string>();
    for (const decl of fm.env) {
      if (seenEnv.has(decl.name)) {
        throwErr(`duplicate env variable name: '${decl.name}'`, decl.loc);
      }
      seenEnv.add(decl.name);
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
        throwErr(
          `parameter name conflicts with constant name: '${param.name}' is declared as both a param and a constant`,
          param.loc,
        );
      }
    }
    for (const env of fm.env) {
      if (param.name === env.name) {
        throwErr(
          `parameter name conflicts with env variable name: '${param.name}' is declared as both a param and an env variable`,
          param.loc,
        );
      }
    }
  }
  for (const cst of fm.consts) {
    for (const env of fm.env) {
      if (cst.name === env.name) {
        throwErr(
          `constant name conflicts with env variable name: '${cst.name}' is declared as both a constant and an env variable`,
          cst.loc,
        );
      }
    }
  }

  // ── R1: PascalCase param/const vs type alias collision ──────────────
  const allDecls: readonly VarDecl[] = [...fm.params, ...fm.consts, ...fm.env];
  for (const decl of allDecls) {
    const declPascal = toPascalCase(decl.name);
    for (const [aliasName, aliasType] of fm.typeAliases) {
      if (declPascal === aliasName) {
        // Exception: if the declaration's type exactly matches the alias type
        // OR if the declaration uses this alias by name, allow it.
        if (varTypeEquals(decl.varType, aliasType)) {
          continue;
        }
        if (
          decl.varType.kind === TYPE_ALIAS &&
          decl.varType.name === aliasName
        ) {
          continue;
        }
        // Enum type aliases are auto-injected as constants, so a user-defined
        // constant with the same name simply takes priority — not a conflict.
        if (aliasType.kind === TYPE_ENUM) {
          continue;
        }
        const label = fm.consts.some((c) => c.name === decl.name)
          ? "constant"
          : "param";
        throwErr(
          `type alias name conflicts with parameter name (PascalCase collision): ${label} '${decl.name}' (PascalCase: '${declPascal}') conflicts with type alias '${aliasName}'`,
          decl.loc,
        );
      }
    }
  }

  // ── R2: Type alias shadows import stem ──────────────────────────────
  for (const imp of fm.imports) {
    for (const aliasName of fm.typeAliases.keys()) {
      if (aliasName === imp.stem) {
        throwErr(
          `type alias shadows import alias: '${aliasName}' shadows '${imp.stem}'`,
          imp.loc,
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
        throwErr(
          `parameter name (PascalCase) shadows import alias: ${label} '${decl.name}' (PascalCase: '${declPascal}') shadows import '${imp.stem}'`,
          decl.loc,
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
      if (aliasType.kind === TYPE_ENUM) continue;
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
      throwErr(
        `inline template name conflicts with import stem: '${imp.stem}'`,
        imp.loc,
      );
    }
  }

  // ── Rule 10: Param/const ↔ inline template name collision ─────────
  for (const decl of fm.params) {
    if (inlineTemplateNames.has(decl.name)) {
      throwErr(
        `inline template name conflicts with parameter name: '${decl.name}'`,
        decl.loc,
      );
    }
  }
  for (const decl of fm.consts) {
    if (inlineTemplateNames.has(decl.name)) {
      throwErr(
        `inline template name conflicts with constant name: '${decl.name}'`,
        decl.loc,
      );
    }
  }
  for (const decl of fm.env) {
    if (inlineTemplateNames.has(decl.name)) {
      throwErr(
        `inline template name conflicts with env variable name: '${decl.name}'`,
        decl.loc,
      );
    }
  }

  // ── Rule 11: For-loop binding shadowing ───────────────────────────
  const declaredNames = new Set<string>([
    ...fm.params.map((d) => d.name),
    ...fm.consts.map((d) => d.name),
    ...fm.env.map((d) => d.name),
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
  if (ty.kind === TYPE_ALIAS) return true;
  return (
    ty.kind === TYPE_STR ||
    ty.kind === TYPE_INT ||
    ty.kind === TYPE_FLOAT ||
    ty.kind === TYPE_BOOL
  );
}

/** Built-in functions that always return a scalar (displayable) value. */
const SCALAR_FUNCTIONS = new Set(["len", "idx", "kind", "has", TYPE_STR]);

/** Human-readable label for a VarType. */
function varTypeLabel(ty: VarType): string {
  switch (ty.kind) {
    case TYPE_LIST:
    case TYPE_SCALAR_LIST:
    case TYPE_UNTYPED_LIST:
      return TYPE_LIST;
    case TYPE_STRUCT:
      return TYPE_STRUCT;
    case TYPE_ENUM:
      return TYPE_ENUM;
    case TYPE_OPTION:
      return TYPE_OPTION;
    default:
      return ty.kind;
  }
}

/** Hint message for non-displayable types. */
function displayHint(ty: VarType): string {
  switch (ty.kind) {
    case TYPE_LIST:
    case TYPE_SCALAR_LIST:
    case TYPE_UNTYPED_LIST:
      return "use {% for %} to iterate, or | join()";
    case TYPE_STRUCT:
      return "access fields with dot notation, e.g. {{ x.field }}";
    case TYPE_ENUM:
      return "use kind(x) for the variant name, or {% match %}";
    case TYPE_OPTION:
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
  private readonly typeAliases?: ReadonlyMap<string, VarType>;

  constructor(
    decls: readonly VarDecl[],
    narrowings?: ReadonlyMap<string, VarType>,
    typeAliases?: ReadonlyMap<string, VarType>,
  ) {
    this.decls = decls;
    this.narrowings = narrowings ?? new Map();
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
    // If filters are applied, skip — filters may transform the type.
    if (expr.indexOf(PIPE) >= 0) return undefined;

    const pathStr = expr.trim();

    // Skip string/numeric literals
    if (
      pathStr.startsWith(QUOTE_DOUBLE) ||
      pathStr.startsWith(QUOTE_SINGLE) ||
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
    const parts = pathStr.split(DOT);
    const root = parts[0]!;

    // Check if a prefix is narrowed (e.g. "x" narrowed, resolving "x.field")
    const rootNarrowed = this.narrowings.get(root);
    let currentType: VarType;
    if (rootNarrowed !== undefined) {
      currentType = rootNarrowed;
    } else {
      const rootDecl = this.decls.find((d) => d.name === root);
      if (!rootDecl) {
        const alias = this.typeAliases?.get(root);
        if (!alias) return undefined;
        currentType = alias;
      } else {
        currentType = rootDecl.varType;
      }
    }

    while (currentType.kind === TYPE_ALIAS && this.typeAliases) {
      const aliasType = this.typeAliases.get(currentType.name);
      if (!aliasType) break;
      currentType = aliasType;
    }

    // Walk remaining path segments
    for (let i = 1; i < parts.length; i++) {
      const field = parts[i]!;
      const resolved = resolveFieldType(currentType, field);
      if (resolved === undefined) return undefined;
      currentType = resolved;
      while (currentType.kind === TYPE_ALIAS && this.typeAliases) {
        const aliasType = this.typeAliases.get(currentType.name);
        if (!aliasType) break;
        currentType = aliasType;
      }
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
    case TYPE_STRUCT:
    case TYPE_LIST: {
      const fieldDecl = ty.fields.find((f) => f.name === field);
      return fieldDecl?.varType;
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

// ---------------------------------------------------------------------------
// Flow-sensitive AST walker
// ---------------------------------------------------------------------------

/**
 * Extract has() narrowing from a condition string.
 *
 * If the condition is `has(path)`, and `path` resolves to `option(T)` in the
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

  if (ty.kind === TYPE_OPTION) {
    return [path, ty.innerType];
  }

  // Legacy enum-based option
  if (ty.kind === TYPE_ENUM && ty.isOption) {
    const someVariant = ty.variants.find((v) => v.name === OPTION_SOME);
    if (someVariant && someVariant.fields.length === 1) {
      return [path, someVariant.fields[0]!.varType];
    }
  }

  return undefined;
}

interface ValidationError {
  message: string;
  loc?: import("./parser.js").SourceLocation;
}

/** Validate static condition checks at compile time (e.g., literal in kinds(Enum)). */
function validateStaticCondition(
  condition: string,
  env: TypeEnv,
  errors: ValidationError[],
  loc?: import("./parser.js").SourceLocation,
): void {
  const trimmed = condition.trim();
  const inIdx = trimmed.indexOf(OP_IN_SPACED);
  if (inIdx !== -1) {
    const left = trimmed.slice(0, inIdx).trim();
    const right = trimmed.slice(inIdx + OP_IN_SPACED.length).trim();
    const kindsMatch = /^kinds\(\s*([a-zA-Z0-9_-]+)\s*\)$/.exec(right);
    if (kindsMatch) {
      const enumName = kindsMatch[1]!;
      const enumType = env.resolveExprType(enumName);
      if (enumType?.kind === TYPE_ENUM) {
        if (
          (left.startsWith(QUOTE_DOUBLE) && left.endsWith(QUOTE_DOUBLE)) ||
          (left.startsWith(QUOTE_SINGLE) && left.endsWith(QUOTE_SINGLE))
        ) {
          const strVal = left.slice(1, -1);
          if (!enumType.variants.some((v) => v.name === strVal)) {
            errors.push({
              message: `static string "${strVal}" is not a valid variant of enum '${enumName}'`,
              loc,
            });
          }
        }
      }
    }
  }
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
  errors: ValidationError[],
): void {
  for (const node of nodes) {
    switch (node.kind) {
      case NODE_EXPR: {
        const resolvedType = env.resolveExprType(node.expr);
        if (resolvedType === undefined) continue;
        if (!isDisplayableType(resolvedType)) {
          const hint = displayHint(resolvedType);
          errors.push({
            message: `'${node.expr.trim()}': cannot display value of type ${varTypeLabel(resolvedType)} — ${hint}`,
            loc: node.loc,
          });
        }
        break;
      }

      case NODE_IF: {
        for (const branch of node.branches) {
          validateStaticCondition(branch.condition, env, errors, node.loc);
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

      case NODE_MATCH: {
        const exprPath = node.expr.trim();
        const exprType = env.resolveExprType(exprPath);

        if (exprType?.kind === TYPE_ENUM) {
          // Narrow each arm to only the matched variant(s)
          for (const arm of node.arms) {
            const matchedVariants = exprType.variants.filter((v) =>
              arm.variants.includes(v.name),
            );
            if (matchedVariants.length > 0) {
              const narrowedType: VarType = {
                kind: TYPE_ENUM,
                variants: matchedVariants,
              };
              const narrowedEnv = env.withNarrowing(exprPath, narrowedType);
              walkNodesWithNarrowing(arm.body, narrowedEnv, errors);
            } else if (
              arm.variants.length === 1 &&
              arm.variants[0] === MATCH_DEFAULT
            ) {
              // Default arm — no narrowing
              walkNodesWithNarrowing(arm.body, env, errors);
            } else {
              walkNodesWithNarrowing(arm.body, env, errors);
            }
          }
          if (
            node.arms.length > 1 &&
            !node.elseArm &&
            !node.arms.some((a) => a.variants.includes(MATCH_DEFAULT))
          ) {
            const covered = new Set<string>();
            for (const arm of node.arms) {
              for (const v of arm.variants) covered.add(v);
            }
            const missing = exprType.variants
              .filter((v) => !covered.has(v.name))
              .map((v) => v.name);
            if (missing.length > 0) {
              const cases = missing.map((m) => `{% case ${m} %}`).join(" ");
              const suggestion =
                missing.length > 1
                  ? `Try adding explicit arms: ${cases} or combined arm: {% case ${missing.join(" | ")} %}`
                  : `Try adding explicit arm: ${cases}`;
              errors.push({
                message: `match on '${exprPath}': non-exhaustive — missing variant(s): ${missing.join(", ")}. ${suggestion}`,
                loc: node.loc,
              });
            }
          }
        } else if (exprType?.kind === TYPE_OPTION) {
          for (const arm of node.arms) {
            if (arm.variants.includes(OPTION_SOME)) {
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
          if (exprType?.kind === TYPE_ENUM) {
            const matchedVariants = exprType.variants.filter(
              (v) => v.name === node.inlineGuard!.variant,
            );
            if (matchedVariants.length > 0) {
              const narrowedType: VarType = {
                kind: TYPE_ENUM,
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

      case NODE_FOR: {
        // Resolve the iterator expression type to determine element type
        const iterType = env.resolveExprType(node.iterExpr);
        if (iterType) {
          let elementType: VarType | undefined;
          if (iterType.kind === TYPE_LIST) {
            // list(name = str, ...) → struct(name = str, ...)
            elementType = { kind: TYPE_STRUCT, fields: iterType.fields };
          } else if (iterType.kind === TYPE_SCALAR_LIST) {
            elementType = iterType.elementType;
          } else if (iterType.kind === TYPE_UNTYPED_LIST) {
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
 * - `{% if has(x) %}` — narrows `option(T)` to `T` in the true branch
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
  consts?: readonly VarDecl[],
  typeAliases?: ReadonlyMap<string, VarType>,
): void {
  const allDecls = consts ? [...declarations, ...consts] : declarations;
  const env = new TypeEnv(allDecls, undefined, typeAliases);
  const errors: ValidationError[] = [];

  walkNodesWithNarrowing(nodes, env, errors);

  if (errors.length > 0) {
    const err = errors[0]!;
    throw new TemplateSyntaxError(
      err.message,
      err.loc?.line,
      err.loc?.column,
      err.loc?.snippet,
    );
  }
}
