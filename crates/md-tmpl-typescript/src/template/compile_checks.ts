/**
 * Compile-time AST safety checks: bare enum access and match-arm
 * type safety.
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import { type Frontmatter } from "../frontmatter.js";
import { nodeLoc } from "../ast.js";
import { type Node } from "../parser.js";
import { ERR_UNDECLARED_PREFIX, EXPR_START, TYPE_ENUM } from "../consts.js";
import { validateConditionSyntax } from "../evaluator.js";
import { closestName } from "../suggest.js";
import {
  collectReferencedParams,
  collectUnquotedCaseLabels,
  extractInterpolationRefs,
} from "./references.js";

/**
 * Walk AST nodes and validate the SYNTAX of every `{% if %}`/`{% elif %}`
 * condition and every `{% case ... && guard %}` guard at construction time.
 *
 * This mirrors the Rust core, which parses conditions during compilation
 * (before semantic analysis such as unused/undeclared checks). Validating
 * here ensures malformed conditions surface as syntax errors first, matching
 * the Rust backend byte-for-byte.
 *
 * @throws {TemplateSyntaxError} On the first syntactically-invalid condition.
 */
export function walkNodesForConditionSyntax(nodes: readonly Node[]): void {
  for (const node of nodes) {
    switch (node.kind) {
      case "if":
        for (const branch of node.branches) {
          validateConditionSyntax(branch.condition);
          walkNodesForConditionSyntax(branch.body);
        }
        if (node.elseBody) {
          walkNodesForConditionSyntax(node.elseBody);
        }
        break;
      case "match":
        for (const arm of node.arms) {
          if (arm.guard !== undefined) {
            validateConditionSyntax(arm.guard);
          }
          walkNodesForConditionSyntax(arm.body);
        }
        if (node.elseArm) {
          walkNodesForConditionSyntax(node.elseArm);
        }
        if (node.inlineGuard) {
          walkNodesForConditionSyntax(node.inlineGuard.body);
        }
        break;
      case "for":
        walkNodesForConditionSyntax(node.body);
        if (node.elseBody) {
          walkNodesForConditionSyntax(node.elseBody);
        }
        break;
    }
  }
}

/**
 * Walk AST nodes and reject bare enum literal expressions.
 *
 * A "bare enum literal" is an expression output like `{{ Stage.Design }}`
 * where `Stage` is an enum type name and the expression is a plain dotted
 * path (not wrapped in `kind()` or another function call).
 *
 * @throws {TemplateSyntaxError} On the first bare enum literal found.
 */
export function walkNodesForBareEnumAccess(
  nodes: readonly Node[],
  enumTypeNames: ReadonlySet<string>,
): void {
  for (const node of nodes) {
    switch (node.kind) {
      case "expr": {
        const barePath = extractBareDottedPath(node.expr);
        if (barePath !== undefined) {
          const dotIdx = barePath.indexOf(".");
          if (dotIdx > 0) {
            const root = barePath.slice(0, dotIdx);
            if (enumTypeNames.has(root)) {
              const loc = nodeLoc(node);
              throw new TemplateSyntaxError(
                `bare enum literal '${barePath}' is not allowed` +
                  ` — use kind(${barePath}) to get the variant name as a string`,
                loc?.line,
                loc?.column,
                loc?.snippet,
              );
            }
          }
        }
        break;
      }
      case "for":
        walkNodesForBareEnumAccess(node.body, enumTypeNames);
        if (node.elseBody) {
          walkNodesForBareEnumAccess(node.elseBody, enumTypeNames);
        }
        break;
      case "if":
        for (const branch of node.branches) {
          walkNodesForBareEnumAccess(branch.body, enumTypeNames);
        }
        if (node.elseBody) {
          walkNodesForBareEnumAccess(node.elseBody, enumTypeNames);
        }
        break;
      case "match":
        for (const arm of node.arms) {
          walkNodesForBareEnumAccess(arm.body, enumTypeNames);
        }
        if (node.elseArm) {
          walkNodesForBareEnumAccess(node.elseArm, enumTypeNames);
        }
        if (node.inlineGuard) {
          walkNodesForBareEnumAccess(node.inlineGuard.body, enumTypeNames);
        }
        break;
    }
  }
}

/**
 * Walk AST nodes and reject type-unsafe match/case combinations:
 * - `{% match str_param %}{% case Foo %}` where `Foo` is NOT a declared param → type error
 * - `{% match enum_param %}{% case "Active" %}` (quoted on enum) → type error
 *
 * Unquoted case labels on str params ARE allowed when the label is a declared
 * param name — this enables dynamic param-reference matching:
 *   `{% match status %}{% case expected_status %}` matches when status == expected_status
 */
export function walkNodesForMatchTypeSafety(
  nodes: readonly Node[],
  paramTypes: ReadonlyMap<string, string>,
  narrowedOptions: ReadonlySet<string> = new Set(),
): void {
  for (const node of nodes) {
    switch (node.kind) {
      case "match": {
        const typeKind = paramTypes.get(node.expr);

        // Detect kind() in match expression.
        if (node.expr.startsWith("kind(") && node.expr.endsWith(")")) {
          const inner = node.expr.slice(5, -1);
          const loc = nodeLoc(node);
          throw new TemplateSyntaxError(
            `match on '${node.expr}': matching on kind() converts the enum to a string` +
              ` — use {% match ${inner} %} with unquoted variant names instead` +
              ` for exhaustiveness checking and type safety`,
            loc?.line,
            loc?.column,
            loc?.snippet,
          );
        }

        const allLabels = collectMatchLabels(node);
        const isQuoted = (l: string) =>
          l.length >= 2 &&
          ((l.startsWith('"') && l.endsWith('"')) ||
            (l.startsWith("'") && l.endsWith("'")));

        if (typeKind === "enum") {
          // Quoted labels on enum types are an error.
          const quotedLabel = allLabels.find(isQuoted);
          if (quotedLabel) {
            const loc = nodeLoc(node);
            throw new TemplateSyntaxError(
              `match on '${node.expr}': quoted string '${quotedLabel}' cannot match enum variants` +
                ` — remove the quotes to match variant name directly`,
              loc?.line,
              loc?.column,
              loc?.snippet,
            );
          }
        } else if (typeKind === "option" && !narrowedOptions.has(node.expr)) {
          if (!node.inlineGuard && node.arms.length > 1 && !node.elseArm) {
            const hasSome = node.arms.some(
              (a) => !a.guard && a.variants.includes("Some"),
            );
            const hasNone = node.arms.some(
              (a) => !a.guard && a.variants.includes("None"),
            );
            if (!hasSome || !hasNone) {
              const missing: string[] = [];
              if (!hasSome) missing.push("Some");
              if (!hasNone) missing.push("None");
              const loc = nodeLoc(node);
              throw new TemplateSyntaxError(
                `match on '${node.expr}': non-exhaustive — missing variant(s): ${missing.join(", ")}`,
                loc?.line,
                loc?.column,
                loc?.snippet,
              );
            }
          }
        } else if (typeKind && !narrowedOptions.has(node.expr)) {
          // Validate label types against scalar match type.
          for (const label of allLabels) {
            if (label === "_") continue;
            validateScalarCaseLabel(node.expr, typeKind, label, node);
          }
        }

        for (const arm of node.arms) {
          const nextNarrowed =
            arm.variants.length === 1 && arm.variants[0] === "Some"
              ? new Set([...narrowedOptions, node.expr])
              : narrowedOptions;
          walkNodesForMatchTypeSafety(arm.body, paramTypes, nextNarrowed);
        }
        if (node.elseArm) {
          walkNodesForMatchTypeSafety(
            node.elseArm,
            paramTypes,
            narrowedOptions,
          );
        }
        if (node.inlineGuard) {
          const nextNarrowed =
            node.inlineGuard.variant === "Some"
              ? new Set([...narrowedOptions, node.expr])
              : narrowedOptions;
          walkNodesForMatchTypeSafety(
            node.inlineGuard.body,
            paramTypes,
            nextNarrowed,
          );
        }
        break;
      }
      case "for":
        walkNodesForMatchTypeSafety(node.body, paramTypes, narrowedOptions);
        if (node.elseBody) {
          walkNodesForMatchTypeSafety(
            node.elseBody,
            paramTypes,
            narrowedOptions,
          );
        }
        break;
      case "if":
        for (const branch of node.branches) {
          const trimmed = branch.condition.trim();
          let nextNarrowed = narrowedOptions;
          if (trimmed.startsWith("has(") && trimmed.endsWith(")")) {
            const inner = trimmed.slice(4, -1).trim();
            if (inner.length > 0 && !inner.includes(" ")) {
              nextNarrowed = new Set([...narrowedOptions, inner]);
            }
          }
          walkNodesForMatchTypeSafety(branch.body, paramTypes, nextNarrowed);
        }
        if (node.elseBody) {
          walkNodesForMatchTypeSafety(
            node.elseBody,
            paramTypes,
            narrowedOptions,
          );
        }
        break;
    }
  }
}

/** Collect all case labels from a match node (both arms and inline guard). */
const HINT_BOOL = "use {% case true %} or {% case false %}";

/**
 * Classify a case label as quoted, bool, int, float, or identifier.
 */
type LabelKind = "quoted" | "interpolated" | "bool" | "int" | "float" | "ident";

function classifyLabel(label: string): LabelKind {
  if (
    label.length >= 2 &&
    ((label.startsWith('"') && label.endsWith('"')) ||
      (label.startsWith("'") && label.endsWith("'")))
  ) {
    const inner = label.slice(1, -1);
    if (inner.includes(EXPR_START)) return "interpolated";
    return "quoted";
  }
  if (label === "true" || label === "false") return "bool";
  if (/^-?\d+$/.test(label)) return "int";
  if (/^-?\d+\.\d+$/.test(label)) return "float";
  return "ident";
}

/**
 * Validate a case label against the match expression's scalar type.
 */
function validateScalarCaseLabel(
  expr: string,
  typeName: string,
  label: string,
  node: Parameters<typeof nodeLoc>[0],
): void {
  const kind = classifyLabel(label);

  const err = (msg: string) => {
    const loc = nodeLoc(node);
    throw new TemplateSyntaxError(msg, loc?.line, loc?.column, loc?.snippet);
  };

  switch (typeName) {
    case "str":
      if (kind === "int" || kind === "float") {
        err(
          `match on '${expr}': case label '${label}' is a numeric literal, but '${expr}' is a str — use {% case "${label}" %} for a string literal`,
        );
      }
      if (kind === "bool") {
        err(
          `match on '${expr}': case label '${label}' is a bool literal, but '${expr}' is a str — use {% case "${label}" %} for a string literal`,
        );
      }
      break;
    case "int":
      if (kind === "quoted" || kind === "interpolated") {
        const inner = label.slice(1, -1);
        err(
          `match on '${expr}': quoted string '${label}' cannot match int values — use {% case ${inner} %} for an integer literal`,
        );
      }
      if (kind === "bool") {
        err(
          `match on '${expr}': case label '${label}' is a bool literal, but '${expr}' is an int`,
        );
      }
      if (kind === "float") {
        err(
          `match on '${expr}': case label '${label}' is a float literal, but '${expr}' is an int`,
        );
      }
      break;
    case "float":
      if (kind === "quoted" || kind === "interpolated") {
        const inner = label.slice(1, -1);
        err(
          `match on '${expr}': quoted string '${label}' cannot match float values — use {% case ${inner} %} for a numeric literal`,
        );
      }
      if (kind === "bool") {
        err(
          `match on '${expr}': case label '${label}' is a bool literal, but '${expr}' is a float`,
        );
      }
      break;
    case "bool":
      if (kind === "quoted" || kind === "interpolated") {
        err(
          `match on '${expr}': quoted string '${label}' cannot match bool values — ${HINT_BOOL}`,
        );
      }
      if (kind === "int" || kind === "float") {
        err(
          `match on '${expr}': case label '${label}' is a numeric literal, but '${expr}' is a bool — ${HINT_BOOL}`,
        );
      }
      break;
  }
}

function collectMatchLabels(node: Extract<Node, { kind: "match" }>): string[] {
  const labels: string[] = [];
  if (node.inlineGuard) {
    labels.push(node.inlineGuard.variant);
  }
  for (const arm of node.arms) {
    labels.push(...arm.variants);
  }
  return labels;
}

/**
 * Extract the bare dotted path from an expression string, or `undefined`
 * if the expression is a function call.
 *
 * The "bare path" is the portion before any `|` filter pipe, trimmed.
 * Returns `undefined` if the expression contains a `(` before the first
 * `.`, indicating a function call (e.g., `kind(Stage.Design)`).
 */
function extractBareDottedPath(expr: string): string | undefined {
  const trimmed = expr.trim();
  const dotIdx = trimmed.indexOf(".");
  if (dotIdx <= 0) return undefined; // No dot or starts with dot

  const parenIdx = trimmed.indexOf("(");
  if (parenIdx >= 0 && parenIdx < dotIdx) return undefined; // Function call

  // Extract the path part before any pipe filter separator.
  let end = trimmed.length;
  let depth = 0;
  for (let i = 0; i < trimmed.length; i++) {
    const ch = trimmed.charCodeAt(i);
    if (ch === 40 /* ( */) depth++;
    else if (ch === 41 /* ) */) depth--;
    else if (ch === 124 /* | */ && depth === 0) {
      end = i;
      break;
    }
  }

  return trimmed.slice(0, end).trim();
}

/** Collect all referenced root variable names from `nodes` and `{# ... #}` comments. */
export function collectAllReferencedVariables(
  nodes: readonly Node[],
  body: string,
): Set<string> {
  const referenced = collectReferencedParams(nodes);
  if (body.includes("{#")) {
    const commentPattern = /\{#(.*?)#\}/gs;
    let match: RegExpExecArray | null;
    const emptySet = new Set<string>();
    while ((match = commentPattern.exec(body)) !== null) {
      extractInterpolationRefs(match[1] ?? "", referenced, emptySet);
    }
  }
  return referenced;
}

/** Names valid in every scope: import stems, enum types, inline templates. */
export function buildTemplateWideNames(
  fm: Frontmatter,
  inlineTemplateNames: Iterable<string>,
): Set<string> {
  const names = new Set<string>(fm.imports.map((imp) => imp.stem));
  for (const [name, ty] of fm.typeAliases) {
    if (ty.kind === TYPE_ENUM) names.add(name);
  }
  for (const name of inlineTemplateNames) {
    names.add(name);
  }
  return names;
}

/** Reject declared parameters that are never referenced in the body or match labels. */
export function checkUnusedParamsWithRefs(
  fm: Frontmatter,
  nodes: readonly Node[],
  referenced: ReadonlySet<string>,
): void {
  let caseLabels: Set<string> | undefined;
  for (const decl of fm.params) {
    if (!referenced.has(decl.name)) {
      caseLabels ??= collectUnquotedCaseLabels(nodes);
      if (!caseLabels.has(decl.name)) {
        throw new TemplateSyntaxError(
          `unused parameter '${decl.name}' declared but not referenced in body`,
          decl.loc?.line,
          decl.loc?.column,
          decl.loc?.snippet,
        );
      }
    }
  }
}

/** Reject references to variables that are not declared anywhere. */
export function checkUndeclaredVariablesWithRefs(
  fm: Frontmatter,
  referenced: ReadonlySet<string>,
  wideNames: ReadonlySet<string>,
): void {
  const declared = new Set<string>(wideNames);
  for (const decl of fm.params) declared.add(decl.name);
  for (const decl of fm.consts) declared.add(decl.name);
  for (const decl of fm.env) declared.add(decl.name);

  const undeclared: string[] = [];
  for (const name of referenced) {
    if (!declared.has(name)) undeclared.push(name);
  }
  if (undeclared.length === 0) return;
  undeclared.sort();

  const suggestions: string[] = [];
  for (const name of undeclared) {
    const best = closestName(name, declared);
    if (best !== undefined) {
      suggestions.push(`'${name}' (did you mean '${best}'?)`);
    }
  }
  const suffix =
    suggestions.length === 0 ? "" : `. Suggestions: ${suggestions.join(", ")}`;
  throw new TemplateSyntaxError(
    `${ERR_UNDECLARED_PREFIX}${undeclared.join(", ")}${suffix}`,
  );
}

/** Reject bare enum literal expressions like `{{ Stage.Design }}`. */
export function checkBareEnumAccess(
  fm: Frontmatter,
  nodes: readonly Node[],
): void {
  const enumTypeNames = new Set<string>();
  for (const [name, varType] of fm.typeAliases) {
    if (varType.kind === "enum" && !varType.isOption) {
      enumTypeNames.add(name);
    }
  }
  if (enumTypeNames.size === 0) return;
  walkNodesForBareEnumAccess(nodes, enumTypeNames);
}

/** Reject type-unsafe match/case combinations at compile time. */
export function checkMatchTypeSafety(
  fm: Frontmatter,
  nodes: readonly Node[],
): void {
  const paramTypes = new Map<string, string>();
  for (const decl of fm.params) {
    let vt = decl.varType;
    while (vt.kind === "alias") {
      const resolved = fm.typeAliases.get(vt.name);
      if (!resolved) break;
      vt = resolved;
    }
    paramTypes.set(decl.name, vt.kind);
  }
  walkNodesForMatchTypeSafety(nodes, paramTypes);
}
