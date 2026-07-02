/**
 * User-facing helpers for defining custom enum variant types.
 *
 * Provides TypeScript equivalents of the Python `@variant` decorator
 * and `Variants` base class:
 *
 * 1. `variant()` — create a struct variant class with the
 *    `_md_tmpl_tag` protocol.
 *
 * 2. `unitVariant()` — create a unit variant sentinel.
 *
 * 3. `defineVariants()` — declare mixed enums (unit + struct variants)
 *    in a single call.
 *
 * @example
 * ```ts
 * import { unitVariant, variant, defineVariants } from "md-tmpl";
 *
 * // --- variant() ---
 * const NeedsChanges = variant("NeedsChanges", ["reason"]);
 * const v = NeedsChanges({ reason: "fix tests" });
 * console.log(v._md_tmpl_tag); // "NeedsChanges"
 * console.log(v.reason); // "fix tests"
 *
 * // --- defineVariants() ---
 * const Status = defineVariants({
 *   Approved: null,
 *   Rejected: null,
 *   NeedsChanges: ["reason"],
 * });
 * Status.Approved;                           // unit sentinel
 * Status.NeedsChanges({ reason: "fix" });    // struct constructor
 * ```
 *
 * @module
 */

import { ENUM_TAG_KEY } from "./consts.js";

// ---------------------------------------------------------------------------
// Variant protocol
// ---------------------------------------------------------------------------

/** A variant instance carries a tag and optional fields. */
export interface VariantInstance {
  readonly _md_tmpl_tag: string;
  readonly _md_tmpl_fields: Readonly<Record<string, unknown>>;
  [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// Unit variant
// ---------------------------------------------------------------------------

/**
 * Create a unit variant sentinel.
 *
 * Unit variants have no fields. They compare by tag name and carry the
 * `_md_tmpl_tag` / `_md_tmpl_fields` protocol.
 *
 * @example
 * ```ts
 * const Approved = unitVariant("Approved");
 * console.log(Approved._md_tmpl_tag); // "Approved"
 * ```
 */
export function unitVariant(tag: string): VariantInstance {
  return Object.freeze({
    _md_tmpl_tag: tag,
    _md_tmpl_fields: Object.freeze({}),
    toString() {
      return tag;
    },
  });
}

// ---------------------------------------------------------------------------
// Struct variant
// ---------------------------------------------------------------------------

/** A constructor function for a struct variant. */
export type VariantConstructor<F extends string = string> = {
  (fields: Record<F, unknown>): VariantInstance;
  readonly _md_tmpl_tag: string;
  readonly __match_args__: readonly F[];
};

/**
 * Create a struct variant constructor.
 *
 * The returned function creates instances that carry the
 * `_md_tmpl_tag` / `_md_tmpl_fields` protocol
 * and expose each field as a direct property.
 *
 * @param tag - Variant name (e.g., "NeedsChanges").
 * @param fieldNames - Ordered list of field names.
 *
 * @example
 * ```ts
 * const NeedsChanges = variant("NeedsChanges", ["reason"]);
 * const v = NeedsChanges({ reason: "fix tests" });
 * console.log(v.reason); // "fix tests"
 * ```
 */
export function variant<F extends string>(
  tag: string,
  fieldNames: readonly F[],
): VariantConstructor<F> {
  const ctor = (fields: Record<F, unknown>): VariantInstance => {
    // Validate required fields
    for (const name of fieldNames) {
      if (!(name in fields)) {
        throw new TypeError(
          `${tag}() missing required keyword argument: '${name}'`,
        );
      }
    }
    // Check for unexpected fields
    const unexpected = Object.keys(fields).filter(
      (k) => !fieldNames.includes(k as F),
    );
    if (unexpected.length > 0) {
      throw new TypeError(
        `${tag}() got unexpected keyword arguments: ${unexpected.join(", ")}`,
      );
    }

    const fieldsObj: Record<string, unknown> = {};
    for (const name of fieldNames) {
      fieldsObj[name] = fields[name];
    }

    const instance: VariantInstance = {
      _md_tmpl_tag: tag,
      _md_tmpl_fields: Object.freeze({ ...fieldsObj }),
      ...fieldsObj,
      toString() {
        const parts = fieldNames
          .map((f) => `${f}=${JSON.stringify(fieldsObj[f])}`)
          .join(", ");
        return `${tag}(${parts})`;
      },
    };

    return Object.freeze(instance);
  };

  // Attach metadata to the constructor function
  Object.defineProperty(ctor, "_md_tmpl_tag", {
    value: tag,
    writable: false,
    enumerable: true,
  });
  Object.defineProperty(ctor, "__match_args__", {
    value: Object.freeze([...fieldNames]),
    writable: false,
    enumerable: true,
  });

  return ctor as VariantConstructor<F>;
}

// ---------------------------------------------------------------------------
// defineVariants — mixed enum builder
// ---------------------------------------------------------------------------

/**
 * Variant specification: `null` for unit variants, array of field names
 * for struct variants.
 */
export type VariantSpec = null | readonly string[];

/**
 * Define a mixed enum with unit and struct variants.
 *
 * Returns an object where:
 * - Unit variants (spec `null`) are frozen sentinels.
 * - Struct variants (spec `["field", ...]`) are constructor functions.
 *
 * @example
 * ```ts
 * const Status = defineVariants({
 *   Approved: null,
 *   Rejected: null,
 *   NeedsChanges: ["reason"],
 * });
 *
 * Status.Approved;                         // unit sentinel
 * Status.NeedsChanges({ reason: "fix" });  // struct constructor
 * ```
 */
export function defineVariants<T extends Record<string, VariantSpec>>(
  specs: T,
): {
  [K in keyof T]: T[K] extends null
    ? VariantInstance
    : VariantConstructor<
        T[K] extends readonly (infer F extends string)[] ? F : string
      >;
} {
  const result: Record<string, unknown> = {};

  for (const [name, spec] of Object.entries(specs)) {
    if (spec === null) {
      result[name] = unitVariant(name);
    } else {
      result[name] = variant(name, spec);
    }
  }

  return Object.freeze(result) as {
    [K in keyof T]: T[K] extends null
      ? VariantInstance
      : VariantConstructor<
          T[K] extends readonly (infer F extends string)[] ? F : string
        >;
  };
}

// ---------------------------------------------------------------------------
// match() — pattern matching for enum variants
// ---------------------------------------------------------------------------

/**
 * Pattern match on a variant value, like Rust's `match`.
 *
 * Exhaustive if you include `_` as a fallback. Each handler receives
 * the variant's fields (for struct variants) or nothing (for unit variants).
 *
 * @example
 * ```ts
 * const Status = defineVariants({
 *   Done: ["summary"],
 *   InProgress: null,
 *   Blocked: ["reason"],
 * });
 *
 * const msg = match(status, {
 *   Done: (v) => `✅ ${v.summary}`,
 *   InProgress: () => "🔄 Working...",
 *   Blocked: (v) => `❌ ${v.reason}`,
 * });
 * ```
 *
 * @example With wildcard fallback
 * ```ts
 * const msg = match(status, {
 *   Done: (v) => `Done: ${v.summary}`,
 *   _: () => "pending",
 * });
 * ```
 */
export function match<R>(
  value: VariantInstance | string | Record<string, unknown>,
  handlers: Record<string, (v: Record<string, unknown>) => R> & {
    _?: () => R;
  },
): R {
  let tag: string;
  let fields: Record<string, unknown>;

  if (typeof value === "string") {
    // Unit variant as string
    tag = value;
    fields = {};
  } else if (typeof value === "object" && value !== null) {
    const obj = value as Record<string, unknown>;
    if (typeof obj._md_tmpl_tag === "string") {
      // VariantInstance protocol
      tag = obj._md_tmpl_tag;
      fields = (obj._md_tmpl_fields as Record<string, unknown>) ?? {};
    } else if (typeof obj[ENUM_TAG_KEY] === "string") {
      // __kind__ protocol (from generated types)
      tag = obj[ENUM_TAG_KEY] as string;
      fields = { ...obj };
      delete fields[ENUM_TAG_KEY];
    } else {
      throw new TypeError("match(): value is not a variant");
    }
  } else {
    throw new TypeError(`match(): expected variant, got ${typeof value}`);
  }

  const handler = handlers[tag];
  if (handler) return handler(fields);
  if (handlers._) return handlers._();
  throw new TypeError(
    `match(): no handler for variant '${tag}' and no wildcard '_'`,
  );
}

/**
 * Check if a value is a specific variant.
 *
 * Works with both the `_md_tmpl_tag` protocol and `__kind__`
 * tagged objects.
 *
 * @example
 * ```ts
 * if (isVariant(status, "Done")) {
 *   console.log(status.summary);
 * }
 * ```
 */
export function isVariant(value: unknown, variantName: string): boolean {
  if (typeof value === "string") return value === variantName;
  if (value !== null && typeof value === "object") {
    const obj = value as Record<string, unknown>;
    if (obj._md_tmpl_tag === variantName) return true;
    if (obj[ENUM_TAG_KEY] === variantName) return true;
  }
  return false;
}
