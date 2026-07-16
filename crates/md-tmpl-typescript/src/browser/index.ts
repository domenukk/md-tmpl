/**
 * `md-tmpl/browser` — run md-tmpl in browsers and other async-only runtimes.
 *
 * Browsers have no synchronous filesystem, but the engine renders
 * synchronously. This entry point bridges the gap with an in-memory
 * {@link MemoryFs} and a lazy fetch-on-miss loader: call {@link loadTemplate}
 * to get a {@link BrowserTemplate}, then {@link BrowserTemplate.renderAsync} to
 * render, fetching only the files each render actually reaches.
 *
 * For advanced/custom setups, {@link MemoryFs} and {@link posixPath} can be
 * combined with the core `setFileSystemProvider` directly.
 *
 * @packageDocumentation
 */

export {
  type FetchLike,
  type LoadTemplateOptions,
  BrowserTemplate,
  clearCache,
  loadTemplate,
} from "./load.js";
export { MemoryFs } from "./memory_fs.js";
export { posixPath } from "./posix_path.js";
