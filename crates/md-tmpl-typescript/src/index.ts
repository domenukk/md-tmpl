/**
 * md-tmpl — Strongly-typed template engine for LLM prompts.
 *
 * TypeScript bindings for the `md-tmpl` engine. Templates are
 * `.tmpl.md` files with YAML frontmatter declaring typed parameters.
 *
 * @example Quick start
 * ```ts
 * import { Template } from "md-tmpl";
 *
 * const tmpl = Template.fromSource(`
 * ---
 * params:
 *   - name = str
 * ---
 * Hello {{ name }}!
 * `);
 * console.log(tmpl.render({ name: "world" }));
 * // → "Hello world!"
 * ```
 *
 * @example Enum variants
 * ```ts
 * import { Template, defineVariants } from "md-tmpl";
 *
 * const Status = defineVariants({
 *   Approved: null,
 *   Rejected: null,
 *   NeedsChanges: ["reason"],
 * });
 *
 * const tmpl = Template.fromSource(`
 * ---
 * params:
 *   - outcome = enum(Approved, Rejected, NeedsChanges(reason = str))
 * ---
 * > {%- match outcome %}
 * > {% case Approved %}
 * Approved!
 * > {% case Rejected %}
 * Rejected.
 * > {% case NeedsChanges %}
 * Needs changes: {{ outcome.reason }}
 * > {% /match %}
 * `);
 *
 * console.log(tmpl.render({ outcome: Status.Approved }));
 * ```
 *
 * @packageDocumentation
 */

// Core classes and interfaces
export {
  type ITemplate,
  type CachedInclude,
  type CompileOptions,
  Template,
  TypedTemplate,
  TemplateCache,
} from "./template.js";
export { Context } from "./context.js";

// Error hierarchy
export {
  type ErrorKind,
  TemplateError,
  TemplateSyntaxError,
  MissingParamsError,
  TypeMismatchError,
  ExtraParamsError,
  UndefinedVariableError,
  UnknownFilterError,
  TemplatePanicError,
  IncludeNotFoundError,
  DeclarationsMutatedError,
} from "./errors.js";

// Value types
export {
  type Value,
  type StrValue,
  type BoolValue,
  type IntValue,
  type FloatValue,
  type ListValue,
  type StructValue,
  type DictValue,
  type NoneValue,
  type TmplValue,
  type TmplRef,
  V,
  str,
  bool,
  int,
  float,
  list,
  structVal,
  dict,
  tmplVal,
  NONE,
  typeName,
  isTruthy,
  display,
  getField,
  fromJs,
  ENUM_TAG_KEY,
} from "./value.js";

// Frontmatter types
export {
  type Frontmatter,
  type VarDecl,
  type VarType,
  type VariantDecl,
  type ImportDecl,
  parseFrontmatter,
  stripFrontmatter,
  parseVarType,
  varTypeToString,
} from "./frontmatter.js";

// Variant helpers
export {
  type VariantInstance,
  type VariantConstructor,
  type VariantSpec,
  unitVariant,
  variant,
  defineVariants,
  match,
  isVariant,
} from "./variants.js";

// Code generation
export {
  type GenerateTypesOptions,
  type InferredTypes,
  type InferredField,
  type InferredTypeAlias,
  type InferredConst,
  generateTypes,
  generateTypesFromFile,
  inferTypes,
} from "./codegen.js";

// Validation
export { validateFrontmatter, toPascalCase } from "./validation.js";
