//! Default value parsing and enum-variant default tests.

use super::*;
use crate::{
    compat::HashMap,
    frontmatter::test_helpers::{parse_consts, parse_decls, parse_decls_stage},
    types::{VarDecl, VarType},
    value::Value,
};

// =========================================================================
// parse_default_value
// =========================================================================

#[test]
fn parse_default_quoted_string() {
    assert_eq!(
        parse_default_value("\"hello\""),
        Some(Value::Str("hello".to_string()))
    );
}

#[test]
fn parse_default_single_quoted_string() {
    assert_eq!(
        parse_default_value("'world'"),
        Some(Value::Str("world".to_string()))
    );
}

#[test]
fn parse_default_integer() {
    assert_eq!(parse_default_value("42"), Some(Value::Int(42)));
}

#[test]
fn parse_default_negative_integer() {
    assert_eq!(parse_default_value("-7"), Some(Value::Int(-7)));
}

#[test]
fn parse_default_float() {
    assert_eq!(parse_default_value("3.125"), Some(Value::Float(3.125)));
}

#[test]
fn parse_default_bool_true() {
    assert_eq!(parse_default_value("true"), Some(Value::Bool(true)));
}

#[test]
fn parse_default_bool_false() {
    assert_eq!(parse_default_value("false"), Some(Value::Bool(false)));
}

#[test]
fn parse_default_list() {
    let result = parse_default_value("[1, 2, 3]").unwrap();
    match result {
        Value::List(items) => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::Int(1));
            assert_eq!(items[1], Value::Int(2));
            assert_eq!(items[2], Value::Int(3));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn parse_default_dict() {
    let result = parse_default_value_with_type(
        "{a = 1, b = 2}",
        &VarType::Struct(vec![
            VarDecl {
                name: "a".into(),
                var_type: VarType::Int,
                default_value: None,
            },
            VarDecl {
                name: "b".into(),
                var_type: VarType::Int,
                default_value: None,
            },
        ]),
        &HashMap::new(),
    )
    .unwrap();
    match result {
        Value::Struct(map) => {
            assert_eq!(map.get("a"), Some(&Value::Int(1)));
            assert_eq!(map.get("b"), Some(&Value::Int(2)));
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

#[test]
fn parse_default_empty_returns_none() {
    assert_eq!(parse_default_value(""), None);
}

#[test]
fn parse_default_whitespace_only_returns_none() {
    assert_eq!(parse_default_value("   "), None);
}

#[test]
fn parse_default_unquoted_string() {
    // Unquoted non-numeric/non-bool strings are no longer allowed.
    assert_eq!(parse_default_value("hello"), None);
}

#[test]
fn parse_default_empty_list() {
    let result = parse_default_value("[]").unwrap();
    match result {
        Value::List(items) => assert!(items.is_empty()),
        other => panic!("Expected empty List, got {other:?}"),
    }
}

#[test]
fn parse_default_empty_dict() {
    let result =
        parse_default_value_with_type("{}", &VarType::Struct(vec![]), &HashMap::new()).unwrap();
    match result {
        Value::Struct(map) => assert!(map.is_empty()),
        other => panic!("Expected empty Struct, got {other:?}"),
    }
}

#[test]
fn parse_default_zero() {
    assert_eq!(parse_default_value("0"), Some(Value::Int(0)));
}

#[test]
fn parse_default_float_zero() {
    assert_eq!(parse_default_value("0.0"), Some(Value::Float(0.0)));
}

#[test]
fn parse_default_nested_list() {
    let result = parse_default_value("[1, [2, 3]]").unwrap();
    match result {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Int(1));
            match &items[1] {
                Value::List(inner) => {
                    assert_eq!(inner.len(), 2);
                    assert_eq!(inner[0], Value::Int(2));
                    assert_eq!(inner[1], Value::Int(3));
                }
                other => panic!("Expected inner List, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn parse_default_dict_with_quoted_keys() {
    let result = parse_default_value_with_type(
        "{key = 42}",
        &VarType::Struct(vec![VarDecl {
            name: "key".into(),
            var_type: VarType::Int,
            default_value: None,
        }]),
        &HashMap::new(),
    )
    .unwrap();
    match result {
        Value::Struct(map) => {
            assert_eq!(map.get("key"), Some(&Value::Int(42)));
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

// =========================================================================
// Enum defaults and consts
// =========================================================================

#[test]
fn enum_unit_variant_default() {
    let decls = parse_decls("[status = enum(Active, Paused) := Active]").unwrap();
    assert_eq!(
        decls[0].default_value,
        Some(Value::Str("Active".to_string()))
    );
}

#[test]
fn enum_unit_variant_default_on_mixed_enum() {
    // Unit variant default on an enum that also has struct variants.
    let decls =
        parse_decls("[outcome = enum(Confirmed(evidence = str), Rejected) := Rejected]").unwrap();
    assert_eq!(
        decls[0].default_value,
        Some(Value::Str("Rejected".to_string()))
    );
}

#[test]
fn enum_struct_variant_default() {
    // Struct variant default with inline field values.
    let decls = parse_decls(
        "[outcome = enum(Confirmed(evidence = str), Rejected) := Confirmed(evidence = \"found it\")]",
    )
    .unwrap();
    let default = decls[0].default_value.as_ref().unwrap();
    match default {
        Value::Struct(map) => {
            assert_eq!(
                map.get("__kind__"),
                Some(&Value::Str("Confirmed".to_string())),
                "should have __kind__ tag"
            );
            assert_eq!(
                map.get("evidence"),
                Some(&Value::Str("found it".to_string())),
                "should have evidence field"
            );
        }
        other => panic!("Expected Struct for struct variant, got {other:?}"),
    }
}

#[test]
fn enum_struct_variant_default_multiple_fields() {
    let decls = parse_decls(
        "[r = enum(Success(msg = str, code = int), Failure) := Success(msg = \"ok\", code = 200)]",
    )
    .unwrap();
    let default = decls[0].default_value.as_ref().unwrap();
    match default {
        Value::Struct(map) => {
            assert_eq!(map.get("__kind__"), Some(&Value::Str("Success".into())));
            assert_eq!(map.get("msg"), Some(&Value::Str("ok".into())));
            assert_eq!(map.get("code"), Some(&Value::Int(200)));
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

#[test]
fn enum_struct_variant_const() {
    let decls =
        parse_consts("[RESULT = enum(Success(msg = str), Failure) := Success(msg = \"done\")]")
            .unwrap();
    let default = decls[0].default_value.as_ref().unwrap();
    match default {
        Value::Struct(map) => {
            assert_eq!(map.get("__kind__"), Some(&Value::Str("Success".into())));
            assert_eq!(map.get("msg"), Some(&Value::Str("done".into())));
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

#[test]
fn enum_bare_struct_variant_rejected() {
    // Struct variant without fields → must be rejected.
    let err = parse_decls("[outcome = enum(Confirmed(evidence = str), Rejected) := Confirmed]")
        .unwrap_err();
    assert!(
        err.to_string().contains("strings must be quoted")
            || err.to_string().contains("invalid default"),
        "bare struct variant should be rejected, got: {err}"
    );
}

#[test]
fn enum_unknown_variant_rejected() {
    let err = parse_decls("[status = enum(Active, Paused) := Nonexistent]").unwrap_err();
    assert!(
        err.to_string().contains("invalid default")
            || err.to_string().contains("strings must be quoted"),
        "unknown variant should be rejected, got: {err}"
    );
}

#[test]
fn enum_unit_variant_with_fields_rejected() {
    // Trying to give fields to a unit variant.
    let err = parse_decls("[status = enum(Active, Paused) := Active(x = 1)]").unwrap_err();
    assert!(
        err.to_string().contains("invalid default"),
        "unit variant with fields should be rejected, got: {err}"
    );
}

// =========================================================================
// D2 — enum-variant defaults (bare variant names, all forms)
// =========================================================================

#[test]
fn d2_inline_unit_default() {
    let decls = parse_decls("[s = enum(Design, Build) := Build]").unwrap();
    assert_eq!(decls[0].default_value, Some(Value::Str("Build".into())));
}

#[test]
fn d2_alias_unit_default() {
    let decls = parse_decls_stage("[s = Stage := Build]").unwrap();
    assert_eq!(decls[0].default_value, Some(Value::Str("Build".into())));
}

#[test]
fn d2_struct_variant_default() {
    let decls =
        parse_decls("[o = enum(Confirmed(e = str), Rejected) := Confirmed(e = \"x\")]").unwrap();
    match decls[0].default_value.as_ref().unwrap() {
        Value::Struct(map) => {
            assert_eq!(map.get("__kind__"), Some(&Value::Str("Confirmed".into())));
            assert_eq!(map.get("e"), Some(&Value::Str("x".into())));
        }
        other => panic!("expected struct variant, got {other:?}"),
    }
}

#[test]
fn d2_enum_in_struct_default() {
    let decls = parse_decls_stage("[x = struct(st = Stage) := {st = Build}]").unwrap();
    match decls[0].default_value.as_ref().unwrap() {
        Value::Struct(map) => {
            assert_eq!(map.get("st"), Some(&Value::Str("Build".into())));
        }
        other => panic!("expected struct, got {other:?}"),
    }
}

#[test]
fn d2_enum_in_list_default() {
    let decls = parse_decls_stage("[xs = list(Stage) := [Build, Deploy]]").unwrap();
    match decls[0].default_value.as_ref().unwrap() {
        Value::List(items) => {
            assert_eq!(
                **items,
                vec![Value::Str("Build".into()), Value::Str("Deploy".into())]
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn d2_option_of_enum_default_some() {
    let decls = parse_decls_stage("[o = option(Stage) := Build]").unwrap();
    assert_eq!(decls[0].default_value, Some(Value::Str("Build".into())));
}

#[test]
fn d2_option_of_enum_default_none() {
    let decls = parse_decls_stage("[o = option(Stage) := None]").unwrap();
    assert_eq!(decls[0].default_value, Some(Value::None));
}

#[test]
fn d2_enum_inside_struct_variant_field_default() {
    let decls = parse_decls_stage("[w = enum(Wrap(s = Stage), Empty) := Wrap(s = Build)]").unwrap();
    match decls[0].default_value.as_ref().unwrap() {
        Value::Struct(map) => {
            assert_eq!(map.get("__kind__"), Some(&Value::Str("Wrap".into())));
            assert_eq!(map.get("s"), Some(&Value::Str("Build".into())));
        }
        other => panic!("expected struct variant, got {other:?}"),
    }
}

#[test]
fn d2_qualified_variant_default_rejected_alias() {
    let err = parse_decls_stage("[s = Stage := Stage.Build]").unwrap_err();
    assert!(
        err.to_string().contains("use the bare variant name"),
        "qualified variant default must be rejected with the shared message, got: {err}"
    );
}

#[test]
fn d2_qualified_variant_default_rejected_inline_enum() {
    let err = parse_decls("[s = enum(Design, Build) := MyEnum.Build]").unwrap_err();
    assert!(
        err.to_string().contains("use the bare variant name"),
        "qualified variant default must be rejected, got: {err}"
    );
}

#[test]
fn d2_qualified_variant_default_rejected_option_enum() {
    let err = parse_decls_stage("[o = option(Stage) := Stage.Deploy]").unwrap_err();
    assert!(
        err.to_string().contains("use the bare variant name"),
        "qualified variant default (option) must be rejected, got: {err}"
    );
}

// =========================================================================
// D4 — implicit-typed declaration with a default is a compile error
// =========================================================================

#[test]
fn d4_implicit_typed_param_rejected() {
    let err = parse_decls("[x := \"hello\"]").unwrap_err();
    assert!(
        err.to_string().contains("must have an explicit type"),
        "implicit-typed param must be rejected with the shared message, got: {err}"
    );
}

#[test]
fn d4_implicit_typed_const_rejected() {
    let err = parse_consts("[X := \"hello\"]").unwrap_err();
    assert!(
        err.to_string().contains("must have an explicit type"),
        "implicit-typed constant must be rejected with the shared message, got: {err}"
    );
}

#[test]
fn d4_implicit_typed_param_no_spaces_rejected() {
    let err = parse_decls("[x:=\"hello\"]").unwrap_err();
    assert!(
        err.to_string().contains("must have an explicit type"),
        "implicit-typed param (no spaces) must be rejected, got: {err}"
    );
}
