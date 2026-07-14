/**
 * Resolve and cache included template files.
 *
 * @module
 */

import { parseFrontmatter } from "../frontmatter.js";
import { parseBody } from "../parser.js";
import { type Value, fromJs, valueToJs } from "../value.js";
import { type CachedInclude } from "./types.js";
import { getFs, getPath, hashString } from "./utils.js";
import {
  injectEnumTypeConstants,
  resolveAliasesInDecls,
  resolveEnvDeclarations,
  resolveImportedConsts,
} from "./resolve.js";

export function resolveIncludeEntry(
  cache: Map<string, { hash: number; mtimeMs: number; cached: CachedInclude }>,
  filePath: string,
  baseDir?: string,
  maxEntries?: number,
  envValues?: Record<string, unknown>,
): CachedInclude | undefined {
  const currentBase = baseDir ?? "";
  const absPath = getPath().resolve(currentBase, filePath);
  let stat: { mtimeMs: number } | undefined;
  try {
    stat = getFs().statSync(absPath, { throwIfNoEntry: false });
  } catch (_err: unknown) {
    /* statSync can throw on permission errors; treat as not found */
    return undefined;
  }
  if (!stat) {
    return undefined;
  }
  const entry = cache.get(absPath);
  if (entry && entry.mtimeMs === stat.mtimeMs) {
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
  if (entry && entry.hash === hash) {
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
    cache.set(absPath, { hash, mtimeMs: stat.mtimeMs, cached });
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
