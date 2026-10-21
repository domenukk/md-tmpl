/**
 * Direct renderer — the main entry point that walks pre-compiled AST nodes and
 * renders them directly from plain JS values without boxing or per-iteration Map allocations.
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
  checkUntrustedIncludePath,
  contextualizeIncludeNodes,
} from "../sanitize_pass.js";
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
import { formatFixedFloat, formatFixedInt } from "../numeric_format.js";
import {
  type CompiledDirectExpr,
  compileDirectExpr,
  evalCompiledDirectExpr,
  evaluateDirectExpr,
} from "./expr.js";
import {
  type CompiledDirectCondition,
  compileDirectCondition,
  evalCompiledDirectCondition,
  getDirectVariantName,
  interpolateDirectString,
  isOptionMatch,
} from "./condition.js";

/**
 * Compile-time AST pass that:
 * 1. Applies `trimBefore` and `trimAfter` against adjacent static `NODE_TEXT` nodes.
 * 2. Pre-compiles `_compiled` expression, filter, and condition plans onto each AST node.
 */
export function precompileDirectNodes(nodes: Node[]): void {
  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    if (node === undefined) continue;
    switch (node.kind) {
      case NODE_EXPR: {
        if (node.trimBefore && i > 0) {
          const prev = nodes[i - 1];
          if (prev?.kind === NODE_TEXT) {
            const trimmed = prev.text.replace(/\s+$/, "");
            prev.text = trimmed;
            if (trimmed.length > 0) {
              node.trimBefore = false;
            }
          }
        }
        if (node.trimAfter) {
          const next = nodes[i + 1];
          if (next?.kind === NODE_TEXT) {
            next.text = next.text.replace(/^[ \t]*(?:\r?\n)?/, "");
          }
          node.trimAfter = false;
        }
        node._compiled = compileDirectExpr(node.expr);
        break;
      }
      case NODE_FOR:
        node._compiled = compileDirectExpr(node.iterExpr);
        precompileDirectNodes(node.body);
        if (node.elseBody) {
          precompileDirectNodes(node.elseBody);
        }
        break;
      case NODE_IF:
        for (const branch of node.branches) {
          branch._compiled = compileDirectCondition(branch.condition);
          precompileDirectNodes(branch.body);
        }
        if (node.elseBody) {
          precompileDirectNodes(node.elseBody);
        }
        break;
      case NODE_MATCH:
        node._compiled = compileDirectExpr(node.expr);
        for (const arm of node.arms) {
          if (arm.guard) {
            arm._compiled = compileDirectCondition(arm.guard);
          }
          precompileDirectNodes(arm.body);
        }
        if (node.elseArm) {
          precompileDirectNodes(node.elseArm);
        }
        if (node.inlineGuard) {
          precompileDirectNodes(node.inlineGuard.body);
        }
        break;
      case NODE_PANIC:
        precompileDirectNodes(node.body);
        break;
      default:
        break;
    }
  }
}

function evalNodeExpr(
  node: { expr: string; _compiled?: CompiledDirectExpr },
  scope: DirectScope,
): unknown {
  let compiled = node._compiled;
  if (compiled === undefined) {
    compiled = compileDirectExpr(node.expr);
    node._compiled = compiled;
  }
  return evalCompiledDirectExpr(compiled, scope);
}

function evalNodeIterExpr(
  node: { iterExpr: string; _compiled?: CompiledDirectExpr },
  scope: DirectScope,
): unknown {
  let compiled = node._compiled;
  if (compiled === undefined) {
    compiled = compileDirectExpr(node.iterExpr);
    node._compiled = compiled;
  }
  return evalCompiledDirectExpr(compiled, scope);
}

function evalBranchCond(
  branch: { condition: string; _compiled?: CompiledDirectCondition },
  scope: DirectScope,
): boolean {
  let compiled = branch._compiled;
  if (compiled === undefined) {
    compiled = compileDirectCondition(branch.condition);
    branch._compiled = compiled;
  }
  return evalCompiledDirectCondition(compiled, scope);
}

function evalArmGuard(
  arm: { guard?: string; _compiled?: CompiledDirectCondition },
  scope: DirectScope,
): boolean {
  if (!arm.guard) return true;
  let compiled = arm._compiled;
  if (compiled === undefined) {
    compiled = compileDirectCondition(arm.guard);
    arm._compiled = compiled;
  }
  return evalCompiledDirectCondition(compiled, scope);
}

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
  const resolved = scope.tryResolve(label);
  return typeof resolved === "string" && resolved === variantName;
}

/**
 * Render AST nodes directly from JS values — no Value conversion.
 *
 * @param nodes - Parsed AST nodes.
 * @param params - Plain JS params object or Map.
 * @param constJsValues - Pre-converted constant values (as JS).
 * @param options - Optional include/depth configuration.
 * @param defaults - Optional pre-computed default values.
 * @param declaredNames - Optional set of declared parameter names.
 */
export function renderDirect(
  nodes: readonly Node[],
  params: ReadonlyMap<string, unknown> | Record<string, unknown>,
  constJsValues: ReadonlyMap<string, unknown>,
  options?: DirectRenderOptions,
  defaults?: ReadonlyMap<string, unknown>,
  declaredNames?: ReadonlySet<string>,
): string {
  const scope = new DirectScope(params, constJsValues, defaults, declaredNames);
  return renderDirectNodes(nodes, scope, options);
}

/**
 * Render AST nodes using a reusable {@link DirectScope} instance.
 */
export function renderDirectWithScope(
  nodes: readonly Node[],
  scope: DirectScope,
  params: Record<string, unknown>,
  options?: DirectRenderOptions,
): string {
  scope.reset(params);
  return renderDirectNodes(nodes, scope, options);
}

/** Render nodes with a direct scope. */
export function renderDirectNodes(
  nodes: readonly Node[],
  scope: DirectScope,
  options?: DirectRenderOptions,
): string {
  let out = "";

  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    if (node === undefined) continue;
    switch (node.kind) {
      case NODE_TEXT:
        out += node.text;
        break;

      case NODE_EXPR: {
        if (!node.trimBefore && !node.trimAfter) {
          let compiled = node._compiled;
          if (compiled === undefined) {
            compiled = compileDirectExpr(node.expr);
            node._compiled = compiled;
          }
          if (compiled.kind === "prop1") {
            const propVal = scope.resolveProp1(
              compiled.root,
              compiled.prop,
              compiled.fullPath,
            );
            if (typeof propVal === "string") {
              out += propVal;
              break;
            }
            if (typeof propVal === "number" && Number.isSafeInteger(propVal)) {
              out += String(propVal);
              break;
            }
            out += directDisplay(propVal);
            break;
          }
          if (compiled.kind === "var") {
            const topVal =
              scope.topFrame === undefined &&
              !scope.hasConsts &&
              scope.defaults === undefined &&
              scope.topObj !== undefined
                ? scope.topObj[compiled.name]
                : scope.resolve(compiled.name);
            if (typeof topVal === "string") {
              out += topVal;
              break;
            }
            if (typeof topVal === "number" && Number.isSafeInteger(topVal)) {
              out += String(topVal);
              break;
            }
            out += directDisplay(topVal);
            break;
          }
          if (
            compiled.kind === "filter1" &&
            compiled.step.kind === "fixed" &&
            compiled.base.kind === "prop1"
          ) {
            const propVal = scope.resolveProp1(
              compiled.base.root,
              compiled.base.prop,
              compiled.base.fullPath,
            );
            if (typeof propVal === "number") {
              out += Number.isInteger(propVal)
                ? formatFixedInt(propVal, compiled.step.precision)
                : formatFixedFloat(propVal, compiled.step.precision);
              break;
            }
          }
          const val = evalCompiledDirectExpr(compiled, scope);
          out += typeof val === "string" ? val : directDisplay(val);
          break;
        }
        if (node.trimBefore && out.length > 0) {
          out = out.replace(/\s+$/, "");
        }
        const val = evalNodeExpr(node, scope);
        out += typeof val === "string" ? val : directDisplay(val);
        if (node.trimAfter) {
          const nextNode = nodes[i + 1];
          if (nextNode?.kind === NODE_TEXT) {
            out += nextNode.text.replace(/^[ \t]*(?:\r?\n)?/, "");
            i++;
          }
        }
        break;
      }

      case NODE_COMMENT:
        break;

      case NODE_FOR: {
        const listVal = evalNodeIterExpr(node, scope);
        if (!Array.isArray(listVal)) {
          throw new TemplateSyntaxError(
            `for loop requires a list, got ${typeof listVal}`,
          );
        }
        const len = listVal.length;
        if (len === 0) {
          if (node.elseBody) {
            out += renderDirectNodes(node.elseBody, scope, options);
          }
        } else {
          const frame = scope.pushLoop(node.binding, len);
          try {
            for (let idx = 0; idx < len; idx++) {
              frame.value = listVal[idx];
              frame.index = idx;
              out += renderDirectNodes(node.body, scope, options);
            }
          } finally {
            scope.popLoop();
          }
        }
        break;
      }

      case NODE_IF: {
        let matched = false;
        for (const branch of node.branches) {
          if (evalBranchCond(branch, scope)) {
            out += renderDirectNodes(branch.body, scope, options);
            matched = true;
            break;
          }
        }
        if (!matched && node.elseBody) {
          out += renderDirectNodes(node.elseBody, scope, options);
        }
        break;
      }

      case NODE_MATCH: {
        const optMatch = isOptionMatch(node);
        if (node.inlineGuard) {
          const val = evalNodeExpr(node, scope);
          const variantName = getDirectVariantName(val, optMatch);
          const label = node.inlineGuard.variant;
          if (directLabelMatches(label, variantName, scope)) {
            out += renderDirectNodes(node.inlineGuard.body, scope, options);
          } else if (node.elseArm) {
            out += renderDirectNodes(node.elseArm, scope, options);
          }
        } else {
          const val = evalNodeExpr(node, scope);
          const variantName = getDirectVariantName(val, optMatch);

          let matched = false;
          for (const arm of node.arms) {
            let armMatches = false;
            for (const variantLabel of arm.variants) {
              if (directLabelMatches(variantLabel, variantName, scope)) {
                armMatches = true;
                break;
              }
            }
            if (armMatches) {
              if (!evalArmGuard(arm, scope)) {
                continue;
              }
              out += renderDirectNodes(arm.body, scope, options);
              matched = true;
              break;
            }
          }
          if (!matched && node.elseArm) {
            out += renderDirectNodes(node.elseArm, scope, options);
          }
        }
        break;
      }

      case KW_RAW:
        out += node.text;
        break;

      case NODE_PANIC: {
        const msg = renderDirectNodes(node.body, scope, options);
        throw new TemplatePanicError(msg.trim());
      }

      case NODE_INCLUDE: {
        const maxDepth = options?.maxIncludeDepth ?? 16;
        if (maxDepth <= 0) {
          throw new TemplateError(
            `maximum include depth exceeded when including '${node.path ?? node.name}'`,
          );
        }
        checkUntrustedIncludePath(node);
        let includedNodes: readonly Node[];
        const loadedConsts = new Map<string, unknown>(scope.allConsts());
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
          const [lNodes, lConsts, lDecls, lBase, lInline] = loaded;
          includedNodes = contextualizeIncludeNodes(lNodes, node.sanitizeCtx);
          decls = lDecls;
          for (const [k, v] of lConsts) {
            loadedConsts.set(k, v);
          }
          childOpts = {
            ...options,
            inlineTemplates: lInline && lInline.size > 0 ? lInline : undefined,
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
          includedNodes =
            node.inlineNodes ??
            contextualizeIncludeNodes(inline.nodes, node.sanitizeCtx);
          decls = inline.declarations;
          const inlineConsts = inline.constsJs ?? inline.consts;
          if (inlineConsts) {
            for (const [k, v] of inlineConsts) {
              loadedConsts.set(k, v);
            }
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
            out += renderDirect(
              includedNodes,
              iterMap,
              loadedConsts,
              childOpts,
            );
          }
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
        out += renderDirect(includedNodes, childMap, loadedConsts, childOpts);
        break;
      }

      case NODE_TMPL:
        break;
    }
  }

  return out;
}
