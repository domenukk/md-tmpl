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
import { TemplateSyntaxError, UnknownFilterError } from "../errors.js";
import {
  type VarDecl,
  type VarType,
  parseFrontmatter,
} from "../frontmatter.js";
import { cloneNodes, getLoc, nodeLoc } from "../ast.js";
import { type Node, parseBody } from "../parser.js";
import {
  type DeclaredSanitize,
  EMPTY_DECLARED_SANITIZE,
  declaredSanitize,
} from "../include_sanitize.js";
import { applySanitizeAstPass, type SanitizeSpec } from "../sanitize_pass.js";
import { validateFrontmatter } from "../validation.js";
import { type Value, valueToJs } from "../value.js";
import { resolveAliasesInDecls } from "./resolve.js";

/** Precompiled inline `{% tmpl name %}` entry. */
export interface InlineTemplateEntry {
  readonly declarations: readonly VarDecl[];
  readonly body: string;
  /** Body nodes parsed without the template's own parameter policy (include target). */
  readonly nodes: readonly Node[];
  readonly consts: Map<string, Value>;
  readonly constsJs: Map<string, unknown>;
  /** Declared parameter sanitization; only chooses the style for untrusted caller values. */
  readonly declared: DeclaredSanitize;
}

function bindInlineIncludes(
  nodes: readonly Node[],
  inlineTmpls: ReadonlyMap<
    string,
    { nodes: Node[]; declared: DeclaredSanitize }
  >,
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
            node.inlineDeclared = inline.declared;
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
 * Parse an inline template body, reporting syntax errors with **file-absolute**
 * line numbers. The body is parsed with body-relative lines (no line map is built
 * unless an error occurs); on error the line is shifted by the inline block's
 * position in the file and the body's offset within the block (`bodyLine`, 1-based).
 */
function parseInlineBody(
  n: Extract<Node, { kind: typeof NODE_TMPL }>,
  body: string,
  bodyLine: number,
): Node[] {
  try {
    return parseBody(body, true);
  } catch (err) {
    if (
      (err instanceof TemplateSyntaxError ||
        err instanceof UnknownFilterError) &&
      err.line !== undefined &&
      n.sourcePos !== undefined
    ) {
      const sourceLine = getLoc(n.sourcePos, n.lineMap)?.line;
      if (sourceLine !== undefined) {
        const line = sourceLine + bodyLine - 1 + err.line - 1;
        if (err instanceof UnknownFilterError) {
          throw new UnknownFilterError(
            err.filter,
            line,
            err.column,
            err.snippet,
          );
        }
        throw new TemplateSyntaxError(
          err.baseMessage,
          line,
          err.column,
          err.snippet,
        );
      }
    }
    throw err;
  }
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
      declared: DeclaredSanitize;
      sanitizeNotice?: string;
    }
  >();

  for (const n of nodes) {
    if (n.kind !== NODE_TMPL) continue;
    if (rawEntries.has(n.name)) {
      const loc = nodeLoc(n);
      throw new TemplateSyntaxError(
        `${ERR_DUPLICATE_INLINE_TMPL_PREFIX}'${n.name}'`,
        loc?.line,
        loc?.column,
        loc?.snippet,
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
      const inlineNodes = parseInlineBody(
        n,
        inlineBody,
        inlineFm.bodyStartLine ?? 1,
      );
      const inlineNotice = inlineFm.sanitizeNotice ?? parentSanitizeNotice;
      // Inline templates are include targets: parameter values take the caller's trust,
      // so only the notice and enclosing-tag passes run here.
      applySanitizeAstPass(inlineNodes, undefined, inlineNotice);
      rawEntries.set(n.name, {
        declarations: resolveAliasesInDecls(
          inlineFm.params,
          inlineFm.typeAliases,
        ),
        body: inlineBody,
        nodes: inlineNodes,
        consts: inlineConsts,
        constsJs: inlineConstsJs,
        declared: declaredSanitize(inlineFm.paramSanitize, inlineNotice),
        sanitizeNotice: inlineNotice,
      });
    } else {
      const inlineNodes = parseInlineBody(n, n.source, 1);
      applySanitizeAstPass(inlineNodes, undefined, parentSanitizeNotice);
      rawEntries.set(n.name, {
        declarations: [],
        body: n.source,
        nodes: inlineNodes,
        consts: new Map(),
        constsJs: new Map(),
        declared: EMPTY_DECLARED_SANITIZE,
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
      declared: entry.declared,
    });
  }
  return inlineTmpls;
}
