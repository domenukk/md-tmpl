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
  return (_fs ??= require("node:fs"));
}
function getPath(): typeof import("node:path") {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return (_path ??= require("node:path"));
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
  type VarType,
  parseFrontmatter,
  varTypeToString,
} from "./frontmatter.js";
import { validateFrontmatter, validateBodyCollisions } from "./validation.js";
import {
  type Node,
  type RenderOptions,
  Scope as ScopeImpl,
  parseBody,
  renderNodes,
} from "./parser.js";
import { type Value, ENUM_TAG_KEY, str, dict } from "./value.js";
import { renderDirect } from "./direct_renderer.js";

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
 * import { Template } from "prompt-templates";         // pure TS
 * import { Template as WasmTemplate } from "prompt-templates-wasm";
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

  /** Return constants imported from other templates. */
  importedConsts(): Record<string, unknown>;

  /** Return the raw template body after frontmatter stripping. */
  body(): string;
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
  private _maxIncludeDepth = 16;

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
    this.constValues = consts;
    const constsJs = new Map<string, unknown>();
    for (const decl of fm.consts) {
      if (decl.defaultValue !== undefined) {
        constsJs.set(decl.name, valueToJs(decl.defaultValue));
      }
    }
    this.constJsValues = constsJs;

    // Inject enum type constants from type aliases.
    // For each enum type (e.g., `Phase = enum<Explore, Build>`), create a
    // constant dict mapping variant names → values, enabling expressions
    // like `{{ Phase.Explore }}`.  User-defined constants are never overwritten.
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
    const nodes = parseBody(body);
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
    return tmpl;
  }

  /**
   * Parse a template, allowing declared parameters that aren't used.
   */
  static fromSourceAllowingUnused(source: string): Template {
    const [fm, body] = parseFrontmatter(source);
    const nodes = parseBody(body);
    const tmpl = new Template(fm, body, nodes, source);
    tmpl.checkBareEnumAccess();
    return tmpl;
  }

  /**
   * Parse a template with a base directory for include resolution.
   */
  static fromSourceWithBaseDir(source: string, baseDir: string): Template {
    const [fm, body] = parseFrontmatter(source);
    validateFrontmatter(fm);
    const resolvedFm = resolveImportedConsts(fm, baseDir);
    const nodes = parseBody(body);
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
    return tmpl;
  }

  /**
   * Load a template from a `.tmpl.md` file.
   *
   * @throws {TemplateError} If the file cannot be read.
   * @throws {TemplateSyntaxError} If the file contains syntax errors.
   */
  static fromFile(filePath: string): Template {
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
    validateFrontmatter(fm);
    const resolvedFm = resolveImportedConsts(fm, baseDir);
    const nodes = parseBody(body);
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
    return renderDirect(this.nodes, flat, this.constJsValues);
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
        const jsVal = valueToJs(decl.defaultValue);
        // For option types, convert internal representation to user-facing API:
        //   { __kind__: "Some", val: X } → X
        //   string "None" → null
        if (decl.varType.kind === "enum" && decl.varType.isOption) {
          if (jsVal === "None") {
            result[decl.name] = null;
          } else if (
            typeof jsVal === "object" &&
            jsVal !== null &&
            !Array.isArray(jsVal) &&
            (jsVal as Record<string, unknown>).__kind__ === "Some"
          ) {
            result[decl.name] = (jsVal as Record<string, unknown>).val;
          } else {
            result[decl.name] = jsVal;
          }
        } else {
          result[decl.name] = jsVal;
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
        ctx.set(name, wrapOptions(value, decl.varType, this.fm.typeAliases));
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

    // Set provided values (with option auto-wrapping)
    for (const [key, value] of Object.entries(params)) {
      if (this.declaredNames.has(key)) {
        const decl = this.fm.params.find((p) => p.name === key);
        if (decl) {
          ctx.set(key, wrapOptions(value, decl.varType, this.fm.typeAliases));
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
      case "str":
        if (value.type !== "str") {
          throw new TypeMismatchError(path, "str", value.type);
        }
        break;
      case "bool":
        if (value.type !== "bool") {
          throw new TypeMismatchError(path, "bool", value.type);
        }
        break;
      case "int":
        if (value.type !== "int") {
          throw new TypeMismatchError(path, "int", value.type);
        }
        break;
      case "float":
        if (value.type !== "float" && value.type !== "int") {
          throw new TypeMismatchError(path, "float", value.type);
        }
        break;
      case "list":
        if (value.type !== "list") {
          throw new TypeMismatchError(path, "list", value.type);
        }
        // Check item types
        if (varType.fields.length > 0) {
          for (let i = 0; i < value.items.length; i++) {
            const item = value.items[i]!;
            if (item.type !== "dict") {
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
      case "struct":
        if (value.type !== "dict") {
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
      case "enum":
        if (varType.isOption) {
          // Option type: accept str("None") for None, or dict with __kind__="Some"
          if (value.type === "str" && value.value === "None") {
            break; // None is always valid
          }
          if (value.type === "dict") {
            const tag = value.fields.get(ENUM_TAG_KEY);
            if (tag && tag.type === "str") {
              if (tag.value === "None") break;
              if (tag.value === "Some") {
                // Validate the inner value
                const someVariant = varType.variants.find(
                  (v) => v.name === "Some",
                );
                if (someVariant && someVariant.fields.length === 1) {
                  const innerVal = value.fields.get("val");
                  if (innerVal !== undefined) {
                    this.typeCheck(
                      `${path}.val`,
                      innerVal,
                      someVariant.fields[0]!.varType,
                    );
                  }
                }
                break;
              }
            }
          }
          throw new TypeMismatchError(path, "option<...>", value.type);
        }
        if (value.type === "str") {
          // Unit variant as string
          const validNames = varType.variants.map((v) => v.name);
          if (!validNames.includes(value.value)) {
            throw new TypeMismatchError(
              path,
              `enum<${validNames.join(", ")}>`,
              `str("${value.value}")`,
            );
          }
        } else if (value.type === "dict") {
          // Struct variant as struct with __kind__
          const tag = value.fields.get(ENUM_TAG_KEY);
          if (tag === undefined || tag.type !== "str") {
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
              `enum<${validNames.join(", ")}>`,
              `variant("${tag.value}")`,
            );
          }
        } else {
          throw new TypeMismatchError(path, "enum", value.type);
        }
        break;
      case "alias":
        // Resolve alias from type aliases
        {
          const resolved = this.fm.typeAliases.get(varType.name);
          if (resolved) {
            this.typeCheck(path, value, resolved);
          }
          // If alias not found, skip check (may be imported type)
        }
        break;
      case "scalar_list":
        if (value.type !== "list") {
          throw new TypeMismatchError(path, "list", value.type);
        }
        // Check each element against the declared element type
        for (let i = 0; i < value.items.length; i++) {
          this.typeCheck(`${path}[${i}]`, value.items[i]!, varType.elementType);
        }
        break;
      case "untyped_list":
        if (value.type !== "list") {
          throw new TypeMismatchError(path, "list", value.type);
        }
        break;
    }
  }

  private renderWithContext(ctx: Context): string {
    const scope = new ScopeImpl(ctx.values, this.constValues);
    const options: RenderOptions = {
      maxIncludeDepth: this._maxIncludeDepth,
    };

    // Wire up file-based include resolution if we have a base path
    if (this._basePath) {
      const basePath = this._basePath;
      options.templateLoader = (
        includePath: string,
      ): [Node[], Map<string, Value>] | undefined => {
        const fullPath = getPath().resolve(basePath, includePath);
        try {
          const source = getFs().readFileSync(fullPath, "utf-8");
          const [fm, body] = parseFrontmatter(source);
          const nodes = parseBody(body);
          // Extract consts from the included template
          const consts = new Map<string, Value>();
          for (const decl of fm.consts) {
            if (decl.defaultValue !== undefined) {
              consts.set(decl.name, decl.defaultValue);
            }
          }
          return [nodes, consts];
        } catch (err) {
          console.debug(
            "Template include resolution failed for path %s: %s",
            fullPath,
            err,
          );
          return undefined;
        }
      };
    }
    // Collect inline template definitions ({% tmpl name %}...{% /tmpl %})
    const inlineTmpls = new Map<
      string,
      { params: Map<string, unknown>; body: string }
    >();
    for (const n of this.nodes) {
      if (n.kind === "tmpl") {
        inlineTmpls.set(n.name, { params: new Map(), body: n.source });
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
        );
      }
    }
  }

  /**
   * Reject bare enum literal expressions like `{{ Phase.Explore }}`.
   *
   * Only `{{ kind(Phase.Explore) }}` is allowed — using the enum
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
}

/**
 * Recursively wrap option values in the user's JS input.
 *
 * Transforms `null`/`undefined` → `"None"` and `val` → `{ __kind__: "Some", val }`
 * for option-typed fields at any nesting level (struct fields, list items),
 * so that the downstream `fromJs` + `typeCheck` pipeline sees the correct shapes.
 *
 * We do this *before* `fromJs` because `fromJs(null)` must not change behavior
 * (it returns `str("")`), but option fields need `null` → `str("None")`.
 */
function wrapOptions(
  value: unknown,
  varType: VarType,
  typeAliases?: ReadonlyMap<string, VarType>,
): unknown {
  // Resolve type aliases before checking
  if (varType.kind === "alias" && typeAliases) {
    const resolved = typeAliases.get(varType.name);
    if (resolved) {
      return wrapOptions(value, resolved, typeAliases);
    }
  }

  if (varType.kind === "enum" && varType.isOption) {
    if (value === null || value === undefined) {
      return "None";
    }
    // String "None" is the None variant (from parseLiteral or user input)
    if (typeof value === "string" && value === "None") {
      return "None";
    }
    // Variant helper sentinel for None
    if (
      typeof value === "object" &&
      value !== null &&
      !Array.isArray(value) &&
      (value as Record<string, unknown>)._prompt_template_tag === "None"
    ) {
      return "None";
    }
    // Already wrapped (e.g. from variant helpers)?
    if (
      typeof value === "object" &&
      value !== null &&
      !Array.isArray(value) &&
      (value as Record<string, unknown>).__kind__ === "Some"
    ) {
      return value;
    }
    return { __kind__: "Some", val: value };
  }
  if (
    varType.kind === "struct" &&
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value)
  ) {
    const obj = value as Record<string, unknown>;
    let changed = false;
    const result: Record<string, unknown> = {};
    for (const field of varType.fields) {
      if (field.name in obj) {
        const wrapped = wrapOptions(
          obj[field.name],
          field.varType,
          typeAliases,
        );
        if (wrapped !== obj[field.name]) changed = true;
        result[field.name] = wrapped;
      }
    }
    if (!changed) return value;
    // Preserve non-declared fields
    for (const [k, v] of Object.entries(obj)) {
      if (!(k in result)) result[k] = v;
    }
    return result;
  }
  if (
    varType.kind === "list" &&
    Array.isArray(value) &&
    varType.fields.length > 0
  ) {
    let changed = false;
    const result = value.map((item) => {
      if (typeof item === "object" && item !== null && !Array.isArray(item)) {
        const obj = item as Record<string, unknown>;
        const wrapped: Record<string, unknown> = {};
        for (const field of varType.fields) {
          if (field.name in obj) {
            const w = wrapOptions(obj[field.name], field.varType, typeAliases);
            if (w !== obj[field.name]) changed = true;
            wrapped[field.name] = w;
          }
        }
        if (changed) {
          for (const [k, v] of Object.entries(obj)) {
            if (!(k in wrapped)) wrapped[k] = v;
          }
          return wrapped;
        }
      }
      return item;
    });
    return changed ? result : value;
  }
  return value;
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
 * import { TypedTemplate, Template } from "prompt-templates";
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
   * import { Template } from "prompt-templates";
   * import { Template as WasmTemplate } from "prompt-templates-wasm";
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
  }

  /** Return the number of cached templates. */
  templateCount(): number {
    return this.cache.size;
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

/** Convert a Value back to plain JS for defaults/consts output. */
function valueToJs(v: Value): unknown {
  switch (v.type) {
    case "str":
      return v.value;
    case "bool":
      return v.value;
    case "int":
      return v.value;
    case "float":
      return v.value;
    case "list":
      return v.items.map(valueToJs);
    case "dict": {
      const obj: Record<string, unknown> = {};
      for (const [k, val] of v.fields) {
        obj[k] = valueToJs(val);
      }
      return obj;
    }
  }
}

/** Escape special regex characters in a string. */
function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Walk AST nodes and reject bare enum literal expressions.
 *
 * A "bare enum literal" is an expression output like `{{ Phase.Explore }}`
 * where `Phase` is an enum type name and the expression is a plain dotted
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
        if (node.defaultArm) {
          walkNodesForBareEnumAccess(node.defaultArm, enumTypeNames);
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
 * `.`, indicating a function call (e.g., `kind(Phase.Explore)`).
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
 * For each enum type alias (e.g., `Phase = enum<Explore, Build>`), creates
 * a dict constant mapping variant names to their values:
 * - Unit variants → string with the variant name
 * - Struct variants → tagged dict with `__kind__` key
 *
 * This enables template expressions like `{{ Phase.Explore }}` or
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
        valueEntries.push([variant.name, dict(fieldEntries)]);
        jsObj[variant.name] = jsFields;
      }
    }

    constValues.set(typeName, dict(valueEntries));
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

  const imported: Record<string, unknown> = {};
  const fsModule = getFs();
  const pathModule = getPath();

  for (const imp of fm.imports) {
    const fullPath = pathModule.resolve(baseDir, imp.path);
    let importSource: string;
    try {
      importSource = fsModule.readFileSync(fullPath, "utf-8");
    } catch (err) {
      // Skip imports whose files cannot be read.
      console.debug(
        "Failed to read imported template file %s: %s",
        fullPath,
        err,
      );
      continue;
    }

    let importedFm: Frontmatter;
    try {
      [importedFm] = parseFrontmatter(importSource);
    } catch (err) {
      // Skip imports whose frontmatter cannot be parsed.
      console.debug(
        "Failed to parse frontmatter for imported template %s: %s",
        fullPath,
        err,
      );
      continue;
    }

    for (const decl of importedFm.consts) {
      if (decl.defaultValue !== undefined) {
        imported[`${imp.stem}.${decl.name}`] = valueToJs(decl.defaultValue);
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
      imported[key] = jsObj;
    }
  }

  if (Object.keys(imported).length === 0) {
    return fm;
  }

  return { ...fm, importedConsts: imported };
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
      if (node.defaultArm) {
        for (const b of collectForBindings(node.defaultArm)) {
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
