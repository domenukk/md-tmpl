/**
 * Direct renderer — renders templates directly from JS values.
 *
 * This module bypasses the `Value` intermediate representation entirely.
 * Instead of `fromJs() → Value → display()`, it works directly with
 * plain JS objects, arrays, strings, etc.
 *
 * This module has been split into the `direct_renderer/` directory for
 * maintainability; this file re-exports the same public surface so that
 * existing `./direct_renderer.js` imports continue to resolve unchanged.
 *
 * @module
 */

export type { DirectRenderOptions } from "./direct_renderer/options.js";
export { DirectScope } from "./direct_renderer/scope.js";
export {
  precompileDirectNodes,
  renderDirect,
  renderDirectWithScope,
} from "./direct_renderer/render.js";
