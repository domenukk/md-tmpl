/**
 * Content-hashed template cache for hot-reload scenarios.
 *
 * @module
 */

import { TemplateError } from "../errors.js";
import { type CachedInclude, type IncludeCacheEntry } from "./types.js";
import { getFs, getPath, hashString } from "./utils.js";
import { resolveIncludeEntry } from "./includes.js";
import { Template } from "./template_class.js";

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
  private readonly cache = new Map<
    string,
    { hash: number; template: Template }
  >();
  private readonly includes = new Map<string, IncludeCacheEntry>();
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
    if (cached?.hash === hash) {
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
    envValues?: Record<string, unknown>,
  ): CachedInclude | undefined {
    return resolveIncludeEntry(
      this.includes,
      filePath,
      baseDir,
      this.maxEntries,
      envValues,
    );
  }

  /** Return the number of cached include templates. */
  includeCount(): number {
    return this.includes.size;
  }
}
