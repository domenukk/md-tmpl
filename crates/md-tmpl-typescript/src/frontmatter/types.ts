/**
 * Frontmatter data types and their string representation.
 *
 * @module
 */

import { type Value } from "../value.js";
import {
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
} from "../consts.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/** A single parameter declaration from frontmatter. */
export interface VarDecl {
  readonly name: string;
  readonly varType: VarType;
  readonly defaultValue?: Value;
  readonly loc?: { line: number; column: number; snippet: string };
}

/** The type of a template variable. */
export type VarType =
  | { kind: typeof TYPE_STR }
  | { kind: typeof TYPE_BOOL }
  | { kind: typeof TYPE_INT }
  | { kind: typeof TYPE_FLOAT }
  | { kind: typeof TYPE_LIST; fields: readonly VarDecl[] }
  | { kind: typeof TYPE_SCALAR_LIST; elementType: VarType }
  | { kind: typeof TYPE_STRUCT; fields: readonly VarDecl[] }
  | { kind: typeof TYPE_TMPL; fields: readonly VarDecl[] }
  | {
      kind: typeof TYPE_ENUM;
      variants: readonly VariantDecl[];
      isOption?: boolean;
    }
  | { kind: typeof TYPE_OPTION; innerType: VarType }
  | { kind: typeof TYPE_ALIAS; name: string }
  | { kind: typeof TYPE_UNTYPED_LIST };

/** A variant in an enum type. */
export interface VariantDecl {
  readonly name: string;
  readonly fields: readonly VarDecl[];
}

/** Parsed frontmatter. */
export interface Frontmatter {
  readonly name?: string;
  readonly description?: string;
  readonly params: readonly VarDecl[];
  readonly allowUnused: boolean;
  readonly typeAliases: ReadonlyMap<string, VarType>;
  readonly consts: readonly VarDecl[];
  /** Compile-time environment variable declarations. */
  readonly env: readonly VarDecl[];
  readonly imports: readonly ImportDecl[];
  /** Resolved constants from imports, keyed by `stem.NAME`. */
  readonly importedConsts: Readonly<Record<string, unknown>>;
  /**
   * Type information for imported namespaces, keyed by import stem.
   *
   * Each entry maps a stem name (e.g. `"artist"`) to a `Struct` type
   * whose fields correspond to the imported template's typed consts.
   * Used by the type checker to validate field accesses and for-loop
   * iteration over imported consts.
   */
  readonly importedNamespaceTypes: ReadonlyMap<string, VarType>;
  /**
   * Param defaults that couldn't be resolved during frontmatter parsing
   * (e.g., references to imported consts like `stem.NAME`).
   * Maps param name → raw default text.
   */
  readonly unresolvedDefaults: ReadonlyMap<
    string,
    { text: string; varType: VarType }
  >;
  readonly bodyStartLine?: number;
}

/** An import declaration. */
export interface ImportDecl {
  readonly stem: string;
  readonly path: string;
  readonly loc?: { line: number; column: number; snippet: string };
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/** Format a VarType as a string (for declarations output). */
export function varTypeToString(vt: VarType): string {
  switch (vt.kind) {
    case TYPE_STR:
    case TYPE_BOOL:
    case TYPE_INT:
    case TYPE_FLOAT:
      return vt.kind;
    case TYPE_LIST:
      if (vt.fields.length === 0) return "list()";
      return `list(${vt.fields.map((f) => `${f.name} = ${varTypeToString(f.varType)}`).join(", ")})`;
    case TYPE_SCALAR_LIST:
      return `list(${varTypeToString(vt.elementType)})`;
    case TYPE_STRUCT:
      if (vt.fields.length === 0) return "struct()";
      return `struct(${vt.fields.map((f) => `${f.name} = ${varTypeToString(f.varType)}`).join(", ")})`;
    case TYPE_TMPL:
      if (vt.fields.length === 0) return "tmpl()";
      return `tmpl(${vt.fields.map((f) => `${f.name} = ${varTypeToString(f.varType)}`).join(", ")})`;
    case TYPE_ENUM: {
      if (vt.isOption) {
        const someVariant = vt.variants.find((v) => v.name === OPTION_SOME);
        if (someVariant?.fields.length === 1) {
          const firstField = someVariant.fields[0];
          if (firstField) {
            return `option(${varTypeToString(firstField.varType)})`;
          }
        }
      }
      const parts = vt.variants.map((v) => {
        if (v.fields.length === 0) return v.name;
        return `${v.name}(${v.fields.map((f) => `${f.name} = ${varTypeToString(f.varType)}`).join(", ")})`;
      });
      return `enum(${parts.join(", ")})`;
    }
    case TYPE_OPTION:
      return `option(${varTypeToString(vt.innerType)})`;
    case TYPE_ALIAS:
      return vt.name;
    case TYPE_UNTYPED_LIST:
      return "list()";
  }
}
