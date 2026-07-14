/**
 * TypedTemplate — compile-time type-safe wrapper around Template.
 *
 * @module
 */

import { type ITemplate } from "./types.js";
import { Template } from "./template_class.js";

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
