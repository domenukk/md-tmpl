//! Frontmatter default value parsing and validation tests.

use super::*;
use crate::value::Value;

// -- Default value tests --

#[test]
fn parse_string_default() {
    let source = r#"---
params: [name = str := "hello world"]
---
body"#;
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].name, "name");
    assert_eq!(fm.declarations[0].var_type, VarType::Str);
    assert_eq!(
        fm.declarations[0].default_value,
        Some(Value::Str("hello world".to_string()))
    );
}

#[test]
fn parse_int_default() {
    let source = r"---
params: [count = int := 42]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].var_type, VarType::Int);
    assert_eq!(fm.declarations[0].default_value, Some(Value::Int(42)));
}

#[test]
fn parse_bool_default() {
    let source = r"---
params: [active = bool := true]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].var_type, VarType::Bool);
    assert_eq!(fm.declarations[0].default_value, Some(Value::Bool(true)));
}

#[test]
fn parse_float_default() {
    let source = r"---
params: [score = float := 3.15]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].var_type, VarType::Float);
    assert_eq!(fm.declarations[0].default_value, Some(Value::Float(3.15)));
}

#[test]
fn parse_mixed_defaults_and_required() {
    let source = r"---
params: [name = str, count = int := 10]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].default_value, None);
    assert_eq!(fm.declarations[1].default_value, Some(Value::Int(10)));
}

#[test]
fn default_does_not_confuse_with_inner_colons() {
    // The `:=` inside `<>` should not be treated as a default separator.
    // This is handled by find_assign_default_at_depth_zero.
    let source = r"---
params: [tasks = list(title = str)]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].default_value, None);
    match &fm.declarations[0].var_type {
        VarType::List(fields) => {
            assert_eq!(fields[0].name, "title");
            assert_eq!(fields[0].var_type, VarType::Str);
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn parse_default_value_types() {
    assert_eq!(
        parse_default_value("\"hello\""),
        Some(Value::Str("hello".to_string()))
    );
    assert_eq!(
        parse_default_value("'world'"),
        Some(Value::Str("world".to_string()))
    );
    assert_eq!(parse_default_value("42"), Some(Value::Int(42)));
    assert_eq!(parse_default_value("-1"), Some(Value::Int(-1)));
    assert_eq!(parse_default_value("3.15"), Some(Value::Float(3.15)));
    assert_eq!(parse_default_value("true"), Some(Value::Bool(true)));
    assert_eq!(parse_default_value("false"), Some(Value::Bool(false)));
    assert_eq!(parse_default_value(""), None);
}

#[test]
fn parse_block_format_with_defaults() {
    let source = r#"---
params:
  - name = str
  - count = int := 5
  - label = str := "default"
---
body"#;
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations.len(), 3);
    assert_eq!(fm.declarations[0].default_value, None);
    assert_eq!(fm.declarations[1].default_value, Some(Value::Int(5)));
    assert_eq!(
        fm.declarations[2].default_value,
        Some(Value::Str("default".to_string()))
    );
}

#[test]
fn parse_nested_types() {
    let source = r"---
params: [data = list(item = struct(name = str, tags = list(label = str)))]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    match &fm.declarations[0].var_type {
        VarType::List(fields) => {
            assert_eq!(fields[0].name, "item");
            match &fields[0].var_type {
                VarType::Struct(struct_fields) => {
                    assert_eq!(struct_fields[0].name, "name");
                    assert_eq!(struct_fields[0].var_type, VarType::Str);
                    match &struct_fields[1].var_type {
                        VarType::List(inner) => {
                            assert_eq!(inner[0].name, "label");
                            assert_eq!(inner[0].var_type, VarType::Str);
                        }
                        other => panic!("Expected inner List, got {other:?}"),
                    }
                }
                other => panic!("Expected Struct, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn default_value_accessor() {
    let decl = VarDecl {
        name: "test".to_string(),
        var_type: VarType::Str,
        default_value: Some(Value::Str("hello".to_string())),
    };
    assert_eq!(decl.default_value(), Some(&Value::Str("hello".to_string())));

    let no_default = VarDecl {
        name: "test".to_string(),
        var_type: VarType::Int,
        default_value: None,
    };
    assert_eq!(no_default.default_value(), None);
}

// -- Strict default type validation --

#[test]
fn reject_int_default_for_str_type() {
    let source = r"---
params: [name = str := 42]
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string().contains("value has type"),
        "expected type mismatch error, got: {err}"
    );
}

#[test]
fn reject_str_default_for_int_type() {
    let source = r#"---
params: [count = int := "hello"]
---
body"#;
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string().contains("value has type"),
        "expected type mismatch error, got: {err}"
    );
}

#[test]
fn reject_bool_default_for_float_type() {
    let source = r"---
params: [score = float := true]
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string().contains("value has type"),
        "expected type mismatch error, got: {err}"
    );
}

#[test]
fn reject_float_default_for_bool_type() {
    let source = r"---
params: [active = bool := 3.15]
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string().contains("value has type"),
        "expected type mismatch error, got: {err}"
    );
}

#[test]
fn accept_matching_int_default() {
    let source = r"---
params: [count = int := 0]
---
{{ count }}";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].default_value, Some(Value::Int(0)));
}

#[test]
fn accept_matching_str_default() {
    let source = r#"---
params: [name = str := "hi"]
---
{{ name }}"#;
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(
        fm.declarations[0].default_value,
        Some(Value::Str("hi".to_string()))
    );
}

#[test]
fn accept_matching_bool_default() {
    let source = r"---
params: [active = bool := false]
---
{{ active }}";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].default_value, Some(Value::Bool(false)));
}

#[test]
fn accept_matching_float_default() {
    let source = r"---
params: [score = float := -1.5]
---
{{ score }}";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].default_value, Some(Value::Float(-1.5)));
}

#[test]
fn reject_negative_int_for_str() {
    let source = r"---
params: [label = str := -99]
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(err.to_string().contains("value has type"));
}
