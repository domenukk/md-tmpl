/**
 * Barrel module for the Template implementation.
 *
 * Re-exports the public API from the internal submodules. The public
 * surface is identical to the former single-file `template.ts`.
 *
 * @module
 */
export type { ITemplate, CompileOptions, CachedInclude } from "./types.js";
export { Template } from "./template_class.js";
export { TypedTemplate } from "./typed_template.js";
export { TemplateCache } from "./cache.js";
export {
  type FsProvider,
  type PathProvider,
  setFileSystemProvider,
  resetFileSystemProvider,
} from "./utils.js";
