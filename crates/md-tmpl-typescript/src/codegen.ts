/**
 * Code generation — emit TypeScript types from frontmatter declarations.
 *
 * This is the TypeScript equivalent of Rust's `include_types!` macro.
 * Given a template source, it generates:
 *
 * - A `Params` interface with all parameter fields properly typed
 * - Nested interfaces for `struct(...)` and `list(...)` item types
 * - Discriminated union types for `enum(...)` variants
 * - A typed `render(tmpl: Template, params: Params)` signature
 *
 * @example
 * ```ts
 * import { generateTypes, generateTypesFromFile } from "md-tmpl/codegen";
 *
 * // From source
 * const code = generateTypes(`
 * ---
 * params:
 *   - name = str
 *   - tasks = list(title = str, priority = str)
 *   - outcome = enum(Confirmed(evidence = str), Rejected)
 * ---
 * Hello {{ name }}!
 * `);
 *
 * console.log(code);
 * // → TypeScript source with Params interface, TasksItem interface,
 * //   Outcome discriminated union, etc.
 * ```
 *
 * @module
 */

import * as fs from "node:fs";
import {
  type VarType,
  type VarDecl,
  type VariantDecl,
  parseFrontmatter,
} from "./frontmatter.js";
import type { Value } from "./value.js";
import {
  OPTION_SOME,
  OPTION_NONE,
  ENUM_TAG_KEY,
  OPTION_VAL_FIELD,
  LIT_TRUE,
  LIT_FALSE,
  TYPE_NONE,
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_TMPL,
  TYPE_ENUM,
  TYPE_OPTION,
  TYPE_ALIAS,
  TYPE_SCALAR_LIST,
  TYPE_UNTYPED_LIST,
} from "./consts.js";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/** Options for type generation. */
export interface GenerateTypesOptions {
  /** Name for the generated params interface. @default "Params" */
  readonly paramsName?: string;
  /** Whether to export the generated types. @default true */
  readonly exportTypes?: boolean;
  /** Whether to include a typed render helper. @default true */
  readonly includeRenderHelper?: boolean;
  /** Whether to generate JSDoc comments. @default true */
  readonly jsdoc?: boolean;
}

/**
 * Generate TypeScript type declarations from a template source string.
 *
 * Returns a string of valid TypeScript source code containing interfaces,
 * type aliases, and an optional typed render function.
 */
export function generateTypes(
  source: string,
  options?: GenerateTypesOptions,
): string {
  const [fm] = parseFrontmatter(source);
  const opts = resolveOptions(options);
  const ctx = new CodegenContext(opts);

  // Resolve type aliases first (emits their definitions)
  for (const [name, varType] of fm.typeAliases) {
    ctx.resolveAlias(name, varType);
  }

  // Collect all param types (may create auxiliary interfaces)
  for (const param of fm.params) {
    ctx.resolveType(param.name, param.varType);
  }

  return ctx.emit(fm.params, fm.consts);
}

/**
 * Generate TypeScript type declarations from a `.tmpl.md` file.
 */
export function generateTypesFromFile(
  filePath: string,
  options?: GenerateTypesOptions,
): string {
  const source = fs.readFileSync(filePath, "utf-8");
  return generateTypes(source, options);
}

/**
 * Infer TypeScript type declarations and return a structured result
 * instead of source code. Useful for programmatic inspection.
 */
export function inferTypes(source: string): InferredTypes {
  const [fm] = parseFrontmatter(source);
  const fields: InferredField[] = fm.params.map((param) => ({
    name: param.name,
    tsType: varTypeToTsType(param.name, param.varType, fm.typeAliases),
    optional: param.defaultValue !== undefined,
    defaultValue:
      param.defaultValue !== undefined
        ? valueToJsLiteral(param.defaultValue)
        : undefined,
    varType: param.varType,
  }));
  const typeAliases: InferredTypeAlias[] = [];
  for (const [name, varType] of fm.typeAliases) {
    typeAliases.push({
      name,
      tsType: varTypeToTsType(name, varType, fm.typeAliases),
      varType,
    });
  }
  const consts: InferredConst[] = fm.consts.map((c) => ({
    name: c.name,
    tsType: varTypeToTsType(c.name, c.varType, fm.typeAliases),
    value:
      c.defaultValue !== undefined
        ? valueToJsLiteral(c.defaultValue)
        : undefined,
    varType: c.varType,
  }));
  return { fields, typeAliases, consts };
}

/** Result of `inferTypes()`. */
export interface InferredTypes {
  readonly fields: readonly InferredField[];
  readonly typeAliases: readonly InferredTypeAlias[];
  readonly consts: readonly InferredConst[];
}

/** A single inferred field (parameter). */
export interface InferredField {
  readonly name: string;
  readonly tsType: string;
  readonly optional: boolean;
  /** The default value as a JS literal, if declared. */
  readonly defaultValue?: unknown;
  readonly varType: VarType;
}

/** A type alias from the `types:` block. */
export interface InferredTypeAlias {
  readonly name: string;
  readonly tsType: string;
  readonly varType: VarType;
}

/** A constant from the `consts:` block. */
export interface InferredConst {
  readonly name: string;
  readonly tsType: string;
  /** The constant value as a JS literal. */
  readonly value: unknown;
  readonly varType: VarType;
}

// ---------------------------------------------------------------------------
// Codegen context — accumulates generated code
// ---------------------------------------------------------------------------

interface ResolvedOptions {
  readonly paramsName: string;
  readonly exportTypes: boolean;
  readonly includeRenderHelper: boolean;
  readonly jsdoc: boolean;
}

function resolveOptions(opts?: GenerateTypesOptions): ResolvedOptions {
  return {
    paramsName: opts?.paramsName ?? "Params",
    exportTypes: opts?.exportTypes ?? true,
    includeRenderHelper: opts?.includeRenderHelper ?? true,
    jsdoc: opts?.jsdoc ?? true,
  };
}

class CodegenContext {
  private readonly opts: ResolvedOptions;
  /** Auxiliary interfaces/types generated from nested structures. */
  private readonly auxiliaryTypes: string[] = [];
  /** Set of names already generated to avoid duplicates. */
  private readonly generatedNames = new Set<string>();

  constructor(opts: ResolvedOptions) {
    this.opts = opts;
  }

  /** Resolve a VarType and return its TypeScript type string. */
  resolveType(fieldName: string, vt: VarType): string {
    return this.varTypeToTs(fieldName, vt);
  }

  /** Resolve a type alias from the `types:` block. */
  resolveAlias(name: string, vt: VarType): void {
    // Generate the actual type definition using the alias name as the field name
    // This will emit interfaces/enums as needed
    const tsType = this.varTypeToTs(name, vt);

    // If it's a simple primitive mapping, emit a type alias
    if (!this.generatedNames.has(name)) {
      this.generatedNames.add(name);
      const exp = this.opts.exportTypes ? "export " : "";
      const lines: string[] = [];
      if (this.opts.jsdoc) {
        lines.push(`/** Type alias for \`${name}\`. */`);
      }
      lines.push(`${exp}type ${name} = ${tsType};`);
      this.auxiliaryTypes.push(lines.join("\n"));
    }
  }

  /** Emit the full generated code. */
  emit(params: readonly VarDecl[], consts: readonly VarDecl[]): string {
    const lines: string[] = [];
    const exp = this.opts.exportTypes ? "export " : "";

    // Auto-generated header
    lines.push("// Auto-generated by md-tmpl codegen");
    lines.push("// Do not edit manually.");
    lines.push("");

    // Auxiliary types first (nested interfaces, enums)
    for (const aux of this.auxiliaryTypes) {
      lines.push(aux);
      lines.push("");
    }

    // Main Params interface
    if (this.opts.jsdoc) {
      lines.push("/** Template parameters. */");
    }
    lines.push(`${exp}interface ${this.opts.paramsName} {`);
    for (const param of params) {
      const tsType = this.varTypeToTs(param.name, param.varType);
      const optional = param.defaultValue !== undefined ? "?" : "";
      if (this.opts.jsdoc) {
        const defaultComment =
          param.defaultValue !== undefined
            ? ` @default ${valueToJsSource(param.defaultValue)}`
            : "";
        lines.push(
          `  /** Type: \`${varTypeToLabel(param.varType)}\`${defaultComment} */`,
        );
      }
      lines.push(`  readonly ${param.name}${optional}: ${tsType};`);
    }
    lines.push("}");

    // Constants object
    if (consts.length > 0) {
      lines.push("");
      if (this.opts.jsdoc) {
        lines.push(
          "/** Template constants — immutable values defined in frontmatter. */",
        );
      }
      lines.push(`${exp}const CONSTANTS = {`);
      for (const c of consts) {
        if (c.defaultValue !== undefined) {
          const jsVal = valueToJsSource(c.defaultValue);
          if (this.opts.jsdoc) {
            lines.push(`  /** Type: \`${varTypeToLabel(c.varType)}\` */`);
          }
          lines.push(`  ${c.name}: ${jsVal},`);
        }
      }
      lines.push("} as const;");
    }

    // Defaults object
    const paramsWithDefaults = params.filter(
      (p) => p.defaultValue !== undefined,
    );
    if (paramsWithDefaults.length > 0) {
      lines.push("");
      if (this.opts.jsdoc) {
        lines.push("/** Default values for optional parameters. */");
      }
      lines.push(`${exp}const DEFAULTS: Partial<${this.opts.paramsName}> = {`);
      for (const param of paramsWithDefaults) {
        if (param.defaultValue !== undefined) {
          const jsVal = valueToJsSource(param.defaultValue);
          lines.push(`  ${param.name}: ${jsVal},`);
        }
      }
      lines.push("};");
    }

    // Typed render helper
    if (this.opts.includeRenderHelper) {
      lines.push("");
      if (this.opts.jsdoc) {
        lines.push("/**");
        lines.push(" * Render a template with type-checked parameters.");
        lines.push(" *");
        lines.push(" * @example");
        lines.push(` * \`\`\`ts`);
        lines.push(` * import { Template } from "md-tmpl";`);
        lines.push(` * const tmpl = Template.fromFile("my_template.tmpl.md");`);
        lines.push(
          ` * const output = render(tmpl, { ${params[0] ? `${params[0].name}: ...` : ""} });`,
        );
        lines.push(` * \`\`\``);
        lines.push(" */");
      }
      lines.push(
        `${exp}function render(tmpl: import("md-tmpl").Template, params: ${this.opts.paramsName}): string {`,
      );
      lines.push(`  return tmpl.render(params as Record<string, unknown>);`);
      lines.push("}");
    }

    lines.push("");
    return lines.join("\n");
  }

  // ── Type resolution ───────────────────────────────────────────────

  private varTypeToTs(fieldName: string, vt: VarType): string {
    switch (vt.kind) {
      case TYPE_STR:
        return "string";
      case TYPE_BOOL:
        return "boolean";
      case TYPE_INT:
      case TYPE_FLOAT:
        return "number";
      case TYPE_SCALAR_LIST:
        return `readonly ${this.varTypeToTs(fieldName, vt.elementType)}[]`;
      case TYPE_LIST: {
        if (vt.fields.length === 0) {
          return "unknown[]";
        }
        // Generate an item interface
        const itemName = `${pascalCase(fieldName)}Item`;
        this.emitInterface(itemName, vt.fields);
        return `readonly ${itemName}[]`;
      }
      case TYPE_TMPL:
      case TYPE_STRUCT: {
        if (vt.fields.length === 0) {
          return "Record<string, unknown>";
        }
        const structName = pascalCase(fieldName);
        this.emitInterface(structName, vt.fields);
        return structName;
      }
      case TYPE_ENUM: {
        if (vt.isOption) {
          const someVariant = vt.variants.find((v) => v.name === OPTION_SOME);
          if (someVariant?.fields.length === 1) {
            const someField = someVariant.fields[0];
            if (someField !== undefined) {
              const innerType = this.varTypeToTs(fieldName, someField.varType);
              return `${innerType} | null`;
            }
          }
        }
        const enumName = pascalCase(fieldName);
        this.emitEnum(enumName, vt.variants);
        return enumName;
      }
      case TYPE_ALIAS:
        return vt.name;
      case TYPE_UNTYPED_LIST:
        return "unknown[]";
      case TYPE_OPTION: {
        const innerType = this.varTypeToTs(fieldName, vt.innerType);
        return `${innerType} | null`;
      }
    }
  }

  private emitInterface(name: string, fields: readonly VarDecl[]): void {
    if (this.generatedNames.has(name)) return;
    this.generatedNames.add(name);

    const exp = this.opts.exportTypes ? "export " : "";
    const lines: string[] = [];

    if (this.opts.jsdoc) {
      lines.push(`/** Nested type for \`${name}\`. */`);
    }
    lines.push(`${exp}interface ${name} {`);
    for (const field of fields) {
      const tsType = this.varTypeToTs(field.name, field.varType);
      lines.push(`  readonly ${field.name}: ${tsType};`);
    }
    lines.push("}");

    this.auxiliaryTypes.push(lines.join("\n"));
  }

  private emitEnum(name: string, variants: readonly VariantDecl[]): void {
    if (this.generatedNames.has(name)) return;
    this.generatedNames.add(name);

    const exp = this.opts.exportTypes ? "export " : "";
    const variantTypes: string[] = [];

    for (const v of variants) {
      if (v.fields.length === 0) {
        // Unit variant — just the string literal (matches the wire format).
        variantTypes.push(`"${v.name}"`);
      } else {
        // Data variant — generate a `__kind__`-tagged interface.
        const variantIfaceName = `${name}_${sanitizeVariantIdent(v.name)}`;
        this.emitVariantInterface(variantIfaceName, v.name, v.fields);
        variantTypes.push(variantIfaceName);
      }
    }

    // Discriminant union of every variant's tag name (unit + data). This is
    // what `kindOf`/`is`/`match` narrow over and what UIs enumerate.
    const kindType = `${name}Kind`;
    const kindUnion = variants.map((v) => `"${v.name}"`).join(" | ");

    const lines: string[] = [];
    if (this.opts.jsdoc) {
      lines.push(`/** Discriminant tag names of \`${name}\`. */`);
    }
    lines.push(`${exp}type ${kindType} = ${kindUnion};`);
    lines.push("");
    if (this.opts.jsdoc) {
      lines.push(
        `/** Enum type: ${variants.map((v) => v.name).join(" | ")}. */`,
      );
    }
    lines.push(`${exp}type ${name} = ${variantTypes.join(" | ")};`);
    this.auxiliaryTypes.push(lines.join("\n"));

    // Companion namespace: variant constructors + helpers, merged with the type.
    this.emitEnumNamespace(name, kindType, variants);
  }

  /**
   * Emit the companion `const <Name>` namespace object that is merged with the
   * `type <Name>` declaration. It provides, for every enum:
   *
   * - variant constructors — unit variants as string constants, data variants
   *   as factory functions returning a `__kind__`-tagged object;
   * - `kinds` — a readonly tuple of every variant tag name;
   * - `kindOf(v)` — the discriminant tag of a value;
   * - `is(v, k)` — a tag guard;
   * - `match(v, arms)` — an exhaustive, type-checked matcher.
   *
   * Object keys are quoted only when the variant name is not a valid identifier
   * (e.g. `"ACTUAL OUTPUT"`), so spaced/punctuated variant names are supported
   * without emitting invalid TypeScript.
   */
  private emitEnumNamespace(
    name: string,
    kindType: string,
    variants: readonly VariantDecl[],
  ): void {
    const exp = this.opts.exportTypes ? "export " : "";
    const unitVariants = variants.filter((v) => v.fields.length === 0);
    const dataVariants = variants.filter((v) => v.fields.length > 0);
    const tag = ENUM_TAG_KEY;

    const lines: string[] = [];
    if (this.opts.jsdoc) {
      lines.push(
        `/** Companion namespace for \`${name}\`: variant constructors + helpers. */`,
      );
    }
    lines.push(`${exp}const ${name} = {`);

    // Variant constructors.
    for (const v of variants) {
      const key = objectKey(v.name);
      if (v.fields.length === 0) {
        lines.push(`  ${key}: "${v.name}" as const,`);
      } else {
        const ifaceName = `${name}_${sanitizeVariantIdent(v.name)}`;
        const fieldParts = v.fields.map((f) => {
          const tsType = this.varTypeToTs(f.name, f.varType);
          return `${objectKey(f.name)}: ${tsType}`;
        });
        const fieldsType = `{ ${fieldParts.join("; ")} }`;
        lines.push(
          `  ${key}: (fields: ${fieldsType}): ${ifaceName} => ` +
            `({ ${tag}: "${v.name}", ...fields }),`,
        );
      }
    }

    // Introspection + narrowing helpers. The discriminant expression depends
    // on the enum's variant composition: unit-only values are strings,
    // data-only values are `__kind__`-tagged objects, and mixed values require
    // a runtime `typeof` check. Specializing avoids a dead `never` branch (and
    // the corresponding `no-unnecessary-condition` lint).
    let kindOfExpr: string;
    if (dataVariants.length === 0) {
      kindOfExpr = "v";
    } else if (unitVariants.length === 0) {
      kindOfExpr = `v.${tag}`;
    } else {
      kindOfExpr = `(typeof v === "string" ? v : v.${tag})`;
    }
    const kindsItems = variants.map((v) => `"${v.name}"`).join(", ");
    lines.push(`  kinds: [${kindsItems}] as const,`);
    lines.push(`  kindOf: (v: ${name}): ${kindType} => ${kindOfExpr},`);
    lines.push(
      `  is: (v: ${name}, k: ${kindType}): boolean => ${kindOfExpr} === k,`,
    );

    // Exhaustive matcher. Arm parameter types are the precise variant types.
    const armParts = variants.map((v) => {
      const argType =
        v.fields.length === 0
          ? `"${v.name}"`
          : `${name}_${sanitizeVariantIdent(v.name)}`;
      return `${objectKey(v.name)}: (v: ${argType}) => R`;
    });
    lines.push(
      `  match: <R>(v: ${name}, arms: { ${armParts.join("; ")} }): R => {`,
    );
    if (unitVariants.length > 0 && dataVariants.length > 0) {
      lines.push(`    if (typeof v === "string") {`);
      lines.push(`      switch (v) {`);
      for (const v of unitVariants) {
        lines.push(
          `        case "${v.name}": return arms[${JSON.stringify(v.name)}](v);`,
        );
      }
      lines.push(`      }`);
      lines.push(`    } else {`);
      lines.push(`      switch (v.${tag}) {`);
      for (const v of dataVariants) {
        lines.push(
          `        case "${v.name}": return arms[${JSON.stringify(v.name)}](v);`,
        );
      }
      lines.push(`      }`);
      lines.push(`    }`);
    } else if (dataVariants.length > 0) {
      lines.push(`    switch (v.${tag}) {`);
      for (const v of dataVariants) {
        lines.push(
          `      case "${v.name}": return arms[${JSON.stringify(v.name)}](v);`,
        );
      }
      lines.push(`    }`);
    } else {
      lines.push(`    switch (v) {`);
      for (const v of unitVariants) {
        lines.push(
          `      case "${v.name}": return arms[${JSON.stringify(v.name)}](v);`,
        );
      }
      lines.push(`    }`);
    }
    lines.push(
      `    throw new Error(\`unhandled ${name} variant: \${JSON.stringify(v)}\`);`,
    );
    lines.push(`  },`);

    lines.push(`} as const;`);
    this.auxiliaryTypes.push(lines.join("\n"));
  }

  private emitVariantInterface(
    name: string,
    tag: string,
    fields: readonly VarDecl[],
  ): void {
    if (this.generatedNames.has(name)) return;
    this.generatedNames.add(name);

    const exp = this.opts.exportTypes ? "export " : "";
    const lines: string[] = [];

    if (this.opts.jsdoc) {
      lines.push(`/** Variant \`${tag}\` with data fields. */`);
    }
    lines.push(`${exp}interface ${name} {`);
    lines.push(`  readonly ${ENUM_TAG_KEY}: "${tag}";`);
    for (const field of fields) {
      const tsType = this.varTypeToTs(field.name, field.varType);
      lines.push(`  readonly ${field.name}: ${tsType};`);
    }
    lines.push("}");

    this.auxiliaryTypes.push(lines.join("\n"));
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Convert a snake_case or lowercase name to PascalCase. */
function pascalCase(s: string): string {
  return s
    .split(/[_\s-]+/)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}

/**
 * Render `name` as an object-literal key: a bare identifier when it is a valid
 * ECMAScript identifier, otherwise a double-quoted string literal. This lets
 * variant/field names containing spaces or punctuation (e.g. `ACTUAL OUTPUT`)
 * appear as valid keys without emitting uncompilable TypeScript.
 */
function objectKey(name: string): string {
  return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : JSON.stringify(name);
}

/**
 * Derive a valid TypeScript identifier suffix for a data variant's tagged
 * interface (e.g. `Outcome_Confirmed`). Non-alphanumeric characters are
 * dropped; a leading digit (or an empty result) is prefixed so the identifier
 * is always valid. The variant's *string* tag is preserved separately, so this
 * only affects the generated type name, never the wire value.
 */
function sanitizeVariantIdent(name: string): string {
  const cleaned = name.replace(/[^A-Za-z0-9_$]/g, "");
  if (cleaned === "" || /^[0-9]/.test(cleaned)) {
    return `V${cleaned}`;
  }
  return cleaned;
}

/** Get a human-readable label for a VarType (used in JSDoc). */
function varTypeToLabel(vt: VarType): string {
  switch (vt.kind) {
    case TYPE_STR:
    case TYPE_BOOL:
    case TYPE_INT:
    case TYPE_FLOAT:
      return vt.kind;
    case TYPE_SCALAR_LIST:
      return `list(${varTypeToLabel(vt.elementType)})`;
    case TYPE_LIST:
      return "list(…)";
    case TYPE_TMPL:
    case TYPE_STRUCT:
      return "struct(…)";
    case TYPE_ENUM:
      if (vt.isOption) {
        const someVariant = vt.variants.find((v) => v.name === OPTION_SOME);
        if (someVariant?.fields.length === 1) {
          const someField = someVariant.fields[0];
          if (someField !== undefined) {
            return `option(${varTypeToLabel(someField.varType)})`;
          }
        }
      }
      return `enum(${vt.variants.map((v) => v.name).join(", ")})`;
    case TYPE_ALIAS:
      return vt.name;
    case TYPE_UNTYPED_LIST:
      return "list()";
    case TYPE_OPTION:
      return `option(${varTypeToLabel(vt.innerType)})`;
  }
}

/** Convert a VarType to its TypeScript type inline (for inferTypes). */
function varTypeToTsType(
  _fieldName: string,
  vt: VarType,
  typeAliases?: ReadonlyMap<string, VarType>,
): string {
  switch (vt.kind) {
    case TYPE_STR:
      return "string";
    case TYPE_BOOL:
      return "boolean";
    case TYPE_INT:
    case TYPE_FLOAT:
      return "number";
    case TYPE_SCALAR_LIST:
      return `${varTypeToTsType(_fieldName, vt.elementType, typeAliases)}[]`;
    case TYPE_LIST:
      if (vt.fields.length === 0) return "unknown[]";
      return `Array<{ ${vt.fields.map((f) => `${f.name}: ${varTypeToTsType(f.name, f.varType, typeAliases)}`).join("; ")} }>`;
    case TYPE_TMPL:
    case TYPE_STRUCT:
      if (vt.fields.length === 0) return "Record<string, unknown>";
      return `{ ${vt.fields.map((f) => `${f.name}: ${varTypeToTsType(f.name, f.varType, typeAliases)}`).join("; ")} }`;
    case TYPE_ENUM: {
      if (vt.isOption) {
        const someVariant = vt.variants.find((v) => v.name === OPTION_SOME);
        if (someVariant?.fields.length === 1) {
          const someField = someVariant.fields[0];
          if (someField !== undefined) {
            return `${varTypeToTsType(_fieldName, someField.varType, typeAliases)} | null`;
          }
        }
      }
      const parts = vt.variants.map((v) => {
        if (v.fields.length === 0) return `"${v.name}"`;
        const fields = v.fields
          .map(
            (f) =>
              `${f.name}: ${varTypeToTsType(f.name, f.varType, typeAliases)}`,
          )
          .join("; ");
        return `{ ${ENUM_TAG_KEY}: "${v.name}"; ${fields} }`;
      });
      return parts.join(" | ");
    }
    case TYPE_ALIAS: {
      // Resolve the alias if we have the definition
      if (typeAliases) {
        const resolved = typeAliases.get(vt.name);
        if (resolved) {
          return varTypeToTsType(vt.name, resolved, typeAliases);
        }
      }
      return vt.name;
    }
    case TYPE_UNTYPED_LIST:
      return "unknown[]";
    case TYPE_OPTION: {
      const inner = varTypeToTsType(_fieldName, vt.innerType, typeAliases);
      return `${inner} | null`;
    }
  }
}

// ---------------------------------------------------------------------------
// Value → JS source/literal helpers
// ---------------------------------------------------------------------------

/** Convert a Value to its TypeScript source code representation (for codegen). */
function valueToJsSource(v: Value): string {
  switch (v.type) {
    case TYPE_STR:
      return JSON.stringify(v.value);
    case TYPE_BOOL:
      return v.value ? LIT_TRUE : LIT_FALSE;
    case TYPE_INT:
    case TYPE_FLOAT:
      return String(v.value);
    case TYPE_LIST:
      return `[${v.items.map(valueToJsSource).join(", ")}]`;
    case TYPE_STRUCT: {
      // Option serde: None → null in codegen output
      const kindTag = v.fields.get(ENUM_TAG_KEY);
      if (
        kindTag?.type === TYPE_STR &&
        kindTag.value === OPTION_NONE &&
        v.fields.size === 1
      ) {
        return "null";
      }
      // Option serde: Some → unwrapped value in codegen output
      if (kindTag?.type === TYPE_STR && kindTag.value === OPTION_SOME) {
        const innerVal = v.fields.get(OPTION_VAL_FIELD);
        if (innerVal && v.fields.size === 2) {
          return valueToJsSource(innerVal);
        }
      }
      const entries: string[] = [];
      for (const [k, val] of v.fields) {
        entries.push(`${k}: ${valueToJsSource(val)}`);
      }
      return `{ ${entries.join(", ")} }`;
    }
    case TYPE_NONE:
      return "null";
    case TYPE_TMPL:
      return "null /* tmpl ref */";
  }
}

/** Convert a Value to a plain JS value (for inferTypes programmatic output). */
function valueToJsLiteral(v: Value): unknown {
  switch (v.type) {
    case TYPE_STR:
      return v.value;
    case TYPE_BOOL:
      return v.value;
    case TYPE_INT:
    case TYPE_FLOAT:
      return v.value;
    case TYPE_LIST:
      return v.items.map(valueToJsLiteral);
    case TYPE_STRUCT: {
      const obj: Record<string, unknown> = {};
      for (const [k, val] of v.fields) {
        obj[k] = valueToJsLiteral(val);
      }
      return obj;
    }
    case TYPE_NONE:
      return null;
    case TYPE_TMPL:
      return null;
  }
}
