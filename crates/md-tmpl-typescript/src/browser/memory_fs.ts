/**
 * An in-memory, synchronous filesystem for non-Node runtimes.
 *
 * The template engine performs all file I/O synchronously (loading a template,
 * resolving `imports:`, and resolving `{% include %}`). Browsers cannot read
 * files synchronously, so {@link MemoryFs} serves already-fetched sources from
 * a `Map` and, crucially, **records every absolute path that was requested but
 * absent**. A caller can then asynchronously fetch exactly those paths and
 * retry the operation — see `loadTemplate` / `renderAsync` in `./load.ts`.
 *
 * @module
 */

import type { FsProvider } from "../index.js";

/** Error thrown by {@link MemoryFs.readFileSync} for an absent path. */
class FileNotFoundError extends Error {
  /** POSIX errno-style code, matching Node's `ENOENT` for familiarity. */
  readonly code = "ENOENT";
  constructor(path: string) {
    super(`ENOENT: no such file in MemoryFs: '${path}'`);
    this.name = "FileNotFoundError";
  }
}

/**
 * A synchronous, in-memory {@link FsProvider} backed by a path→source map.
 *
 * Writes assign a monotonically increasing `mtimeMs`, so overwriting a path
 * correctly invalidates the engine's include cache.
 */
export class MemoryFs implements FsProvider {
  private readonly files = new Map<string, string>();
  private readonly mtimes = new Map<string, number>();
  private readonly missing = new Set<string>();
  private clock = 1;

  /** Store (or overwrite) a file's UTF-8 source at an absolute path. */
  write(path: string, contents: string): void {
    this.files.set(path, contents);
    this.mtimes.set(path, this.clock++);
    this.missing.delete(path);
  }

  /** Whether a file exists at the given absolute path. */
  has(path: string): boolean {
    return this.files.has(path);
  }

  /**
   * Return the set of absolute paths requested-but-absent since the last
   * {@link clearMissing} / {@link takeMissing}, then clear the record.
   */
  takeMissing(): string[] {
    const paths = [...this.missing];
    this.missing.clear();
    return paths;
  }

  /** Discard any recorded missing paths without returning them. */
  clearMissing(): void {
    this.missing.clear();
  }

  /** Evict all cached files and missing-path records. */
  clear(): void {
    this.files.clear();
    this.mtimes.clear();
    this.missing.clear();
  }

  readFileSync(path: string, _encoding: "utf-8"): string {
    const contents = this.files.get(path);
    if (contents === undefined) {
      this.missing.add(path);
      throw new FileNotFoundError(path);
    }
    return contents;
  }

  statSync(
    path: string,
    _options: { throwIfNoEntry: false },
  ): { mtimeMs: number } | undefined {
    const mtimeMs = this.mtimes.get(path);
    if (mtimeMs === undefined) {
      this.missing.add(path);
      return undefined;
    }
    return { mtimeMs };
  }
}
