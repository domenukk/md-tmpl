/**
 * Compile-time AST safety checks: bare enum access and match-arm
 * type safety.
 *
 * @module
 */

import { TemplateSyntaxError } from "../errors.js";
import { type Node } from "../parser.js";
import { EXPR_START } from "../consts.js";

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
              throw new TemplateSyntaxError(
                `bare enum literal '${barePath}' is not allowed` +
                  ` — use kind(${barePath}) to get the variant name as a string`,
                node.loc?.line,
                node.loc?.column,
                node.loc?.snippet,
              );
            }
          }
        }
        break;
      }
      case "for":
        walkNodesForBareEnumAccess(node.body, enumTypeNames);
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
): void {
  for (const node of nodes) {
    switch (node.kind) {
      case "match": {
        const typeKind = paramTypes.get(node.expr);

        // Detect kind() in match expression.
        if (node.expr.startsWith("kind(") && node.expr.endsWith(")")) {
          const inner = node.expr.slice(5, -1);
          throw new TemplateSyntaxError(
            `match on '${node.expr}': matching on kind() converts the enum to a string` +
              ` — use {% match ${inner} %} with unquoted variant names instead` +
              ` for exhaustiveness checking and type safety`,
            node.loc?.line,
            node.loc?.column,
            node.loc?.snippet,
          );
        }

        const allLabels = collectMatchLabels(node);
        const isQuoted = (l: string) =>
          l.length >= 2 &&
          ((l[0] === '"' && l[l.length - 1] === '"') ||
            (l[0] === "'" && l[l.length - 1] === "'"));

        if (typeKind === "enum") {
          // Quoted labels on enum types are an error.
          const quotedLabel = allLabels.find(isQuoted);
          if (quotedLabel) {
            throw new TemplateSyntaxError(
              `match on '${node.expr}': quoted string '${quotedLabel}' cannot match enum variants` +
                ` — remove the quotes to match variant name directly`,
              node.loc?.line,
              node.loc?.column,
              node.loc?.snippet,
            );
          }
        } else if (typeKind) {
          // Validate label types against scalar match type.
          for (const label of allLabels) {
            if (label === "_") continue;
            validateScalarCaseLabel(node.expr, typeKind, label, node.loc);
          }
        }

        for (const arm of node.arms) {
          walkNodesForMatchTypeSafety(arm.body, paramTypes);
        }
        if (node.elseArm) {
          walkNodesForMatchTypeSafety(node.elseArm, paramTypes);
        }
        if (node.inlineGuard) {
          walkNodesForMatchTypeSafety(node.inlineGuard.body, paramTypes);
        }
        break;
      }
      case "for":
        walkNodesForMatchTypeSafety(node.body, paramTypes);
        if (node.elseBody) {
          walkNodesForMatchTypeSafety(node.elseBody, paramTypes);
        }
        break;
      case "if":
        for (const branch of node.branches) {
          walkNodesForMatchTypeSafety(branch.body, paramTypes);
        }
        if (node.elseBody) {
          walkNodesForMatchTypeSafety(node.elseBody, paramTypes);
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
    ((label[0] === '"' && label[label.length - 1] === '"') ||
      (label[0] === "'" && label[label.length - 1] === "'"))
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
  loc?: { line?: number; column?: number; snippet?: string },
): void {
  const kind = classifyLabel(label);

  const err = (msg: string) => {
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
