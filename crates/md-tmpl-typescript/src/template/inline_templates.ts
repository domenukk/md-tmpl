/**
 * AST inspection helpers for inline `{% tmpl %}` templates.
 *
 * @module
 */

import { type VarDecl, parseFrontmatter } from "../frontmatter.js";
import { type Node } from "../parser.js";
import { type Value } from "../value.js";

// ---------------------------------------------------------------------------
// AST inspection helpers (for validation)
// ---------------------------------------------------------------------------

/** Collect all inline template names (`{% tmpl name %}`) from parsed nodes. */
export function collectInlineTemplateNames(
  nodes: readonly Node[],
): Set<string> {
  const names = new Set<string>();
  for (const node of nodes) {
    if (node.kind === "tmpl") {
      names.add(node.name);
    }
  }
  return names;
}

/**
 * Parse inline `{% tmpl name %}` blocks and return a map for the renderer.
 *
 * Each entry carries the child template's declarations (for contract
 * validation at include time), body text, and its own constants.
 */
export function collectInlineTemplateMap(nodes: readonly Node[]): Map<
  string,
  {
    declarations: readonly VarDecl[];
    body: string;
    consts: Map<string, Value>;
  }
> {
  const inlineTmpls = new Map<
    string,
    {
      declarations: readonly VarDecl[];
      body: string;
      consts: Map<string, Value>;
    }
  >();
  for (const n of nodes) {
    if (n.kind === "tmpl") {
      if (n.source.trimStart().startsWith("---")) {
        const [inlineFm, inlineBody] = parseFrontmatter(n.source);
        const inlineConsts = new Map<string, Value>();
        for (const decl of inlineFm.consts) {
          if (decl.defaultValue !== undefined) {
            inlineConsts.set(decl.name, decl.defaultValue);
          }
        }
        inlineTmpls.set(n.name, {
          declarations: inlineFm.params,
          body: inlineBody,
          consts: inlineConsts,
        });
      } else {
        inlineTmpls.set(n.name, {
          declarations: [],
          body: n.source,
          consts: new Map(),
        });
      }
    }
  }
  return inlineTmpls;
}
