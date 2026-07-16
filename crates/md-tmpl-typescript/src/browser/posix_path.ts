/**
 * A minimal, dependency-free POSIX path implementation.
 *
 * The template engine only needs {@link PathProvider.resolve} and
 * {@link PathProvider.dirname}, and browsers have no `node:path`. This shim
 * follows POSIX semantics so include/import resolution behaves identically to
 * Node. Paths handled here are URL pathnames (always `/`-separated and, in
 * practice, absolute), so there is no drive-letter or backslash handling.
 *
 * @module
 */

import type { PathProvider } from "../index.js";

/**
 * Normalize a POSIX path, collapsing `.` / `..` segments and duplicate
 * slashes. Leading `..` segments are preserved only for relative paths; on an
 * absolute path they can never escape the root and are dropped.
 */
function normalize(path: string): string {
  const isAbsolute = path.startsWith("/");
  const out: string[] = [];
  for (const segment of path.split("/")) {
    if (segment === "" || segment === ".") continue;
    if (segment === "..") {
      const top = out[out.length - 1];
      if (out.length > 0 && top !== "..") {
        out.pop();
      } else if (!isAbsolute) {
        out.push("..");
      }
      // On an absolute path, `..` at the root is a no-op.
      continue;
    }
    out.push(segment);
  }
  const joined = out.join("/");
  if (isAbsolute) return "/" + joined;
  return joined === "" ? "." : joined;
}

/**
 * Resolve a sequence of path segments into an absolute, normalized path.
 *
 * Mirrors `node:path.posix.resolve`: segments are processed right-to-left
 * until an absolute segment is found. Because browsers have no current working
 * directory, a non-absolute result is anchored at the root (`/`). All engine
 * call sites pass an absolute base, so this fallback is not normally reached.
 */
function resolve(...segments: string[]): string {
  let resolved = "";
  let isAbsolute = false;
  for (let i = segments.length - 1; i >= 0 && !isAbsolute; i--) {
    const segment = segments[i];
    if (segment === undefined || segment === "") continue;
    resolved = resolved === "" ? segment : `${segment}/${resolved}`;
    isAbsolute = segment.startsWith("/");
  }
  if (!isAbsolute) resolved = `/${resolved}`;
  return normalize(resolved);
}

/** Return the directory portion of a POSIX path. */
function dirname(path: string): string {
  const idx = path.lastIndexOf("/");
  if (idx < 0) return ".";
  if (idx === 0) return "/";
  return path.slice(0, idx);
}

/** A POSIX-semantics {@link PathProvider} suitable for browser environments. */
export const posixPath: PathProvider = { resolve, dirname };
