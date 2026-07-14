/**
 * Runtime type checking of parameter values against declared VarTypes.
 *
 * @module
 */
import { MissingParamsError, TypeMismatchError } from "../errors.js";
import { type VarDecl, type VarType, varTypeToString } from "../frontmatter.js";
import { type Value, type TmplRef, ENUM_TAG_KEY } from "../value.js";
import {
  OPTION_SOME,
  TYPE_STR,
  TYPE_BOOL,
  TYPE_INT,
  TYPE_FLOAT,
  TYPE_LIST,
  TYPE_STRUCT,
  TYPE_TMPL,
  TYPE_ENUM,
  TYPE_OPTION,
  TYPE_NONE,
  TYPE_ALIAS,
  TYPE_SCALAR_LIST,
  TYPE_UNTYPED_LIST,
} from "../consts.js";

/** Validate a runtime parameter value against its declared type. */
export function typeCheckValue(
  path: string,
  value: Value,
  varType: VarType,
  typeAliases: ReadonlyMap<string, VarType>,
): void {
  const typeCheck = (path: string, value: Value, varType: VarType): void =>
    typeCheckValue(path, value, varType, typeAliases);
  switch (varType.kind) {
    case TYPE_STR:
      if (value.type !== TYPE_STR) {
        throw new TypeMismatchError(path, "str", value.type);
      }
      break;
    case TYPE_BOOL:
      if (value.type !== TYPE_BOOL) {
        throw new TypeMismatchError(path, "bool", value.type);
      }
      break;
    case TYPE_INT:
      if (value.type !== TYPE_INT) {
        throw new TypeMismatchError(path, "int", value.type);
      }
      break;
    case TYPE_FLOAT:
      if (value.type !== TYPE_FLOAT && value.type !== TYPE_INT) {
        throw new TypeMismatchError(path, "float", value.type);
      }
      break;
    case TYPE_LIST:
      if (value.type !== TYPE_LIST) {
        throw new TypeMismatchError(path, "list", value.type);
      }
      // Check item types
      if (varType.fields.length > 0) {
        for (let i = 0; i < value.items.length; i++) {
          const item = value.items[i]!;
          if (item.type !== TYPE_STRUCT) {
            throw new TypeMismatchError(`${path}[${i}]`, "struct", item.type);
          }
          for (const field of varType.fields) {
            const fieldVal = item.fields.get(field.name);
            if (fieldVal === undefined) {
              throw new MissingParamsError([`${path}[${i}].${field.name}`]);
            }
            typeCheck(`${path}[${i}].${field.name}`, fieldVal, field.varType);
          }
        }
      }
      break;
    case TYPE_TMPL:
      if (value.type === TYPE_TMPL) {
        // Higher-order: validate template signature
        typeCheckTmplSignature(path, value.ref, varType.fields);
      } else if (value.type === TYPE_STRUCT) {
        // Backward compat: accept struct as tmpl (legacy behavior)
        for (const field of varType.fields) {
          const fieldVal = value.fields.get(field.name);
          if (fieldVal === undefined) {
            throw new MissingParamsError([`${path}.${field.name}`]);
          }
          typeCheck(`${path}.${field.name}`, fieldVal, field.varType);
        }
      } else {
        throw new TypeMismatchError(path, "tmpl", value.type);
      }
      break;
    case TYPE_STRUCT:
      if (value.type !== TYPE_STRUCT) {
        throw new TypeMismatchError(path, "struct", value.type);
      }
      // Check fields
      for (const field of varType.fields) {
        const fieldVal = value.fields.get(field.name);
        if (fieldVal === undefined) {
          throw new MissingParamsError([`${path}.${field.name}`]);
        }
        typeCheck(`${path}.${field.name}`, fieldVal, field.varType);
      }
      break;
    case TYPE_ENUM:
      if (varType.isOption) {
        // Legacy isOption: transparent — none is valid, otherwise check inner
        if (value.type === TYPE_NONE) break;
        const someVariant = varType.variants.find(
          (v) => v.name === OPTION_SOME,
        );
        if (someVariant && someVariant.fields.length === 1) {
          typeCheck(path, value, someVariant.fields[0]!.varType);
        }
        break;
      }
      if (value.type === TYPE_STR) {
        // Unit variant as string
        const validNames = varType.variants.map((v) => v.name);
        if (!validNames.includes(value.value)) {
          throw new TypeMismatchError(
            path,
            `enum(${validNames.join(", ")})`,
            `str("${value.value}")`,
          );
        }
      } else if (value.type === TYPE_STRUCT) {
        // Struct variant as struct with __kind__
        const tag = value.fields.get(ENUM_TAG_KEY);
        if (tag === undefined || tag.type !== TYPE_STR) {
          throw new TypeMismatchError(
            path,
            "enum variant",
            "struct without __kind__",
          );
        }
        const matchedVariant = varType.variants.find(
          (v) => v.name === tag.value,
        );
        if (!matchedVariant) {
          const validNames = varType.variants.map((v) => v.name);
          throw new TypeMismatchError(
            path,
            `enum(${validNames.join(", ")})`,
            `variant("${tag.value}")`,
          );
        }
        // Recursively validate variant field types (matches Rust behavior)
        for (const field of matchedVariant.fields) {
          const fieldVal = value.fields.get(field.name);
          if (fieldVal === undefined) {
            throw new MissingParamsError([`${path}.${field.name}`]);
          }
          typeCheck(`${path}.${field.name}`, fieldVal, field.varType);
        }
      } else {
        throw new TypeMismatchError(path, "enum", value.type);
      }
      break;
    case TYPE_OPTION:
      // Transparent option: none is always valid, otherwise check inner type
      if (value.type === TYPE_NONE) break;
      typeCheck(path, value, varType.innerType);
      break;
    case TYPE_ALIAS:
      // Resolve alias from type aliases
      {
        const resolved = typeAliases.get(varType.name);
        if (resolved) {
          typeCheck(path, value, resolved);
        }
        // If alias not found, skip check (may be imported type)
      }
      break;
    case TYPE_SCALAR_LIST:
      if (value.type !== TYPE_LIST) {
        throw new TypeMismatchError(path, "list", value.type);
      }
      // Check each element against the declared element type
      for (let i = 0; i < value.items.length; i++) {
        typeCheck(`${path}[${i}]`, value.items[i]!, varType.elementType);
      }
      break;
    case TYPE_UNTYPED_LIST:
      if (value.type !== TYPE_LIST) {
        throw new TypeMismatchError(path, "list", value.type);
      }
      break;
  }
}

function typeCheckTmplSignature(
  path: string,
  ref: TmplRef,
  expectedFields: readonly VarDecl[],
): void {
  const actualDecls = ref.rawDeclarations();
  for (const exp of expectedFields) {
    const actual = actualDecls.find((d) => d.name === exp.name);
    if (!actual) {
      throw new TypeMismatchError(
        `${path}.${exp.name}`,
        varTypeToString(exp.varType),
        "missing",
      );
    }
    if (
      varTypeToString(actual.varType as VarType) !==
      varTypeToString(exp.varType)
    ) {
      throw new TypeMismatchError(
        `${path}.${exp.name}`,
        varTypeToString(exp.varType),
        varTypeToString(actual.varType as VarType),
      );
    }
  }
  for (const actual of actualDecls) {
    if (
      actual.defaultValue === undefined &&
      !expectedFields.some((e) => e.name === actual.name)
    ) {
      throw new TypeMismatchError(
        `${path}.${actual.name}`,
        "in signature",
        "extra required param without default",
      );
    }
  }
}
