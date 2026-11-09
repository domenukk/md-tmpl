//! Type annotation, nesting, and `option(T)` tests.

use super::*;
use crate::{
    compat::HashMap,
    frontmatter::test_helpers::{parse_decls, parse_type},
    types::{VarDecl, VarType},
    value::Value,
};

// =========================================================================
// parse_type_annotation
// =========================================================================

#[test]
fn type_str() {
    assert_eq!(parse_type("str").unwrap(), VarType::Str);
}

#[test]
fn type_bool() {
    assert_eq!(parse_type("bool").unwrap(), VarType::Bool);
}

#[test]
fn type_int() {
    assert_eq!(parse_type("int").unwrap(), VarType::Int);
}

#[test]
fn type_float() {
    assert_eq!(parse_type("float").unwrap(), VarType::Float);
}

#[test]
fn type_str_with_whitespace() {
    assert_eq!(parse_type("  str  ").unwrap(), VarType::Str);
}

#[test]
fn type_list() {
    let result = parse_type("list(name = str)").unwrap();
    match result {
        VarType::List(fields) => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name, "name");
            assert_eq!(fields[0].var_type, VarType::Str);
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn type_list_multiple_fields() {
    let result = parse_type("list(name = str, count = int)").unwrap();
    match result {
        VarType::List(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "name");
            assert_eq!(fields[0].var_type, VarType::Str);
            assert_eq!(fields[1].name, "count");
            assert_eq!(fields[1].var_type, VarType::Int);
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn type_struct() {
    let result = parse_type("struct(key = str, value = int)").unwrap();
    match result {
        VarType::Struct(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "key");
            assert_eq!(fields[0].var_type, VarType::Str);
            assert_eq!(fields[1].name, "value");
            assert_eq!(fields[1].var_type, VarType::Int);
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

#[test]
fn type_enum_simple() {
    let result = parse_type("enum(A, B, C)").unwrap();
    match result {
        VarType::Enum(variants) => {
            assert_eq!(variants.len(), 3);
            assert_eq!(variants[0].name, "A");
            assert!(variants[0].fields.is_empty());
            assert_eq!(variants[1].name, "B");
            assert_eq!(variants[2].name, "C");
        }
        other => panic!("Expected Enum, got {other:?}"),
    }
}

#[test]
fn type_enum_with_fields() {
    let result = parse_type("enum(A, B(field = str))").unwrap();
    match result {
        VarType::Enum(variants) => {
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].name, "A");
            assert!(variants[0].fields.is_empty());
            assert_eq!(variants[1].name, "B");
            assert_eq!(variants[1].fields.len(), 1);
            assert_eq!(variants[1].fields[0].name, "field");
            assert_eq!(variants[1].fields[0].var_type, VarType::Str);
        }
        other => panic!("Expected Enum, got {other:?}"),
    }
}

#[test]
fn type_tmpl() {
    let result = parse_type("tmpl(name = str, count = int)").unwrap();
    match result {
        VarType::Tmpl(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "name");
            assert_eq!(fields[0].var_type, VarType::Str);
            assert_eq!(fields[1].name, "count");
            assert_eq!(fields[1].var_type, VarType::Int);
        }
        other => panic!("Expected Tmpl, got {other:?}"),
    }
}

#[test]
fn type_unknown_errors() {
    let err = parse_type("garbage").unwrap_err();
    assert!(err.contains("unknown type"), "got: {err}");
}

#[test]
fn type_bare_list_errors() {
    let err = parse_type("list").unwrap_err();
    assert!(err.contains("unknown type"), "got: {err}");
}

#[test]
fn type_bare_struct_errors() {
    let err = parse_type("struct").unwrap_err();
    assert!(err.contains("unknown type"), "got: {err}");
}

#[test]
fn type_nested_list_in_struct() {
    let result = parse_type("struct(items = list(name = str))").unwrap();
    match result {
        VarType::Struct(fields) => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name, "items");
            match &fields[0].var_type {
                VarType::List(inner) => {
                    assert_eq!(inner.len(), 1);
                    assert_eq!(inner[0].name, "name");
                    assert_eq!(inner[0].var_type, VarType::Str);
                }
                other => panic!("Expected inner List, got {other:?}"),
            }
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

#[test]
fn type_alias_lookup() {
    let mut aliases = HashMap::new();
    aliases.insert("Priority".to_string(), VarType::Enum(vec![]));
    let imports = HashMap::new();
    let result = parse_type_annotation("Priority", &aliases, &imports).unwrap();
    assert_eq!(result, VarType::Enum(vec![]));
}

#[test]
fn type_dotted_import_lookup() {
    let aliases = HashMap::new();
    let mut imports = HashMap::new();
    let mut ns = ImportedNamespace::default();
    ns.type_aliases.insert("Severity".to_string(), VarType::Str);
    imports.insert("types".to_string(), ns);
    let result = parse_type_annotation("types.Severity", &aliases, &imports).unwrap();
    assert_eq!(result, VarType::Str);
}

#[test]
fn type_dotted_import_not_found() {
    let aliases = HashMap::new();
    let mut imports = HashMap::new();
    let ns = ImportedNamespace::default();
    imports.insert("types".to_string(), ns);
    let err = parse_type_annotation("types.Missing", &aliases, &imports).unwrap_err();
    assert!(err.contains("has no type"), "got: {err}");
}

// =========================================================================
// Type nesting rules
// =========================================================================

// --- Positive: valid nesting ---

#[test]
fn list_of_scalars_valid() {
    let decls = parse_decls("[items = list(str)]").unwrap();
    assert!(matches!(&decls[0].var_type, VarType::List(fields) if fields.len() == 1));
}

#[test]
fn list_with_named_fields_valid() {
    let decls = parse_decls("[items = list(name = str, score = int)]").unwrap();
    assert!(matches!(&decls[0].var_type, VarType::List(fields) if fields.len() == 2));
}

#[test]
fn list_of_list_valid() {
    // Nested lists (e.g. matrix, grid, coordinates).
    let decls = parse_decls("[grid = list(list(str))]").unwrap();
    if let VarType::List(fields) = &decls[0].var_type {
        assert_eq!(fields.len(), 1);
        assert!(matches!(&fields[0].var_type, VarType::List(_)));
    } else {
        panic!("expected list type");
    }
}

#[test]
fn list_of_enum_valid() {
    // List where each element is an enum value.
    let decls = parse_decls("[statuses = list(enum(Active, Paused))]").unwrap();
    if let VarType::List(fields) = &decls[0].var_type {
        assert_eq!(fields.len(), 1);
        assert!(matches!(&fields[0].var_type, VarType::Enum(_)));
    } else {
        panic!("expected list type");
    }
}

#[test]
fn list_of_enum_with_default_values() {
    let mut aliases = HashMap::new();
    aliases.insert(
        "Status".to_string(),
        VarType::Enum(vec![
            crate::types::VariantDecl {
                name: "UnitVal".into(),
                fields: vec![],
            },
            crate::types::VariantDecl {
                name: "StructVal".into(),
                fields: vec![VarDecl {
                    name: "blubb".into(),
                    var_type: VarType::Str,
                    default_value: None,
                }],
            },
        ]),
    );
    let (decls, _) = parse_declarations(
        "[statuses = list(Status) := [UnitVal, StructVal(blubb = \"test\")]]",
        &aliases,
        &HashMap::new(),
        false,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(decls.len(), 1);
    let default = decls[0].default_value.as_ref().unwrap();
    match default {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("UnitVal".into()));
            match &items[1] {
                Value::Struct(map) => {
                    assert_eq!(map.get("__kind__"), Some(&Value::Str("StructVal".into())));
                    assert_eq!(map.get("blubb"), Some(&Value::Str("test".into())));
                }
                other => panic!("Expected Struct, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn struct_with_list_field_valid() {
    let decls = parse_decls("[cfg = struct(tags = list(str), name = str)]").unwrap();
    if let VarType::Struct(fields) = &decls[0].var_type {
        assert_eq!(fields.len(), 2);
        assert!(matches!(&fields[0].var_type, VarType::List(_)));
    } else {
        panic!("expected struct type");
    }
}

#[test]
fn struct_with_enum_field_valid() {
    let decls = parse_decls("[cfg = struct(status = enum(On, Off), name = str)]").unwrap();
    if let VarType::Struct(fields) = &decls[0].var_type {
        assert_eq!(fields.len(), 2);
        assert!(matches!(&fields[0].var_type, VarType::Enum(_)));
    } else {
        panic!("expected struct type");
    }
}

#[test]
fn struct_with_nested_struct_field_valid() {
    let decls =
        parse_decls("[cfg = struct(inner = struct(x = int, y = int), name = str)]").unwrap();
    if let VarType::Struct(fields) = &decls[0].var_type {
        assert_eq!(fields.len(), 2);
        assert!(matches!(&fields[0].var_type, VarType::Struct(_)));
    } else {
        panic!("expected struct type");
    }
}

#[test]
fn list_of_list_of_int_valid() {
    // Matrix of ints — deeply nested.
    let decls = parse_decls("[matrix = list(list(int))]").unwrap();
    if let VarType::List(outer) = &decls[0].var_type {
        if let VarType::List(inner) = &outer[0].var_type {
            assert_eq!(inner.len(), 1);
            assert!(matches!(&inner[0].var_type, VarType::Int));
        } else {
            panic!("expected inner list");
        }
    } else {
        panic!("expected outer list");
    }
}

// --- Negative: forbidden nesting ---

#[test]
fn list_of_raw_struct_rejected_as_redundant() {
    let err = parse_decls("[items = list(struct(name = str, score = int))]").unwrap_err();
    assert!(err.to_string().contains("redundant"), "got: {err}");
}

#[test]
fn list_of_strong_struct_alias_unwraps_cleanly() {
    let mut aliases = HashMap::new();
    aliases.insert(
        "MyItem".to_string(),
        VarType::Struct(vec![
            VarDecl {
                name: "name".to_string(),
                var_type: VarType::Str,
                default_value: None,
            },
            VarDecl {
                name: "score".to_string(),
                var_type: VarType::Int,
                default_value: None,
            },
        ]),
    );
    let var_type = parse_type_annotation("list(MyItem)", &aliases, &HashMap::new()).unwrap();
    if let VarType::List(ref fields) = var_type {
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "name");
        assert_eq!(fields[1].name, "score");
    } else {
        panic!("expected VarType::List");
    }
}

#[test]
fn list_of_named_struct_field_allowed() {
    let decls = parse_decls("[items = list(item = struct(name = str, score = int))]").unwrap();
    if let VarType::List(ref fields) = decls[0].var_type {
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "item");
        if let VarType::Struct(ref inner) = fields[0].var_type {
            assert_eq!(inner.len(), 2);
            assert_eq!(inner[0].name, "name");
            assert_eq!(inner[1].name, "score");
        } else {
            panic!("expected inner VarType::Struct");
        }
    } else {
        panic!("expected VarType::List");
    }
}

// =========================================================================
// option(T) type parsing
// =========================================================================

#[test]
fn type_option_str() {
    let result = parse_type("option(str)").unwrap();
    match result {
        VarType::Option(inner) => {
            assert_eq!(*inner, VarType::Str);
        }
        other => panic!("Expected Option, got {other:?}"),
    }
}

#[test]
fn type_option_int() {
    let result = parse_type("option(int)").unwrap();
    assert!(result.is_option());
    assert_eq!(*result.option_inner_type().unwrap(), VarType::Int);
}

#[test]
fn type_option_with_spaces() {
    let result = parse_type("option( str )").unwrap();
    assert!(result.is_option());
    assert_eq!(*result.option_inner_type().unwrap(), VarType::Str);
}

#[test]
fn type_option_nested_list() {
    let result = parse_type("option(list(name = str))").unwrap();
    assert!(result.is_option());
    assert!(matches!(
        result.option_inner_type().unwrap(),
        VarType::List(_)
    ));
}

#[test]
fn type_option_nested_struct() {
    let result = parse_type("option(struct(x = int, y = int))").unwrap();
    assert!(result.is_option());
    assert!(matches!(
        result.option_inner_type().unwrap(),
        VarType::Struct(_)
    ));
}

#[test]
fn type_option_nested_option() {
    let result = parse_type("option(option(str))").unwrap();
    assert!(result.is_option());
    let inner = result.option_inner_type().unwrap();
    assert!(inner.is_option());
    assert_eq!(*inner.option_inner_type().unwrap(), VarType::Str);
}

#[test]
fn type_option_display() {
    let result = parse_type("option(str)").unwrap();
    assert_eq!(format!("{result}"), "option(str)");
}

#[test]
fn type_option_display_nested() {
    let result = parse_type("option(option(int))").unwrap();
    assert_eq!(format!("{result}"), "option(option(int))");
}

#[test]
fn type_option_empty_rejected() {
    assert!(parse_type("option()").is_err());
}

#[test]
fn type_option_malformed_rejected() {
    assert!(parse_type("option").is_err());
    assert!(parse_type("option(").is_err());
}

#[test]
fn option_default_none() {
    let decls = parse_decls("[x = option(str) := None]").unwrap();
    assert_eq!(decls[0].default_value, Some(Value::None));
}

#[test]
fn option_default_some() {
    // Transparent option: := "hello" stores the raw string, not a Some(val=...) struct.
    let decls = parse_decls("[x = option(str) := \"hello\"]").unwrap();
    assert_eq!(decls[0].default_value, Some(Value::Str("hello".into())));
}

#[test]
fn option_reserved_name() {
    let result = parse_decls("[option = str]");
    assert!(result.is_err());
}

#[test]
fn list_of_option() {
    let result = parse_type("list(option(str))").unwrap();
    match result {
        VarType::List(fields) => {
            assert_eq!(fields.len(), 1);
            assert!(fields[0].var_type.is_option());
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn prohibited_angle_and_square_brackets() {
    assert!(parse_type("list<str>").is_err());
    assert!(parse_type("list[str]").is_err());
    assert!(parse_type("struct<x = int>").is_err());
    assert!(parse_type("struct[x = int]").is_err());
    assert!(parse_type("enum<A, B>").is_err());
    assert!(parse_type("enum[A, B]").is_err());
    assert!(parse_type("tmpl<x = int>").is_err());
    assert!(parse_type("tmpl[x = int]").is_err());
    assert!(parse_type("option<str>").is_err());
    assert!(parse_type("option[str]").is_err());
}
