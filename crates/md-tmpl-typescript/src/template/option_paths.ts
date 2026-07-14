/**
 * Collect option-typed parameter paths from a VarType tree.
 *
 * @module
 */

import { type VarType } from "../frontmatter.js";
import { TYPE_ALIAS, TYPE_ENUM, TYPE_OPTION, TYPE_STRUCT } from "../consts.js";

/**
 * Recursively collect parameter paths that are option-typed.
 *
 * For a param like `person = struct(name = str, email = option(str))`,
 * this adds `"person.email"` to the set.  For a top-level
 * `x = option(str)`, it adds `"x"`.
 */
export function collectOptionPaths(
  prefix: string,
  varType: VarType,
  typeAliases: ReadonlyMap<string, VarType>,
  out: Set<string>,
): void {
  // Resolve type aliases
  if (varType.kind === TYPE_ALIAS) {
    const resolved = typeAliases.get(varType.name);
    if (resolved) {
      collectOptionPaths(prefix, resolved, typeAliases, out);
    }
    return;
  }

  if (varType.kind === TYPE_OPTION) {
    out.add(prefix);
    return;
  }

  // Legacy isOption enum
  if (varType.kind === TYPE_ENUM && varType.isOption) {
    out.add(prefix);
    return;
  }

  // Recurse into struct fields
  if (varType.kind === TYPE_STRUCT) {
    for (const field of varType.fields) {
      collectOptionPaths(
        `${prefix}.${field.name}`,
        field.varType,
        typeAliases,
        out,
      );
    }
  }

  // Note: list item option fields are accessed via loop bindings (e.g.,
  // `item.field`) whose prefix isn't known at declaration time. The
  // match/kind heuristic (isOptionMatchNode) handles those cases.
}
