/**
 * AST inspection and compilation helpers for inline `{% tmpl %}` templates.
 *
 * @module
 */

import {
  ERR_DUPLICATE_INLINE_TMPL_PREFIX,
  FM_DELIMITER,
  NODE_FOR,
  NODE_IF,
  NODE_INCLUDE,
  NODE_MATCH,
  NODE_PANIC,
  NODE_TMPL,
} from "../consts.js";
import { TemplateSyntaxError } from "../errors.js";
import {
  type VarDecl,
  type VarType,
  parseFrontmatter,
} from "../frontmatter.js";
import { type Node, parseBody } from "../parser.js";
import { applySanitizeAstPass, type SanitizeSpec } from "../sanitize_pass.js";
import { validateFrontmatter } from "../validation.js";
import { type Value, valueToJs } from "../value.js";
import { resolveAliasesInDecls } from "./resolve.js";

/** Precompiled inline `{% tmpl name %}` entry. */
export interface InlineTemplateEntry {
  readonly declarations: readonly VarDecl[];
  readonly body: string;
  readonly nodes: readonly Node[];
  readonly consts: Map<string, Value>;
  readonly constsJs: Map<string, unknown>;
}

/** Deep-clone AST nodes so site-specific sanitization can mutate expressions independently. */
function cloneNodes(nodes: readonly Node[]): Node[] {
  return nodes.map((node): Node => {
    switch (node.kind) {
      case NODE_FOR:
        return {
          ...node,
          body: cloneNodes(node.body),
          elseBody: node.elseBody ? cloneNodes(node.elseBody) : undefined,
        };
      case NODE_IF:
        return {
          ...node,
          branches: node.branches.map((b) => ({
            ...b,
            body: cloneNodes(b.body),
          })),
          elseBody: node.elseBody ? cloneNodes(node.elseBody) : undefined,
        };
      case NODE_MATCH:
        return {
          ...node,
          arms: node.arms.map((a) => ({
            ...a,
            variants: [...a.variants],
            body: cloneNodes(a.body),
          })),
          elseArm: node.elseArm ? cloneNodes(node.elseArm) : undefined,
          inlineGuard: node.inlineGuard
            ? {
                ...node.inlineGuard,
                body: cloneNodes(node.inlineGuard.body),
              }
            : undefined,
        };
      case NODE_PANIC:
        return {
          ...node,
          body: cloneNodes(node.body),
        };
      case NODE_INCLUDE:
        return {
          ...node,
          withMappings: new Map(node.withMappings),
          inlineNodes: node.inlineNodes
            ? cloneNodes(node.inlineNodes)
            : undefined,
        };
      default:
        return { ...node };
    }
  });
}

function bindInlineIncludes(
  nodes: readonly Node[],
  inlineTmpls: ReadonlyMap<string, { nodes: Node[] }>,
  activeStack: Set<string>,
): void {
  for (const node of nodes) {
    switch (node.kind) {
      case NODE_FOR:
        bindInlineIncludes(node.body, inlineTmpls, activeStack);
        if (node.elseBody) {
          bindInlineIncludes(node.elseBody, inlineTmpls, activeStack);
        }
        break;
      case NODE_IF:
        for (const branch of node.branches) {
          bindInlineIncludes(branch.body, inlineTmpls, activeStack);
        }
        if (node.elseBody) {
          bindInlineIncludes(node.elseBody, inlineTmpls, activeStack);
        }
        break;
      case NODE_MATCH:
        for (const arm of node.arms) {
          bindInlineIncludes(arm.body, inlineTmpls, activeStack);
        }
        if (node.elseArm) {
          bindInlineIncludes(node.elseArm, inlineTmpls, activeStack);
        }
        if (node.inlineGuard) {
          bindInlineIncludes(node.inlineGuard.body, inlineTmpls, activeStack);
        }
        break;
      case NODE_PANIC:
        bindInlineIncludes(node.body, inlineTmpls, activeStack);
        break;
      case NODE_INCLUDE:
        if (node.path === undefined && !activeStack.has(node.name)) {
          const inline = inlineTmpls.get(node.name);
          if (inline) {
            const cloned = cloneNodes(inline.nodes);
            activeStack.add(node.name);
            bindInlineIncludes(cloned, inlineTmpls, activeStack);
            activeStack.delete(node.name);
            node.inlineNodes = cloned;
          }
        }
        break;
      default:
        break;
    }
  }
}

/** Collect all inline template names (`{% tmpl name %}`) from parsed nodes. */
export function collectInlineTemplateNames(
  nodes: readonly Node[],
): Set<string> {
  const names = new Set<string>();
  for (const node of nodes) {
    if (node.kind === NODE_TMPL) {
      names.add(node.name);
    }
  }
  return names;
}

/**
 * Parse inline `{% tmpl name %}` blocks once, bind site-specific AST clones onto
 * matching `{% include name %}` nodes, and return the compiled inline map.
 */
export function collectInlineTemplateMap(
  nodes: readonly Node[],
  parentTypeAliases?: ReadonlyMap<string, VarType>,
  parentTypeAliasSanitize?: ReadonlyMap<
    string,
    ReadonlyMap<string, SanitizeSpec>
  >,
  parentSanitizeNotice?: string,
): Map<string, InlineTemplateEntry> {
  const rawEntries = new Map<
    string,
    {
      declarations: readonly VarDecl[];
      body: string;
      nodes: Node[];
      consts: Map<string, Value>;
      constsJs: Map<string, unknown>;
      paramSanitize?: ReadonlyMap<string, SanitizeSpec>;
      sanitizeNotice?: string;
    }
  >();

  for (const n of nodes) {
    if (n.kind !== NODE_TMPL) continue;
    if (rawEntries.has(n.name)) {
      throw new TemplateSyntaxError(
        `${ERR_DUPLICATE_INLINE_TMPL_PREFIX}'${n.name}'`,
        n.loc?.line,
        n.loc?.column,
        n.loc?.snippet,
      );
    }
    if (n.source.trimStart().startsWith(FM_DELIMITER)) {
      const [inlineFm, inlineBody] = parseFrontmatter(
        n.source,
        parentTypeAliases,
        parentTypeAliasSanitize,
      );
      validateFrontmatter(inlineFm);
      const inlineConsts = new Map<string, Value>();
      const inlineConstsJs = new Map<string, unknown>();
      for (const decl of inlineFm.consts) {
        if (decl.defaultValue !== undefined) {
          inlineConsts.set(decl.name, decl.defaultValue);
          inlineConstsJs.set(decl.name, valueToJs(decl.defaultValue));
        }
      }
      const inlineNodes = parseBody(inlineBody, true);
      applySanitizeAstPass(
        inlineNodes,
        inlineFm.paramSanitize,
        inlineFm.sanitizeNotice ?? parentSanitizeNotice,
      );
      rawEntries.set(n.name, {
        declarations: resolveAliasesInDecls(
          inlineFm.params,
          inlineFm.typeAliases,
        ),
        body: inlineBody,
        nodes: inlineNodes,
        consts: inlineConsts,
        constsJs: inlineConstsJs,
        paramSanitize: inlineFm.paramSanitize,
        sanitizeNotice: inlineFm.sanitizeNotice ?? parentSanitizeNotice,
      });
    } else {
      const inlineNodes = parseBody(n.source, true);
      applySanitizeAstPass(inlineNodes, undefined, parentSanitizeNotice);
      rawEntries.set(n.name, {
        declarations: [],
        body: n.source,
        nodes: inlineNodes,
        consts: new Map(),
        constsJs: new Map(),
        sanitizeNotice: parentSanitizeNotice,
      });
    }
  }

  if (rawEntries.size === 0) {
    return new Map();
  }

  // Bind nested inline includes inside each inline template and propagate enclosing tags.
  for (const [name, entry] of rawEntries) {
    bindInlineIncludes(entry.nodes, rawEntries, new Set([name]));
    applySanitizeAstPass(entry.nodes, undefined, entry.sanitizeNotice);
  }

  // Bind site-specific AST clones onto every inline `{% include %}` in `nodes`.
  bindInlineIncludes(nodes, rawEntries, new Set());

  const inlineTmpls = new Map<string, InlineTemplateEntry>();
  for (const [name, entry] of rawEntries) {
    inlineTmpls.set(name, {
      declarations: entry.declarations,
      body: entry.body,
      nodes: entry.nodes,
      consts: entry.consts,
      constsJs: entry.constsJs,
    });
  }
  return inlineTmpls;
}
