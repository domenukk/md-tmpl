/**
 * Internal utilities for the Template module: lazy Node.js module loaders
 * and FNV-1a content hashing.
 *
 * @module
 */

// Lazy Node.js built-in access — avoids top-level imports that break
// browsers, Deno, edge runtimes, and other non-Node environments.
// `process.getBuiltinModule` resolves a built-in synchronously without any
// static `import`/`require`, so merely importing this module never pulls in
// `node:fs`/`node:path`; only file-I/O code paths ever call these getters.
let _fs: typeof import("node:fs") | undefined;
let _path: typeof import("node:path") | undefined;

// ---------------------------------------------------------------------------
// Pluggable filesystem provider
// ---------------------------------------------------------------------------

/**
 * The subset of `node:fs` that the template engine requires for synchronous
 * file I/O (loading templates, resolving includes and imports).
 *
 * Provide a custom implementation via {@link setFileSystemProvider} to run in
 * environments without a real filesystem (browsers, Deno, edge runtimes). A
 * common pattern is an in-memory map of pre-fetched sources; see the
 * `md-tmpl/browser` entry point for a ready-made implementation.
 */
export interface FsProvider {
  /** Read a file's contents as UTF-8 text. Throws if the file is absent. */
  readFileSync(path: string, encoding: "utf-8"): string;
  /**
   * Return stat info for a path, or `undefined` when it does not exist.
   *
   * Only `mtimeMs` is consumed (for cache invalidation); an in-memory
   * provider may return a constant or monotonically increasing value.
   */
  statSync(
    path: string,
    options: { throwIfNoEntry: false },
  ): { mtimeMs: number } | undefined;
}

/**
 * The subset of `node:path` that the template engine requires. Implementations
 * should follow POSIX semantics so include/import resolution behaves
 * identically across platforms.
 */
export interface PathProvider {
  /** Resolve a sequence of path segments into an absolute path. */
  resolve(...segments: string[]): string;
  /** Return the directory portion of a path. */
  dirname(path: string): string;
}

let _fsProvider: FsProvider | undefined;
let _pathProvider: PathProvider | undefined;

/**
 * Override the filesystem (and, optionally, path) implementation used for all
 * synchronous file I/O: {@link Template.fromFile}, {@link TemplateCache},
 * `{% include %}`, and `imports:` resolution.
 *
 * Call {@link resetFileSystemProvider} to restore the default lazy Node
 * built-ins. Intended for non-Node runtimes; on Node the defaults already work.
 *
 * @param fs - Synchronous filesystem implementation.
 * @param path - Optional POSIX path implementation. When omitted, the Node
 *   `node:path` built-in is used (only valid on Node); non-Node runtimes should
 *   supply one (the `md-tmpl/browser` entry point provides a POSIX shim).
 */
export function setFileSystemProvider(
  fs: FsProvider,
  path?: PathProvider,
): void {
  _fsProvider = fs;
  _pathProvider = path;
}

/** Restore the default lazy Node.js built-in filesystem and path modules. */
export function resetFileSystemProvider(): void {
  _fsProvider = undefined;
  _pathProvider = undefined;
}

/**
 * Synchronously resolve a Node built-in module without a static import, so
 * this module stays importable in browsers, Deno, and edge runtimes. Only
 * file-I/O code paths reach this; it throws a clear error when invoked
 * outside a Node runtime exposing `process.getBuiltinModule` (Node 20.16+ /
 * 22.3+).
 */
function loadBuiltin(id: string): unknown {
  if (
    typeof process === "undefined" ||
    typeof process.getBuiltinModule !== "function"
  ) {
    throw new Error(
      `Cannot load Node built-in "${id}": file I/O requires a Node.js ` +
        `runtime (v20.16+ / v22.3+) exposing process.getBuiltinModule. ` +
        `In non-Node environments, call setFileSystemProvider() first.`,
    );
  }
  return process.getBuiltinModule(id);
}

export function getFs(): FsProvider {
  if (_fsProvider) return _fsProvider;
  _fs ??= loadBuiltin("node:fs") as typeof import("node:fs");
  return _fs;
}
export function getPath(): PathProvider {
  if (_pathProvider) return _pathProvider;
  _path ??= loadBuiltin("node:path") as typeof import("node:path");
  return _path;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Simple FNV-1a hash for content hashing. */
export function hashString(s: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    hash ^= s.charCodeAt(i);
    hash = (hash * 0x01000193) | 0;
  }
  return hash >>> 0; // unsigned 32-bit
}
