/**
 * AST node rendering for md-tmpl templates against a layered scope.
 */

import type { Node } from "./ast.js";
import { Scope } from "./scope.js";
import {
  evaluateExpression,
  evaluateCondition,
  validateIncludeTypes,
  getVariantName,
  isOptionMatchNode,
} from "./evaluator.js";
import { parseBody } from "./parser.js";
import { type Value, display } from "./value.js";
import {
  TemplateError,
  TemplateSyntaxError,
  TemplatePanicError,
} from "./errors.js";
import {
  type VarDecl,
  varTypeToString,
  stripStringLiteral,
} from "./frontmatter.js";
import {
  PIPE,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_OPTION,
  TYPE_NONE,
  OPTION_SOME,
  NODE_TEXT,
  NODE_EXPR,
  NODE_COMMENT,
  NODE_FOR,
  NODE_IF,
  NODE_MATCH,
  NODE_RAW,
  NODE_INCLUDE,
  NODE_TMPL,
  NODE_PANIC,
  EXPR_START,
  EXPR_END,
  isValidResolvedPath,
  PREFIX_CONSTS_DOT,
  PREFIX_OPTS_DOT,
  PREFIX_OPTIONS_DOT,
  PREFIX_PARAMS_DOT,
} from "./consts.js";

export interface RenderOptions {
  /** Inline template definitions available for `{% include %}`. */
  inlineTemplates?: Map<
    string,
    {
      declarations: readonly VarDecl[];
      body: string;
      consts: Map<string, Value>;
    }
  >;
  /** Loader for file inclusions `{% include "path" %}`. */
  templateLoader?: (
    path: string,
    basePath?: string,
  ) =>
    | [readonly Node[], ReadonlyMap<string, Value>, readonly VarDecl[], string?]
    | undefined;
  /** Current base directory path for resolving relative includes. */
  currentBasePath?: string;
  /** Maximum recursion depth for file includes (default 16). */
  maxIncludeDepth?: number;
}

/**
 * Render AST nodes against a scope and options.
 */
export function renderNodes(
  nodes: readonly Node[],
  scope: Scope,
  options?: RenderOptions,
): string {
  const parts: string[] = [];

  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i]!;
    switch (node.kind) {
      case NODE_TEXT:
        parts.push(node.text);
        break;

      case NODE_EXPR: {
        if (node.trimBefore && parts.length > 0) {
          const last = parts[parts.length - 1]!;
          parts[parts.length - 1] = last.replace(/\s+$/, "");
        }
        const val = evaluateExpression(node.expr, scope);
        if (node.expr.includes(PIPE)) {
          parts.push(display(val));
        } else if (val.type === TYPE_LIST) {
          throw new TemplateError(
            `cannot display list '${node.expr}' directly; use a {% for %} loop or | join() filter`,
          );
        } else if (val.type === TYPE_STRUCT) {
          throw new TemplateError(
            `cannot display struct '${node.expr}' directly; access specific fields`,
          );
        } else {
          parts.push(display(val));
        }
        if (node.trimAfter) {
          if (i + 1 < nodes.length && nodes[i + 1]!.kind === "text") {
            const next = nodes[i + 1]! as { kind: "text"; text: string };
            parts.push(next.text.replace(/^\s+/, ""));
            i++;
          }
        }
        break;
      }

      case NODE_COMMENT:
        break;

      case NODE_FOR: {
        const listVal = evaluateExpression(node.iterExpr, scope);
        if (listVal.type !== TYPE_LIST) {
          throw new TemplateSyntaxError(
            `for loop requires list, got ${listVal.type}`,
          );
        }
        if (listVal.items.length === 0) {
          if (node.elseBody) {
            parts.push(renderNodes(node.elseBody, scope, options));
          }
        } else {
          const layer = scope.pushLayer();
          try {
            for (let idx = 0; idx < listVal.items.length; idx++) {
              const item = listVal.items[idx]!;
              layer.set(node.binding, item);
              scope.setLoopMeta(node.binding, { index: idx });
              parts.push(renderNodes(node.body, scope, options));
            }
          } finally {
            scope.popLayer();
          }
        }
        break;
      }

      case NODE_IF: {
        let matched = false;
        for (const branch of node.branches) {
          if (evaluateCondition(branch.condition, scope)) {
            parts.push(renderNodes(branch.body, scope, options));
            matched = true;
            break;
          }
        }
        if (!matched && node.elseBody) {
          parts.push(renderNodes(node.elseBody, scope, options));
        }
        break;
      }

      case NODE_MATCH: {
        const val = scope.resolvePath(node.expr);
        const isOpt = isOptionMatchNode(node) || scope.isOptionParam(node.expr);
        const variant = getVariantName(val, isOpt);

        if (node.inlineGuard) {
          if (node.inlineGuard.variant === variant) {
            const layer = scope.pushLayer();
            try {
              if (variant === OPTION_SOME && val.type !== TYPE_NONE) {
                layer.set(node.expr, val);
              }
              parts.push(renderNodes(node.inlineGuard.body, scope, options));
            } finally {
              scope.popLayer();
            }
          } else if (node.elseArm) {
            parts.push(renderNodes(node.elseArm, scope, options));
          }
          break;
        }

        let matched = false;
        for (const arm of node.arms) {
          if (arm.variants.includes(variant)) {
            const layer = scope.pushLayer();
            try {
              if (variant === OPTION_SOME && val.type !== TYPE_NONE) {
                layer.set(node.expr, val);
              }
              // If the arm has a guard, evaluate it
              if (arm.guard && !evaluateCondition(arm.guard, scope)) {
                continue;
              }
              parts.push(renderNodes(arm.body, scope, options));
            } finally {
              scope.popLayer();
            }
            matched = true;
            break;
          }
        }
        if (!matched && node.elseArm) {
          parts.push(renderNodes(node.elseArm, scope, options));
        } else if (!matched) {
          throw new TemplateError(
            `non-exhaustive match: variant '${variant}' not covered`,
          );
        }
        break;
      }

      case NODE_RAW:
        parts.push(node.text);
        break;

      case NODE_INCLUDE: {
        const maxDepth = options?.maxIncludeDepth ?? 16;
        let includedNodes: readonly Node[];
        let decls: readonly VarDecl[];
        let consts: ReadonlyMap<string, Value>;
        let fileChildOpts: RenderOptions | undefined = options;

        if (node.path !== undefined) {
          const resolvedPath = interpolateIncludePath(node.path, scope);
          if (!options?.templateLoader) {
            throw new TemplateError(
              `cannot resolve '{% include "${resolvedPath}" %}': file includes require a base directory (compile with fromFile or baseDir option)`,
            );
          }
          const loaded = options.templateLoader(
            resolvedPath,
            options.currentBasePath,
          );
          if (!loaded) {
            throw new TemplateError(
              `cannot resolve '{% include "${resolvedPath}" %}': file not found or load failed`,
            );
          }
          const [loadedNodes, loadedConsts, loadedDecls, loadedBasePath] =
            loaded;
          includedNodes = loadedNodes;
          decls = loadedDecls;
          consts = loadedConsts;
          fileChildOpts = {
            ...options,
            currentBasePath: loadedBasePath ?? options.currentBasePath,
            maxIncludeDepth: maxDepth - 1,
          };
        } else {
          const inline = options?.inlineTemplates?.get(node.name);
          if (!inline) {
            throw new TemplateError(
              `undefined inline template '${node.name}' (available: ${Array.from(options?.inlineTemplates?.keys() ?? []).join(", ")})`,
            );
          }
          includedNodes = parseBody(inline.body, true);
          decls = inline.declarations;
          consts = inline.consts;
          fileChildOpts = {
            ...options,
            maxIncludeDepth: maxDepth - 1,
          };
        }

        if (maxDepth <= 0) {
          throw new TemplateError(
            `maximum include depth exceeded when including '${node.path ?? node.name}'`,
          );
        }

        if (node.forBinding && node.forExpr) {
          const listVal = evaluateExpression(node.forExpr, scope);
          if (listVal.type !== TYPE_LIST) {
            throw new TemplateSyntaxError(
              `include ... for ... in requires list, got ${listVal.type}`,
            );
          }
          const results: string[] = [];
          for (const item of listVal.items) {
            const iterCtx = new Map<string, Value>();
            iterCtx.set(node.forBinding, item);
            for (const [targetKey, sourceExpr] of node.withMappings) {
              iterCtx.set(targetKey, evaluateExpression(sourceExpr, scope));
            }
            validateIncludeTypes(decls, iterCtx, node.path ?? node.name);
            for (const decl of decls) {
              if (!iterCtx.has(decl.name) && decl.defaultValue !== undefined) {
                iterCtx.set(decl.name, decl.defaultValue);
              }
            }
            for (const decl of decls) {
              if (!iterCtx.has(decl.name)) {
                if (decl.varType.kind === TYPE_OPTION) {
                  iterCtx.set(decl.name, { type: TYPE_NONE });
                } else {
                  throw new TemplateError(
                    `missing required parameter '${decl.name}' for include '${node.path ?? node.name}' (expected ${varTypeToString(decl.varType)})`,
                  );
                }
              }
            }
            const optParams = new Set<string>();
            for (const decl of decls) {
              if (decl.varType.kind === TYPE_OPTION) {
                optParams.add(decl.name);
              }
            }
            const childScope = new Scope(iterCtx, consts, optParams);
            results.push(renderNodes(includedNodes, childScope, fileChildOpts));
          }
          parts.push(results.join(""));
          break;
        }

        const childCtx = new Map<string, Value>();
        for (const [targetKey, sourceExpr] of node.withMappings) {
          childCtx.set(targetKey, evaluateExpression(sourceExpr, scope));
        }

        validateIncludeTypes(decls, childCtx, node.path ?? node.name);

        for (const decl of decls) {
          if (!childCtx.has(decl.name) && decl.defaultValue !== undefined) {
            childCtx.set(decl.name, decl.defaultValue);
          }
        }

        for (const decl of decls) {
          if (!childCtx.has(decl.name)) {
            if (decl.varType.kind === TYPE_OPTION) {
              childCtx.set(decl.name, { type: TYPE_NONE });
            } else {
              throw new TemplateError(
                `missing required parameter '${decl.name}' for include '${node.path ?? node.name}' (expected ${varTypeToString(decl.varType)})`,
              );
            }
          }
        }

        const optParams = new Set<string>();
        for (const decl of decls) {
          if (decl.varType.kind === TYPE_OPTION) {
            optParams.add(decl.name);
          }
        }

        const childScope = new Scope(childCtx, consts, optParams);
        parts.push(renderNodes(includedNodes, childScope, fileChildOpts));
        break;
      }

      case NODE_PANIC: {
        const msg = renderNodes(node.body, scope, options);
        throw new TemplatePanicError(msg.trim());
      }

      case NODE_TMPL:
        // Template definitions are stored at parse time, not rendered inline
        break;
    }
  }

  return parts.join("");
}

function interpolateIncludePath(path: string, scope: Scope): string {
  if (!path.includes(EXPR_START)) return path;
  let result = "";
  let remaining = path;
  while (true) {
    const startIdx = remaining.indexOf(EXPR_START);
    if (startIdx === -1) break;
    result += remaining.slice(0, startIdx);
    const afterStart = remaining.slice(startIdx + EXPR_START.length);
    const endIdx = afterStart.indexOf(EXPR_END);
    if (endIdx === -1) {
      throw new TemplateSyntaxError(
        `unclosed '${EXPR_START}' in include path '${path}'`,
      );
    }
    const expr = afterStart.slice(0, endIdx).trim();
    if (expr === "") {
      throw new TemplateSyntaxError(
        `empty expression '${EXPR_START}${EXPR_END}' in include path '${path}'`,
      );
    }
    let valStr: string;
    const lit = stripStringLiteral(expr);
    if (lit !== expr) {
      valStr = lit;
    } else {
      try {
        let val: any;
        try {
          val = evaluateExpression(expr, scope);
        } catch (err: any) {
          if (
            err?.name === "UndefinedVariableError" ||
            err?.message?.includes("undefined variable")
          ) {
            let stripped = expr;
            if (stripped.startsWith(PREFIX_CONSTS_DOT))
              stripped = stripped.slice(PREFIX_CONSTS_DOT.length).trim();
            else if (stripped.startsWith(PREFIX_OPTS_DOT))
              stripped = stripped.slice(PREFIX_OPTS_DOT.length).trim();
            else if (stripped.startsWith(PREFIX_OPTIONS_DOT))
              stripped = stripped.slice(PREFIX_OPTIONS_DOT.length).trim();
            else if (stripped.startsWith(PREFIX_PARAMS_DOT))
              stripped = stripped.slice(PREFIX_PARAMS_DOT.length).trim();
            if (stripped !== expr) {
              try {
                val = evaluateExpression(stripped, scope);
              } catch {
                throw err;
              }
            } else {
              throw err;
            }
          } else {
            throw err;
          }
        }
        valStr = display(val);
      } catch (err: any) {
        if (
          err?.name === "UndefinedVariableError" ||
          err?.message?.includes("undefined variable")
        ) {
          throw new TemplateSyntaxError(
            `undeclared variable '${expr}' in include path '${path}'`,
          );
        }
        throw new TemplateSyntaxError(
          `unresolvable expression '${EXPR_START}${expr}${EXPR_END}' in include path '${path}'`,
        );
      }
    }
    result += valStr;
    remaining = afterStart.slice(endIdx + EXPR_END.length);
  }
  result += remaining;
  if (!isValidResolvedPath(result) || result.includes(EXPR_START)) {
    throw new TemplateSyntaxError(
      `include path '${result}' must start with './', '../', or '/'`,
    );
  }
  return result;
}
