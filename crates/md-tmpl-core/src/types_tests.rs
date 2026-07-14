//! Unit tests for [`super`] frontmatter type declarations and validation.

use std::sync::Arc;

use super::*;
use crate::{compat::HashMap, consts::ENUM_TAG_KEY};

// -- Display --

#[test]
fn display_scalar_types() {
    assert_eq!(VarType::Str.to_string(), "str");
    assert_eq!(VarType::Bool.to_string(), "bool");
    assert_eq!(VarType::Int.to_string(), "int");
    assert_eq!(VarType::Float.to_string(), "float");
}

#[test]
fn display_list_with_fields() {
    let var_type = VarType::List(vec![
        VarDecl {
            name: "name".into(),
            var_type: VarType::Str,
            default_value: None,
        },
        VarDecl {
            name: "score".into(),
            var_type: VarType::Int,
            default_value: None,
        },
    ]);
    assert_eq!(var_type.to_string(), "list(name = str, score = int)");
}

#[test]
fn display_struct_with_fields() {
    let var_type = VarType::Struct(vec![VarDecl {
        name: "label".into(),
        var_type: VarType::Str,
        default_value: None,
    }]);
    assert_eq!(var_type.to_string(), "struct(label = str)");
}

// -- matches --

#[test]
fn str_matches_str_only() {
    assert!(VarType::Str.matches(&Value::Str("hello".into())));
    assert!(!VarType::Str.matches(&Value::Bool(true)));
    assert!(!VarType::Str.matches(&Value::Int(1)));
}

#[test]
fn bool_matches_bool_only() {
    assert!(VarType::Bool.matches(&Value::Bool(false)));
    assert!(!VarType::Bool.matches(&Value::Str("true".into())));
}

#[test]
fn int_matches_int_only() {
    assert!(VarType::Int.matches(&Value::Int(42)));
    assert!(!VarType::Int.matches(&Value::Float(42.0)));
}

#[test]
fn float_matches_float_and_int() {
    assert!(VarType::Float.matches(&Value::Float(3.25)));
    // Int is accepted as Float (lossless widening).
    assert!(VarType::Float.matches(&Value::Int(3)));
    assert!(!VarType::Float.matches(&Value::Str("3.0".into())));
}

#[test]
fn list_no_fields_matches_any_list() {
    assert!(VarType::List(vec![]).matches(&Value::List(Arc::new(vec![]))));
    assert!(VarType::List(vec![]).matches(&Value::List(Arc::new(vec![Value::Int(1)]))));
    assert!(!VarType::List(vec![]).matches(&Value::Str("x".into())));
}

#[test]
fn list_with_fields_validates_all_items() {
    let var_type = VarType::List(vec![VarDecl {
        name: "name".into(),
        var_type: VarType::Str,
        default_value: None,
    }]);

    // Empty list passes (nothing to validate).
    assert!(var_type.matches(&Value::List(Arc::new(vec![]))));

    // Single valid item.
    let valid_item = Value::Struct(Arc::new(HashMap::from([(
        "name".into(),
        Value::Str("a".into()),
    )])));
    assert!(var_type.matches(&Value::List(Arc::new(vec![valid_item]))));

    // Missing key in first item.
    let invalid_item = Value::Struct(Arc::new(HashMap::from([("id".into(), Value::Int(1))])));
    assert!(!var_type.matches(&Value::List(Arc::new(vec![invalid_item]))));

    // First item is not a Struct.
    assert!(!var_type.matches(&Value::List(Arc::new(vec![Value::Int(1)]))));
}

#[test]
fn list_with_fields_rejects_wrong_value_type() {
    let var_type = VarType::List(vec![VarDecl {
        name: "name".into(),
        var_type: VarType::Str,
        default_value: None,
    }]);

    // Key present but wrong type (Int instead of Str).
    let wrong_type = Value::Struct(Arc::new(HashMap::from([("name".into(), Value::Int(42))])));
    assert!(
        !var_type.matches(&Value::List(Arc::new(vec![wrong_type]))),
        "should reject list item where 'name' is int, not str"
    );
}

#[test]
fn list_validates_all_items_not_just_first() {
    let var_type = VarType::List(vec![VarDecl {
        name: "name".into(),
        var_type: VarType::Str,
        default_value: None,
    }]);

    let good = Value::Struct(Arc::new(HashMap::from([(
        "name".into(),
        Value::Str("ok".into()),
    )])));
    let bad = Value::Struct(Arc::new(HashMap::from([("name".into(), Value::Int(99))])));

    // First item good, second bad → reject.
    assert!(
        !var_type.matches(&Value::List(Arc::new(vec![good.clone(), bad]))),
        "should validate ALL items, not just the first"
    );

    // Both good → accept.
    assert!(var_type.matches(&Value::List(Arc::new(vec![good.clone(), good]))));
}

#[test]
fn struct_validates_required_keys_and_types() {
    let var_type = VarType::Struct(vec![
        VarDecl {
            name: "title".into(),
            var_type: VarType::Str,
            default_value: None,
        },
        VarDecl {
            name: "count".into(),
            var_type: VarType::Int,
            default_value: None,
        },
    ]);

    let valid = Value::Struct(Arc::new(HashMap::from([
        ("title".into(), Value::Str("task".into())),
        ("count".into(), Value::Int(5)),
    ])));
    assert!(var_type.matches(&valid));

    // Missing "count".
    let missing_field = Value::Struct(Arc::new(HashMap::from([(
        "title".into(),
        Value::Str("task".into()),
    )])));
    assert!(!var_type.matches(&missing_field));

    // Not a dict at all.
    assert!(!var_type.matches(&Value::Str("oops".into())));
}

#[test]
fn struct_rejects_wrong_field_type() {
    let var_type = VarType::Struct(vec![VarDecl {
        name: "count".into(),
        var_type: VarType::Int,
        default_value: None,
    }]);

    // Key present but wrong type (Str instead of Int).
    let wrong = Value::Struct(Arc::new(HashMap::from([(
        "count".into(),
        Value::Str("five".into()),
    )])));
    assert!(
        !var_type.matches(&wrong),
        "should reject struct where 'count' is str, not int"
    );
}

#[test]
fn struct_nested_type_checking() {
    // struct(meta = struct(version = int))
    let var_type = VarType::Struct(vec![VarDecl {
        name: "meta".into(),
        var_type: VarType::Struct(vec![VarDecl {
            name: "version".into(),
            var_type: VarType::Int,
            default_value: None,
        }]),
        default_value: None,
    }]);

    let valid = Value::Struct(Arc::new(HashMap::from([(
        "meta".into(),
        Value::Struct(Arc::new(HashMap::from([("version".into(), Value::Int(3))]))),
    )])));
    assert!(var_type.matches(&valid));

    // Nested field wrong type.
    let wrong = Value::Struct(Arc::new(HashMap::from([(
        "meta".into(),
        Value::Struct(Arc::new(HashMap::from([(
            "version".into(),
            Value::Str("3".into()),
        )]))),
    )])));
    assert!(
        !var_type.matches(&wrong),
        "should recursively check nested struct field types"
    );
}

#[test]
fn struct_no_fields_matches_any_dict() {
    assert!(VarType::Struct(vec![]).matches(&Value::Struct(Arc::new(HashMap::new()))));
    assert!(!VarType::Struct(vec![]).matches(&Value::List(Arc::new(vec![]))));
}

#[test]
fn display_enum_with_fields() {
    let var_type = VarType::Enum(vec![
        VariantDecl {
            name: "Confirmed".into(),
            fields: vec![VarDecl {
                name: "evidence".into(),
                var_type: VarType::Str,
                default_value: None,
            }],
        },
        VariantDecl {
            name: "Inconclusive".into(),
            fields: vec![],
        },
    ]);
    assert_eq!(
        var_type.to_string(),
        "enum(Confirmed(evidence = str), Inconclusive)"
    );
}

#[test]
fn enum_matches_validation() {
    let var_type = VarType::Enum(vec![
        VariantDecl {
            name: "Confirmed".into(),
            fields: vec![VarDecl {
                name: "evidence".into(),
                var_type: VarType::Str,
                default_value: None,
            }],
        },
        VariantDecl {
            name: "Inconclusive".into(),
            fields: vec![],
        },
    ]);

    // String value matching unit variant
    assert!(var_type.matches(&Value::Str("Inconclusive".into())));
    assert!(!var_type.matches(&Value::Str("Confirmed".into())));

    // Internally tagged dict matching struct variant
    let valid_dict = Value::Struct(Arc::new(HashMap::from([
        (ENUM_TAG_KEY.into(), Value::Str("Confirmed".into())),
        ("evidence".into(), Value::Str("some evidence".into())),
    ])));
    assert!(var_type.matches(&valid_dict));

    // Missing required field
    let missing_field = Value::Struct(Arc::new(HashMap::from([(
        ENUM_TAG_KEY.into(),
        Value::Str("Confirmed".into()),
    )])));
    assert!(!var_type.matches(&missing_field));

    // Invalid variant name
    let invalid_variant = Value::Struct(Arc::new(HashMap::from([(
        ENUM_TAG_KEY.into(),
        Value::Str("Unknown".into()),
    )])));
    assert!(!var_type.matches(&invalid_variant));
}

#[test]
fn enum_rejects_wrong_field_type() {
    let var_type = VarType::Enum(vec![VariantDecl {
        name: "Confirmed".into(),
        fields: vec![VarDecl {
            name: "evidence".into(),
            var_type: VarType::Str,
            default_value: None,
        }],
    }]);

    // Field present but wrong type (Int instead of Str).
    let wrong = Value::Struct(Arc::new(HashMap::from([
        (ENUM_TAG_KEY.into(), Value::Str("Confirmed".into())),
        ("evidence".into(), Value::Int(42)),
    ])));
    assert!(
        !var_type.matches(&wrong),
        "should reject enum variant where 'evidence' is int, not str"
    );
}

// -- check() path diagnostics --

#[test]
fn check_scalar_error_has_empty_path() {
    let err = VarType::Int.check(&Value::Str("oops".into())).unwrap_err();
    assert!(
        err.path.is_empty(),
        "scalar mismatch should have empty path"
    );
    assert_eq!(err.expected, "int");
    assert_eq!(err.actual, "str");
}

#[test]
fn check_list_item_field_path() {
    let var_type = VarType::List(vec![VarDecl {
        name: "score".into(),
        var_type: VarType::Int,
        default_value: None,
    }]);
    // Second item has wrong type for score.
    let items = Value::List(Arc::new(vec![
        Value::Struct(Arc::new(HashMap::from([("score".into(), Value::Int(10))]))),
        Value::Struct(Arc::new(HashMap::from([(
            "score".into(),
            Value::Str("bad".into()),
        )]))),
    ]));
    let err = var_type.check(&items).unwrap_err();
    assert_eq!(err.path, "[1].score", "should point to items[1].score");
    assert_eq!(err.expected, "int");
}

#[test]
fn check_dict_missing_field_path() {
    let var_type = VarType::Struct(vec![VarDecl {
        name: "title".into(),
        var_type: VarType::Str,
        default_value: None,
    }]);
    let value = Value::Struct(Arc::new(HashMap::new())); // missing 'title'
    let err = var_type.check(&value).unwrap_err();
    assert_eq!(err.path, "title");
    assert_eq!(err.actual, "missing");
}

#[test]
fn check_nested_dict_path() {
    let var_type = VarType::Struct(vec![VarDecl {
        name: "meta".into(),
        var_type: VarType::Struct(vec![VarDecl {
            name: "version".into(),
            var_type: VarType::Int,
            default_value: None,
        }]),
        default_value: None,
    }]);
    let value = Value::Struct(Arc::new(HashMap::from([(
        "meta".into(),
        Value::Struct(Arc::new(HashMap::from([(
            "version".into(),
            Value::Str("3".into()),
        )]))),
    )])));
    let err = var_type.check(&value).unwrap_err();
    assert_eq!(err.path, "meta.version", "should show nested path");
}

#[test]
fn check_enum_invalid_tag_path() {
    let var_type = VarType::Enum(vec![VariantDecl {
        name: "Confirmed".into(),
        fields: vec![],
    }]);
    let value = Value::Struct(Arc::new(HashMap::from([(
        ENUM_TAG_KEY.into(),
        Value::Str("Unknown".into()),
    )])));
    let err = var_type.check(&value).unwrap_err();
    assert_eq!(err.path, format!(".{ENUM_TAG_KEY}"));
}

#[test]
fn check_display_with_path() {
    let err = TypeCheckError {
        path: "tasks[2].title".into(),
        expected: "str".into(),
        actual: "int".into(),
        actual_value: "42".into(),
    };
    assert_eq!(
        err.to_string(),
        "at 'tasks[2].title': expected str, got int (42)"
    );
}

#[test]
fn check_display_empty_path() {
    let err = TypeCheckError {
        path: String::new(),
        expected: "str".into(),
        actual: "int".into(),
        actual_value: "42".into(),
    };
    assert_eq!(err.to_string(), "expected str, got int (42)");
}

// -- to_pascal_case tests -------------------------------------------------

#[test]
fn pascal_case_snake_case() {
    assert_eq!(super::to_pascal_case("code_review"), "CodeReview");
    assert_eq!(super::to_pascal_case("simple_greeting"), "SimpleGreeting");
}

#[test]
fn pascal_case_kebab_case() {
    assert_eq!(super::to_pascal_case("task-report"), "TaskReport");
}

#[test]
fn pascal_case_single_word() {
    assert_eq!(super::to_pascal_case("single"), "Single");
}

#[test]
fn pascal_case_empty() {
    assert_eq!(super::to_pascal_case(""), "");
}

#[test]
fn pascal_case_mixed() {
    assert_eq!(
        super::to_pascal_case("already_PascalCase"),
        "AlreadyPascalCase"
    );
}

#[test]
fn pascal_case_leading_trailing_separators() {
    assert_eq!(super::to_pascal_case("_leading"), "Leading");
    assert_eq!(super::to_pascal_case("trailing_"), "Trailing");
    assert_eq!(super::to_pascal_case("__double__"), "Double");
}

// -- BUILTIN_TYPE_NAMES tests ---------------------------------------------

#[test]
fn builtin_type_names_contains_all_expected() {
    for name in &[
        "str", "bool", "int", "float", "list", "struct", "enum", "option",
    ] {
        assert!(
            super::BUILTIN_TYPE_NAMES.contains(name),
            "BUILTIN_TYPE_NAMES should contain '{name}'"
        );
    }
}

// -- Structural (duck) typing: extra fields silently accepted ----------

#[test]
fn struct_accepts_extra_fields() {
    // struct(title = str) should accept {title: "x", extra: 42}
    let var_type = VarType::Struct(vec![VarDecl {
        name: "title".into(),
        var_type: VarType::Str,
        default_value: None,
    }]);

    let value = Value::Struct(Arc::new(HashMap::from([
        ("title".into(), Value::Str("hello".into())),
        ("extra_field".into(), Value::Int(42)),
        ("another".into(), Value::Bool(true)),
    ])));
    assert!(
        var_type.matches(&value),
        "struct with extra fields should match (duck typing)"
    );
    // Also confirm check() returns Ok
    assert!(var_type.check(&value).is_ok());
}

#[test]
fn list_items_accept_extra_fields() {
    // list(name = str) should accept items with extra fields
    let var_type = VarType::List(vec![VarDecl {
        name: "name".into(),
        var_type: VarType::Str,
        default_value: None,
    }]);

    let item = Value::Struct(Arc::new(HashMap::from([
        ("name".into(), Value::Str("Alice".into())),
        ("age".into(), Value::Int(30)),
        ("email".into(), Value::Str("alice@example.com".into())),
    ])));
    assert!(
        var_type.matches(&Value::List(Arc::new(vec![item]))),
        "list items with extra fields should match (duck typing)"
    );
}

#[test]
fn nested_struct_accepts_extra_fields_at_every_depth() {
    // struct(meta = struct(version = int)) — extra fields at both levels
    let var_type = VarType::Struct(vec![VarDecl {
        name: "meta".into(),
        var_type: VarType::Struct(vec![VarDecl {
            name: "version".into(),
            var_type: VarType::Int,
            default_value: None,
        }]),
        default_value: None,
    }]);

    let value = Value::Struct(Arc::new(HashMap::from([
        (
            "meta".into(),
            Value::Struct(Arc::new(HashMap::from([
                ("version".into(), Value::Int(3)),
                ("build_hash".into(), Value::Str("abc123".into())),
            ]))),
        ),
        ("top_level_extra".into(), Value::Bool(false)),
    ])));
    assert!(
        var_type.matches(&value),
        "nested structs should accept extra fields at every depth"
    );
}

#[test]
fn enum_struct_variant_accepts_extra_fields() {
    // enum(Confirmed(evidence = str), Inconclusive)
    let var_type = VarType::Enum(vec![
        VariantDecl {
            name: "Confirmed".into(),
            fields: vec![VarDecl {
                name: "evidence".into(),
                var_type: VarType::Str,
                default_value: None,
            }],
        },
        VariantDecl {
            name: "Inconclusive".into(),
            fields: vec![],
        },
    ]);

    // Struct variant with extra fields
    let value = Value::Struct(Arc::new(HashMap::from([
        (ENUM_TAG_KEY.into(), Value::Str("Confirmed".into())),
        ("evidence".into(), Value::Str("found it".into())),
        ("debug_info".into(), Value::Str("extra debug".into())),
    ])));
    assert!(
        var_type.matches(&value),
        "enum struct variant with extra fields should match"
    );
}

#[test]
fn struct_still_rejects_missing_required_fields() {
    // Confirm duck typing doesn't weaken required-field checking
    let var_type = VarType::Struct(vec![
        VarDecl {
            name: "title".into(),
            var_type: VarType::Str,
            default_value: None,
        },
        VarDecl {
            name: "count".into(),
            var_type: VarType::Int,
            default_value: None,
        },
    ]);

    // Has extra fields but missing "count"
    let value = Value::Struct(Arc::new(HashMap::from([
        ("title".into(), Value::Str("hello".into())),
        ("bonus".into(), Value::Int(99)),
    ])));
    assert!(
        !var_type.matches(&value),
        "extra fields don't compensate for missing required fields"
    );
}

#[test]
fn struct_still_rejects_wrong_types_even_with_extras() {
    let var_type = VarType::Struct(vec![VarDecl {
        name: "count".into(),
        var_type: VarType::Int,
        default_value: None,
    }]);

    // Has the field but wrong type, plus extras
    let value = Value::Struct(Arc::new(HashMap::from([
        ("count".into(), Value::Str("not a number".into())),
        ("extra".into(), Value::Int(42)),
    ])));
    assert!(
        !var_type.matches(&value),
        "extra fields don't fix type mismatches"
    );
}
