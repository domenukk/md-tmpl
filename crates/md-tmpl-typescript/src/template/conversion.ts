/**
 * JS-to-Value conversion helpers for the Template module.
 *
 * @module
 */

import { TemplateError } from "../errors.js";
import { type VarType } from "../frontmatter.js";
import {
  ENUM_TAG_KEY,
  NONE,
  type Value,
  fromJs,
  list,
  str,
  structVal,
} from "../value.js";
import {
  OPTION_SOME,
  TYPE_ALIAS,
  TYPE_ENUM,
  TYPE_OPTION,
  TYPE_STRUCT,
} from "../consts.js";

/**
 * Convert a JS value to a template Value, handling option types transparently.
 *
 * For `option(T)` fields:
 * - `null`/`undefined` → `NONE` (absent value)
 * - any other value → `fromJs(value)` (the inner value directly)
 *
 * For struct/list fields, recursively converts nested option fields.
 */
export function jsToValue(
  value: unknown,
  varType: VarType,
  typeAliases?: ReadonlyMap<string, VarType>,
  seen = new WeakSet(),
  depth = 0,
): Value {
  if (depth > 256) {
    throw new TemplateError(
      "maximum recursion depth exceeded in template parameter",
    );
  }

  // Resolve type aliases before checking
  if (varType.kind === TYPE_ALIAS && typeAliases) {
    const resolved = typeAliases.get(varType.name);
    if (resolved) {
      return jsToValue(value, resolved, typeAliases, seen, depth);
    }
  }

  // Option types: null/undefined → NONE, otherwise convert the inner value
  if (varType.kind === TYPE_OPTION) {
    if (value === null || value === undefined) {
      return NONE;
    }
    return jsToValue(value, varType.innerType, typeAliases, seen, depth);
  }

  // Legacy isOption handling
  if (varType.kind === TYPE_ENUM && varType.isOption) {
    if (value === null || value === undefined) {
      return NONE;
    }
    const someVariant = varType.variants.find((v) => v.name === OPTION_SOME);
    const someField = someVariant?.fields[0];
    if (someVariant?.fields.length === 1 && someField) {
      return jsToValue(value, someField.varType, typeAliases, seen, depth);
    }
    return fromJs(value, seen, depth + 1);
  }

  // Non-option enum types: handle struct variants passed as { VariantName: { fields } }
  if (
    varType.kind === TYPE_ENUM &&
    !varType.isOption &&
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value)
  ) {
    const obj = value as Record<string, unknown>;
    const keys = Object.keys(obj);
    const variantName = keys[0];
    if (keys.length === 1 && variantName !== undefined) {
      const variant = varType.variants.find((v) => v.name === variantName);
      if (variant && variant.fields.length > 0) {
        // This is a struct variant: { VariantName: { field1: val1, ... } }
        const inner = obj[variantName];
        if (
          typeof inner === "object" &&
          inner !== null &&
          !Array.isArray(inner)
        ) {
          const innerObj = inner as Record<string, unknown>;
          const entries: [string, Value][] = [[ENUM_TAG_KEY, str(variantName)]];
          for (const field of variant.fields) {
            if (field.name in innerObj) {
              entries.push([
                field.name,
                jsToValue(
                  innerObj[field.name],
                  field.varType,
                  typeAliases,
                  seen,
                  depth + 1,
                ),
              ]);
            }
          }
          return structVal(entries);
        }
      }
    }
  }

  // Structs: recursively handle nested option fields
  if (
    varType.kind === TYPE_STRUCT &&
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value)
  ) {
    seen.add(value);
    try {
      const obj = value as Record<string, unknown>;
      const entries: [string, Value][] = [];
      for (const field of varType.fields) {
        if (field.name in obj) {
          entries.push([
            field.name,
            jsToValue(
              obj[field.name],
              field.varType,
              typeAliases,
              seen,
              depth + 1,
            ),
          ]);
        }
      }
      // Preserve non-declared fields
      for (const [k, v] of Object.entries(obj)) {
        if (!varType.fields.some((f) => f.name === k)) {
          entries.push([k, fromJs(v, seen, depth + 1)]);
        }
      }
      return structVal(entries);
    } finally {
      seen.delete(value);
    }
  }

  // Lists with structured items: recursively handle nested option fields
  if (
    varType.kind === "list" &&
    Array.isArray(value) &&
    varType.fields.length > 0
  ) {
    seen.add(value as object);
    try {
      const items = value.map((item) => {
        if (typeof item === "object" && item !== null && !Array.isArray(item)) {
          const obj = item as Record<string, unknown>;
          const entries: [string, Value][] = [];
          for (const field of varType.fields) {
            if (field.name in obj) {
              entries.push([
                field.name,
                jsToValue(
                  obj[field.name],
                  field.varType,
                  typeAliases,
                  seen,
                  depth + 1,
                ),
              ]);
            }
          }
          // Preserve non-declared fields
          for (const [k, v] of Object.entries(obj)) {
            if (!varType.fields.some((f) => f.name === k)) {
              entries.push([k, fromJs(v, seen, depth + 1)]);
            }
          }
          return structVal(entries);
        }
        return fromJs(item, seen, depth + 1);
      });
      return list(items);
    } finally {
      seen.delete(value as object);
    }
  }

  // Default: use standard fromJs conversion
  return fromJs(value, seen, depth + 1);
}
