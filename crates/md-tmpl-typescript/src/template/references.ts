/**
 * AST-based referenced-parameter collection (for unused-param checks).
 *
 * @module
 */

import type { Node } from "../parser.js";
import {
  BACKSLASH,
  EXPR_START,
  PIPE,
  PATH_SEP,
  PREFIX_CONSTS_DOT,
  PREFIX_OPTS_DOT,
  PREFIX_OPTIONS_DOT,
  PREFIX_PARAMS_DOT,
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
} from "../consts.js";

// ---------------------------------------------------------------------------
// AST-based referenced parameter collection
// ---------------------------------------------------------------------------

/** Built-in function names whose argument is extracted as the reference. */
const BUILTIN_FUNCTIONS = new Set(["idx", "len", "kind", "kinds", "has"]);

/**
 * Extract the root variable name from an expression string.
 * Handles `path.field`, `func(arg)`, `expr | filter`, and literals.
 * Returns undefined for literals, loop bindings, and unknown functions.
 */
function extractRootVariable(
  expr: string,
  loopBindings: ReadonlySet<string>,
): string | undefined {
  const trimmed = expr.trim();
  if (trimmed.length === 0) return undefined;

  // Handle function calls: func(arg)
  const parenIdx = trimmed.indexOf("(");
  if (parenIdx > 0 && trimmed.endsWith(")")) {
    const funcName = trimmed.slice(0, parenIdx).trim();
    if (BUILTIN_FUNCTIONS.has(funcName)) {
      const arg = stripPathPrefix(trimmed.slice(parenIdx + 1, -1).trim());
      // split() always yields at least one element; the fallback satisfies
      // noUncheckedIndexedAccess without a non-null assertion.
      const root = (arg.split(PATH_SEP)[0] ?? arg).trim();
      if (
        !loopBindings.has(root) &&
        !isLiteralToken(root) &&
        isValidIdentifier(root)
      ) {
        return root;
      }
      return undefined;
    }
  }

  // Handle pipe expressions: take the part before the first `|`
  const pipeIdx = trimmed.indexOf(PIPE);
  const base = stripPathPrefix(
    (pipeIdx >= 0 ? trimmed.slice(0, pipeIdx) : trimmed).trim(),
  );
  const root = (base.split(PATH_SEP)[0] ?? base).trim();
  // isValidIdentifier rejects malformed roots (e.g. fragments containing
  // whitespace or operators), mirroring the Rust core exactly.
  if (
    root.length === 0 ||
    isLiteralToken(root) ||
    loopBindings.has(root) ||
    !isValidIdentifier(root)
  ) {
    return undefined;
  }
  return root;
}

/**
 * Strip a leading namespace path prefix (`consts.`, `opts.`, `options.`,
 * `params.`) from a token, mirroring the Rust core's `extract_root_variable`.
 */
function stripPathPrefix(token: string): string {
  for (const prefix of [
    PREFIX_CONSTS_DOT,
    PREFIX_OPTS_DOT,
    PREFIX_OPTIONS_DOT,
    PREFIX_PARAMS_DOT,
  ]) {
    if (token.startsWith(prefix)) {
      return token.slice(prefix.length).trim();
    }
  }
  return token;
}

/**
 * Returns true if `token` is a syntactically valid identifier
 * (`[A-Za-z_][A-Za-z0-9_]*`). Mirrors the Rust core so both engines agree on
 * which tokens can name a variable, rejecting malformed roots that would
 * otherwise produce spurious "undeclared variable" errors.
 */
function isValidIdentifier(token: string): boolean {
  return /^[a-zA-Z_][a-zA-Z0-9_]*$/.test(token);
}

/** Returns true if the token looks like a literal (string, number, bool). */
function isLiteralToken(token: string): boolean {
  if (token === "true" || token === "false") return true;
  if (
    (token.startsWith('"') && token.endsWith('"')) ||
    (token.startsWith("'") && token.endsWith("'"))
  ) {
    return true;
  }
  if (/^-?\d+(\.\d+)?$/.test(token)) return true;
  return false;
}

/**
 * Extract variable references from a condition string (used in {% if %} tags).
 * Handles &&, ||, !, comparisons, 'in' operator, and match-as-condition.
 */
function extractConditionVariables(
  condition: string,
  refs: Set<string>,
  loopBindings: ReadonlySet<string>,
): void {
  const trimmed = condition.trim();

  // Remove balanced outer parens
  if (trimmed.startsWith("(") && trimmed.endsWith(")")) {
    let depth = 0;
    let balanced = true;
    for (let i = 0; i < trimmed.length - 1; i++) {
      if (trimmed[i] === "(") depth++;
      else if (trimmed[i] === ")") depth--;
      if (depth === 0) {
        balanced = false;
        break;
      }
    }
    if (balanced) {
      extractConditionVariables(trimmed.slice(1, -1), refs, loopBindings);
      return;
    }
  }

  // Remove leading ! or `not ` (logical negation)
  if (trimmed.startsWith("!")) {
    extractConditionVariables(trimmed.slice(1), refs, loopBindings);
    return;
  }
  if (trimmed.startsWith("not ")) {
    extractConditionVariables(trimmed.slice(4), refs, loopBindings);
    return;
  }

  // Try to split on top-level && or ||
  for (const delim of [" && ", " || "]) {
    const parts = splitTopLevel(trimmed, delim);
    if (parts.length > 1) {
      for (const part of parts) {
        extractConditionVariables(part, refs, loopBindings);
      }
      return;
    }
  }

  // Handle match-as-condition: "match expr case Variant"
  if (trimmed.startsWith("match ")) {
    const caseIdx = trimmed.indexOf(" case ");
    if (caseIdx > 0) {
      const matchExpr = trimmed.slice(6, caseIdx).trim();
      const root = extractRootVariable(matchExpr, loopBindings);
      if (root) refs.add(root);
      return;
    }
  }

  // Handle comparisons: ==, !=, <=, >=, <, >, ' in ', ' not in '
  for (const op of ["==", "!=", "<=", ">=", "<", ">", " in ", " not in "]) {
    const idx = findTopLevelOp(trimmed, op);
    if (idx >= 0) {
      const left = trimmed.slice(0, idx).trim();
      const right = trimmed.slice(idx + op.length).trim();
      extractOperandRefs(left, refs, loopBindings);
      extractOperandRefs(right, refs, loopBindings);
      return;
    }
  }

  // Plain truthiness: extract the root variable of the operand only.
  // Mirrors the Rust core, which extracts path roots and never scans raw
  // identifiers — so struct field names (e.g. `item.active`) are never
  // mistaken for standalone variable references.
  extractOperandRefs(trimmed, refs, loopBindings);
}

/**
 * Extract variable refs from a condition operand.
 * Handles plain variables, function calls, and string interpolation.
 */
function extractOperandRefs(
  operand: string,
  refs: Set<string>,
  loopBindings: ReadonlySet<string>,
): void {
  // If it's a string literal with interpolation, extract {{ expr }} refs
  if (
    (operand.startsWith('"') && operand.endsWith('"')) ||
    (operand.startsWith("'") && operand.endsWith("'"))
  ) {
    const inner = operand.slice(1, -1);
    extractInterpolationRefs(inner, refs, loopBindings);
    return;
  }
  // Otherwise extract the root variable. If none is found, there is no
  // reference to record (mirrors the Rust core — no raw identifier scan).
  const root = extractRootVariable(operand, loopBindings);
  if (root) {
    refs.add(root);
  }
}

/** Extract variable references from {{ expr }} interpolations inside a string. */
export function extractInterpolationRefs(
  s: string,
  refs: Set<string>,
  loopBindings: ReadonlySet<string>,
): void {
  let remaining = s;
  while (remaining.includes(EXPR_START)) {
    const startIdx = remaining.indexOf(EXPR_START);
    remaining = remaining.slice(startIdx + EXPR_START.length);
    const endIdx = remaining.indexOf("}}");
    if (endIdx >= 0) {
      const expr = remaining.slice(0, endIdx).trim();
      const root = extractRootVariable(expr, loopBindings);
      if (root) refs.add(root);
      remaining = remaining.slice(endIdx + 2);
    } else {
      break;
    }
  }
}

/** Split a string at top-level occurrences of a delimiter (not inside parens
 * or string literals). */
function splitTopLevel(s: string, delim: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (ch === "(") depth++;
    else if (ch === ")") depth--;
    else if (ch === "'" || ch === '"') {
      // Skip string literals; a backslash escapes the next char so an escaped
      // quote does not close the literal (mirrors the Rust analysis parser).
      const quote = ch;
      i++;
      while (i < s.length && s[i] !== quote) {
        if (s[i] === BACKSLASH && i + 1 < s.length) i += 2;
        else i++;
      }
    } else if (
      depth === 0 &&
      i + delim.length <= s.length &&
      s.slice(i, i + delim.length) === delim
    ) {
      parts.push(s.slice(start, i).trim());
      i += delim.length - 1;
      start = i + 1;
    }
  }
  const last = s.slice(start).trim();
  if (last.length > 0) parts.push(last);
  return parts;
}

/** Find the first top-level occurrence of an operator string. */
function findTopLevelOp(s: string, op: string): number {
  let depth = 0;
  for (let i = 0; i < s.length; i++) {
    // Capture once so control-flow narrowing proves `quote` is defined,
    // avoiding a non-null assertion under noUncheckedIndexedAccess.
    const ch = s[i];
    if (ch === "(") depth++;
    else if (ch === ")") depth--;
    else if (ch === "'" || ch === '"') {
      const quote = ch;
      i++;
      // A backslash escapes the next char so an escaped quote does not close
      // the literal (mirrors the Rust analysis parser).
      while (i < s.length && s[i] !== quote) {
        if (s[i] === BACKSLASH && i + 1 < s.length) i += 2;
        else i++;
      }
    } else if (
      depth === 0 &&
      i + op.length <= s.length &&
      s.slice(i, i + op.length) === op
    ) {
      return i;
    }
  }
  return -1;
}

/**
 * Collect all root parameter names referenced in parsed AST nodes.
 *
 * Mirrors Rust's `collect_referenced_params`:
 * - Walks expressions, conditions, match targets, includes
 * - Excludes text and raw nodes (plain text does NOT count as a reference)
 * - Tracks for-loop bindings to exclude shadowed names
 */
export function collectReferencedParams(nodes: readonly Node[]): Set<string> {
  const refs = new Set<string>();
  const loopBindings = new Set<string>();
  collectRefsInner(nodes, refs, loopBindings);
  return refs;
}

function collectRefsInner(
  nodes: readonly Node[],
  refs: Set<string>,
  loopBindings: Set<string>,
): void {
  for (const node of nodes) {
    switch (node.kind) {
      case NODE_TEXT:
      case NODE_RAW:
      case NODE_COMMENT:
        // Plain text, raw blocks, and comments do NOT count as variable refs.
        break;

      case NODE_EXPR: {
        const root = extractRootVariable(node.expr, loopBindings);
        if (root) refs.add(root);
        break;
      }

      case NODE_FOR:
        // Collect refs from the list expression
        {
          const iterRoot = extractRootVariable(node.iterExpr, loopBindings);
          if (iterRoot) refs.add(iterRoot);
        }
        // The binding is local — track it to exclude from refs
        loopBindings.add(node.binding);
        collectRefsInner(node.body, refs, loopBindings);
        loopBindings.delete(node.binding);
        // else_body runs when the list is empty — binding NOT in scope
        if (node.elseBody) {
          collectRefsInner(node.elseBody, refs, loopBindings);
        }
        break;

      case NODE_IF:
        for (const branch of node.branches) {
          extractConditionVariables(branch.condition, refs, loopBindings);
          collectRefsInner(branch.body, refs, loopBindings);
        }
        if (node.elseBody) {
          collectRefsInner(node.elseBody, refs, loopBindings);
        }
        break;

      case NODE_MATCH: {
        const matchRoot = extractRootVariable(node.expr, loopBindings);
        if (matchRoot) refs.add(matchRoot);
        for (const arm of node.arms) {
          // Scan quoted case labels for {{ expr }} interpolation refs.
          // Unquoted labels are either enum variant names or param-ref
          // matches — the collector cannot distinguish them without type
          // context, so param-ref labels are handled separately by the
          // unused-param checker (which has the declared param set).
          for (const variant of arm.variants) {
            if (
              (variant.startsWith('"') && variant.endsWith('"')) ||
              (variant.startsWith("'") && variant.endsWith("'"))
            ) {
              extractInterpolationRefs(
                variant.slice(1, -1),
                refs,
                loopBindings,
              );
            }
          }
          collectRefsInner(arm.body, refs, loopBindings);
        }
        if (node.elseArm) {
          collectRefsInner(node.elseArm, refs, loopBindings);
        }
        if (node.inlineGuard) {
          collectRefsInner(node.inlineGuard.body, refs, loopBindings);
        }
        break;
      }

      case NODE_INCLUDE:
        // Include with-mappings reference variables. A quoted value is a
        // string literal that may embed {{ expr }} interpolation (mirrors the
        // renderer); collect those refs the same way. An unquoted value is a
        // plain expression (path/function/filter).
        for (const [, valExpr] of node.withMappings) {
          if (
            (valExpr.startsWith('"') && valExpr.endsWith('"')) ||
            (valExpr.startsWith("'") && valExpr.endsWith("'"))
          ) {
            extractInterpolationRefs(valExpr.slice(1, -1), refs, loopBindings);
          } else {
            const root = extractRootVariable(valExpr, loopBindings);
            if (root) refs.add(root);
          }
        }
        // Include for-binding iteration expression
        if (node.forExpr) {
          const root = extractRootVariable(node.forExpr, loopBindings);
          if (root) refs.add(root);
        }
        // Dynamic include paths with {{ expr }}
        if (node.path?.includes(EXPR_START)) {
          let remaining = node.path;
          while (remaining.includes(EXPR_START)) {
            const startIdx = remaining.indexOf(EXPR_START);
            remaining = remaining.slice(startIdx + EXPR_START.length);
            const endIdx = remaining.indexOf("}}");
            if (endIdx >= 0) {
              const innerExpr = remaining.slice(0, endIdx);
              const root = extractRootVariable(innerExpr, loopBindings);
              if (root) refs.add(root);
              remaining = remaining.slice(endIdx + 2);
            } else {
              break;
            }
          }
        }
        // Static include name that isn't a file path → might be a variable
        if (
          node.name &&
          !node.path &&
          !node.name.endsWith(".tmpl.md") &&
          !node.name.endsWith(".md")
        ) {
          const root = extractRootVariable(node.name, loopBindings);
          if (root) refs.add(root);
        }
        break;

      case NODE_TMPL:
        // Inline template definitions don't reference parent params
        break;

      case NODE_PANIC:
        collectRefsInner(node.body, refs, loopBindings);
        break;
    }
  }
}

/** Collect all for-loop binding names from parsed nodes (recursive). */
export function collectForBindings(nodes: readonly Node[]): Set<string> {
  const bindings = new Set<string>();
  for (const node of nodes) {
    if (node.kind === "for") {
      bindings.add(node.binding);
      // Recurse into body
      for (const b of collectForBindings(node.body)) {
        bindings.add(b);
      }
    } else if (node.kind === "if") {
      for (const branch of node.branches) {
        for (const b of collectForBindings(branch.body)) {
          bindings.add(b);
        }
      }
      if (node.elseBody) {
        for (const b of collectForBindings(node.elseBody)) {
          bindings.add(b);
        }
      }
    } else if (node.kind === "match") {
      for (const arm of node.arms) {
        for (const b of collectForBindings(arm.body)) {
          bindings.add(b);
        }
      }
      if (node.elseArm) {
        for (const b of collectForBindings(node.elseArm)) {
          bindings.add(b);
        }
      }
      if (node.inlineGuard) {
        for (const b of collectForBindings(node.inlineGuard.body)) {
          bindings.add(b);
        }
      }
    } else if (node.kind === "include" && node.forBinding) {
      bindings.add(node.forBinding);
    }
  }
  return bindings;
}

/**
 * Collect all unquoted case labels from match nodes in the AST.
 *
 * Used by the unused-param checker: if a declared param name appears as an
 * unquoted case label, the runtime reads that param's value for comparison,
 * so it counts as "used". The collector cannot do this directly because it
 * doesn't have access to the declared param set (and would confuse enum
 * variant names like "Active" with param names).
 */
export function collectUnquotedCaseLabels(nodes: readonly Node[]): Set<string> {
  const labels = new Set<string>();
  collectLabelsInner(nodes, labels);
  return labels;
}

function collectLabelsInner(nodes: readonly Node[], labels: Set<string>): void {
  for (const node of nodes) {
    if (node.kind === NODE_MATCH) {
      for (const arm of node.arms) {
        for (const variant of arm.variants) {
          // Only collect unquoted labels (not quoted strings)
          if (!(
            (variant.startsWith('"') && variant.endsWith('"')) ||
            (variant.startsWith("'") && variant.endsWith("'"))
          )) {
            labels.add(variant);
          }
        }
        collectLabelsInner(arm.body, labels);
      }
      if (node.elseArm) {
        collectLabelsInner(node.elseArm, labels);
      }
      if (node.inlineGuard) {
        collectLabelsInner(node.inlineGuard.body, labels);
      }
    } else if ("body" in node && Array.isArray(node.body)) {
      collectLabelsInner(node.body, labels);
    }
    if ("branches" in node && Array.isArray(node.branches)) {
      for (const branch of node.branches as { body: Node[] }[]) {
        collectLabelsInner(branch.body, labels);
      }
    }
    if ("elseBody" in node && Array.isArray(node.elseBody)) {
      collectLabelsInner(node.elseBody, labels);
    }
  }
}
