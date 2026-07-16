/**
 * Browser template loading via lazy, on-demand `fetch`.
 *
 * The engine renders synchronously, but browsers can only fetch
 * asynchronously. This module bridges the two without making the evaluator
 * async: it installs an in-memory {@link MemoryFs} and drives a
 * **fetch-on-miss retry loop**. Each synchronous attempt (construction or
 * render) that touches an absent file records the missing absolute path; the
 * loop fetches exactly those paths and retries. Nothing is preloaded — only
 * the files a real render actually reaches are fetched, then cached.
 *
 * @module
 */

import {
  type CompileOptions,
  Template,
  TemplateError,
  setFileSystemProvider,
} from "../index.js";
import { MemoryFs } from "./memory_fs.js";
import { posixPath } from "./posix_path.js";

/**
 * The subset of the `fetch` API used by the loader. Kept minimal so the
 * package needs neither the DOM nor Node `fetch` type libs, and so callers can
 * inject a custom transport (auth headers, a service worker, tests, …).
 */
export type FetchLike = (
  url: string,
) => Promise<{ ok: boolean; status: number; text(): Promise<string> }>;

/** Options for {@link loadTemplate}. */
export interface LoadTemplateOptions {
  /** Values for the template's `env:` frontmatter declarations. */
  env?: Record<string, unknown>;
  /**
   * Custom fetch implementation. Defaults to the global `fetch`. Provide one to
   * add authentication, use a service worker, or run under test.
   */
  fetch?: FetchLike;
  /**
   * Base URL for resolving a relative `url`. Defaults to the document location
   * when available. Required (or an absolute `url`) outside a browser.
   */
  baseUrl?: string | URL;
  /**
   * Defensive backstop on the total number of dependency fetches. The loader
   * is guaranteed to terminate on its own (each round fetches a new file and a
   * render reaches finitely many paths), so this only ever trips for
   * pathological inputs. Defaults to 1000.
   */
  maxRounds?: number;
}

// A single shared VFS across all loads acts as a cross-template fetch cache.
// The provider is global, so it is installed exactly once, lazily.
const sharedFs = new MemoryFs();
let providerInstalled = false;

function ensureProviderInstalled(): void {
  if (providerInstalled) return;
  setFileSystemProvider(sharedFs, posixPath);
  providerInstalled = true;
}

/** Resolve the default base URL from the ambient document, if any. */
function defaultBaseUrl(): string | undefined {
  const globalWithLocation = globalThis as { location?: { href?: string } };
  return globalWithLocation.location?.href;
}

/** Resolve the ambient global `fetch`, if any. */
function defaultFetch(): FetchLike | undefined {
  const globalWithFetch = globalThis as { fetch?: FetchLike };
  return globalWithFetch.fetch
    ? globalWithFetch.fetch.bind(globalThis)
    : undefined;
}

/**
 * Fetch each missing absolute VFS path from `origin` and store it in `fs`.
 * Fetches run in parallel; a non-OK response throws (breaking the retry loop
 * with a clear, attributable error instead of spinning forever).
 */
async function fetchMissingInto(
  fs: MemoryFs,
  origin: string,
  fetchFn: FetchLike,
  paths: readonly string[],
): Promise<void> {
  await Promise.all(
    paths.map(async (absPath) => {
      const url = `${origin}${absPath}`;
      let response: Awaited<ReturnType<FetchLike>>;
      try {
        response = await fetchFn(url);
      } catch (err) {
        throw new TemplateError(
          `failed to fetch template '${url}': ${err instanceof Error ? err.message : String(err)}`,
        );
      }
      if (!response.ok) {
        throw new TemplateError(
          `failed to fetch template '${url}' (HTTP ${String(response.status)})`,
        );
      }
      fs.write(absPath, await response.text());
    }),
  );
}

/**
 * Run a synchronous `attempt`, fetching any files it reports missing and
 * retrying until it succeeds (or a genuine, non-missing-file error is thrown).
 *
 * This always terminates: each failed round fetches at least one *new* file,
 * and a given render/construction reaches a finite, fixed set of paths (the
 * engine caps include recursion and a real fetch 404s for anything absent).
 * `maxRounds` is therefore only a defensive backstop.
 *
 * Correctness relies on `attempt` being pure and the missing-path window in
 * {@link MemoryFs} being mutated only synchronously (there is no `await`
 * between `clearMissing` and `takeMissing`), so concurrent callers cannot
 * corrupt each other's records.
 */
async function resolveWithFetch<T>(
  fs: MemoryFs,
  fetchMissing: (paths: readonly string[]) => Promise<void>,
  attempt: () => T,
  maxRounds: number,
): Promise<T> {
  for (let round = 0; round <= maxRounds; round++) {
    fs.clearMissing();
    try {
      return attempt();
    } catch (err) {
      const missing = fs.takeMissing();
      if (missing.length === 0) throw err; // real error, not an absent file
      await fetchMissing(missing);
    }
  }
  throw new TemplateError(
    `exceeded ${String(maxRounds)} fetches while resolving template ` +
      `dependencies (raise options.maxRounds if this is legitimate)`,
  );
}

/**
 * A {@link Template} whose file dependencies are fetched lazily from the
 * network. Use {@link renderAsync} for the first render (or any render that may
 * reach a not-yet-fetched include); {@link render} is the plain synchronous
 * path, valid once the needed files are cached.
 */
export class BrowserTemplate {
  constructor(
    private readonly inner: Template,
    private readonly resolveLazily: <T>(attempt: () => T) => Promise<T>,
  ) {}

  /** The underlying core {@link Template} (advanced escape hatch). */
  get template(): Template {
    return this.inner;
  }

  // ---------------------------------------------------------------------------
  // Synchronous render variants — succeed only when all includes are cached
  // ---------------------------------------------------------------------------

  /**
   * Synchronous render. Succeeds only if every `{% include %}` this render
   * reaches is already cached; otherwise it throws `IncludeNotFoundError`.
   * Prefer {@link renderAsync} unless you know the dependencies are warm.
   */
  render(
    params: Record<string, unknown> = {},
    options?: { allowExtra?: boolean },
  ): string {
    return this.inner.render(params, options);
  }

  /** Render without strict parameter validation (synchronous, cache must be warm). */
  renderUnchecked(params: Record<string, unknown> = {}): string {
    return this.inner.renderUnchecked(params);
  }

  /** Render from a `Map` or `Record` (synchronous, cache must be warm). */
  renderDict(
    params: Record<string, unknown> | Map<string, unknown>,
    options?: { allowExtra?: boolean; allowUnused?: boolean },
  ): string {
    return this.inner.renderDict(params, options);
  }

  /** Render using only default values (synchronous, cache must be warm). */
  renderEmpty(): string {
    return this.inner.renderEmpty();
  }

  // ---------------------------------------------------------------------------
  // Async render variants — fetch missing includes on demand
  // ---------------------------------------------------------------------------

  /**
   * Render, fetching any not-yet-cached includes on demand. Discovers the
   * exact files these `params` reach — including dynamic, param-dependent
   * include paths — and caches them for subsequent synchronous {@link render}.
   */
  async renderAsync(
    params: Record<string, unknown> = {},
    options?: { allowExtra?: boolean },
  ): Promise<string> {
    return this.resolveLazily(() => this.inner.render(params, options));
  }

  /** Async variant of {@link renderDict}. */
  async renderDictAsync(
    params: Record<string, unknown> | Map<string, unknown>,
    options?: { allowExtra?: boolean; allowUnused?: boolean },
  ): Promise<string> {
    return this.resolveLazily(() => this.inner.renderDict(params, options));
  }

  /** Async variant of {@link renderEmpty}. */
  async renderEmptyAsync(): Promise<string> {
    return this.resolveLazily(() => this.inner.renderEmpty());
  }

  // ---------------------------------------------------------------------------
  // Metadata (pure — no file I/O, always synchronous)
  // ---------------------------------------------------------------------------

  /** Return parameter declarations as `[name, typeString]` tuples. */
  declarations(): [string, string][] {
    return this.inner.declarations();
  }

  /** Return default values for parameters that declare them. */
  defaults(): Record<string, unknown> {
    return this.inner.defaults();
  }

  /** Return constants defined in the template's frontmatter. */
  consts(): Record<string, unknown> {
    return this.inner.consts();
  }

  /** Return a content hash of the template source. */
  sourceHash(): number {
    return this.inner.sourceHash();
  }

  /** Return the raw template body after frontmatter stripping. */
  body(): string {
    return this.inner.body();
  }

  /** The parsed frontmatter object. */
  get frontmatter(): Template["frontmatter"] {
    return this.inner.frontmatter;
  }
}

/**
 * Evict all cached template sources from the shared in-memory VFS.
 *
 * After calling this, the next {@link loadTemplate} or {@link BrowserTemplate.renderAsync}
 * will re-fetch every file it touches from the network. Useful for cache
 * busting or live-reload workflows.
 */
export function clearCache(): void {
  sharedFs.clear();
}

/**
 * Load a template (and, lazily, its dependencies) from a URL for use in the
 * browser or any async-only runtime.
 *
 * The entry file and its `imports:` are fetched during construction;
 * `{% include %}` targets are fetched on demand at render time. Only files a
 * real render reaches are fetched — nothing is preloaded.
 *
 * @example
 * ```ts
 * import { loadTemplate } from "md-tmpl/browser";
 *
 * const tmpl = await loadTemplate("/prompts/agent.tmpl.md");
 * const out = await tmpl.renderAsync({ role: "reviewer" });
 * // subsequent same-shape renders can be synchronous (cache is warm):
 * const out2 = tmpl.render({ role: "planner" });
 * ```
 */
export async function loadTemplate(
  url: string | URL,
  options: LoadTemplateOptions = {},
): Promise<BrowserTemplate> {
  ensureProviderInstalled();

  const base = options.baseUrl ?? defaultBaseUrl();
  const entry = base ? new URL(url, base) : new URL(url);
  const origin = entry.origin;
  const vfsPath = entry.pathname;

  const fetchFn = options.fetch ?? defaultFetch();
  if (!fetchFn) {
    throw new TemplateError(
      "no fetch implementation available; pass options.fetch",
    );
  }
  const maxRounds = options.maxRounds ?? 1000;

  const resolveLazily = <T>(attempt: () => T): Promise<T> =>
    resolveWithFetch(
      sharedFs,
      (paths) => fetchMissingInto(sharedFs, origin, fetchFn, paths),
      attempt,
      maxRounds,
    );

  const compileOptions: CompileOptions | undefined = options.env
    ? { env: options.env }
    : undefined;

  const inner = await resolveLazily(() =>
    Template.fromFileWithEnv(vfsPath, compileOptions),
  );
  return new BrowserTemplate(inner, resolveLazily);
}
