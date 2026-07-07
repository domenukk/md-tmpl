/**
 * Main Template class — the public API for parsing and rendering templates.
 *
 * Mirrors the Python `Template` class API:
 * - `Template.fromSource(source)` — parse from string
 * - `Template.fromFile(path)` — load from file
 * - `template.render({ name: "world" })` — render with params
 * - `template.renderDict(params)` — render from a dict
 * - `template.declarations()` — get param declarations
 *
 * @module
 */

// Lazy-loaded Node.js modules — avoids top-level imports that break
// browsers, Deno, edge runtimes, and other non-Node environments.
// Only code paths that perform file I/O will trigger the require().
let _fs: typeof import("node:fs") | undefined;
let _path: typeof import("node:path") | undefined;
function getFs(): typeof import("node:fs") {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const mod = _fs ?? (_fs = require("node:fs"));
  return mod;
}
function getPath(): typeof import("node:path") {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const mod = _path ?? (_path = require("node:path"));
  return mod;
}
import { Context } from "./context.js";
import {
  TemplateError,
  TemplateSyntaxError,
  MissingParamsError,
  TypeMismatchError,
  ExtraParamsError,
} from "./errors.js";
import {
  type Frontmatter,
  type VarDecl,
  type VarType,
  parseFrontmatter,
  varTypeToString,
  interpolatePathStr,
} from "./frontmatter.js";
import {
  validateFrontmatter,
  validateBodyCollisions,
  validateDisplayability,
} from "./validation.js";
import {
  type Node,
  type RenderOptions,
  Scope as ScopeImpl,
  parseBody,
  renderNodes,
} from "./parser.js";
import {
  type Value,
  ENUM_TAG_KEY,
  ENUM_VARIANTS_KEY,
  NONE,
  str,
  list,
  structVal,
  fromJs,
  valueToJs,
} from "./value.js";
import { renderDirect, type DirectRenderOptions } from "./direct_renderer.js";
import {
  OPTION_SOME,
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_ENUM,
  TYPE_OPTION,
  TYPE_NONE,
  TYPE_ALIAS,
  TYPE_SCALAR_LIST,
  TYPE_UNTYPED_LIST,
  EXPR_START,
  isValidResolvedPath,
} from "./consts.js";
import { parseLiteral } from "./frontmatter.js";

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
 * const tmpl = Template.fromSourceWithEnv(source, {
 *   env: { PATH: '/usr/local/prompts', MAX_RETRIES: 5, DEBUG: true },
 * });
 * ```
 */
export interface CompileOptions {
  /** Compile-time environment variable values (typed). */
  readonly env?: Record<string, unknown>;
  /** Base directory for resolving imported templates. */
  readonly baseDir?: string;
}

// ---------------------------------------------------------------------------
// Template class
// ---------------------------------------------------------------------------

/**
 * A parsed, validated template ready for rendering.
 *
 * Implements {@link ITemplate} — can be used interchangeably with the WASM backend.
 *
 * @example
 * ```ts
 * const tmpl = Template.fromSource(`
 * ---
 * params:
 *   - name = str
 * ---
 * Hello {{ name }}!
 * `);
 * console.log(tmpl.render({ name: "world" }));
 * // → "Hello world!"
 * ```
 */
export class Template implements ITemplate {
  private readonly fm: Frontmatter;
  private readonly bodyStr: string;
  private readonly nodes: Node[];
  private readonly hash: number;
  private readonly _basePath: string | undefined;
  /** Pre-computed set of declared parameter names. */
  private readonly declaredNames: ReadonlySet<string>;
  /** Pre-computed default values (as JS values). */
  private readonly defaultValues: ReadonlyMap<string, unknown>;
  /** Pre-computed constant values. */
  private readonly constValues: ReadonlyMap<string, Value>;
  /** Pre-computed constant values as plain JS (for direct renderer). */
  private readonly constJsValues: ReadonlyMap<string, unknown>;
  /** Pre-computed set of option-typed parameter names/paths. */
  private readonly optionParams: ReadonlySet<string>;
  private _maxIncludeDepth = 16;
  /** Optional reference to the TemplateCache that loaded this template. */
  _cache?: TemplateCache;
  private readonly _includeCache = new Map<
    string,
    { hash: number; mtimeMs: number; cached: CachedInclude }
  >();

  /** Get the base path for include resolution (if set). */
  get basePath(): string | undefined {
    return this._basePath;
  }

  /** Get the current max include depth. */
  get maxIncludeDepth(): number {
    return this._maxIncludeDepth;
  }

  private constructor(
    fm: Frontmatter,
    bodyStr: string,
    nodes: Node[],
    source: string,
    basePath?: string,
  ) {
    this.fm = fm;
    this.bodyStr = bodyStr;
    this.nodes = nodes;
    this.hash = hashString(source);
    this._basePath = basePath;

    // Pre-compute immutable render data
    this.declaredNames = new Set(fm.params.map((d) => d.name));
    const defaults = new Map<string, unknown>();
    for (const decl of fm.params) {
      if (decl.defaultValue !== undefined) {
        defaults.set(decl.name, valueToJs(decl.defaultValue));
      }
    }
    this.defaultValues = defaults;
    const consts = new Map<string, Value>();
    for (const decl of fm.consts) {
      if (decl.defaultValue !== undefined) {
        consts.set(decl.name, decl.defaultValue);
      }
    }
    // Env declarations are resolved at compile time and behave like consts.
    for (const decl of fm.env) {
      if (decl.defaultValue !== undefined) {
        consts.set(decl.name, decl.defaultValue);
      }
    }
    for (const [key, jsVal] of Object.entries(fm.importedConsts)) {
      consts.set(key, fromJs(jsVal));
    }
    this.constValues = consts;
    const constsJs = new Map<string, unknown>();
    for (const decl of fm.consts) {
      if (decl.defaultValue !== undefined) {
        constsJs.set(decl.name, valueToJs(decl.defaultValue));
      }
    }
    // Env declarations are resolved at compile time and behave like consts.
    for (const decl of fm.env) {
      if (decl.defaultValue !== undefined) {
        constsJs.set(decl.name, valueToJs(decl.defaultValue));
      }
    }
    for (const [key, jsVal] of Object.entries(fm.importedConsts)) {
      constsJs.set(key, jsVal);
    }
    this.constJsValues = constsJs;

    // Pre-compute option-typed parameter names for kind()/match awareness
    const optParams = new Set<string>();
    for (const decl of fm.params) {
      collectOptionPaths(decl.name, decl.varType, fm.typeAliases, optParams);
    }
    this.optionParams = optParams;

    // Inject enum type constants from type aliases.
    // For each enum type (e.g., `Stage = enum(Design, Build)`), create a
    // constant dict mapping variant names → values, enabling expressions
    // like `{{ Stage.Design }}`.  User-defined constants are never overwritten.
    injectEnumTypeConstants(fm.typeAliases, consts, constsJs);
  }

  // ── Static constructors ──────────────────────────────────────────────

  /**
   * Parse a template from an in-memory string.
   *
   * Unused declared parameters (present in frontmatter but not in
   * the template body) are rejected. Use `fromSourceAllowingUnused()`
   * to suppress this check.
   *
   * @throws {TemplateSyntaxError} If the source contains syntax errors.
   */
  static fromSource(source: string): Template {
    const [fm, body] = parseFrontmatter(source);
    validateFrontmatter(fm);
    const nodes = parseBody(body, false, fm.bodyStartLine ?? 1);
    const tmpl = new Template(fm, body, nodes, source);
    if (!fm.allowUnused) {
      tmpl.checkUnusedParams(body);
    }
    tmpl.checkBareEnumAccess();
    validateBodyCollisions(
      fm,
      collectInlineTemplateNames(nodes),
      collectForBindings(nodes),
    );
    validateDisplayability(nodes, fm.params, fm.consts, fm.typeAliases);
    return tmpl;
  }

  /**
   * Parse a template from source with compile-time environment variables.
   *
   * Resolves `env:` declarations using the provided `CompileOptions.env`
   * values, type-checks them, and injects them into the template scope.
   *
   * @throws {TemplateSyntaxError} If an env var is missing without a default.
   * @throws {TemplateSyntaxError} If an env var value fails type checking.
   */
  static fromSourceWithEnv(source: string, options: CompileOptions): Template {
    const [fm, body] = parseFrontmatter(source);
    let resolvedFm = resolveEnvDeclarations(fm, options.env ?? {});
    validateFrontmatter(resolvedFm);
    const baseDir = options.baseDir;
    if (baseDir) {
      resolvedFm = resolveImportedConsts(resolvedFm, baseDir);
    }
    const nodes = parseBody(body, false, resolvedFm.bodyStartLine ?? 1);
    const tmpl = new Template(resolvedFm, body, nodes, source, baseDir);
    if (!resolvedFm.allowUnused) {
      tmpl.checkUnusedParams(body);
    }
    tmpl.checkBareEnumAccess();
    validateBodyCollisions(
      resolvedFm,
      collectInlineTemplateNames(nodes),
      collectForBindings(nodes),
    );
    validateDisplayability(
      nodes,
      resolvedFm.params,
      resolvedFm.consts,
      resolvedFm.typeAliases,
    );
    return tmpl;
  }

  /**
   * Parse a template, allowing declared parameters that aren't used.
   */
  static fromSourceAllowingUnused(source: string): Template {
    const [fm, body] = parseFrontmatter(source);
    const nodes = parseBody(body, false, fm.bodyStartLine ?? 1);
    const tmpl = new Template(fm, body, nodes, source);
    tmpl.checkBareEnumAccess();
    validateDisplayability(nodes, fm.params, fm.consts, fm.typeAliases);
    return tmpl;
  }

  /**
   * Parse a template with a base directory for include resolution.
   */
  static fromSourceWithBaseDir(source: string, baseDir: string): Template {
    const [fm, body] = parseFrontmatter(source);
    validateFrontmatter(fm);
    const resolvedFm = resolveImportedConsts(fm, baseDir);
    const nodes = parseBody(body, false, resolvedFm.bodyStartLine ?? 1);
    const tmpl = new Template(resolvedFm, body, nodes, source, baseDir);
    if (!resolvedFm.allowUnused) {
      tmpl.checkUnusedParams(body);
    }
    tmpl.checkBareEnumAccess();
    validateBodyCollisions(
      resolvedFm,
      collectInlineTemplateNames(nodes),
      collectForBindings(nodes),
    );
    validateDisplayability(
      nodes,
      resolvedFm.params,
      resolvedFm.consts,
      resolvedFm.typeAliases,
    );
    return tmpl;
  }

  /**
   * Load a template from a `.tmpl.md` file.
   *
   * @throws {TemplateError} If the file cannot be read.
   * @throws {TemplateSyntaxError} If the file contains syntax errors.
   */
  static fromFile(filePath: string): Template {
    return Template.fromFileWithEnv(filePath);
  }

  /**
   * Load a template from a `.tmpl.md` file with compile-time env values.
   *
   * Resolves `env:` declarations using the provided options before
   * resolving imports, so `{{ PROMPTS_DIR }}` can be used in import paths.
   *
   * @throws {TemplateError} If the file cannot be read.
   * @throws {TemplateSyntaxError} If the file contains syntax errors.
   * @throws {TemplateSyntaxError} If an env var is missing without a default.
   */
  static fromFileWithEnv(filePath: string, options?: CompileOptions): Template {
    let source: string;
    try {
      source = getFs().readFileSync(filePath, "utf-8");
    } catch (err) {
      throw new TemplateError(
        `failed to load template: ${filePath}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
    const absPath = getPath().resolve(filePath);
    const baseDir = getPath().dirname(absPath);
    const [fm, body] = parseFrontmatter(source);
    // Resolve env declarations before import resolution so env values
    // (e.g. PROMPTS_DIR) are available in import paths.
    const envResolved = resolveEnvDeclarations(fm, options?.env ?? {});
    validateFrontmatter(envResolved);
    const resolvedFm = resolveImportedConsts(envResolved, baseDir);
    const nodes = parseBody(body, false, resolvedFm.bodyStartLine ?? 1);
    const tmpl = new Template(resolvedFm, body, nodes, source, baseDir);
    if (!resolvedFm.allowUnused) {
      tmpl.checkUnusedParams(body);
    }
    tmpl.checkBareEnumAccess();
    validateBodyCollisions(
      resolvedFm,
      collectInlineTemplateNames(nodes),
      collectForBindings(nodes),
    );
    validateDisplayability(
      nodes,
      resolvedFm.params,
      resolvedFm.consts,
      resolvedFm.typeAliases,
    );
    return tmpl;
  }

  // ── Rendering ────────────────────────────────────────────────────────

  /**
   * Render the template with keyword arguments.
   *
   * All arguments are validated against frontmatter type declarations.
   * Extra undeclared parameters produce an error (use `allowExtra` to suppress).
   *
   * @param params - Template parameters as a plain object.
   * @param options - Render options.
   * @throws {MissingParamsError} If required parameters are missing.
   * @throws {TypeMismatchError} If a value has the wrong type.
   * @throws {ExtraParamsError} If undeclared parameters are provided.
   *
   * @example
   * ```ts
   * tmpl.render({ name: "world", count: 42 });
   * ```
   */
  render(
    params: Record<string, unknown> = {},
    options?: { allowExtra?: boolean },
  ): string {
    const ctx = this.buildContext(params, options?.allowExtra ?? false);
    return this.renderWithContext(ctx);
  }

  /**
   * Render without type validation — fastest path.
   *
   * Converts params to Values and renders directly, skipping all
   * type-checking. Use after you've validated once with `render()`,
   * or when rendering with `TypedTemplate<P>` and trusting
   * TypeScript's compile-time checks.
   *
   * @example
   * ```ts
   * // Validate once, then render fast in a loop:
   * tmpl.render(params);  // validates types
   * for (const p of manyParams) {
   *   tmpl.renderUnchecked(p);  // no validation overhead
   * }
   * ```
   */
  renderUnchecked(params: Record<string, unknown> = {}): string {
    // Build a flat Map<string, unknown> with defaults + provided params
    const flat = new Map<string, unknown>(this.defaultValues);
    for (const [key, value] of Object.entries(params)) {
      if (this.declaredNames.has(key)) {
        flat.set(key, value);
      }
    }
    // Use direct renderer — no Value conversion, no Map wrapping
    return renderDirect(
      this.nodes,
      flat,
      this.constJsValues,
      this.getDirectOptions(),
    );
  }

  /**
   * Render the template from a Map or Record of parameters.
   */
  renderDict(
    params: Record<string, unknown> | Map<string, unknown>,
    options?: { allowExtra?: boolean },
  ): string {
    const obj = params instanceof Map ? Object.fromEntries(params) : params;
    return this.render(obj, options);
  }

  /**
   * Render a template that takes no user-provided parameters.
   *
   * If the template declares parameters, they must all have defaults.
   * Calling `renderEmpty()` on a template with required (no-default)
   * parameters throws an error.
   *
   * More efficient than `render({})` — skips context building and
   * validation entirely.
   *
   * @example
   * ```ts
   * const tmpl = Template.fromSource("Hello world!");
   * tmpl.renderEmpty(); // => "Hello world!"
   *
   * const tmpl2 = Template.fromSource(
   *   `---
params:
  - greeting = str := "Hi"
---
{{ greeting }}!`
   * );
   * tmpl2.renderEmpty(); // => "Hi!"
   * ```
   *
   * @throws Error if any declared parameter lacks a default value.
   */
  renderEmpty(): string {
    // Check for required params (no default)
    const missing = this.fm.params
      .filter((d) => d.defaultValue === undefined)
      .map((d) => d.name);
    if (missing.length > 0) {
      throw new Error(
        `render_empty: template has required parameters without defaults: ${missing.join(", ")}`,
      );
    }
    // All params have defaults — use direct renderer with just defaults + consts
    return renderDirect(
      this.nodes,
      this.defaultValues,
      this.constJsValues,
      this.getDirectOptions(),
    );
  }

  // ── Metadata ─────────────────────────────────────────────────────────

  /**
   * Return parameter declarations as `[name, typeString]` tuples.
   */
  declarations(): [string, string][] {
    return this.fm.params.map((d) => [d.name, varTypeToString(d.varType)]);
  }

  /**
   * Return a content hash of the template source.
   *
   * Two templates with the same source produce the same hash.
   */
  sourceHash(): number {
    return this.hash;
  }

  /**
   * Return default values for parameters that declare them.
   */
  defaults(): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    for (const decl of this.fm.params) {
      if (decl.defaultValue !== undefined) {
        if (decl.defaultValue.type === TYPE_NONE) {
          result[decl.name] = null;
        } else {
          result[decl.name] = valueToJs(decl.defaultValue);
        }
      }
    }
    return result;
  }

  /**
   * Return constants defined in the template's frontmatter.
   */
  consts(): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    for (const decl of this.fm.consts) {
      if (decl.defaultValue !== undefined) {
        result[decl.name] = valueToJs(decl.defaultValue);
      }
    }
    return result;
  }

  /**
   * Return constants imported from other templates.
   *
   * These are keyed by `stem.NAME` (e.g. `other.MAX_RETRIES`).
   * Only populated when the template is loaded from a file or
   * constructed with a base directory.
   */
  importedConsts(): Record<string, unknown> {
    return { ...this.fm.importedConsts };
  }

  /**
   * Return the raw template body after frontmatter stripping.
   */
  body(): string {
    return this.bodyStr;
  }

  /**
   * Set the maximum include depth for rendering.
   */
  setMaxIncludeDepth(depth: number): void {
    this._maxIncludeDepth = depth;
  }

  /**
   * Validate that this template's declarations match an expected set.
   *
   * @throws {TemplateError} If the declarations don't match.
   */
  validateDeclarationsAgainst(expected: [string, string][]): void {
    const current = this.declarations();
    if (current.length !== expected.length) {
      throw new TemplateError(
        `template declarations changed: got ${JSON.stringify(current)}`,
      );
    }
    for (let i = 0; i < current.length; i++) {
      if (
        current[i]![0] !== expected[i]![0] ||
        current[i]![1] !== expected[i]![1]
      ) {
        throw new TemplateError(
          `template declarations changed: got ${JSON.stringify(current)}`,
        );
      }
    }
  }

  /** String representation. */
  toString(): string {
    const decls = this.fm.params
      .map((d) => `${d.name}=${varTypeToString(d.varType)}`)
      .join(", ");
    return `Template(params=[${decls}])`;
  }

  /** Get the frontmatter (for type generation). */
  get frontmatter(): Frontmatter {
    return this.fm;
  }

  // ── Private ──────────────────────────────────────────────────────────

  private buildContext(
    params: Record<string, unknown>,
    allowExtra: boolean,
  ): Context {
    const ctx = new Context();

    // Apply pre-computed defaults (with option auto-wrapping)
    for (const [name, value] of this.defaultValues) {
      const decl = this.fm.params.find((p) => p.name === name);
      if (decl) {
        ctx.setRaw(name, jsToValue(value, decl.varType, this.fm.typeAliases));
      } else {
        ctx.set(name, value);
      }
    }

    // Check for extra params
    const providedKeys = Object.keys(params);
    if (!allowExtra) {
      const extra = providedKeys.filter((k) => !this.declaredNames.has(k));
      if (extra.length > 0) {
        throw new ExtraParamsError(extra);
      }
    }

    // Set provided values (with option-transparent conversion)
    for (const [key, value] of Object.entries(params)) {
      if (this.declaredNames.has(key)) {
        const decl = this.fm.params.find((p) => p.name === key);
        if (decl) {
          ctx.setRaw(key, jsToValue(value, decl.varType, this.fm.typeAliases));
        } else {
          ctx.set(key, value);
        }
      }
    }

    // Check for missing required params (those without defaults)
    const missing: string[] = [];
    for (const decl of this.fm.params) {
      if (decl.defaultValue === undefined && !(decl.name in params)) {
        missing.push(decl.name);
      }
    }
    if (missing.length > 0) {
      throw new MissingParamsError(missing);
    }

    // Type-check all values
    for (const decl of this.fm.params) {
      const value = ctx.get(decl.name);
      if (value !== undefined) {
        this.typeCheck(decl.name, value, decl.varType);
      }
    }

    return ctx;
  }

  private typeCheck(path: string, value: Value, varType: VarType): void {
    switch (varType.kind) {
      case TYPE_STR:
        if (value.type !== TYPE_STR) {
          throw new TypeMismatchError(path, "str", value.type);
        }
        break;
      case TYPE_BOOL:
        if (value.type !== TYPE_BOOL) {
          throw new TypeMismatchError(path, "bool", value.type);
        }
        break;
      case TYPE_INT:
        if (value.type !== TYPE_INT) {
          throw new TypeMismatchError(path, "int", value.type);
        }
        break;
      case TYPE_FLOAT:
        if (value.type !== TYPE_FLOAT && value.type !== TYPE_INT) {
          throw new TypeMismatchError(path, "float", value.type);
        }
        break;
      case TYPE_LIST:
        if (value.type !== TYPE_LIST) {
          throw new TypeMismatchError(path, "list", value.type);
        }
        // Check item types
        if (varType.fields.length > 0) {
          for (let i = 0; i < value.items.length; i++) {
            const item = value.items[i]!;
            if (item.type !== TYPE_STRUCT) {
              throw new TypeMismatchError(`${path}[${i}]`, "struct", item.type);
            }
            for (const field of varType.fields) {
              const fieldVal = item.fields.get(field.name);
              if (fieldVal === undefined) {
                throw new MissingParamsError([`${path}[${i}].${field.name}`]);
              }
              this.typeCheck(
                `${path}[${i}].${field.name}`,
                fieldVal,
                field.varType,
              );
            }
          }
        }
        break;
      case TYPE_STRUCT:
        if (value.type !== TYPE_STRUCT) {
          throw new TypeMismatchError(path, "struct", value.type);
        }
        // Check fields
        for (const field of varType.fields) {
          const fieldVal = value.fields.get(field.name);
          if (fieldVal === undefined) {
            throw new MissingParamsError([`${path}.${field.name}`]);
          }
          this.typeCheck(`${path}.${field.name}`, fieldVal, field.varType);
        }
        break;
      case TYPE_ENUM:
        if (varType.isOption) {
          // Legacy isOption: transparent — none is valid, otherwise check inner
          if (value.type === TYPE_NONE) break;
          const someVariant = varType.variants.find(
            (v) => v.name === OPTION_SOME,
          );
          if (someVariant && someVariant.fields.length === 1) {
            this.typeCheck(path, value, someVariant.fields[0]!.varType);
          }
          break;
        }
        if (value.type === TYPE_STR) {
          // Unit variant as string
          const validNames = varType.variants.map((v) => v.name);
          if (!validNames.includes(value.value)) {
            throw new TypeMismatchError(
              path,
              `enum(${validNames.join(", ")})`,
              `str("${value.value}")`,
            );
          }
        } else if (value.type === TYPE_STRUCT) {
          // Struct variant as struct with __kind__
          const tag = value.fields.get(ENUM_TAG_KEY);
          if (tag === undefined || tag.type !== TYPE_STR) {
            throw new TypeMismatchError(
              path,
              "enum variant",
              "struct without __kind__",
            );
          }
          const validNames = varType.variants.map((v) => v.name);
          if (!validNames.includes(tag.value)) {
            throw new TypeMismatchError(
              path,
              `enum(${validNames.join(", ")})`,
              `variant("${tag.value}")`,
            );
          }
        } else {
          throw new TypeMismatchError(path, "enum", value.type);
        }
        break;
      case TYPE_OPTION:
        // Transparent option: none is always valid, otherwise check inner type
        if (value.type === TYPE_NONE) break;
        this.typeCheck(path, value, varType.innerType);
        break;
      case TYPE_ALIAS:
        // Resolve alias from type aliases
        {
          const resolved = this.fm.typeAliases.get(varType.name);
          if (resolved) {
            this.typeCheck(path, value, resolved);
          }
          // If alias not found, skip check (may be imported type)
        }
        break;
      case TYPE_SCALAR_LIST:
        if (value.type !== TYPE_LIST) {
          throw new TypeMismatchError(path, "list", value.type);
        }
        // Check each element against the declared element type
        for (let i = 0; i < value.items.length; i++) {
          this.typeCheck(`${path}[${i}]`, value.items[i]!, varType.elementType);
        }
        break;
      case TYPE_UNTYPED_LIST:
        if (value.type !== TYPE_LIST) {
          throw new TypeMismatchError(path, "list", value.type);
        }
        break;
    }
  }

  private renderWithContext(ctx: Context): string {
    const scope = new ScopeImpl(
      ctx.values,
      this.constValues,
      this.optionParams,
    );
    const options: RenderOptions = {
      maxIncludeDepth: this._maxIncludeDepth,
    };

    // Wire up file-based include resolution if we have a base path or cache
    if (this._basePath || this._cache) {
      const defaultBase = this._basePath ?? "";
      options.templateLoader = (
        includePath: string,
        basePath?: string,
      ):
        | [
            readonly Node[],
            ReadonlyMap<string, Value>,
            readonly VarDecl[],
            string?,
          ]
        | undefined => {
        const cached = this.resolveInclude(
          includePath,
          basePath ?? defaultBase,
        );
        if (!cached) return undefined;
        return [
          cached.nodes,
          cached.consts,
          cached.declarations,
          cached.baseDir,
        ];
      };
    }
    // Collect inline template definitions ({% tmpl name %}...{% /tmpl %})
    // Parse each inline template's frontmatter to extract declarations
    // for contract validation and type checking at include time.
    const inlineTmpls = new Map<
      string,
      {
        declarations: readonly VarDecl[];
        body: string;
        consts: Map<string, Value>;
      }
    >();
    for (const n of this.nodes) {
      if (n.kind === "tmpl") {
        if (n.source.trimStart().startsWith("---")) {
          const [inlineFm, inlineBody] = parseFrontmatter(n.source);
          const inlineConsts = new Map<string, Value>();
          for (const decl of inlineFm.consts) {
            if (decl.defaultValue !== undefined) {
              inlineConsts.set(decl.name, decl.defaultValue);
            }
          }
          inlineTmpls.set(n.name, {
            declarations: inlineFm.params,
            body: inlineBody,
            consts: inlineConsts,
          });
        } else {
          inlineTmpls.set(n.name, {
            declarations: [],
            body: n.source,
            consts: new Map(),
          });
        }
      }
    }
    if (inlineTmpls.size > 0) {
      options.inlineTemplates = inlineTmpls;
    }

    return renderNodes(this.nodes, scope, options);
  }

  private checkUnusedParams(body: string): void {
    for (const decl of this.fm.params) {
      const pattern = new RegExp(`\\b${escapeRegex(decl.name)}\\b`);
      if (!pattern.test(body)) {
        throw new TemplateSyntaxError(
          `unused parameter '${decl.name}' declared but not referenced in body`,
          decl.loc?.line,
          decl.loc?.column,
          decl.loc?.snippet,
        );
      }
    }
  }

  private getDirectOptions(): DirectRenderOptions {
    const inlineTemplates = new Map<
      string,
      {
        declarations: readonly VarDecl[];
        nodes: readonly Node[];
        consts: Map<string, unknown>;
      }
    >();
    for (const n of this.nodes) {
      if (n.kind === "tmpl") {
        if (n.source.trimStart().startsWith("---")) {
          const [inlineFm, inlineBody] = parseFrontmatter(n.source);
          const inlineConsts = new Map<string, unknown>();
          for (const decl of inlineFm.consts) {
            if (decl.defaultValue !== undefined) {
              inlineConsts.set(decl.name, valueToJs(decl.defaultValue));
            }
          }
          inlineTemplates.set(n.name, {
            declarations: inlineFm.params,
            nodes: parseBody(inlineBody, true),
            consts: inlineConsts,
          });
        } else {
          inlineTemplates.set(n.name, {
            declarations: [],
            nodes: parseBody(n.source, true),
            consts: new Map(),
          });
        }
      }
    }

    const defaultBase = this._basePath ?? "";
    const templateLoader = (
      includePath: string,
      basePath?: string,
    ):
      | [
          readonly Node[],
          ReadonlyMap<string, unknown>,
          readonly VarDecl[],
          string?,
        ]
      | undefined => {
      const cached = this.resolveInclude(includePath, basePath ?? defaultBase);
      if (!cached) return undefined;
      const constsJs = new Map<string, unknown>();
      for (const [k, v] of cached.consts) {
        constsJs.set(k, valueToJs(v));
      }
      return [cached.nodes, constsJs, cached.declarations, cached.baseDir];
    };

    return {
      inlineTemplates: inlineTemplates.size > 0 ? inlineTemplates : undefined,
      templateLoader:
        this._basePath || this._cache ? templateLoader : undefined,
      maxIncludeDepth: this._maxIncludeDepth,
    };
  }

  /**
   * Reject bare enum literal expressions like `{{ Stage.Design }}`.
   *
   * Only `{{ kind(Stage.Design) }}` is allowed — using the enum
   * literal as a bare expression output is a compile error.
   */
  private checkBareEnumAccess(): void {
    const enumTypeNames = new Set<string>();
    for (const [name, varType] of this.fm.typeAliases) {
      if (varType.kind === "enum" && !varType.isOption) {
        enumTypeNames.add(name);
      }
    }
    if (enumTypeNames.size === 0) return;
    walkNodesForBareEnumAccess(this.nodes, enumTypeNames);
  }

  private resolveInclude(
    includePath: string,
    basePath?: string,
  ): CachedInclude | undefined {
    if (this._cache) {
      return this._cache.resolveInclude(includePath, basePath);
    }
    const currentBase = basePath ?? this._basePath ?? "";
    return resolveIncludeEntry(this._includeCache, includePath, currentBase);
  }
}

/**
 * Convert a JS value to a template Value, handling option types transparently.
 *
 * For `option(T)` fields:
 * - `null`/`undefined` → `NONE` (absent value)
 * - any other value → `fromJs(value)` (the inner value directly)
 *
 * For struct/list fields, recursively converts nested option fields.
 */
function jsToValue(
  value: unknown,
  varType: VarType,
  typeAliases?: ReadonlyMap<string, VarType>,
  seen: WeakSet<object> = new WeakSet(),
  depth = 0,
): Value {
  if (depth > 256) {
    throw new TemplateError(
      "maximum recursion depth exceeded in template parameter",
    );
  }

  // Resolve type aliases before checking
  if (varType.kind === TYPE_ALIAS && typeAliases) {
    const resolved = typeAliases.get(varType.name);
    if (resolved) {
      return jsToValue(value, resolved, typeAliases, seen, depth);
    }
  }

  // Option types: null/undefined → NONE, otherwise convert the inner value
  if (varType.kind === TYPE_OPTION) {
    if (value === null || value === undefined) {
      return NONE;
    }
    return jsToValue(value, varType.innerType, typeAliases, seen, depth);
  }

  // Legacy isOption handling
  if (varType.kind === TYPE_ENUM && varType.isOption) {
    if (value === null || value === undefined) {
      return NONE;
    }
    const someVariant = varType.variants.find((v) => v.name === OPTION_SOME);
    if (someVariant && someVariant.fields.length === 1) {
      return jsToValue(
        value,
        someVariant.fields[0]!.varType,
        typeAliases,
        seen,
        depth,
      );
    }
    return fromJs(value, seen, depth + 1);
  }

  // Non-option enum types: handle struct variants passed as { VariantName: { fields } }
  if (
    varType.kind === TYPE_ENUM &&
    !varType.isOption &&
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value)
  ) {
    const obj = value as Record<string, unknown>;
    const keys = Object.keys(obj);
    if (keys.length === 1) {
      const variantName = keys[0]!;
      const variant = varType.variants.find((v) => v.name === variantName);
      if (variant && variant.fields.length > 0) {
        // This is a struct variant: { VariantName: { field1: val1, ... } }
        const inner = obj[variantName];
        if (
          typeof inner === "object" &&
          inner !== null &&
          !Array.isArray(inner)
        ) {
          const innerObj = inner as Record<string, unknown>;
          const entries: [string, Value][] = [[ENUM_TAG_KEY, str(variantName)]];
          for (const field of variant.fields) {
            if (field.name in innerObj) {
              entries.push([
                field.name,
                jsToValue(
                  innerObj[field.name],
                  field.varType,
                  typeAliases,
                  seen,
                  depth + 1,
                ),
              ]);
            }
          }
          return structVal(entries);
        }
      }
    }
  }

  // Structs: recursively handle nested option fields
  if (
    varType.kind === TYPE_STRUCT &&
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value)
  ) {
    seen.add(value as object);
    try {
      const obj = value as Record<string, unknown>;
      const entries: [string, Value][] = [];
      for (const field of varType.fields) {
        if (field.name in obj) {
          entries.push([
            field.name,
            jsToValue(
              obj[field.name],
              field.varType,
              typeAliases,
              seen,
              depth + 1,
            ),
          ]);
        }
      }
      // Preserve non-declared fields
      for (const [k, v] of Object.entries(obj)) {
        if (!varType.fields.some((f) => f.name === k)) {
          entries.push([k, fromJs(v, seen, depth + 1)]);
        }
      }
      return structVal(entries);
    } finally {
      seen.delete(value as object);
    }
  }

  // Lists with structured items: recursively handle nested option fields
  if (
    varType.kind === "list" &&
    Array.isArray(value) &&
    varType.fields.length > 0
  ) {
    seen.add(value as object);
    try {
      const items = value.map((item) => {
        if (typeof item === "object" && item !== null && !Array.isArray(item)) {
          const obj = item as Record<string, unknown>;
          const entries: [string, Value][] = [];
          for (const field of varType.fields) {
            if (field.name in obj) {
              entries.push([
                field.name,
                jsToValue(
                  obj[field.name],
                  field.varType,
                  typeAliases,
                  seen,
                  depth + 1,
                ),
              ]);
            }
          }
          // Preserve non-declared fields
          for (const [k, v] of Object.entries(obj)) {
            if (!varType.fields.some((f) => f.name === k)) {
              entries.push([k, fromJs(v, seen, depth + 1)]);
            }
          }
          return structVal(entries);
        }
        return fromJs(item, seen, depth + 1);
      });
      return { type: "list", items };
    } finally {
      seen.delete(value as object);
    }
  }

  // Default: use standard fromJs conversion
  return fromJs(value, seen, depth + 1);
}

// ---------------------------------------------------------------------------
// TypedTemplate — compile-time type safety
// ---------------------------------------------------------------------------

/**
 * A typed wrapper around `Template` that provides compile-time
 * parameter type checking.
 *
 * Use with generated types from `generateTypes()` or `inferTypes()`:
 *
 * @example
 * ```ts
 * // 1. Generate types (typically from a build script):
 * //    generateTypesFromFile("greeting.tmpl.md") → greeting.ts
 * //
 * // 2. Use the typed template:
 * import type { Params } from "./greeting.js";
 * import { TypedTemplate, Template } from "md-tmpl";
 *
 * const tmpl = TypedTemplate.fromSource<Params>(
 *   `---
 *   params:
 *     - name = str
 *     - count = int
 *   ---
 *   Hello {{ name }}! ({{ count }})`
 * );
 *
 * // ✅ Type-safe — TypeScript catches wrong types and missing fields
 * tmpl.render({ name: "world", count: 42 });
 *
 * // ❌ Compile error: missing 'count'
 * // tmpl.render({ name: "world" });
 *
 * // ❌ Compile error: wrong type for 'count'
 * // tmpl.render({ name: "world", count: "not a number" });
 * ```
 *
 * @typeParam P - The parameter type (generated from frontmatter).
 */
export class TypedTemplate<P extends object> {
  private readonly inner: ITemplate;
  private validated = false;

  private constructor(inner: ITemplate) {
    this.inner = inner;
  }

  /** Create a typed template from source (uses the pure-TS backend). */
  static fromSource<P extends object>(source: string): TypedTemplate<P> {
    return new TypedTemplate<P>(Template.fromSource(source));
  }

  /** Create a typed template from a file (uses the pure-TS backend). */
  static fromFile<P extends object>(filePath: string): TypedTemplate<P> {
    return new TypedTemplate<P>(Template.fromFile(filePath));
  }

  /**
   * Wrap any `ITemplate` implementation in a typed wrapper.
   *
   * Works with both the pure-TS `Template` and the WASM `Template`:
   *
   * @example
   * ```ts
   * import { Template } from "md-tmpl";
   * import { Template as WasmTemplate } from "md-tmpl-wasm";
   *
   * // Both work:
   * const ts   = TypedTemplate.wrap<Params>(Template.fromSource(src));
   * const wasm = TypedTemplate.wrap<Params>(WasmTemplate.fromSource(src));
   * ```
   */
  static wrap<P extends object>(template: ITemplate): TypedTemplate<P> {
    return new TypedTemplate<P>(template);
  }

  /** Render with compile-time checked parameters. Always validates at runtime. */
  render(params: P): string {
    return this.inner.render(params as Record<string, unknown>);
  }

  /**
   * Render without runtime type validation — fastest path.
   *
   * Trusts TypeScript's compile-time checking. Use when you know
   * the parameter types are correct (e.g., from generated types).
   *
   * @example
   * ```ts
   * const tmpl = TypedTemplate.fromSource<Params>(src);
   * const output = tmpl.renderUnchecked({ name: "world", count: 42 });
   * ```
   */
  renderUnchecked(params: P): string {
    return this.inner.renderUnchecked(params as Record<string, unknown>);
  }

  /**
   * Render with validation on the first call only.
   *
   * The first invocation validates types fully (like `render()`).
   * Subsequent calls skip validation (like `renderUnchecked()`).
   *
   * Ideal for loops where the same parameter shape is rendered
   * many times with different values.
   *
   * @example
   * ```ts
   * const tmpl = TypedTemplate.fromSource<Params>(src);
   * for (const item of items) {
   *   // First iteration validates, subsequent iterations are fast
   *   const output = tmpl.renderTrusted({ name: item.name, count: item.n });
   * }
   * ```
   */
  renderTrusted(params: P): string {
    if (!this.validated) {
      const result = this.inner.render(params as Record<string, unknown>);
      this.validated = true;
      return result;
    }
    return this.inner.renderUnchecked(params as Record<string, unknown>);
  }

  /** Access the underlying template (may be pure-TS or WASM). */
  get template(): ITemplate {
    return this.inner;
  }

  /** Delegate metadata methods. */
  declarations(): [string, string][] {
    return this.inner.declarations();
  }

  sourceHash(): number {
    return this.inner.sourceHash();
  }

  defaults(): Partial<P> {
    return this.inner.defaults() as Partial<P>;
  }

  consts(): Record<string, unknown> {
    return this.inner.consts();
  }

  importedConsts(): Record<string, unknown> {
    return this.inner.importedConsts();
  }

  body(): string {
    return this.inner.body();
  }

  toString(): string {
    return `TypedTemplate(${this.inner})`;
  }
}

// ---------------------------------------------------------------------------
// Template Cache
// ---------------------------------------------------------------------------

/**
 * A compiled include entry, ready for rendering without re-parsing.
 */
export interface CachedInclude {
  readonly nodes: readonly Node[];
  readonly consts: ReadonlyMap<string, Value>;
  readonly declarations: readonly VarDecl[];
  readonly baseDir: string;
}

/**
 * Content-hashed template cache for hot-reload scenarios.
 *
 * Unchanged files return cached compilations with zero re-parsing.
 *
 * @example
 * ```ts
 * const cache = new TemplateCache();
 * const tmpl = cache.load("prompts/greeting.tmpl.md");
 * console.log(tmpl.render({ name: "world" }));
 * ```
 */
export class TemplateCache {
  private readonly cache: Map<string, { hash: number; template: Template }> =
    new Map();
  private readonly includes: Map<
    string,
    { hash: number; mtimeMs: number; cached: CachedInclude }
  > = new Map();
  private readonly maxEntries: number | undefined;

  constructor(options?: { maxEntries?: number }) {
    this.maxEntries = options?.maxEntries;
  }

  /** Load a template, returning a cached version if unchanged. */
  load(filePath: string): Template {
    const absPath = getPath().resolve(filePath);
    let source: string;
    try {
      source = getFs().readFileSync(absPath, "utf-8");
    } catch (err) {
      throw new TemplateError(
        `failed to load template: ${filePath}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }

    const hash = hashString(source);
    const cached = this.cache.get(absPath);
    if (cached && cached.hash === hash) {
      return cached.template;
    }

    const dir = getPath().dirname(absPath);
    const tmpl = Template.fromSourceWithBaseDir(source, dir);
    tmpl._cache = this;
    this.cache.set(absPath, { hash, template: tmpl });

    // LRU eviction: if maxEntries is set and we exceeded capacity, drop oldest
    if (this.maxEntries !== undefined && this.cache.size > this.maxEntries) {
      const oldest = this.cache.keys().next().value;
      if (oldest !== undefined) {
        this.cache.delete(oldest);
      }
    }

    return tmpl;
  }

  /** Invalidate all cached entries. */
  clear(): void {
    this.cache.clear();
    this.includes.clear();
  }

  /** Return the number of cached templates. */
  templateCount(): number {
    return this.cache.size;
  }

  /** Resolve an include from cache or compile it from disk. */
  resolveInclude(
    filePath: string,
    baseDir?: string,
  ): CachedInclude | undefined {
    return resolveIncludeEntry(
      this.includes,
      filePath,
      baseDir,
      this.maxEntries,
    );
  }

  /** Return the number of cached include templates. */
  includeCount(): number {
    return this.includes.size;
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Simple FNV-1a hash for content hashing. */
function hashString(s: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    hash ^= s.charCodeAt(i);
    hash = (hash * 0x01000193) | 0;
  }
  return hash >>> 0; // unsigned 32-bit
}

function resolveIncludeEntry(
  cache: Map<string, { hash: number; mtimeMs: number; cached: CachedInclude }>,
  filePath: string,
  baseDir?: string,
  maxEntries?: number,
): CachedInclude | undefined {
  const currentBase = baseDir ?? "";
  const absPath = getPath().resolve(currentBase, filePath);
  let stat: { mtimeMs: number } | undefined;
  try {
    stat = getFs().statSync(absPath, { throwIfNoEntry: false });
  } catch (_err: unknown) {
    /* statSync can throw on permission errors; treat as not found */
    return undefined;
  }
  if (!stat) {
    return undefined;
  }
  const entry = cache.get(absPath);
  if (entry && entry.mtimeMs === stat.mtimeMs) {
    return entry.cached;
  }
  let source: string;
  try {
    source = getFs().readFileSync(absPath, "utf-8");
  } catch (err) {
    console.debug(
      "Template include resolution failed for path %s: %s",
      absPath,
      err,
    );
    return undefined;
  }
  const hash = hashString(source);
  if (entry && entry.hash === hash) {
    entry.mtimeMs = stat.mtimeMs;
    return entry.cached;
  }
  try {
    const [fm, body] = parseFrontmatter(source);
    const nodes = parseBody(body, false, fm.bodyStartLine ?? 1);
    const consts = new Map<string, Value>();
    for (const decl of fm.consts) {
      if (decl.defaultValue !== undefined) {
        consts.set(decl.name, decl.defaultValue);
      }
    }
    const cached: CachedInclude = {
      nodes,
      consts,
      declarations: fm.params,
      baseDir: getPath().dirname(absPath),
    };
    cache.delete(absPath);
    cache.set(absPath, { hash, mtimeMs: stat.mtimeMs, cached });
    if (maxEntries !== undefined && cache.size > maxEntries) {
      const oldest = cache.keys().next().value;
      if (oldest !== undefined) {
        cache.delete(oldest);
      }
    }
    return cached;
  } catch (err) {
    console.debug(
      "Template include resolution failed for path %s: %s",
      absPath,
      err,
    );
    return undefined;
  }
}

/** Escape special regex characters in a string. */
function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Recursively collect parameter paths that are option-typed.
 *
 * For a param like `person = struct(name = str, email = option(str))`,
 * this adds `"person.email"` to the set.  For a top-level
 * `x = option(str)`, it adds `"x"`.
 */
function collectOptionPaths(
  prefix: string,
  varType: VarType,
  typeAliases: ReadonlyMap<string, VarType>,
  out: Set<string>,
): void {
  // Resolve type aliases
  if (varType.kind === TYPE_ALIAS) {
    const resolved = typeAliases.get(varType.name);
    if (resolved) {
      collectOptionPaths(prefix, resolved, typeAliases, out);
    }
    return;
  }

  if (varType.kind === TYPE_OPTION) {
    out.add(prefix);
    return;
  }

  // Legacy isOption enum
  if (varType.kind === TYPE_ENUM && varType.isOption) {
    out.add(prefix);
    return;
  }

  // Recurse into struct fields
  if (varType.kind === TYPE_STRUCT) {
    for (const field of varType.fields) {
      collectOptionPaths(
        `${prefix}.${field.name}`,
        field.varType,
        typeAliases,
        out,
      );
    }
  }

  // Note: list item option fields are accessed via loop bindings (e.g.,
  // `item.field`) whose prefix isn't known at declaration time. The
  // match/kind heuristic (isOptionMatchNode) handles those cases.
}

/**
 * Walk AST nodes and reject bare enum literal expressions.
 *
 * A "bare enum literal" is an expression output like `{{ Stage.Design }}`
 * where `Stage` is an enum type name and the expression is a plain dotted
 * path (not wrapped in `kind()` or another function call).
 *
 * @throws {TemplateSyntaxError} On the first bare enum literal found.
 */
function walkNodesForBareEnumAccess(
  nodes: readonly Node[],
  enumTypeNames: ReadonlySet<string>,
): void {
  for (const node of nodes) {
    switch (node.kind) {
      case "expr": {
        const barePath = extractBareDottedPath(node.expr);
        if (barePath !== undefined) {
          const dotIdx = barePath.indexOf(".");
          if (dotIdx > 0) {
            const root = barePath.slice(0, dotIdx);
            if (enumTypeNames.has(root)) {
              throw new TemplateSyntaxError(
                `bare enum literal '${barePath}' is not allowed` +
                  ` — use kind(${barePath}) to get the variant name as a string`,
                node.loc?.line,
                node.loc?.column,
                node.loc?.snippet,
              );
            }
          }
        }
        break;
      }
      case "for":
        walkNodesForBareEnumAccess(node.body, enumTypeNames);
        break;
      case "if":
        for (const branch of node.branches) {
          walkNodesForBareEnumAccess(branch.body, enumTypeNames);
        }
        if (node.elseBody) {
          walkNodesForBareEnumAccess(node.elseBody, enumTypeNames);
        }
        break;
      case "match":
        for (const arm of node.arms) {
          walkNodesForBareEnumAccess(arm.body, enumTypeNames);
        }
        if (node.elseArm) {
          walkNodesForBareEnumAccess(node.elseArm, enumTypeNames);
        }
        if (node.inlineGuard) {
          walkNodesForBareEnumAccess(node.inlineGuard.body, enumTypeNames);
        }
        break;
    }
  }
}

/**
 * Extract the bare dotted path from an expression string, or `undefined`
 * if the expression is a function call.
 *
 * The "bare path" is the portion before any `|` filter pipe, trimmed.
 * Returns `undefined` if the expression contains a `(` before the first
 * `.`, indicating a function call (e.g., `kind(Stage.Design)`).
 */
function extractBareDottedPath(expr: string): string | undefined {
  const trimmed = expr.trim();
  const dotIdx = trimmed.indexOf(".");
  if (dotIdx <= 0) return undefined; // No dot or starts with dot

  const parenIdx = trimmed.indexOf("(");
  if (parenIdx >= 0 && parenIdx < dotIdx) return undefined; // Function call

  // Extract the path part before any pipe filter separator.
  let end = trimmed.length;
  let depth = 0;
  for (let i = 0; i < trimmed.length; i++) {
    const ch = trimmed.charCodeAt(i);
    if (ch === 40 /* ( */) depth++;
    else if (ch === 41 /* ) */) depth--;
    else if (ch === 124 /* | */ && depth === 0) {
      end = i;
      break;
    }
  }

  return trimmed.slice(0, end).trim();
}

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
function injectEnumTypeConstants(
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
 * Resolve imported template files and collect their exported constants.
 *
 * For each import in `fm.imports`, reads the referenced `.tmpl.md` file
 * relative to `baseDir`, parses its frontmatter, and collects constants
 * as `stem.NAME` entries. Returns a new `Frontmatter` with the
 * `importedConsts` field populated.
 *
 * Silently skips imports whose files cannot be read or parsed.
 */
function resolveImportedConsts(fm: Frontmatter, baseDir: string): Frontmatter {
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
        `cannot read imported template file '${fullPath}' for stem '${imp.stem}': ${err}`,
      );
    }

    const [importedFm] = parseFrontmatter(importSource);

    for (const decl of importedFm.consts) {
      if (decl.defaultValue !== undefined) {
        imported[`${imp.stem}.${decl.name}`] = valueToJs(decl.defaultValue);
        // Accumulate for sequential/chained resolution: subsequent imports
        // can reference this const via {{ stem.NAME }} in their paths.
        availableConsts.set(`${imp.stem}.${decl.name}`, decl.defaultValue);
      }
    }

    // Inject enum type constants from the imported template's type aliases.
    for (const [typeName, varType] of importedFm.typeAliases) {
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

  if (Object.keys(imported).length === 0) {
    return fm;
  }

  // Post-process: resolve param defaults that reference imported consts.
  // During parseFrontmatter(), imported consts weren't available yet, so
  // param defaults like `stem.NAME` were deferred in unresolvedDefaults.
  if (fm.unresolvedDefaults.size === 0) {
    return { ...fm, importedConsts: imported };
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
function resolveEnvDeclarations(
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

// ---------------------------------------------------------------------------
// AST inspection helpers (for validation)
// ---------------------------------------------------------------------------

/** Collect all inline template names (`{% tmpl name %}`) from parsed nodes. */
function collectInlineTemplateNames(nodes: readonly Node[]): Set<string> {
  const names = new Set<string>();
  for (const node of nodes) {
    if (node.kind === "tmpl") {
      names.add(node.name);
    }
  }
  return names;
}

/** Collect all for-loop binding names from parsed nodes (recursive). */
function collectForBindings(nodes: readonly Node[]): Set<string> {
  const bindings = new Set<string>();
  for (const node of nodes) {
    if (node.kind === "for") {
      bindings.add(node.binding);
      // Recurse into body
      for (const b of collectForBindings(node.body)) {
        bindings.add(b);
      }
    } else if (node.kind === "if") {
      for (const branch of node.branches) {
        for (const b of collectForBindings(branch.body)) {
          bindings.add(b);
        }
      }
      if (node.elseBody) {
        for (const b of collectForBindings(node.elseBody)) {
          bindings.add(b);
        }
      }
    } else if (node.kind === "match") {
      for (const arm of node.arms) {
        for (const b of collectForBindings(arm.body)) {
          bindings.add(b);
        }
      }
      if (node.elseArm) {
        for (const b of collectForBindings(node.elseArm)) {
          bindings.add(b);
        }
      }
      if (node.inlineGuard) {
        for (const b of collectForBindings(node.inlineGuard.body)) {
          bindings.add(b);
        }
      }
    } else if (node.kind === "include" && node.forBinding) {
      bindings.add(node.forBinding);
    }
  }
  return bindings;
}
