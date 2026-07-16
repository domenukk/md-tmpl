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
  parseLiteral,
} from "../frontmatter.js";
import {
  ENUM_TAG_KEY,
  ENUM_VARIANTS_KEY,
  type Value,
  fromJs,
  list,
  str,
  structVal,
  valueToJs,
} from "../value.js";
import {
  EXPR_START,
  TYPE_BOOL,
  TYPE_FLOAT,
  TYPE_INT,
  TYPE_STR,
  TYPE_STRUCT,
  isValidResolvedPath,
} from "../consts.js";
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
      importedNamespaceTypes.size > 0
    ) {
      return { ...fm, typeAliases: mergedTypeAliases, importedNamespaceTypes };
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
    importedNamespaceTypes,
    unresolvedDefaults: new Map(),
  };
}

/**
 * Resolve `env:` declarations against provided compile-time values.
 *
 * For each env declaration:
 * - If a value is provided, parse the string to the declared type.
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
      let parsedValue: Value;
      try {
        if (typeof provided === "string") {
          // String value: parse according to declared type (backward compat).
          parsedValue = parseEnvStringValue(provided, decl.varType);
        } else {
          // Already typed value: convert directly.
          parsedValue = fromJs(provided);
        }
      } catch (err) {
        throw new TemplateSyntaxError(
          `env variable '${decl.name}': failed to convert value: ${err instanceof Error ? err.message : String(err)}`,
          decl.loc?.line,
          decl.loc?.column,
          decl.loc?.snippet,
        );
      }
      resolvedEnv.push({
        name: decl.name,
        varType: decl.varType,
        defaultValue: parsedValue,
        loc: decl.loc,
      });
    } else if (decl.defaultValue !== undefined) {
      resolvedEnv.push(decl);
    } else {
      throw new TemplateSyntaxError(
        `env variable '${decl.name}': no value provided and no default`,
        decl.loc?.line,
        decl.loc?.column,
        decl.loc?.snippet,
      );
    }
  }

  return { ...fm, env: resolvedEnv };
}

/**
 * Parse a string value into a typed Value based on the declared VarType.
 *
 * This is used for env: values which are always provided as strings
 * and need to be converted to the appropriate typed value.
 */
function parseEnvStringValue(value: string, varType: VarType): Value {
  switch (varType.kind) {
    case TYPE_STR:
      return str(value);
    case TYPE_INT: {
      const n = parseInt(value, 10);
      if (Number.isNaN(n) || String(n) !== value.trim()) {
        throw new TemplateSyntaxError(`invalid integer value: '${value}'`);
      }
      return { type: TYPE_INT, value: n };
    }
    case TYPE_FLOAT: {
      const f = parseFloat(value);
      if (Number.isNaN(f)) {
        throw new TemplateSyntaxError(`invalid float value: '${value}'`);
      }
      return { type: TYPE_FLOAT, value: f };
    }
    case TYPE_BOOL: {
      if (value === "true") return { type: TYPE_BOOL, value: true };
      if (value === "false") return { type: TYPE_BOOL, value: false };
      throw new TemplateSyntaxError(
        `invalid bool value: '${value}' (expected 'true' or 'false')`,
      );
    }
    default:
      // For other types, try parseLiteral as fallback
      return parseLiteral(value, varType);
  }
}
