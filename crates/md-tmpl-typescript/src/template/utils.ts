/**
 * Internal utilities for the Template module: lazy Node.js module loaders
 * and FNV-1a content hashing.
 *
 * @module
 */

// Lazy-loaded Node.js modules — avoids top-level imports that break
// browsers, Deno, edge runtimes, and other non-Node environments.
// Only code paths that perform file I/O will trigger the require().
let _fs: typeof import("node:fs") | undefined;
let _path: typeof import("node:path") | undefined;
export function getFs(): typeof import("node:fs") {
  const mod = _fs ?? (_fs = require("node:fs"));
  return mod;
}
export function getPath(): typeof import("node:path") {
  const mod = _path ?? (_path = require("node:path"));
  return mod;
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
