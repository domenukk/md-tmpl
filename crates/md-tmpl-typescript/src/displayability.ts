import { TemplateSyntaxError } from "./errors.js";
import type { VarDecl, VarType } from "./frontmatter.js";
import {
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_ENUM,
  TYPE_TMPL,
  TYPE_OPTION,
  TYPE_ALIAS,
  TYPE_SCALAR_LIST,
  TYPE_UNTYPED_LIST,
  PIPE,
  OPTION_SOME,
  MATCH_DEFAULT,
  OP_IN_SPACED,
  NODE_EXPR,
  NODE_IF,
  NODE_MATCH,
  NODE_FOR,
  NODE_INCLUDE,
  DOT,
  TAG_MATCH_PREFIX,
} from "./consts.js";
import { splitPipes } from "./evaluator.js";
import { TypeEnv } from "./type_env.js";
import { collectReferencedParams } from "./template/references.js";
import type { Node } from "./parser.js";
import {
  comparisonError,
  formatVarType,
  includeContractError,
  operandError,
  pathAccessError,
  splitComparison,
} from "./type_rules.js";

// ---------------------------------------------------------------------------
// Type environment for flow-sensitive narrowing
// ---------------------------------------------------------------------------
// Compile-time displayability check (with flow-sensitive narrowing)
// ---------------------------------------------------------------------------

/** Scalar types that can appear in {{ expr }} interpolations. */
function isDisplayableType(ty: VarType): boolean {
  // Alias types can't be resolved here (we'd need the full type alias map).
  // Conservatively allow them — the resolved type may be scalar.
  if (ty.kind === TYPE_ALIAS) return true;
  // Enum types with only unit variants (no fields) are displayable since
  // they're just string values at runtime (e.g., enum(admin, guest)).
  if (ty.kind === TYPE_ENUM) {
    return ty.variants.every((v) => v.fields.length === 0);
  }
  return (
    ty.kind === TYPE_STR ||
    ty.kind === TYPE_INT ||
    ty.kind === TYPE_FLOAT ||
    ty.kind === TYPE_BOOL
  );
}

/** Hint message for non-displayable types. */
function displayHint(ty: VarType): string {
  switch (ty.kind) {
    case TYPE_LIST:
    case TYPE_SCALAR_LIST:
    case TYPE_UNTYPED_LIST:
      return "use {% for %} to iterate, or | join()";
    case TYPE_TMPL:
    case TYPE_STRUCT:
      return "access fields with dot notation, e.g. {{ x.field }}";
    case TYPE_ENUM:
      return "use kind(x) for the variant name, or {% match %}";
    case TYPE_OPTION:
      return "use {% if has(x) %} to unwrap, or {% match %}";
    default:
      return "only str, int, float, bool can be displayed";
  }
}

/**
 * Returns a diagnostic message if `ty` has no truthiness and therefore cannot
 * be used directly as a bare `{% if %}` / `{% elif %}` condition. `struct`,
 * (non-option) `enum`, and `tmpl` have no truthiness; all other types do.
 * Mirrors the Rust core's `non_truthy_message`.
 */
function nonTruthyMessage(ty: VarType): string | undefined {
  if (ty.kind === TYPE_STRUCT) {
    return "cannot evaluate truthiness of struct — access a field or compare it instead";
  }
  if (ty.kind === TYPE_ENUM && !ty.isOption) {
    return "cannot evaluate truthiness of enum — use {% match %} instead";
  }
  if (ty.kind === TYPE_TMPL) {
    return "cannot evaluate truthiness of template handle — use {% include %} instead";
  }
  if (ty.kind === TYPE_OPTION || (ty.kind === TYPE_ENUM && ty.isOption)) {
    return "cannot evaluate truthiness of option — use has(x) to check presence";
  }
  return undefined;
}

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Flow-sensitive AST walker
// ---------------------------------------------------------------------------

/**
 * Extract has() narrowing from a condition string.
 *
 * If the condition is `has(path)`, and `path` resolves to `option(T)` in the
 * current environment, returns `[path, T]` — the narrowed type.
 */
function extractHasNarrowing(
  condition: string,
  env: TypeEnv,
): [string, VarType] | undefined {
  const trimmed = condition.trim();
  const match = /^has\(\s*([^)]+?)\s*\)$/.exec(trimmed);
  if (!match?.[1]) return undefined;
  const path = match[1].trim();

  const ty = env.resolveExprType(path);
  if (!ty) return undefined;

  if (ty.kind === TYPE_OPTION) {
    return [path, ty.innerType];
  }

  if (ty.kind === TYPE_ENUM && ty.isOption) {
    const someVariant = ty.variants.find((v) => v.name === OPTION_SOME);
    if (someVariant?.fields.length === 1) {
      const someField = someVariant.fields[0];
      if (someField !== undefined) {
        return [path, someField.varType];
      }
    }
  }

  return undefined;
}

function extractAllHasNarrowings(
  condition: string,
  env: TypeEnv,
): [string, VarType][] {
  const results: [string, VarType][] = [];
  const opIdx = findTopLevelOp(condition, ["&&"]);
  if (opIdx !== -1) {
    const left = condition.slice(0, opIdx).trim();
    const right = condition.slice(opIdx + 2).trim();
    results.push(...extractAllHasNarrowings(left, env));
    results.push(...extractAllHasNarrowings(right, env));
  } else {
    const n = extractHasNarrowing(condition, env);
    if (n) results.push(n);
  }
  return results;
}

/**
 * Extract !has() or !option_var negative narrowing.
 * When `!has(path)` or `!path` occurs in a branch, `path: option(T)` is proven
 * to be `Some(T)` in all subsequent fall-through branches and `else` blocks.
 */
function extractNotHasNarrowing(
  condition: string,
  env: TypeEnv,
): [string, VarType] | undefined {
  const trimmed = condition.trim();
  if (trimmed.startsWith("!")) {
    const inner = trimmed.slice(1).trim();
    return extractHasNarrowing(inner, env);
  }
  return undefined;
}

interface ValidationError {
  message: string;
  loc?: import("./parser.js").SourceLocation;
}

/** What the type checker needs to know about an inline `{% tmpl %}` definition. */
export interface InlineTemplateSignature {
  /** Declared params of the inline template. */
  readonly declarations: readonly VarDecl[];
  /** Parsed body of the inline template. */
  readonly nodes: readonly Node[];
  /** Names of the inline template's own constants. */
  readonly constNames: ReadonlySet<string>;
}

/** Shared state of one compile-time walk over a template body. */
interface WalkContext {
  readonly errors: ValidationError[];
  /** Inline `{% tmpl %}` definitions of the template, by name. */
  readonly inlineTemplates: ReadonlyMap<string, InlineTemplateSignature>;
  /** Names valid in every scope: import stems, enum types, inline templates. */
  readonly globalNames: ReadonlySet<string>;
  readonly typeAliases: ReadonlyMap<string, VarType> | undefined;
  /** Inline templates whose bodies are being walked (cycle guard). */
  readonly visitingInline: Set<string>;
}

/** Validate static condition checks at compile time (e.g., literal in kinds(Enum), condition operand types). */
function validateStaticCondition(
  condition: string,
  env: TypeEnv,
  errors: ValidationError[],
  loc?: import("./parser.js").SourceLocation,
): void {
  const trimmed = condition.trim();

  const inIdx = trimmed.indexOf(OP_IN_SPACED);
  if (inIdx !== -1) {
    const left = trimmed.slice(0, inIdx).trim();
    const right = trimmed.slice(inIdx + OP_IN_SPACED.length).trim();
    const kindsMatch = /^kinds\(\s*([a-zA-Z0-9_-]+)\s*\)$/.exec(right);
    if (kindsMatch?.[1] !== undefined) {
      const enumName = kindsMatch[1];
      const enumType = env.resolveExprType(enumName);
      if (enumType?.kind === TYPE_ENUM) {
        if (left.startsWith('"') && left.endsWith('"') && left.length >= 2) {
          const strVal = left.slice(1, -1);
          if (!enumType.variants.some((v) => v.name === strVal)) {
            errors.push({
              message: `static string "${strVal}" is not a valid variant of enum '${enumName}'`,
              loc,
            });
          }
        }
      }
    }
  }

  function checkLeaf(sub: string, currentEnv: TypeEnv): void {
    let s = sub.trim();
    while (s.startsWith("(") && s.endsWith(")")) {
      s = s.slice(1, -1).trim();
    }
    while (s.startsWith("!")) {
      s = s.slice(1).trim();
      while (s.startsWith("(") && s.endsWith(")")) {
        s = s.slice(1, -1).trim();
      }
    }
    if (!s) return;

    const opIdx = findTopLevelOp(s, ["&&", "||"]);
    if (opIdx !== -1) {
      const op = s.slice(opIdx, opIdx + 2);
      const leftStr = s.slice(0, opIdx).trim();
      const rightStr = s.slice(opIdx + 2).trim();
      checkLeaf(leftStr, currentEnv);
      let rightEnv = currentEnv;
      if (op === "&&") {
        const hasMatch = /^has\(\s*([^)]+?)\s*\)$/.exec(leftStr);
        if (hasMatch?.[1]) {
          const target = hasMatch[1].trim();
          const targetType = currentEnv.resolveExprType(target);
          if (
            targetType &&
            (targetType.kind === TYPE_OPTION ||
              (targetType.kind === TYPE_ENUM && targetType.isOption))
          ) {
            const inner =
              targetType.kind === TYPE_OPTION
                ? targetType.innerType
                : targetType.variants.find((v) => v.name === "Some")?.fields[0]
                    ?.varType;
            if (inner) {
              rightEnv = currentEnv.withNarrowing(target, inner);
            }
          }
        }
      }
      checkLeaf(rightStr, rightEnv);
      return;
    }

    if (s.includes(OP_IN_SPACED) || s.startsWith(TAG_MATCH_PREFIX)) {
      return;
    }

    const comparison = splitComparison(s);
    if (comparison !== undefined) {
      const message = comparisonError(comparison, currentEnv);
      if (message !== undefined) errors.push({ message, loc });
      return;
    }

    const operandMessage = operandError(s, currentEnv);
    if (operandMessage !== undefined) {
      errors.push({ message: operandMessage, loc });
      return;
    }

    // Bare truthiness: struct, enum (non-option), tmpl, and option have no
    // truthiness and cannot be used directly as a condition.
    const leafType = currentEnv.resolveExprType(s);
    if (leafType) {
      const nonTruthy = nonTruthyMessage(leafType);
      if (nonTruthy) {
        errors.push({
          message: `type error in condition: ${nonTruthy}, got ${formatVarType(leafType)}`,
          loc,
        });
      }
    }
  }

  checkLeaf(trimmed, env);
}

function findTopLevelOp(str: string, ops: string[]): number {
  let parenDepth = 0;
  let inString = false;
  let stringChar = "";
  for (let i = 0; i < str.length - 1; i++) {
    const ch = str[i];
    if (inString) {
      if (ch === stringChar && str[i - 1] !== "\\") inString = false;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inString = true;
      stringChar = ch;
      continue;
    }
    if (ch === "(") parenDepth++;
    else if (ch === ")") parenDepth--;
    else if (parenDepth === 0) {
      const pair = str.slice(i, i + 2);
      if (ops.includes(pair)) return i;
    }
  }
  return -1;
}

/**
 * Walk AST nodes with flow-sensitive narrowing, collecting displayability errors.
 *
 * This is the core of the compile-time type checker for the TS backend.
 * It mirrors the Rust `walk_segments` + `validate_compiled_path` logic.
 */
function walkNodesWithNarrowing(
  nodes: readonly import("./parser.js").Node[],
  env: TypeEnv,
  ctx: WalkContext,
): void {
  const { errors } = ctx;
  for (const node of nodes) {
    switch (node.kind) {
      case NODE_EXPR: {
        const operandMessage = operandError(node.expr, env);
        if (operandMessage !== undefined) {
          errors.push({ message: operandMessage, loc: node.loc });
          break;
        }
        const resolvedType = env.resolveExprType(node.expr);
        if (resolvedType === undefined) continue;
        if (!isDisplayableType(resolvedType)) {
          const hint = displayHint(resolvedType);
          errors.push({
            message: `'${node.expr.trim()}': cannot display value of type ${formatVarType(resolvedType)} — ${hint}`,
            loc: node.loc,
          });
        }
        break;
      }

      case NODE_IF: {
        let currentEnv = env;
        for (const branch of node.branches) {
          validateStaticCondition(
            branch.condition,
            currentEnv,
            errors,
            node.loc,
          );
          const posNarrowings = extractAllHasNarrowings(
            branch.condition,
            currentEnv,
          );
          let branchBodyEnv = currentEnv;
          for (const [path, innerType] of posNarrowings) {
            branchBodyEnv = branchBodyEnv.withNarrowing(path, innerType);
          }
          walkNodesWithNarrowing(branch.body, branchBodyEnv, ctx);

          const negNarrowing = extractNotHasNarrowing(
            branch.condition,
            currentEnv,
          );
          if (negNarrowing) {
            currentEnv = currentEnv.withNarrowing(
              negNarrowing[0],
              negNarrowing[1],
            );
          }
        }
        if (node.elseBody) {
          walkNodesWithNarrowing(node.elseBody, currentEnv, ctx);
        }
        break;
      }

      case NODE_MATCH: {
        const exprPath = node.expr.trim();
        const exprPathMessage = pathAccessError(exprPath, env);
        if (exprPathMessage !== undefined) {
          errors.push({ message: exprPathMessage, loc: node.loc });
        }
        const exprType = env.resolveExprType(exprPath);

        if (exprType?.kind === TYPE_ENUM) {
          // Narrow each arm to only the matched variant(s)
          for (const arm of node.arms) {
            const matchedVariants = exprType.variants.filter((v) =>
              arm.variants.includes(v.name),
            );
            if (matchedVariants.length > 0) {
              const narrowedType: VarType = {
                kind: TYPE_ENUM,
                variants: matchedVariants,
              };
              const narrowedEnv = env.withNarrowing(exprPath, narrowedType);
              walkNodesWithNarrowing(arm.body, narrowedEnv, ctx);
            } else if (
              arm.variants.length === 1 &&
              arm.variants[0] === MATCH_DEFAULT
            ) {
              // Default arm — no narrowing
              walkNodesWithNarrowing(arm.body, env, ctx);
            } else {
              walkNodesWithNarrowing(arm.body, env, ctx);
            }
          }
          if (
            node.arms.length > 1 &&
            !node.elseArm &&
            !node.arms.some((a) => a.variants.includes(MATCH_DEFAULT))
          ) {
            const covered = new Set<string>();
            for (const arm of node.arms) {
              for (const v of arm.variants) covered.add(v);
            }
            const missing = exprType.variants
              .filter((v) => !covered.has(v.name))
              .map((v) => v.name);
            if (missing.length > 0) {
              const cases = missing.map((m) => `{% case ${m} %}`).join(" ");
              const suggestion =
                missing.length > 1
                  ? `Try adding explicit arms: ${cases} or combined arm: {% case ${missing.join(" | ")} %}`
                  : `Try adding explicit arm: ${cases}`;
              errors.push({
                message: `match on '${exprPath}': non-exhaustive — missing variant(s): ${missing.join(", ")}. ${suggestion}`,
                loc: node.loc,
              });
            }
          }
        } else if (exprType?.kind === TYPE_OPTION) {
          for (const arm of node.arms) {
            if (arm.variants.includes(OPTION_SOME)) {
              // Narrow option to inner type
              const narrowedEnv = env.withNarrowing(
                exprPath,
                exprType.innerType,
              );
              walkNodesWithNarrowing(arm.body, narrowedEnv, ctx);
            } else {
              walkNodesWithNarrowing(arm.body, env, ctx);
            }
          }
        } else {
          // Can't resolve match expression type — walk arms without narrowing
          for (const arm of node.arms) {
            walkNodesWithNarrowing(arm.body, env, ctx);
          }
        }

        if (node.elseArm) {
          walkNodesWithNarrowing(node.elseArm, env, ctx);
        }

        // Inline guard
        if (node.inlineGuard) {
          const guard = node.inlineGuard;
          if (exprType?.kind === TYPE_ENUM) {
            const matchedVariants = exprType.variants.filter(
              (v) => v.name === guard.variant,
            );
            if (matchedVariants.length > 0) {
              const narrowedType: VarType = {
                kind: TYPE_ENUM,
                variants: matchedVariants,
              };
              const narrowedEnv = env.withNarrowing(exprPath, narrowedType);
              walkNodesWithNarrowing(node.inlineGuard.body, narrowedEnv, ctx);
            } else {
              walkNodesWithNarrowing(node.inlineGuard.body, env, ctx);
            }
          } else {
            walkNodesWithNarrowing(node.inlineGuard.body, env, ctx);
          }
        }
        break;
      }

      case NODE_FOR: {
        if (node.iterExpr.includes(PIPE)) {
          const filterParts = splitPipes(node.iterExpr).slice(1);
          for (const rawFilter of filterParts) {
            const trimmedFilter = rawFilter.trim();
            const parenIdx = trimmedFilter.indexOf("(");
            const fname = (
              parenIdx === -1 ? trimmedFilter : trimmedFilter.slice(0, parenIdx)
            ).trim();
            if (
              fname !== "limit" &&
              fname !== "truncate" &&
              fname !== "truncate_middle"
            ) {
              errors.push({
                message: `for-loop iterable '${node.iterExpr.trim()}': filter '${fname}' does not return a list (only 'limit' and 'truncate' are valid on for-loop iterables)`,
                loc: node.loc,
              });
            }
          }
        }
        const iterPathMessage = pathAccessError(node.iterExpr, env);
        if (iterPathMessage !== undefined) {
          errors.push({ message: iterPathMessage, loc: node.loc });
        }
        // Resolve the iterator expression type to determine element type
        const iterType = env.resolveExprType(node.iterExpr);
        if (iterType) {
          let elementType: VarType | undefined;
          if (iterType.kind === TYPE_LIST) {
            // list(name = str, ...) → struct(name = str, ...)
            elementType = { kind: TYPE_STRUCT, fields: iterType.fields };
          } else if (iterType.kind === TYPE_SCALAR_LIST) {
            elementType = iterType.elementType;
          } else if (iterType.kind === TYPE_UNTYPED_LIST) {
            // Can't determine element type
            elementType = undefined;
          }
          if (elementType) {
            const forEnv = env.withBinding(node.binding, elementType);
            walkNodesWithNarrowing(node.body, forEnv, ctx);
          } else {
            walkNodesWithNarrowing(node.body, env, ctx);
          }
        } else {
          walkNodesWithNarrowing(node.body, env, ctx);
        }

        if (node.elseBody) {
          walkNodesWithNarrowing(node.elseBody, env, ctx);
        }
        break;
      }

      case NODE_INCLUDE: {
        for (const valueExpr of node.withMappings.values()) {
          const message = pathAccessError(valueExpr, env);
          if (message !== undefined) errors.push({ message, loc: node.loc });
        }
        const inline =
          node.path === undefined
            ? ctx.inlineTemplates.get(node.name)
            : undefined;
        if (inline !== undefined) {
          const provided = new Set(node.withMappings.keys());
          if (node.forBinding !== undefined) provided.add(node.forBinding);
          const message = includeContractError(
            node.name,
            inline.declarations,
            provided,
          );
          if (message !== undefined) errors.push({ message, loc: node.loc });
          walkInlineTemplateBody(node.name, inline, ctx);
        }
        break;
      }

      default:
        // text, comment, raw, tmpl — no expressions to check
        break;
    }
  }
}

/**
 * Type-check an included inline template's body in its own scope (its params
 * and consts plus the template-wide names), once per include chain — mirrors
 * the Rust include walker.
 */
function walkInlineTemplateBody(
  name: string,
  inline: InlineTemplateSignature,
  ctx: WalkContext,
): void {
  if (ctx.visitingInline.has(name)) return;
  ctx.visitingInline.add(name);
  const inScope = new Set([
    ...inline.declarations.map((d) => d.name),
    ...inline.constNames,
    ...ctx.globalNames,
  ]);
  const undeclared = [...collectReferencedParams(inline.nodes)]
    .filter((root) => !inScope.has(root))
    .sort();
  for (const root of undeclared) {
    ctx.errors.push({ message: `'${root}': undeclared variable` });
  }
  const childEnv = new TypeEnv(inline.declarations, undefined, ctx.typeAliases);
  walkNodesWithNarrowing(inline.nodes, childEnv, ctx);
  ctx.visitingInline.delete(name);
}

/**
 * Extend an import stem's namespace struct with the stem's imported type
 * aliases (keyed `stem.Name` in `typeAliases`), so `kinds(stem.Enum)` and
 * `stem.Enum.Variant` resolve like in the Rust checker. Const fields win on
 * a name collision.
 */
function withImportedTypeAliases(
  stem: string,
  nsType: VarType,
  typeAliases: ReadonlyMap<string, VarType> | undefined,
): VarType {
  if (nsType.kind !== TYPE_STRUCT || typeAliases === undefined) return nsType;
  const prefix = `${stem}${DOT}`;
  const constNames = new Set(nsType.fields.map((f) => f.name));
  const aliasFields: VarDecl[] = [...typeAliases]
    .filter(([key]) => key.startsWith(prefix))
    .map(([key, varType]) => ({ name: key.slice(prefix.length), varType }))
    .filter((f) => !constNames.has(f.name));
  return { kind: TYPE_STRUCT, fields: [...nsType.fields, ...aliasFields] };
}

/**
 * Validate that all `{{ expr }}` interpolations resolve to displayable
 * (scalar) types, with proper flow-sensitive narrowing through:
 *
 * - `{% if has(x) %}` — narrows `option(T)` to `T` in the true branch
 * - `{% match x %}{% case V %}` — narrows enum to matched variant
 * - `{% for item in list %}` — introduces element binding
 *
 * This is a compile-time check — called during `fromSource()`.
 *
 * @param nodes - Parsed AST nodes from the template body.
 * @param declarations - Parameter declarations from frontmatter.
 * @param consts - Constant declarations from frontmatter.
 * @param typeAliases - Type alias map from frontmatter.
 * @param importedNamespaceTypes - Typed import namespace map (stem → struct type).
 * @param inlineTemplates - Inline `{% tmpl %}` definitions, by name.
 * @param globalNames - Names valid in every scope (import stems, enum types, inline templates).
 * @throws {TemplateSyntaxError} If an expression resolves to a non-displayable type.
 */
export function validateDisplayability(
  nodes: readonly import("./parser.js").Node[],
  declarations: readonly VarDecl[],
  consts?: readonly VarDecl[],
  typeAliases?: ReadonlyMap<string, VarType>,
  importedNamespaceTypes?: ReadonlyMap<string, VarType>,
  inlineTemplates: ReadonlyMap<string, InlineTemplateSignature> = new Map(),
  globalNames: ReadonlySet<string> = new Set(),
): void {
  const allDecls: VarDecl[] = consts
    ? [...declarations, ...consts]
    : [...declarations];
  // Register typed import stems as declarations so the type checker can
  // resolve field accesses on imported consts (e.g. `artist.SEVERITY_LADDER`).
  if (importedNamespaceTypes) {
    for (const [stem, nsType] of importedNamespaceTypes) {
      allDecls.push({
        name: stem,
        varType: withImportedTypeAliases(stem, nsType, typeAliases),
      });
    }
  }
  const env = new TypeEnv(allDecls, undefined, typeAliases);
  const errors: ValidationError[] = [];

  walkNodesWithNarrowing(nodes, env, {
    errors,
    inlineTemplates,
    globalNames,
    typeAliases,
    visitingInline: new Set(),
  });

  if (errors.length > 0) {
    const err = errors[0];
    if (err !== undefined) {
      throw new TemplateSyntaxError(
        err.message,
        err.loc?.line,
        err.loc?.column,
        err.loc?.snippet,
      );
    }
  }
}
