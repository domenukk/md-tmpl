/**
 * Main Template class — the public API for parsing and rendering templates.
 *
 * This module has been split into the `template/` directory for
 * maintainability; this file re-exports the same public surface so that
 * existing `./template.js` imports continue to resolve unchanged.
 *
 * @module
 */
export type {
  ITemplate,
  CompileOptions,
  CachedInclude,
} from "./template/index.js";
export { Template, TypedTemplate, TemplateCache } from "./template/index.js";
