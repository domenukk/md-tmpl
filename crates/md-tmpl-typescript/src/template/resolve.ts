/**
 * Frontmatter resolution: enum constants, type aliases, imported
 * consts, and compile-time environment declarations.
 *
 * @module
 */

import { TemplateError, TemplateSyntaxError } from "../errors.js";
import {
  type Frontmatter,
  type VarDecl,
  type VarType,
  interpolatePathStr,
  parseFrontmatter,
  varTypeToString,
} from "../frontmatter.js";
import {
  collectInheritedAliasSanitizeFromVarType,
  registerDeclSanitizeSpecs,
  type SanitizeSpec,
} from "../sanitize_pass.js";
import {
  ENUM_TAG_KEY,
  ENUM_VARIANTS_KEY,
  type Value,
  bool,
  float,
  fromJs,
  int,
  list,
  str,
  structVal,
  valueToJs,
} from "../value.js";
import {
  EXPR_START,
  LIT_FALSE,
  LIT_TRUE,
  TYPE_ALIAS,
  TYPE_BOOL,
  TYPE_ENUM,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_LIST,
  TYPE_SCALAR_LIST,
  TYPE_STR,
  TYPE_STRUCT,
  isValidResolvedPath,
} from "../consts.js";
import { parseF64, parseI64 } from "../filter_args.js";
import { typeCheckValue } from "./type_check.js";
import { toPascalCase } from "../validation.js";
import { getFs, getPath } from "./utils.js";

/**
 * Inject auto-generated constants for enum types declared in `types:`.
 *
 * For each enum type alias (e.g., `Stage = enum(Design, Build)`), creates
 * a dict constant mapping variant names to their values:
 * - Unit variants → string with the variant name
 * - Struct variants → tagged dict with `__kind__` key
 *
 * This enables template expressions like `{{ Stage.Design }}` or
 * `{{ kind(Status.Paused) }}`.
 *
 * User-defined constants with the same name are never overwritten.
 */
export function injectEnumTypeConstants(
  typeAliases: ReadonlyMap<string, VarType>,
  constValues: Map<string, Value>,
  constJsValues: Map<string, unknown>,
): void {
  for (const [typeName, varType] of typeAliases) {
    if (varType.kind !== "enum") continue;
    if (varType.isOption) continue; // Skip option types — they're not user-facing enum constants
    // Don't overwrite user-defined constants.
    if (constValues.has(typeName)) continue;

    const valueEntries: [string, Value][] = [];
    const jsObj: Record<string, unknown> = {};

    for (const variant of varType.variants) {
      if (variant.fields.length === 0) {
        // Unit variant → string value
        valueEntries.push([variant.name, str(variant.name)]);
        jsObj[variant.name] = variant.name;
      } else {
        // Struct variant → tagged dict with __kind__
        const fieldEntries: [string, Value][] = [
          [ENUM_TAG_KEY, str(variant.name)],
        ];
        const jsFields: Record<string, unknown> = {
          [ENUM_TAG_KEY]: variant.name,
        };
        valueEntries.push([variant.name, structVal(fieldEntries)]);
        jsObj[variant.name] = jsFields;
      }
    }

    const variantNames = varType.variants.map((v) => str(v.name));
    const jsVariantNames = varType.variants.map((v) => v.name);
    valueEntries.push([ENUM_VARIANTS_KEY, list(variantNames)]);
    jsObj[ENUM_VARIANTS_KEY] = jsVariantNames;

    constValues.set(typeName, structVal(valueEntries));
    constJsValues.set(typeName, jsObj);
  }
}

/**
 * Resolve alias types in VarDecl arrays using type aliases.
 *
 * Walks through declarations and replaces `{ kind: "alias", name: X }`
 * with the actual type from the typeAliases map. This ensures that
 * `validateIncludeTypes` can properly type-check params that reference
 * imported types (e.g., `types.Role` → `enum(admin, editor, viewer)`).
 */
export function resolveAliasesInDecls(
  decls: readonly VarDecl[],
  typeAliases: ReadonlyMap<string, VarType>,
): VarDecl[] {
  if (typeAliases.size === 0) return [...decls];
  return decls.map((decl) => {
    const resolved = resolveAliasType(decl.varType, typeAliases);
    if (resolved === decl.varType) return decl;
    return { ...decl, varType: resolved };
  });
}

/**
 * Recursively resolve an alias VarType through the typeAliases map.
 * Returns the original type unchanged if it's not an alias.
 */
function resolveAliasType(
  vt: VarType,
  typeAliases: ReadonlyMap<string, VarType>,
): VarType {
  if (vt.kind !== "alias") return vt;
  let resolved: VarType = vt;
  const seen = new Set<string>();
  while (resolved.kind === "alias") {
    if (seen.has(resolved.name)) break; // prevent infinite loops
    seen.add(resolved.name);
    const target = typeAliases.get(resolved.name);
    if (!target) break;
    resolved = target;
  }
  return resolved;
}

/**
 * Resolve imported template files and collect their exported constants.
 *
 * For each import in `fm.imports`, reads the referenced `.tmpl.md` file
 * relative to `baseDir`, parses its frontmatter, and collects constants
 * as `stem.NAME` entries. Returns a new `Frontmatter` with the
 * `importedConsts` field populated.
 *
 * Silently skips imports whose files cannot be read or parsed.
 */
export function resolveImportedConsts(
  fm: Frontmatter,
  baseDir: string,
): Frontmatter {
  if (fm.imports.length === 0) {
    return fm;
  }

  const availableConsts = new Map<string, Value>();
  for (const c of fm.consts) {
    if (c.defaultValue !== undefined) {
      availableConsts.set(c.name, c.defaultValue);
    }
  }
  // Include env declarations so import paths can reference env: values.
  for (const e of fm.env) {
    if (e.defaultValue !== undefined) {
      availableConsts.set(e.name, e.defaultValue);
    }
  }

  const imported: Record<string, unknown> = {};
  const fsModule = getFs();
  const pathModule = getPath();
  const mergedTypeAliases = new Map(fm.typeAliases);
  const mergedTypeAliasSanitize = new Map(fm.typeAliasSanitize);
  // Collect const type declarations per import stem, so we can build
  // typed namespace structs for the type checker.
  const constTypesPerStem = new Map<string, VarDecl[]>();

  for (const imp of fm.imports) {
    let impPath = imp.path;
    if (impPath.includes(EXPR_START)) {
      impPath = interpolatePathStr(impPath, availableConsts);
      if (!isValidResolvedPath(impPath) || impPath.includes(EXPR_START)) {
        throw new TemplateSyntaxError(
          `import path must begin with './', '../', or '/': '${impPath}'`,
        );
      }
    }

    const fullPath = pathModule.resolve(baseDir, impPath);
    let importSource: string;
    try {
      importSource = fsModule.readFileSync(fullPath, "utf-8");
    } catch (err) {
      throw new TemplateError(
        `cannot read imported template file '${fullPath}' for stem '${imp.stem}': ${String(err)}`,
      );
    }

    const [importedFm] = parseFrontmatter(importSource);

    const stemConstFields: VarDecl[] = [];
    for (const decl of importedFm.consts) {
      if (decl.defaultValue !== undefined) {
        imported[`${imp.stem}.${decl.name}`] = valueToJs(decl.defaultValue);
        // Accumulate for sequential/chained resolution: subsequent imports
        // can reference this const via {{ stem.NAME }} in their paths.
        availableConsts.set(`${imp.stem}.${decl.name}`, decl.defaultValue);
      }
      // Record the const's declared type for namespace type construction.
      stemConstFields.push({ name: decl.name, varType: decl.varType });
    }
    if (stemConstFields.length > 0) {
      constTypesPerStem.set(imp.stem, stemConstFields);
    }

    // Inject enum type constants from the imported template's type aliases.
    // Also merge ALL imported type aliases (prefixed by stem) so param
    // declarations with alias types (e.g., types.Role) can be resolved.
    for (const [typeName, varType] of importedFm.typeAliases) {
      mergedTypeAliases.set(`${imp.stem}.${typeName}`, varType);
      if (varType.kind !== "enum") continue;
      const key = `${imp.stem}.${typeName}`;
      if (key in imported) continue;

      const jsObj: Record<string, unknown> = {};
      for (const v of varType.variants) {
        if (v.fields.length === 0) {
          jsObj[v.name] = v.name;
        } else {
          jsObj[v.name] = { [ENUM_TAG_KEY]: v.name };
        }
      }
      const jsVariantNames = varType.variants.map((v) => v.name);
      jsObj[ENUM_VARIANTS_KEY] = jsVariantNames;
      imported[key] = jsObj;
    }

    for (const [typeName, specs] of importedFm.typeAliasSanitize) {
      mergedTypeAliasSanitize.set(`${imp.stem}.${typeName}`, specs);
    }

    registerImplicitImportedTypes(
      imp.stem,
      [...importedFm.params, ...importedFm.consts],
      mergedTypeAliases,
    );
  }

  let mergedParamSanitize: ReadonlyMap<string, SanitizeSpec> = fm.paramSanitize;
  if (mergedTypeAliasSanitize.size > fm.typeAliasSanitize.size) {
    const paramSanitizeCopy = new Map(fm.paramSanitize);
    for (const decl of fm.params) {
      const relSpecs = new Map<string, SanitizeSpec>();
      collectInheritedAliasSanitizeFromVarType(
        decl.varType,
        "",
        mergedTypeAliasSanitize,
        relSpecs,
        mergedTypeAliases,
      );
      if (relSpecs.size > 0) {
        registerDeclSanitizeSpecs(
          decl,
          relSpecs,
          paramSanitizeCopy,
          mergedTypeAliases,
          varTypeToString,
        );
      }
    }
    mergedParamSanitize = paramSanitizeCopy;
  }

  // Build typed namespace structs from the collected const type declarations.
  // Each import stem with typed consts becomes a struct VarType whose fields
  // are the imported const names and their declared types.
  const importedNamespaceTypes = new Map<string, VarType>();
  for (const [stem, fields] of constTypesPerStem) {
    importedNamespaceTypes.set(stem, { kind: TYPE_STRUCT, fields });
  }

  // Merge imported type aliases into fm.typeAliases so they're available
  // for resolving alias types (e.g., types.Role → enum(admin, editor, viewer)).
  // The mergedTypeAliases map was populated during the import loop above.

  if (Object.keys(imported).length === 0) {
    // Even if no consts were imported, we may have imported type aliases.
    if (
      mergedTypeAliases.size > fm.typeAliases.size ||
      mergedTypeAliasSanitize.size > fm.typeAliasSanitize.size ||
      importedNamespaceTypes.size > 0
    ) {
      return {
        ...fm,
        typeAliases: mergedTypeAliases,
        typeAliasSanitize: mergedTypeAliasSanitize,
        paramSanitize: mergedParamSanitize,
        importedNamespaceTypes,
      };
    }
    return fm;
  }

  // Post-process: resolve param defaults that reference imported consts.
  // During parseFrontmatter(), imported consts weren't available yet, so
  // param defaults like `stem.NAME` were deferred in unresolvedDefaults.
  if (fm.unresolvedDefaults.size === 0) {
    return {
      ...fm,
      importedConsts: imported,
      typeAliases: mergedTypeAliases,
      typeAliasSanitize: mergedTypeAliasSanitize,
      paramSanitize: mergedParamSanitize,
      importedNamespaceTypes,
    };
  }

  const importedValues = new Map<string, Value>();
  for (const [key, jsVal] of Object.entries(imported)) {
    importedValues.set(key, fromJs(jsVal));
  }

  const newParams: VarDecl[] = [];
  for (const decl of fm.params) {
    const unresolved = fm.unresolvedDefaults.get(decl.name);
    if (!unresolved) {
      newParams.push(decl);
      continue;
    }
    // Try to resolve the dotted const reference (e.g., stem.NAME)
    const constVal = importedValues.get(unresolved.text);
    if (constVal === undefined) {
      throw new TemplateSyntaxError(
        `unresolved default '${unresolved.text}' for param '${decl.name}': ` +
          `no imported const with that name found`,
        decl.loc?.line,
        decl.loc?.column,
        decl.loc?.snippet,
      );
    }
    // Validate type compatibility
    newParams.push({
      name: decl.name,
      varType: decl.varType,
      defaultValue: constVal,
    });
  }

  return {
    ...fm,
    params: newParams,
    importedConsts: imported,
    typeAliases: mergedTypeAliases,
    typeAliasSanitize: mergedTypeAliasSanitize,
    paramSanitize: mergedParamSanitize,
    importedNamespaceTypes,
    unresolvedDefaults: new Map(),
  };
}

/** Declared kinds that get an implicit PascalCase type name (Rust `add_implicit_param_types`). */
const IMPLICIT_TYPE_KINDS: ReadonlySet<string> = new Set([
  TYPE_LIST,
  TYPE_SCALAR_LIST,
  TYPE_STRUCT,
  TYPE_ENUM,
]);

/**
 * Expose an imported template's list/struct/enum params and consts as types
 * `stem.PascalName` (unless an explicit alias exists) and `stem.raw_name`,
 * mirroring Rust `add_implicit_param_types` and `ImportedNamespace.param_types`.
 */
function registerImplicitImportedTypes(
  stem: string,
  decls: readonly VarDecl[],
  mergedTypeAliases: Map<string, VarType>,
): void {
  for (const decl of decls) {
    if (!IMPLICIT_TYPE_KINDS.has(decl.varType.kind)) continue;
    for (const name of [toPascalCase(decl.name), decl.name]) {
      const key = `${stem}.${name}`;
      if (!mergedTypeAliases.has(key)) {
        mergedTypeAliases.set(key, decl.varType);
      }
    }
  }
}

/** Detail for a required `env:` declaration that received no value. */
const ENV_ERR_NO_VALUE = "no value provided and no default";

function envError(decl: VarDecl, detail: string): TemplateSyntaxError {
  return new TemplateSyntaxError(
    `env variable '${decl.name}': ${detail}`,
    decl.loc?.line,
    decl.loc?.column,
    decl.loc?.snippet,
  );
}

/** Resolve a (possibly aliased) declared type to its underlying type. */
function resolveAliasedType(
  varType: VarType,
  typeAliases: ReadonlyMap<string, VarType>,
): VarType {
  let current = varType;
  const seen = new Set<string>();
  while (current.kind === TYPE_ALIAS && !seen.has(current.name)) {
    seen.add(current.name);
    const target = typeAliases.get(current.name);
    if (target === undefined) {
      return current;
    }
    current = target;
  }
  return current;
}

/**
 * Coerce a string `env:` value for a bare scalar type exactly like the Rust core:
 * `int` via `str::parse::<i64>()`, `float` via `str::parse::<f64>()`, and `bool`
 * only from `true` / `false`. Returns `undefined` for non-scalar declared types.
 */
function coerceEnvString(
  decl: VarDecl,
  raw: string,
  varType: VarType,
): Value | undefined {
  switch (varType.kind) {
    case TYPE_INT: {
      const outcome = parseI64(raw);
      if (!outcome.ok) {
        throw envError(decl, `expected int, got '${raw}' (${outcome.reason})`);
      }
      return int(Number(outcome.value));
    }
    case TYPE_FLOAT: {
      const outcome = parseF64(raw);
      if (!outcome.ok) {
        throw envError(
          decl,
          `expected float, got '${raw}' (${outcome.reason})`,
        );
      }
      return float(outcome.value);
    }
    case TYPE_BOOL:
      if (raw === LIT_TRUE) return bool(true);
      if (raw === LIT_FALSE) return bool(false);
      throw envError(decl, `expected bool, got '${raw}'`);
    default:
      return undefined;
  }
}

/** Returns `true` if `value` satisfies `varType` (runtime parameter type rules). */
function envValueMatches(
  value: Value,
  varType: VarType,
  typeAliases: ReadonlyMap<string, VarType>,
): boolean {
  try {
    typeCheckValue("", value, varType, typeAliases);
    return true;
  } catch (err) {
    if (err instanceof TemplateError) {
      return false;
    }
    throw err;
  }
}

/**
 * Validate and coerce a provided `env:` value against its declared type
 * (byte-identical rules and messages to Rust `validate_env_value`).
 */
function validateEnvValue(
  decl: VarDecl,
  provided: unknown,
  typeAliases: ReadonlyMap<string, VarType>,
): Value {
  const value = fromJs(provided);
  const varType = resolveAliasedType(decl.varType, typeAliases);
  if (value.type === TYPE_STR) {
    const coerced = coerceEnvString(decl, value.value, varType);
    if (coerced !== undefined) {
      return coerced;
    }
  }
  if (envValueMatches(value, varType, typeAliases)) {
    return value;
  }
  throw envError(
    decl,
    `expected ${varTypeToString(decl.varType)}, got ${value.type}`,
  );
}

/**
 * Resolve `env:` declarations against provided compile-time values.
 *
 * For each env declaration:
 * - If a value is provided, validate it against the declared type (strings
 *   are coerced only for bare `int` / `float` / `bool`).
 * - If no value is provided and a default exists, use the default.
 * - If no value is provided and no default exists, throw a compile error.
 *
 * Returns a new Frontmatter with env declarations resolved
 * (each VarDecl has its `defaultValue` set to the resolved value).
 */
export function resolveEnvDeclarations(
  fm: Frontmatter,
  envValues: Record<string, unknown>,
): Frontmatter {
  if (fm.env.length === 0) {
    return fm;
  }

  const resolvedEnv: VarDecl[] = [];
  for (const decl of fm.env) {
    const provided = envValues[decl.name];
    if (provided !== undefined) {
      resolvedEnv.push({
        name: decl.name,
        varType: decl.varType,
        defaultValue: validateEnvValue(decl, provided, fm.typeAliases),
        loc: decl.loc,
      });
    } else if (decl.defaultValue !== undefined) {
      resolvedEnv.push(decl);
    } else {
      throw envError(decl, ENV_ERR_NO_VALUE);
    }
  }

  return { ...fm, env: resolvedEnv };
}
