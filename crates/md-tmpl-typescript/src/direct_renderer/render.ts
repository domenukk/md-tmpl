/**
 * Direct renderer — the main entry point that walks AST nodes and
 * renders them directly from plain JS values.
 *
 * @module
 */

import { type Node } from "../parser.js";
import { type VarDecl } from "../frontmatter.js";
import {
  IncludeNotFoundError,
  TemplateError,
  TemplatePanicError,
  TemplateSyntaxError,
} from "../errors.js";
import { valueToJs } from "../value.js";
import {
  EXPR_START,
  KW_RAW,
  MATCH_DEFAULT,
  NODE_COMMENT,
  NODE_EXPR,
  NODE_FOR,
  NODE_IF,
  NODE_INCLUDE,
  NODE_MATCH,
  NODE_PANIC,
  NODE_TEXT,
  NODE_TMPL,
  unescapeStringLiteral,
} from "../consts.js";
import { type DirectRenderOptions } from "./options.js";
import { DirectScope } from "./scope.js";
import { directDisplay } from "./display.js";
import { evaluateDirectExpr } from "./expr.js";
import {
  evaluateDirectCondition,
  getDirectVariantName,
  interpolateDirectString,
  isOptionMatch,
} from "./condition.js";

/**
 * Check if a case arm label matches the active variant (direct renderer).
 * Supports quoted strings with {{ expr }} interpolation.
 */
export function directLabelMatches(
  label: string,
  variantName: string,
  scope: DirectScope,
): boolean {
  if (label === MATCH_DEFAULT || label === variantName) return true;
  // Quoted string literal: strip quotes and compare (with interpolation).
  if (
    label.length >= 2 &&
    ((label.startsWith('"') && label.endsWith('"')) ||
      (label.startsWith("'") && label.endsWith("'")))
  ) {
    const inner = unescapeStringLiteral(label.slice(1, -1));
    if (inner.includes(EXPR_START)) {
      try {
        return interpolateDirectString(inner, scope) === variantName;
      } catch {
        return false;
      }
    }
    return inner === variantName;
  }
  // Param-ref: resolve label as a variable and compare.
  const resolved = scope.resolve(label);
  return typeof resolved === "string" && resolved === variantName;
}
/**
 * Render AST nodes directly from JS values — no Value conversion.
 *
 * **Limitation:** `{% include %}` and `{% tmpl %}` nodes are silently
 * skipped in the direct renderer because resolving them would require
 * a template-loader / inline-template map.  This is acceptable because
 * `renderDirect` is only used by `renderUnchecked()`, where the caller
 * has explicitly opted out of full validation.
 *
 * @param nodes - Parsed AST nodes.
 * @param params - Plain JS params object.
 * @param constJsValues - Pre-converted constant values (as JS).
 */
export function renderDirect(
  nodes: readonly Node[],
  params: ReadonlyMap<string, unknown>,
  constJsValues: ReadonlyMap<string, unknown>,
  options?: DirectRenderOptions,
): string {
  const scope = new DirectScope(params, constJsValues);
  return renderDirectNodes(nodes, scope, options);
}

/** Render nodes with a direct scope. */
export function renderDirectNodes(
  nodes: readonly Node[],
  scope: DirectScope,
  options?: DirectRenderOptions,
): string {
  const parts: string[] = [];

  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    if (node === undefined) continue;
    switch (node.kind) {
      case NODE_TEXT:
        parts.push(node.text);
        break;

      case NODE_EXPR: {
        if (node.trimBefore && parts.length > 0) {
          const last = parts[parts.length - 1];
          if (last !== undefined) {
            parts[parts.length - 1] = last.replace(/\s+$/, "");
          }
        }
        const val = evaluateDirectExpr(node.expr, scope);
        parts.push(directDisplay(val));
        if (node.trimAfter) {
          // Trim leading whitespace from the next text node without
          // mutating the AST (which would corrupt subsequent renders).
          const nextNode = nodes[i + 1];
          if (nextNode?.kind === NODE_TEXT) {
            parts.push(nextNode.text.replace(/^\s+/, ""));
            i++; // skip the next node — we already handled it
          }
        }
        break;
      }

      case NODE_COMMENT:
        break;

      case NODE_FOR: {
        const listVal = evaluateDirectExpr(node.iterExpr, scope);
        if (!Array.isArray(listVal)) {
          throw new TemplateSyntaxError(
            `for loop requires a list, got ${typeof listVal}`,
          );
        }
        if (listVal.length === 0 && node.elseBody) {
          parts.push(renderDirectNodes(node.elseBody, scope, options));
        } else {
          for (let idx = 0; idx < listVal.length; idx++) {
            const item: unknown = listVal[idx];
            const layer = scope.pushLayer();
            layer.set(node.binding, item);
            scope.setLoopIndex(node.binding, idx);
            parts.push(renderDirectNodes(node.body, scope, options));
            scope.popLayer();
          }
        }
        break;
      }

      case NODE_IF: {
        let matched = false;
        for (const branch of node.branches) {
          if (evaluateDirectCondition(branch.condition, scope)) {
            parts.push(renderDirectNodes(branch.body, scope, options));
            matched = true;
            break;
          }
        }
        if (!matched && node.elseBody) {
          parts.push(renderDirectNodes(node.elseBody, scope, options));
        }
        break;
      }

      case NODE_MATCH: {
        const optMatch = isOptionMatch(node);
        if (node.inlineGuard) {
          const val = evaluateDirectExpr(node.expr, scope);
          const variantName = getDirectVariantName(val, optMatch);
          const label = node.inlineGuard.variant;
          if (directLabelMatches(label, variantName, scope)) {
            parts.push(
              renderDirectNodes(node.inlineGuard.body, scope, options),
            );
          }
        } else {
          const val = evaluateDirectExpr(node.expr, scope);
          const variantName = getDirectVariantName(val, optMatch);

          let matched = false;
          for (const arm of node.arms) {
            const armMatches = arm.variants.some((v) =>
              directLabelMatches(v, variantName, scope),
            );
            if (armMatches) {
              // If the arm has a guard, evaluate it
              if (arm.guard && !evaluateDirectCondition(arm.guard, scope)) {
                continue;
              }
              parts.push(renderDirectNodes(arm.body, scope, options));
              matched = true;
              break;
            }
          }
          if (!matched && node.elseArm) {
            parts.push(renderDirectNodes(node.elseArm, scope, options));
          }
        }
        break;
      }

      case KW_RAW:
        parts.push(node.text);
        break;

      case NODE_PANIC: {
        const msg = renderDirectNodes(node.body, scope, options);
        throw new TemplatePanicError(msg);
      }

      case NODE_INCLUDE: {
        const maxDepth = options?.maxIncludeDepth ?? 16;
        if (maxDepth <= 0) {
          throw new TemplateError(
            `maximum include depth exceeded when including '${node.path ?? node.name}'`,
          );
        }
        let includedNodes: readonly Node[];
        const loadedConsts = new Map<string, unknown>();
        let decls: readonly VarDecl[];
        let childOpts: DirectRenderOptions | undefined;

        if (node.path !== undefined) {
          if (!options?.templateLoader) {
            throw new TemplateError(
              `cannot resolve '{% include "${node.path}" %}': file includes require a base directory (compile with fromFile or baseDir option)`,
            );
          }
          const loaded = options.templateLoader(
            node.path,
            options.currentBasePath,
          );
          if (!loaded) {
            throw new IncludeNotFoundError(node.path);
          }
          const [lNodes, lConsts, lDecls, lBase] = loaded;
          includedNodes = lNodes;
          decls = lDecls;
          for (const [k, v] of lConsts) {
            loadedConsts.set(k, v);
          }
          childOpts = {
            ...options,
            currentBasePath: lBase ?? options.currentBasePath,
            maxIncludeDepth: maxDepth - 1,
          };
        } else {
          const inline = options?.inlineTemplates?.get(node.name);
          if (!inline) {
            throw new TemplateError(
              `undefined inline template '${node.name}' (available: ${Array.from(options?.inlineTemplates?.keys() ?? []).join(", ")})`,
            );
          }
          includedNodes = inline.nodes;
          decls = inline.declarations;
          for (const [k, v] of inline.consts) {
            loadedConsts.set(k, v);
          }
          childOpts = {
            ...options,
            maxIncludeDepth: maxDepth - 1,
          };
        }

        if (node.forBinding && node.forExpr) {
          const listVal = evaluateDirectExpr(node.forExpr, scope);
          if (!Array.isArray(listVal)) {
            throw new TemplateSyntaxError(
              `include ... for ... in requires list, got ${typeof listVal}`,
            );
          }
          const results: string[] = [];
          for (const item of listVal) {
            const iterMap = new Map<string, unknown>();
            iterMap.set(node.forBinding, item);
            for (const [targetKey, sourceExpr] of node.withMappings) {
              iterMap.set(targetKey, evaluateDirectExpr(sourceExpr, scope));
            }
            for (const decl of decls) {
              if (!iterMap.has(decl.name) && decl.defaultValue !== undefined) {
                iterMap.set(decl.name, valueToJs(decl.defaultValue));
              }
            }
            results.push(
              renderDirect(includedNodes, iterMap, loadedConsts, childOpts),
            );
          }
          parts.push(results.join(""));
          break;
        }

        const childMap = new Map<string, unknown>();
        for (const [targetKey, sourceExpr] of node.withMappings) {
          childMap.set(targetKey, evaluateDirectExpr(sourceExpr, scope));
        }
        for (const decl of decls) {
          if (!childMap.has(decl.name) && decl.defaultValue !== undefined) {
            childMap.set(decl.name, valueToJs(decl.defaultValue));
          }
        }
        parts.push(
          renderDirect(includedNodes, childMap, loadedConsts, childOpts),
        );
        break;
      }

      case NODE_TMPL:
        break;
    }
  }

  return parts.join("");
}
