//! `parse_declarations` params and constants mode tests.

use super::*;
use crate::{
    compat::HashMap,
    frontmatter::test_helpers::{parse_consts, parse_decls},
    types::VarType,
    value::Value,
};

// =========================================================================
// parse_declarations (params mode)
// =========================================================================

#[test]
fn decls_inline_basic() {
    let decls = parse_decls("[name = str, count = int]").unwrap();
    assert_eq!(decls.len(), 2);
    assert_eq!(decls[0].name, "name");
    assert_eq!(decls[0].var_type, VarType::Str);
    assert_eq!(decls[1].name, "count");
    assert_eq!(decls[1].var_type, VarType::Int);
}

#[test]
fn decls_empty_string() {
    let decls = parse_decls("").unwrap();
    assert!(decls.is_empty());
}

#[test]
fn decls_empty_brackets() {
    let decls = parse_decls("[]").unwrap();
    assert!(decls.is_empty());
}

#[test]
fn decls_with_default_values() {
    let decls = parse_decls("[name = str := \"hello\", count = int := 42]").unwrap();
    assert_eq!(decls.len(), 2);
    assert_eq!(decls[0].name, "name");
    assert_eq!(decls[0].var_type, VarType::Str);
    assert_eq!(
        decls[0].default_value,
        Some(Value::Str("hello".to_string()))
    );
    assert_eq!(decls[1].name, "count");
    assert_eq!(decls[1].var_type, VarType::Int);
    assert_eq!(decls[1].default_value, Some(Value::Int(42)));
}

#[test]
fn decls_mixed_default_and_required() {
    let decls = parse_decls("[name = str, count = int := 10]").unwrap();
    assert_eq!(decls[0].default_value, None);
    assert_eq!(decls[1].default_value, Some(Value::Int(10)));
}

#[test]
fn decls_with_kinds_default() {
    let mut aliases = HashMap::new();
    aliases.insert(
        "ForumTag".to_string(),
        VarType::Enum(vec![
            crate::types::VariantDecl {
                name: "FINDING".into(),
                fields: vec![],
            },
            crate::types::VariantDecl {
                name: "HYPOTHESIS".into(),
                fields: vec![],
            },
        ]),
    );
    let (decls, _) = parse_declarations(
        "[tags = list(str) := kinds(ForumTag)]",
        &aliases,
        &HashMap::new(),
        false,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(decls.len(), 1);
    match decls[0].default_value.as_ref().unwrap() {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("FINDING".into()));
            assert_eq!(items[1], Value::Str("HYPOTHESIS".into()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn decls_duplicate_name_error() {
    let err = parse_decls("[name = str, name = int]").unwrap_err();
    assert!(
        err.to_string().contains("duplicate parameter name"),
        "got: {err}"
    );
}

#[test]
fn decls_reserved_keyword_error() {
    let err = parse_decls("[list = str]").unwrap_err();
    assert!(err.to_string().contains("reserved keyword"), "got: {err}");
}

#[test]
fn decls_reserved_keyword_params() {
    let err = parse_decls("[params = str]").unwrap_err();
    assert!(err.to_string().contains("reserved keyword"), "got: {err}");
}

#[test]
fn enum_variant_reserved_keyword_rejected() {
    let err = parse_decls("[x = enum(struct, ok)]").unwrap_err();
    assert!(
        err.to_string().contains("shadows a builtin type keyword"),
        "got: {err}"
    );
}

#[test]
fn enum_variant_reserved_keyword_list_rejected() {
    let err = parse_decls("[x = enum(list, enum)]").unwrap_err();
    assert!(
        err.to_string().contains("shadows a builtin type keyword"),
        "got: {err}"
    );
}

#[test]
fn decls_missing_type_annotation() {
    let err = parse_decls("[untyped_param]").unwrap_err();
    assert!(
        err.to_string().contains("missing a type annotation"),
        "got: {err}"
    );
}

#[test]
fn decls_with_complex_types() {
    let decls = parse_decls("[items = list(name = str, score = float), active = bool]").unwrap();
    assert_eq!(decls.len(), 2);
    match &decls[0].var_type {
        VarType::List(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "name");
            assert_eq!(fields[1].name, "score");
            assert_eq!(fields[1].var_type, VarType::Float);
        }
        other => panic!("Expected List, got {other:?}"),
    }
    assert_eq!(decls[1].name, "active");
    assert_eq!(decls[1].var_type, VarType::Bool);
}

#[test]
fn decls_block_format() {
    // After continuation joining, block entries look like:
    // "- name = str - count = int"
    let decls = parse_decls("- name = str - count = int").unwrap();
    assert_eq!(decls.len(), 2);
    assert_eq!(decls[0].name, "name");
    assert_eq!(decls[0].var_type, VarType::Str);
    assert_eq!(decls[1].name, "count");
    assert_eq!(decls[1].var_type, VarType::Int);
}

#[test]
fn decls_default_type_mismatch() {
    let err = parse_decls("[name = str := 42]").unwrap_err();
    assert!(
        err.to_string().contains("value has type"),
        "expected type mismatch error, got: {err}"
    );
}

// =========================================================================
// parse_declarations (constants mode)
// =========================================================================

#[test]
fn consts_requires_value() {
    let err = parse_consts("[MAX = int]").unwrap_err();
    assert!(err.to_string().contains("missing a value"), "got: {err}");
}

#[test]
fn consts_with_value() {
    let decls = parse_consts("[MAX = int := 100]").unwrap();
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].name, "MAX");
    assert_eq!(decls[0].var_type, VarType::Int);
    assert_eq!(decls[0].default_value, Some(Value::Int(100)));
}

#[test]
fn consts_duplicate_name_error() {
    let err = parse_consts("[A = int := 1, A = int := 2]").unwrap_err();
    assert!(
        err.to_string().contains("duplicate constant name"),
        "got: {err}"
    );
}

#[test]
fn consts_reserved_keyword_error() {
    let err = parse_consts("[struct = str := \"hello\"]").unwrap_err();
    assert!(err.to_string().contains("reserved keyword"), "got: {err}");
}

#[test]
fn consts_bool_default() {
    let decls = parse_consts("[ENABLED = bool := true]").unwrap();
    assert_eq!(decls[0].default_value, Some(Value::Bool(true)));
}

#[test]
fn consts_str_default() {
    let decls = parse_consts("[GREETING = str := \"hi\"]").unwrap();
    assert_eq!(decls[0].default_value, Some(Value::Str("hi".to_string())));
}

#[test]
fn untyped_list_fails() {
    let err = parse_decls("[items = list()]").unwrap_err();
    assert!(
        err.to_string().contains("untyped list() is not allowed"),
        "got: {err}"
    );
}

#[test]
fn untyped_struct_fails() {
    let err = parse_decls("[data = struct()]").unwrap_err();
    assert!(
        err.to_string().contains("untyped struct() is not allowed"),
        "got: {err}"
    );
}

#[test]
fn unnamed_multiple_fields_list_fails() {
    let err = parse_decls("[items = list(str, int)]").unwrap_err();
    assert!(
        err.to_string()
            .contains("list with multiple fields must use named fields"),
        "got: {err}"
    );
}

#[test]
fn unquoted_string_default_fails() {
    let err = parse_decls("[name = str := hello]").unwrap_err();
    assert!(
        err.to_string().contains("strings must be quoted"),
        "got: {err}"
    );
}

#[test]
fn consts_type_mismatch() {
    let err = parse_consts("[X = int := \"not_a_number\"]").unwrap_err();
    assert!(
        err.to_string().contains("value has type"),
        "expected type mismatch, got: {err}"
    );
}
