/**
 * Shared type definitions for the Template module.
 *
 * @module
 */

import { type VarDecl, type VarType } from "../frontmatter.js";
import { type Node } from "../parser.js";
import { type Value } from "../value.js";

// ---------------------------------------------------------------------------
// ITemplate — shared interface for all template backends
// ---------------------------------------------------------------------------

/**
 * Common interface for template backends (pure-TS and WASM).
 *
 * Both `Template` (pure TypeScript) and the WASM `Template` implement this
 * interface. Use `ITemplate` when you want backend-agnostic code, e.g.
 * `TypedTemplate.wrap()` accepts any `ITemplate`.
 *
 * @example
 * ```ts
 * // Works with either backend:
 * import { Template } from "md-tmpl";         // pure TS
 * import { Template as WasmTemplate } from "md-tmpl-wasm";
 *
 * function greet(tmpl: ITemplate, name: string): string {
 *   return tmpl.render({ name });
 * }
 * ```
 */
export interface ITemplate {
  /** Render the template with the given parameters. */
  render(params: Record<string, unknown>): string;

  /** Render without strict parameter validation. */
  renderUnchecked(params: Record<string, unknown>): string;

  /** Return parameter declarations as `[name, typeString]` tuples. */
  declarations(): [string, string][];

  /** Return a content hash of the template source. */
  sourceHash(): number;

  /** Return default values for parameters that declare them. */
  defaults(): Record<string, unknown>;

  /** Return constants defined in the template's frontmatter. */
  consts(): Record<string, unknown>;

  /** Render a template using only default values (no user-provided params). */
  renderEmpty(): string;

  /** Return constants imported from other templates. */
  importedConsts(): Record<string, unknown>;

  /** Return the raw template body after frontmatter stripping. */
  body(): string;
}

// ---------------------------------------------------------------------------
// CompileOptions — compile-time configuration
// ---------------------------------------------------------------------------

/**
 * Options for compile-time template configuration.
 *
 * `env` provides values for `env:` frontmatter declarations.
 * Values can be any JS type — strings are parsed to the declared type
 * (backward compat), other types are converted directly via `fromJs`.
 *
 * @example
 * ```ts
 * const tmpl = Template.fromSourceWithOptions(source, {
 *   env: { PATH: '/usr/local/prompts', MAX_RETRIES: 5, DEBUG: true },
 *   allowUnused: true,
 * });
 * ```
 */
export interface CompileOptions {
  /** Compile-time environment variable values (typed). */
  readonly env?: Record<string, unknown>;
  /** Base directory for resolving imported templates. */
  readonly baseDir?: string;
  /**
   * Allow declared parameters that are never referenced in the template
   * body. When omitted, unused parameters are rejected (unless the
   * frontmatter itself opts in via `allow_unused`).
   */
  readonly allowUnused?: boolean;
}

/**
 * A compiled include entry, ready for rendering without re-parsing.
 */
export interface CachedInclude {
  readonly nodes: readonly Node[];
  readonly consts: ReadonlyMap<string, Value>;
  readonly declarations: readonly VarDecl[];
  readonly baseDir: string;
  /** Type aliases from the child's own frontmatter + imports. */
  readonly typeAliases?: ReadonlyMap<string, VarType>;
}
