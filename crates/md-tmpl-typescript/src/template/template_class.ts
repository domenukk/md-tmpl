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

import { Context } from "../context.js";
import {
  DeclarationsMutatedError,
  ExtraParamsError,
  MissingParamsError,
  TemplateError,
} from "../errors.js";
import {
  type Frontmatter,
  type VarType,
  parseFrontmatter,
  varTypeToString,
} from "../frontmatter.js";
import {
  validateBodyCollisions,
  validateDisplayability,
  validateFrontmatter,
} from "../validation.js";
import { applySanitizeAstPass } from "../sanitize_pass.js";
import {
  type Node,
  type RenderOptions,
  Scope as ScopeImpl,
  parseBody,
  renderNodes,
} from "../parser.js";
import {
  type TmplRef,
  type TmplValue,
  type Value,
  fromJs,
  tmplVal,
  valueToJs,
} from "../value.js";
import {
  type DirectRenderOptions,
  DirectScope,
  precompileDirectNodes,
  renderDirect,
  renderDirectWithScope,
} from "../direct_renderer.js";
import {
  KW_MATCH,
  TYPE_ALIAS,
  TYPE_ENUM,
  TYPE_NONE,
  TYPE_OPTION,
  TYPE_STRUCT,
  TYPE_TMPL,
} from "../consts.js";
import {
  type CachedInclude,
  type CompileOptions,
  type IncludeCacheEntry,
  type ITemplate,
} from "./types.js";
import { getFs, getPath, hashString } from "./utils.js";
import { jsToValue } from "./conversion.js";
import { collectOptionPaths } from "./option_paths.js";
import { typeCheckValue } from "./type_check.js";
import {
  injectEnumTypeConstants,
  resolveEnvDeclarations,
  resolveImportedConsts,
} from "./resolve.js";
import { resolveIncludeEntry } from "./includes.js";
import {
  type InlineTemplateEntry,
  collectInlineTemplateMap,
} from "./inline_templates.js";
import {
  type CompiledParamCheck,
  buildParamChecks,
  canUseDirectValidatedRender,
  isValidFastParams,
} from "./fast_validator.js";
import {
  buildTemplateWideNames,
  checkBareEnumAccess,
  checkMatchTypeSafety,
  checkUndeclaredVariablesWithRefs,
  checkUnusedParamsWithRefs,
  collectAllReferencedVariables,
  walkNodesForConditionSyntax,
} from "./compile_checks.js";
import { collectRefsAndForBindings } from "./references.js";
import { type TemplateCache } from "./cache.js";

type CompiledDeclaration = readonly [string, string];
const EMPTY_INLINE_TEMPLATES = new Map<string, InlineTemplateEntry>();
const EMPTY_STRING_SET: ReadonlySet<string> = new Set<string>();
const EMPTY_UNKNOWN_MAP: ReadonlyMap<string, unknown> = new Map<
  string,
  unknown
>();
const EMPTY_VALUE_MAP: ReadonlyMap<string, Value> = new Map<string, Value>();
const EMPTY_PARAMS: Record<string, unknown> = {};

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
export class Template implements ITemplate, TmplRef {
  private readonly fm: Frontmatter;
  private readonly bodyStr: string;
  private readonly nodes: Node[];
  private readonly sourceStr: string;
  private _hash: number | undefined;
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
  private _nonEnumParams: ReadonlySet<string> | undefined;
  private _nonOptionParams: ReadonlySet<string> | undefined;
  private readonly inlineTemplates: Map<string, InlineTemplateEntry>;
  private readonly paramChecks: readonly CompiledParamCheck[];
  private _compiledDeclarations: readonly CompiledDeclaration[] | undefined;
  private readonly directScope: DirectScope;
  private canDirectValidateRender: boolean;
  private _maxIncludeDepth = 16;
  /** Optional reference to the TemplateCache that loaded this template. */
  _cache?: TemplateCache;
  /** Compile-time env values, stored for propagation to included templates. */
  private readonly _compileEnv: Record<string, unknown>;
  private _includeCache?: Map<string, IncludeCacheEntry>;

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
    compileEnv?: Record<string, unknown>,
  ) {
    this.fm = fm;
    this.bodyStr = bodyStr;
    this.nodes = nodes;
    this.sourceStr = source;
    this._basePath = basePath;
    this._compileEnv = compileEnv ?? {};

    // Pre-compute immutable render data
    const declaredNames = new Set<string>();
    let defaults: Map<string, unknown> | undefined;
    let optParams: Set<string> | undefined;
    for (const decl of fm.params) {
      declaredNames.add(decl.name);
      if (decl.defaultValue !== undefined) {
        defaults ??= new Map<string, unknown>();
        defaults.set(decl.name, valueToJs(decl.defaultValue));
      }
      if (
        decl.varType.kind === TYPE_OPTION ||
        decl.varType.kind === TYPE_STRUCT ||
        decl.varType.kind === TYPE_ALIAS ||
        (decl.varType.kind === TYPE_ENUM && decl.varType.isOption)
      ) {
        optParams ??= new Set<string>();
        collectOptionPaths(decl.name, decl.varType, fm.typeAliases, optParams);
      }
    }
    this.declaredNames = declaredNames;
    this.defaultValues = defaults ?? EMPTY_UNKNOWN_MAP;
    this.optionParams =
      optParams && optParams.size > 0 ? optParams : EMPTY_STRING_SET;

    if (
      fm.consts.length === 0 &&
      fm.env.length === 0 &&
      fm.imports.length === 0 &&
      fm.typeAliases.size === 0
    ) {
      this.constValues = EMPTY_VALUE_MAP;
      this.constJsValues = EMPTY_UNKNOWN_MAP;
    } else {
      const consts = new Map<string, Value>();
      const constsJs = new Map<string, unknown>();
      for (const decl of fm.consts) {
        if (decl.defaultValue !== undefined) {
          consts.set(decl.name, decl.defaultValue);
          constsJs.set(decl.name, valueToJs(decl.defaultValue));
        }
      }
      // Env declarations are resolved at compile time and behave like consts.
      for (const decl of fm.env) {
        if (decl.defaultValue !== undefined) {
          consts.set(decl.name, decl.defaultValue);
          constsJs.set(decl.name, valueToJs(decl.defaultValue));
        }
      }
      if (fm.imports.length > 0) {
        for (const [key, jsVal] of Object.entries(fm.importedConsts)) {
          consts.set(key, fromJs(jsVal));
          constsJs.set(key, jsVal);
        }
      }
      if (fm.typeAliases.size > 0) {
        injectEnumTypeConstants(fm.typeAliases, consts, constsJs);
      }
      this.constValues = consts;
      this.constJsValues = constsJs;
    }
    this.inlineTemplates = bodyStr.includes(TYPE_TMPL)
      ? collectInlineTemplateMap(
          nodes,
          fm.typeAliases,
          fm.typeAliasSanitize,
          fm.sanitizeNotice,
        )
      : EMPTY_INLINE_TEMPLATES;
    this.paramChecks = buildParamChecks(fm.params, fm.typeAliases);
    this.directScope = new DirectScope(
      EMPTY_PARAMS,
      this.constJsValues,
      this.defaultValues,
      this.declaredNames,
    );
    this.canDirectValidateRender = false;
  }

  private getNonEnumAndNonOptionSets(): [
    ReadonlySet<string>,
    ReadonlySet<string>,
  ] {
    if (
      this._nonEnumParams === undefined ||
      this._nonOptionParams === undefined
    ) {
      const nonEnum = new Set<string>();
      const nonOpt = new Set<string>();
      for (const decl of this.fm.params) {
        let vt = decl.varType;
        while (vt.kind === TYPE_ALIAS) {
          const resolved = this.fm.typeAliases.get(vt.name);
          if (!resolved) break;
          vt = resolved;
        }
        const isOpt =
          vt.kind === TYPE_OPTION || this.optionParams.has(decl.name);
        if (vt.kind !== TYPE_ENUM && !isOpt) {
          nonEnum.add(decl.name);
        }
        if (!isOpt) {
          nonOpt.add(decl.name);
        }
      }
      this._nonEnumParams = nonEnum;
      this._nonOptionParams = nonOpt;
    }
    return [this._nonEnumParams, this._nonOptionParams];
  }

  private finalizeCompile(body: string, allowUnused = false): this {
    const [referenced, forBindings] = collectRefsAndForBindings(this.nodes);
    if (body.includes("{#")) {
      for (const r of collectAllReferencedVariables(this.nodes, body)) {
        referenced.add(r);
      }
    }
    const wideNames =
      this.fm.imports.length === 0 &&
      this.fm.typeAliases.size === 0 &&
      this.inlineTemplates.size === 0
        ? EMPTY_STRING_SET
        : buildTemplateWideNames(this.fm, this.inlineTemplates.keys());
    checkUndeclaredVariablesWithRefs(this.fm, referenced, wideNames);
    if (!this.fm.allowUnused && !allowUnused) {
      checkUnusedParamsWithRefs(this.fm, this.nodes, referenced);
    }
    if (this.fm.typeAliases.size > 0) {
      checkBareEnumAccess(this.fm, this.nodes);
    }
    if (body.includes(KW_MATCH)) {
      checkMatchTypeSafety(this.fm, this.nodes);
    }
    const inlineNames =
      this.inlineTemplates.size > 0
        ? new Set(this.inlineTemplates.keys())
        : EMPTY_STRING_SET;
    validateBodyCollisions(this.fm, inlineNames, forBindings);
    validateDisplayability(
      this.nodes,
      this.fm.params,
      this.fm.consts,
      this.fm.typeAliases,
      this.fm.importedNamespaceTypes,
      this.inlineTemplates.size > 0
        ? new Map(
            [...this.inlineTemplates].map(([name, entry]) => [
              name,
              {
                declarations: entry.declarations,
                nodes: entry.nodes,
                constNames: new Set(entry.consts.keys()),
              },
            ]),
          )
        : undefined,
      wideNames,
    );
    applySanitizeAstPass(
      this.nodes,
      this.fm.paramSanitize,
      this.fm.sanitizeNotice,
    );
    this.canDirectValidateRender = canUseDirectValidatedRender(
      this.nodes,
      this.optionParams,
    );
    if (this.canDirectValidateRender) {
      precompileDirectNodes(this.nodes);
    }
    return this;
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
    const resolvedFm =
      fm.env.length > 0 ? resolveEnvDeclarations(fm, EMPTY_PARAMS) : fm;
    validateFrontmatter(resolvedFm);
    const nodes = parseBody(body, false, resolvedFm.bodyStartLine ?? 1);
    walkNodesForConditionSyntax(nodes);
    return new Template(
      resolvedFm,
      body,
      nodes,
      source,
      undefined,
      EMPTY_PARAMS,
    ).finalizeCompile(body, false);
  }

  /**
   * Parse a template from source with full compile-time options.
   *
   * Resolves `env:` declarations using the provided `CompileOptions.env`
   * values, type-checks them, and injects them into the template scope.
   * Imports are resolved relative to `CompileOptions.baseDir`, and unused
   * declared parameters are permitted when `CompileOptions.allowUnused`
   * is set.
   *
   * @throws {TemplateSyntaxError} If an env var is missing without a default.
   * @throws {TemplateSyntaxError} If an env var value fails type checking.
   */
  static fromSourceWithOptions(
    source: string,
    options: CompileOptions,
  ): Template {
    const baseDir = options.baseDir;
    const envValues = options.env ?? EMPTY_PARAMS;
    const allowUnused = options.allowUnused ?? false;
    const [fm, body] = parseFrontmatter(source);
    let resolvedFm = resolveEnvDeclarations(fm, envValues);
    validateFrontmatter(resolvedFm);
    if (baseDir) {
      resolvedFm = resolveImportedConsts(resolvedFm, baseDir);
    }
    const nodes = parseBody(body, false, resolvedFm.bodyStartLine ?? 1);
    walkNodesForConditionSyntax(nodes);
    return new Template(
      resolvedFm,
      body,
      nodes,
      source,
      baseDir,
      envValues,
    ).finalizeCompile(body, allowUnused);
  }

  /**
   * Parse a template from source with compile-time environment variables.
   *
   * Convenience wrapper over {@link fromSourceWithOptions} that accepts a
   * flat `env` record directly. This mirrors the WASM backend's
   * `fromSourceWithEnv(source, env)` signature for cross-backend
   * consistency.
   *
   * @param source - The template source.
   * @param env - Values for `env:` frontmatter declarations.
   *
   * @throws {TemplateSyntaxError} If an env var is missing without a default.
   * @throws {TemplateSyntaxError} If an env var value fails type checking.
   */
  static fromSourceWithEnv(
    source: string,
    env: Record<string, unknown> = {},
  ): Template {
    return Template.fromSourceWithOptions(source, { env });
  }

  /**
   * Parse a template, allowing declared parameters that aren't used.
   */
  static fromSourceAllowingUnused(source: string): Template {
    return Template.fromSourceWithOptions(source, { allowUnused: true });
  }

  /**
   * Parse a template with a base directory for include resolution.
   */
  static fromSourceWithBaseDir(source: string, baseDir: string): Template {
    const [fm, body] = parseFrontmatter(source);
    validateFrontmatter(fm);
    const resolvedFm = resolveImportedConsts(fm, baseDir);
    const nodes = parseBody(body, false, resolvedFm.bodyStartLine ?? 1);
    walkNodesForConditionSyntax(nodes);
    return new Template(
      resolvedFm,
      body,
      nodes,
      source,
      baseDir,
    ).finalizeCompile(body);
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
    const envValues = options?.env ?? {};
    const envResolved = resolveEnvDeclarations(fm, envValues);
    validateFrontmatter(envResolved);
    const resolvedFm = resolveImportedConsts(envResolved, baseDir);
    const nodes = parseBody(body, false, resolvedFm.bodyStartLine ?? 1);
    walkNodesForConditionSyntax(nodes);
    return new Template(
      resolvedFm,
      body,
      nodes,
      source,
      baseDir,
      envValues,
    ).finalizeCompile(body);
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
    params: Record<string, unknown> = EMPTY_PARAMS,
    options?: { allowExtra?: boolean },
  ): string {
    const allowExtra = options?.allowExtra ?? false;
    if (
      this.canDirectValidateRender &&
      isValidFastParams(
        params,
        this.paramChecks,
        this.declaredNames,
        allowExtra,
      )
    ) {
      return renderDirectWithScope(
        this.nodes,
        this.directScope,
        params,
        this.getDirectOptionsFast(),
      );
    }
    const ctx = this.buildContext(params, allowExtra);
    return this.renderWithContext(ctx);
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
   * Render the template using a pre-built {@link Context}.
   *
   * This is an advanced escape hatch for callers that construct a
   * {@link Context} directly (e.g. to reuse converted {@link Value}s across
   * many renders).
   *
   * @example
   * ```ts
   * const ctx = Context.from({ name: "world" });
   * tmpl.renderContext(ctx); // => "Hello world!"
   * ```
   */
  renderContext(ctx: Context): string {
    return this.renderWithContext(ctx);
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
   * @throws {MissingParamsError} If any declared parameter lacks a default value.
   */
  renderEmpty(): string {
    // Check for required params (no default)
    const missing = this.fm.params
      .filter((d) => d.defaultValue === undefined)
      .map((d) => d.name);
    if (missing.length > 0) {
      throw new MissingParamsError(missing);
    }
    // All params have defaults — use direct renderer with just defaults + consts
    return renderDirect(
      this.nodes,
      this.defaultValues,
      this.constJsValues,
      this.getDirectOptionsFast(),
    );
  }

  // ── Metadata ─────────────────────────────────────────────────────────

  /**
   * Return parameter declarations as `[name, typeString]` tuples.
   */
  declarations(): [string, string][] {
    this._compiledDeclarations ??= this.fm.params.map(
      (d) => [d.name, varTypeToString(d.varType)] as const,
    );
    return this._compiledDeclarations as [string, string][];
  }

  /**
   * Return raw parameter declarations with VarType objects.
   * Used by TmplRef for higher-order template signature validation.
   */
  rawDeclarations(): readonly {
    name: string;
    varType: VarType;
    defaultValue?: Value;
  }[] {
    return this.fm.params;
  }

  /**
   * Render this template when included as a higher-order tmpl value.
   *
   * Merges parent constants and option params, wires up template loading,
   * and renders the body with the provided params.
   */
  renderForInclude(
    params: ReadonlyMap<string, Value>,
    parentConsts: ReadonlyMap<string, Value>,
    parentOptionParams: ReadonlySet<string>,
    maxDepth: number,
    templateLoader?: unknown,
    basePath?: string,
  ): string {
    // Merge parent consts with our own
    const mergedConsts = new Map<string, Value>(parentConsts);
    for (const [k, v] of this.constValues) {
      mergedConsts.set(k, v);
    }
    const combinedOpts = new Set<string>(parentOptionParams);
    for (const p of this.optionParams) {
      combinedOpts.add(p);
    }
    const [nonEnumParams, nonOptionParams] = this.getNonEnumAndNonOptionSets();
    const scope = new ScopeImpl(
      params,
      mergedConsts,
      combinedOpts,
      this.fm.typeAliases,
      nonEnumParams,
      nonOptionParams,
    );
    const opts: RenderOptions = {
      maxIncludeDepth: maxDepth,
    };
    // Set up pre-compiled inline templates from this template's body
    if (this.inlineTemplates.size > 0) {
      opts.inlineTemplates = this.inlineTemplates;
    }
    // Wire up file includes if we have a base path
    const effectiveBase = this._basePath ?? basePath;
    if (effectiveBase || this._cache) {
      opts.templateLoader = this.makeTemplateLoader(effectiveBase ?? "");
      opts.currentBasePath = effectiveBase;
    } else if (templateLoader && typeof templateLoader === "function") {
      opts.templateLoader = templateLoader as RenderOptions["templateLoader"];
      opts.currentBasePath = basePath;
    }
    return renderNodes(this.nodes, scope, opts);
  }

  /** Convert this Template to a TmplValue for passing as a parameter. */
  toValue(): TmplValue {
    return tmplVal(this);
  }

  /**
   * Return a content hash of the template source.
   *
   * Two templates with the same source produce the same hash.
   */
  sourceHash(): number {
    this._hash ??= hashString(this.sourceStr);
    return this._hash;
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
   * @throws {DeclarationsMutatedError} If the declarations don't match.
   */
  validateDeclarationsAgainst(expected: [string, string][]): void {
    const current = this.declarations();
    if (current.length !== expected.length) {
      throw new DeclarationsMutatedError(
        `expected ${JSON.stringify(expected)}, got ${JSON.stringify(current)}`,
      );
    }
    for (const [i, cur] of current.entries()) {
      const exp = expected[i];
      if (cur[0] !== exp?.[0] || cur[1] !== exp[1]) {
        throw new DeclarationsMutatedError(
          `expected ${JSON.stringify(expected)}, got ${JSON.stringify(current)}`,
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
      if (
        decl.defaultValue === undefined &&
        !Object.hasOwn(params, decl.name)
      ) {
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
        typeCheckValue(decl.name, value, decl.varType, this.fm.typeAliases);
      }
    }

    return ctx;
  }

  private renderWithContext(ctx: Context): string {
    let values = ctx.values;
    if (this.defaultValues.size > 0) {
      let merged: Map<string, Value> | undefined;
      for (const decl of this.fm.params) {
        if (decl.defaultValue !== undefined && !values.has(decl.name)) {
          merged ??= new Map(values);
          merged.set(decl.name, decl.defaultValue);
        }
      }
      if (merged !== undefined) {
        values = merged;
      }
    }
    const [nonEnumParams, nonOptionParams] = this.getNonEnumAndNonOptionSets();
    const scope = new ScopeImpl(
      values,
      this.constValues,
      this.optionParams,
      this.fm.typeAliases,
      nonEnumParams,
      nonOptionParams,
    );
    const options: RenderOptions = {
      maxIncludeDepth: this._maxIncludeDepth,
    };

    // Wire up file-based include resolution if we have a base path or cache
    if (this._basePath || this._cache) {
      options.templateLoader = this.makeTemplateLoader(this._basePath ?? "");
    }
    if (this.inlineTemplates.size > 0) {
      options.inlineTemplates = this.inlineTemplates;
    }

    return renderNodes(this.nodes, scope, options);
  }

  private makeTemplateLoader(
    defaultBase: string,
  ): RenderOptions["templateLoader"] {
    return (includePath: string, basePath?: string) => {
      const cached = this.resolveInclude(includePath, basePath ?? defaultBase);
      if (!cached) return undefined;
      return [
        cached.nodes,
        cached.consts,
        cached.declarations,
        cached.baseDir,
        cached.inlineTemplates,
      ];
    };
  }

  private getDirectOptionsFast(): DirectRenderOptions | undefined {
    if (
      this.inlineTemplates.size === 0 &&
      !this._basePath &&
      !this._cache &&
      this._maxIncludeDepth === 16
    ) {
      return undefined;
    }
    return this.getDirectOptions();
  }

  private getDirectOptions(): DirectRenderOptions {
    const defaultBase = this._basePath ?? "";
    const templateLoader: DirectRenderOptions["templateLoader"] = (
      includePath: string,
      basePath?: string,
    ) => {
      const cached = this.resolveInclude(includePath, basePath ?? defaultBase);
      if (!cached) return undefined;
      return [
        cached.nodes,
        cached.constsJs,
        cached.declarations,
        cached.baseDir,
        cached.inlineTemplates,
      ];
    };

    return {
      inlineTemplates:
        this.inlineTemplates.size > 0 ? this.inlineTemplates : undefined,
      templateLoader:
        this._basePath || this._cache ? templateLoader : undefined,
      maxIncludeDepth: this._maxIncludeDepth,
    };
  }

  private resolveInclude(
    includePath: string,
    basePath?: string,
  ): CachedInclude | undefined {
    if (this._cache) {
      return this._cache.resolveInclude(
        includePath,
        basePath,
        this._compileEnv,
      );
    }
    const currentBase = basePath ?? this._basePath ?? "";
    this._includeCache ??= new Map<string, IncludeCacheEntry>();
    return resolveIncludeEntry(
      this._includeCache,
      includePath,
      currentBase,
      undefined,
      this._compileEnv,
    );
  }
}
