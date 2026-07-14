/**
 * YAML frontmatter parser.
 *
 * Parses the `---` delimited frontmatter block from `.tmpl.md` files.
 * Extracts params, types, consts, and imports declarations.
 *
 * This is a lightweight parser — not a full YAML parser. It handles
 * the subset of YAML used by md-tmpl frontmatter.
 *
 * This module has been split into the `frontmatter/` directory for
 * maintainability; this file re-exports the same public surface so that
 * existing `./frontmatter.js` imports continue to resolve unchanged.
 *
 * @module
 */

export type {
  VarDecl,
  VarType,
  VariantDecl,
  Frontmatter,
  ImportDecl,
} from "./frontmatter/types.js";
export { varTypeToString } from "./frontmatter/types.js";
export { parseFrontmatter, stripFrontmatter } from "./frontmatter/yaml.js";
export {
  interpolatePathStr,
  interpolateImports,
  stripStringLiteral,
  isValidPathPrefix,
} from "./frontmatter/paths.js";
export { parseVarType } from "./frontmatter/var_type.js";
export { parseLiteral } from "./frontmatter/literals.js";
export {
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
} from "./consts.js";
