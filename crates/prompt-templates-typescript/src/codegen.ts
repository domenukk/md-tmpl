/**
 * Code generation — emit TypeScript types from frontmatter declarations.
 *
 * This is the TypeScript equivalent of Rust's `include_types!` macro.
 * Given a template source, it generates:
 *
 * - A `Params` interface with all parameter fields properly typed
 * - Nested interfaces for `struct<...>` and `list<...>` item types
 * - Discriminated union types for `enum<...>` variants
 * - A typed `render(tmpl: Template, params: Params)` signature
 *
 * @example
 * ```ts
 * import { generateTypes, generateTypesFromFile } from "prompt-templates/codegen";
 *
 * // From source
 * const code = generateTypes(`
 * ---
 * params:
 *   - name = str
 *   - tasks = list<title = str, priority = str>
 *   - outcome = enum<Confirmed(evidence = str), Rejected>
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
  const ctx = new CodegenContext(opts, fm.typeAliases);

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

  constructor(
    opts: ResolvedOptions,
    _typeAliases: ReadonlyMap<string, VarType>,
  ) {
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
    lines.push("// Auto-generated by prompt-templates codegen");
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
        lines.push(` * import { Template } from "prompt-templates";`);
        lines.push(` * const tmpl = Template.fromFile("my_template.tmpl.md");`);
        lines.push(
          ` * const output = render(tmpl, { ${params.length > 0 ? params[0]!.name + ": ..." : ""} });`,
        );
        lines.push(` * \`\`\``);
        lines.push(" */");
      }
      lines.push(
        `${exp}function render(tmpl: import("prompt-templates").Template, params: ${this.opts.paramsName}): string {`,
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
      case "str":
        return "string";
      case "bool":
        return "boolean";
      case "int":
      case "float":
        return "number";
      case "scalar_list":
        return `readonly ${this.varTypeToTs(fieldName, vt.elementType)}[]`;
      case "list": {
        if (vt.fields.length === 0) {
          return "unknown[]";
        }
        // Generate an item interface
        const itemName = pascalCase(fieldName) + "Item";
        this.emitInterface(itemName, vt.fields);
        return `readonly ${itemName}[]`;
      }
      case "struct": {
        if (vt.fields.length === 0) {
          return "Record<string, unknown>";
        }
        const structName = pascalCase(fieldName);
        this.emitInterface(structName, vt.fields);
        return structName;
      }
      case "enum": {
        if (vt.isOption) {
          const someVariant = vt.variants.find((v) => v.name === "Some");
          if (someVariant && someVariant.fields.length === 1) {
            const innerType = this.varTypeToTs(
              fieldName,
              someVariant.fields[0]!.varType,
            );
            return `${innerType} | null`;
          }
        }
        const enumName = pascalCase(fieldName);
        this.emitEnum(enumName, vt.variants);
        return enumName;
      }
      case "alias":
        return vt.name;
      case "untyped_list":
        return "unknown[]";
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
        // Unit variant — just the string literal
        variantTypes.push(`"${v.name}"`);
      } else {
        // Struct variant — generate a tagged interface
        const variantIfaceName = `${name}_${v.name}`;
        this.emitVariantInterface(variantIfaceName, v.name, v.fields);
        variantTypes.push(variantIfaceName);
      }
    }

    const lines: string[] = [];
    if (this.opts.jsdoc) {
      lines.push(
        `/** Enum type: ${variants.map((v) => v.name).join(" | ")}. */`,
      );
    }
    lines.push(`${exp}type ${name} = ${variantTypes.join(" | ")};`);
    this.auxiliaryTypes.push(lines.join("\n"));

    // Emit factory functions for each variant
    this.emitVariantFactories(name, variants);
  }

  /**
   * Emit factory functions for enum variants.
   *
   * - Unit variants: `export const Rejected: Outcome = "Rejected";`
   * - Struct variants: `export function Confirmed(fields: { evidence: string }): Outcome_Confirmed { ... }`
   */
  private emitVariantFactories(
    enumName: string,
    variants: readonly VariantDecl[],
  ): void {
    const exp = this.opts.exportTypes ? "export " : "";

    for (const v of variants) {
      const lines: string[] = [];

      if (v.fields.length === 0) {
        // Unit variant → const
        if (this.opts.jsdoc) {
          lines.push(`/** Create a \`${v.name}\` variant. */`);
        }
        lines.push(`${exp}const ${v.name}: ${enumName} = "${v.name}";`);
      } else {
        // Struct variant → factory function
        const ifaceName = `${enumName}_${v.name}`;
        const fieldParts = v.fields.map((f) => {
          const tsType = this.varTypeToTs(f.name, f.varType);
          return `${f.name}: ${tsType}`;
        });
        const fieldsType = `{ ${fieldParts.join("; ")} }`;

        if (this.opts.jsdoc) {
          lines.push(`/** Create a \`${v.name}\` variant. */`);
        }
        lines.push(
          `${exp}function ${v.name}(fields: ${fieldsType}): ${ifaceName} {`,
        );
        lines.push(`  return { __kind__: "${v.name}", ...fields };`);
        lines.push("}");
      }

      this.auxiliaryTypes.push(lines.join("\n"));
    }
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
    lines.push(`  readonly __kind__: "${tag}";`);
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

/** Get a human-readable label for a VarType (used in JSDoc). */
function varTypeToLabel(vt: VarType): string {
  switch (vt.kind) {
    case "str":
    case "bool":
    case "int":
    case "float":
      return vt.kind;
    case "scalar_list":
      return `list<${varTypeToLabel(vt.elementType)}>`;
    case "list":
      return "list<…>";
    case "struct":
      return "struct<…>";
    case "enum":
      if (vt.isOption) {
        const someVariant = vt.variants.find((v) => v.name === "Some");
        if (someVariant && someVariant.fields.length === 1) {
          return `option<${varTypeToLabel(someVariant.fields[0]!.varType)}>`;
        }
      }
      return `enum<${vt.variants.map((v) => v.name).join(", ")}>`;
    case "alias":
      return vt.name;
    case "untyped_list":
      return "list<>";
  }
}

/** Convert a VarType to its TypeScript type inline (for inferTypes). */
function varTypeToTsType(
  _fieldName: string,
  vt: VarType,
  typeAliases?: ReadonlyMap<string, VarType>,
): string {
  switch (vt.kind) {
    case "str":
      return "string";
    case "bool":
      return "boolean";
    case "int":
    case "float":
      return "number";
    case "scalar_list":
      return `${varTypeToTsType(_fieldName, vt.elementType, typeAliases)}[]`;
    case "list":
      if (vt.fields.length === 0) return "unknown[]";
      return `Array<{ ${vt.fields.map((f) => `${f.name}: ${varTypeToTsType(f.name, f.varType, typeAliases)}`).join("; ")} }>`;
    case "struct":
      if (vt.fields.length === 0) return "Record<string, unknown>";
      return `{ ${vt.fields.map((f) => `${f.name}: ${varTypeToTsType(f.name, f.varType, typeAliases)}`).join("; ")} }`;
    case "enum": {
      if (vt.isOption) {
        const someVariant = vt.variants.find((v) => v.name === "Some");
        if (someVariant && someVariant.fields.length === 1) {
          return `${varTypeToTsType(_fieldName, someVariant.fields[0]!.varType, typeAliases)} | null`;
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
        return `{ __kind__: "${v.name}"; ${fields} }`;
      });
      return parts.join(" | ");
    }
    case "alias": {
      // Resolve the alias if we have the definition
      if (typeAliases) {
        const resolved = typeAliases.get(vt.name);
        if (resolved) {
          return varTypeToTsType(vt.name, resolved, typeAliases);
        }
      }
      return vt.name;
    }
    case "untyped_list":
      return "unknown[]";
  }
}

// ---------------------------------------------------------------------------
// Value → JS source/literal helpers
// ---------------------------------------------------------------------------

/** Convert a Value to its TypeScript source code representation (for codegen). */
function valueToJsSource(v: Value): string {
  switch (v.type) {
    case "str":
      return JSON.stringify(v.value);
    case "bool":
      return v.value ? "true" : "false";
    case "int":
    case "float":
      return String(v.value);
    case "list":
      return `[${v.items.map(valueToJsSource).join(", ")}]`;
    case "dict": {
      // Option serde: None → null in codegen output
      const kindTag = v.fields.get("__kind__");
      if (
        kindTag?.type === "str" &&
        kindTag.value === "None" &&
        v.fields.size === 1
      ) {
        return "null";
      }
      // Option serde: Some → unwrapped value in codegen output
      if (kindTag?.type === "str" && kindTag.value === "Some") {
        const innerVal = v.fields.get("val");
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
  }
}

/** Convert a Value to a plain JS value (for inferTypes programmatic output). */
function valueToJsLiteral(v: Value): unknown {
  switch (v.type) {
    case "str":
      return v.value;
    case "bool":
      return v.value;
    case "int":
    case "float":
      return v.value;
    case "list":
      return v.items.map(valueToJsLiteral);
    case "dict": {
      const obj: Record<string, unknown> = {};
      for (const [k, val] of v.fields) {
        obj[k] = valueToJsLiteral(val);
      }
      return obj;
    }
  }
}
