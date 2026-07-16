/**
 * Resolve and cache included template files.
 *
 * @module
 */

import { parseFrontmatter } from "../frontmatter.js";
import { parseBody } from "../parser.js";
import { type Value, fromJs, valueToJs } from "../value.js";
import { type CachedInclude, type IncludeCacheEntry } from "./types.js";
import { getFs, getPath, hashString } from "./utils.js";
import {
  injectEnumTypeConstants,
  resolveAliasesInDecls,
  resolveEnvDeclarations,
  resolveImportedConsts,
} from "./resolve.js";

export function resolveIncludeEntry(
  cache: Map<string, IncludeCacheEntry>,
  filePath: string,
  baseDir?: string,
  maxEntries?: number,
  envValues?: Record<string, unknown>,
): CachedInclude | undefined {
  const currentBase = baseDir ?? "";
  const absPath = getPath().resolve(currentBase, filePath);
  // Env values are baked into the cached result (as injected consts), so a
  // change in env must invalidate the entry even when the file is untouched.
  const envHash = hashEnv(envValues);
  let stat: { mtimeMs: number } | undefined;
  try {
    stat = getFs().statSync(absPath, { throwIfNoEntry: false });
  } catch {
    /* statSync can throw on permission errors; treat as not found */
    return undefined;
  }
  if (!stat) {
    return undefined;
  }
  const entry = cache.get(absPath);
  if (entry?.mtimeMs === stat.mtimeMs && entry.envHash === envHash) {
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
  if (entry?.hash === hash && entry.envHash === envHash) {
    entry.mtimeMs = stat.mtimeMs;
    return entry.cached;
  }
  try {
    const [rawFm, body] = parseFrontmatter(source);
    const childBaseDir = getPath().dirname(absPath);

    // Resolve env declarations from parent's compile-time env values.
    let fm = rawFm;
    if (rawFm.env.length > 0 && envValues) {
      fm = resolveEnvDeclarations(rawFm, envValues);
    }

    // Resolve imports for the child template (types, consts from siblings).
    if (fm.imports.length > 0) {
      fm = resolveImportedConsts(fm, childBaseDir);
    }

    const nodes = parseBody(body, false, fm.bodyStartLine ?? 1);
    const consts = new Map<string, Value>();
    for (const decl of fm.consts) {
      if (decl.defaultValue !== undefined) {
        consts.set(decl.name, decl.defaultValue);
      }
    }
    // Include env-resolved values as consts so they're available in scope.
    for (const decl of fm.env) {
      if (decl.defaultValue !== undefined) {
        consts.set(decl.name, decl.defaultValue);
      }
    }
    // Include imported consts (e.g. config.APP_NAME, types.Role).
    for (const [key, jsVal] of Object.entries(fm.importedConsts)) {
      consts.set(key, fromJs(jsVal));
    }
    // Inject enum type constants from imported type aliases.
    if (fm.typeAliases.size > 0) {
      const constsJs = new Map<string, unknown>();
      for (const [k, v] of consts) {
        constsJs.set(k, valueToJs(v));
      }
      injectEnumTypeConstants(fm.typeAliases, consts, constsJs);
    }
    // Resolve alias types in declarations (e.g., types.Role → enum(...))
    // so validateIncludeTypes can properly type-check params.
    const resolvedDecls = resolveAliasesInDecls(fm.params, fm.typeAliases);
    const cached: CachedInclude = {
      nodes,
      consts,
      declarations: resolvedDecls,
      baseDir: childBaseDir,
      typeAliases: fm.typeAliases.size > 0 ? fm.typeAliases : undefined,
    };
    cache.delete(absPath);
    cache.set(absPath, { hash, envHash, mtimeMs: stat.mtimeMs, cached });
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

/**
 * Hash the compile-time env values for include-cache invalidation.
 *
 * Keys are sorted so the hash is independent of insertion order, giving a
 * stable fingerprint of the `(name, value)` pairs baked into a cached include.
 */
function hashEnv(envValues?: Record<string, unknown>): number {
  if (!envValues) {
    return 0;
  }
  const parts = Object.keys(envValues)
    .sort()
    .map((key) => `${key}=${JSON.stringify(envValues[key])}`);
  return hashString(parts.join("\u001f"));
}
