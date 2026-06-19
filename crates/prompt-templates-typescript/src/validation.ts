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
