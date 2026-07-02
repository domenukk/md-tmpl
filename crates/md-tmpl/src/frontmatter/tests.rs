use super::*;
use crate::value::Value;

/// Wrapper for `parse_type_annotation` without aliases — for baseline tests.
fn parse_type_annotation(s: &str) -> Result<VarType, String> {
    let empty_aliases = HashMap::new();
    let empty_imports = HashMap::new();
    super::parse_type_annotation(s, &empty_aliases, &empty_imports)
}

/// Wrapper for `parse_declarations` without aliases — for baseline tests.
fn parse_params_value(rest: &str) -> Result<Vec<VarDecl>, TemplateError> {
    let empty_aliases = HashMap::new();
    let empty_imports = HashMap::new();
    let empty_consts = HashMap::new();
    super::parse_declarations(rest, &empty_aliases, &empty_imports, false, &empty_consts)
}

#[test]
fn parse_empty_source() {
    let err = parse_frontmatter("").unwrap_err();
    assert!(
        err.to_string()
            .contains("missing mandatory YAML frontmatter block")
    );
}

#[test]
fn parse_no_frontmatter() {
    let source = "Hello {{ name }}!";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string()
            .contains("missing mandatory YAML frontmatter block")
    );
}

#[test]
fn parse_basic_frontmatter() {
    let source = r"---
name: greeting
description: A greeting template
params: [name = str, count = int]
---
Hello {{ name }}!";
    let (fm, body) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.name, Some("greeting".to_string()));
    assert_eq!(fm.description, Some("A greeting template".to_string()));
    assert_eq!(fm.params, vec!["name", "count"]);
    assert_eq!(fm.declarations.len(), 2);
    assert_eq!(fm.declarations[0].name, "name");
    assert_eq!(fm.declarations[0].var_type, VarType::Str);
    assert_eq!(fm.declarations[1].name, "count");
    assert_eq!(fm.declarations[1].var_type, VarType::Int);
    assert_eq!(body, "Hello {{ name }}!");
}

#[test]
fn reject_untyped_params() {
    let source = r"---
params: [a, b, c]
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(err.to_string().contains("missing a type annotation"));
}

#[test]
fn parse_multiline_block_format() {
    let source = r"---
name: test
params:
  - a = str
  - b = int
---
{{ a }} {{ b }}";
    let (fm, body) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.params, vec!["a", "b"]);
    assert_eq!(fm.declarations[0].var_type, VarType::Str);
    assert_eq!(fm.declarations[1].var_type, VarType::Int);
    assert_eq!(body, "{{ a }} {{ b }}");
}

#[test]
fn parse_list_with_fields() {
    let source = r"---
params: [items = list(title = str, score = float)]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations.len(), 1);
    assert_eq!(fm.declarations[0].name, "items");
    match &fm.declarations[0].var_type {
        VarType::List(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "title");
            assert_eq!(fields[0].var_type, VarType::Str);
            assert_eq!(fields[1].name, "score");
            assert_eq!(fields[1].var_type, VarType::Float);
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn parse_struct_type() {
    let source = r"---
params: [config = struct(key = str, enabled = bool)]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations.len(), 1);
    match &fm.declarations[0].var_type {
        VarType::Struct(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "key");
            assert_eq!(fields[0].var_type, VarType::Str);
            assert_eq!(fields[1].name, "enabled");
            assert_eq!(fields[1].var_type, VarType::Bool);
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

#[test]
fn reject_bare_list_type() {
    let source = r"---
params: [items = list]
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(err.to_string().contains("unknown type"));
}

#[test]
fn parse_float_type() {
    let source = r"---
params: [score = float]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].var_type, VarType::Float);
}

#[test]
fn parse_bool_type() {
    let source = r"---
params: [active = bool]
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations[0].var_type, VarType::Bool);
}

#[test]
fn reject_unknown_type() {
    let source = r"---
params: [x = unknown_type]
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(err.to_string().contains("unknown type 'unknown_type'"));
}

#[test]
fn reject_mixed_typed_and_untyped() {
    let source = r"---
params: [name = str, label, count = int]
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(err.to_string().contains("missing a type annotation"));
}

#[test]
fn parse_empty_params_list() {
    let source = r"---
params: []
---
body";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert!(fm.declarations.is_empty());
    assert!(fm.params.is_empty());
}

#[test]
fn reject_missing_blank_line_after_block_list() {
    let source = r"---
consts:
  - FOO = str := 'bar'
params:
  - x = int
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string()
            .contains("A blank line is required after a block list"),
        "got: {err}"
    );
}

#[test]
fn reject_missing_blank_line_after_types_block_list() {
    let source = r"---
types:
  - Severity = enum(Low, Medium, High)
consts:
  - FOO = str := 'bar'
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string()
            .contains("A blank line is required after a block list"),
        "got: {err}"
    );
}

#[test]
fn reject_missing_blank_line_after_imports_block_list() {
    let source = r#"---
imports:
  - "[shared](./shared.tmpl.md)"
params:
  - x = int
---
body"#;
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string()
            .contains("A blank line is required after a block list"),
        "got: {err}"
    );
}

#[test]
fn reject_missing_blank_line_before_allow_unused() {
    let source = r"---
consts:
  - FOO = str := 'bar'
allow_unused: true
---
body";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string()
            .contains("A blank line is required after a block list"),
        "got: {err}"
    );
}

#[test]
fn types_only_template_no_params_block() {
    let source = r"---
name: types
types:
  - Priority = enum(High, Medium, Low)
---
{# no body #}";
    let (fm, body) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.name, Some("types".to_string()));
    assert!(fm.declarations.is_empty());
    assert!(fm.params.is_empty());
    assert!(!fm.has_params);
    assert!(fm.type_aliases.contains_key("Priority"));
    assert_eq!(body, "{# no body #}");
}

#[test]
fn frontmatter_not_at_start() {
    let source = "some text\n---\nname: test\n---\nbody";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string()
            .contains("missing mandatory YAML frontmatter block")
    );
}

#[test]
fn frontmatter_without_closing_delimiter() {
    let source = r"---
name: test
no closing delimiter";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(err.to_string().contains("unclosed YAML frontmatter block"));
}

#[test]
fn join_continuation_lines_basic() {
    let block = "key1: val1\nkey2:\n  continued\n  more";
    let lines = join_continuation_lines(block);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "key1: val1");
    assert!(lines[1].contains("continued"));
    assert!(lines[1].contains("more"));
}

#[test]
fn parse_type_annotation_all_simple_types() {
    assert_eq!(parse_type_annotation("str").unwrap(), VarType::Str);
    assert_eq!(parse_type_annotation("bool").unwrap(), VarType::Bool);
    assert_eq!(parse_type_annotation("int").unwrap(), VarType::Int);
    assert_eq!(parse_type_annotation("float").unwrap(), VarType::Float);
    parse_type_annotation("garbage").expect_err("unknown type 'garbage' should be rejected");
    parse_type_annotation("list").expect_err("bare 'list' without <fields> should be rejected");
    parse_type_annotation("struct").expect_err("bare 'struct' without <fields> should be rejected");
}

#[test]
fn parse_type_annotation_with_whitespace() {
    assert_eq!(parse_type_annotation("  str  ").unwrap(), VarType::Str);
    assert_eq!(parse_type_annotation("\tint\t").unwrap(), VarType::Int);
}

#[test]
fn parse_params_complex() {
    let rest = "[name = str, items = list(label = str, count = int), active = bool]";
    let decls = parse_params_value(rest).unwrap();
    assert_eq!(decls.len(), 3);
    assert_eq!(decls[0].name, "name");
    assert_eq!(decls[0].var_type, VarType::Str);
    assert_eq!(decls[2].name, "active");
    assert_eq!(decls[2].var_type, VarType::Bool);
    match &decls[1].var_type {
        VarType::List(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "label");
            assert_eq!(fields[1].name, "count");
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn parse_enum_with_associated_data() {
    let rest = "[outcome = enum(Confirmed(evidence = list(text = str)), Inconclusive)]";
    let decls = parse_params_value(rest).unwrap();
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].name, "outcome");
    match &decls[0].var_type {
        VarType::Enum(variants) => {
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].name, "Confirmed");
            assert_eq!(variants[0].fields.len(), 1);
            assert_eq!(variants[0].fields[0].name, "evidence");
            assert_eq!(variants[1].name, "Inconclusive");
            assert!(variants[1].fields.is_empty());
        }
        other => panic!("Expected Enum, got {other:?}"),
    }
}

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

// -- Type library (allow_unused) tests --

#[test]
fn allow_unused_suppresses_unused_type_alias() {
    let source = "\
---

types:
  - Severity = enum(Low, Medium, High)

params:
  - x = str

allow_unused: true
---
type library";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert!(fm.allow_unused);
    assert!(fm.type_aliases.contains_key("Severity"));
}

#[test]
fn reject_unused_type_alias_without_allow_unused() {
    // Enum types are exempt from R4 (always auto-injected as constants).
    // Use a struct type alias to test the unused check.
    let source = "\
---

types:
  - Config = struct(host = str, port = int)

params:
  - x = str
---
{{ x }}";
    let err = parse_frontmatter(source).unwrap_err();
    assert!(
        err.to_string().contains("unused type alias"),
        "expected unused type alias error, got: {err}"
    );
}

#[test]
fn type_library_with_exported_types_and_params() {
    let source = "\
---

name: types
types:
  - Labelled = enum(Known(label = str), Unknown)
  - Severity = enum(Informational, Low, Medium, High, Critical)

params:
  - tasks = list(title = str, category = Labelled, component = Labelled)
  - post_types = list(tag = str)

allow_unused: true
---
{# type library #}";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.declarations.len(), 2);
    // Labelled is used by tasks param, so it remains in type_aliases.
    assert!(fm.type_aliases.contains_key("Labelled"));
    // Severity is NOT used by any param, but allow_unused suppresses the error.
    // It remains in the explicit type_aliases map.
    assert!(
        fm.type_aliases.contains_key("Severity"),
        "Severity should remain in type_aliases with allow_unused: {:?}",
        fm.type_aliases.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_consts_referencing_previous_consts_in_list() {
    let source = "\
---

name: test_const_ref
consts:
  - SCRATCH = str := \"scratch\"
  - EVIDENCE = str := \"evidence\"
  - DIRS = list(str) := [SCRATCH, EVIDENCE]
---
hello";
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.consts.len(), 3);
    let dirs = fm.consts.iter().find(|d| d.name == "DIRS").unwrap();
    let val = dirs.default_value.as_ref().unwrap();
    match val {
        crate::value::Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], crate::value::Value::Str("scratch".to_string()));
            assert_eq!(items[1], crate::value::Value::Str("evidence".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn test_inline_params_after_imports() {
    let source = r#"---
imports:
  - "[artist](../artist.tmpl.md)"
params: []
---
body"#;
    let res = parse_frontmatter(source);
    assert!(res.is_err());
}

#[test]
fn test_frontmatter_import_interpolation() {
    let source = r#"---
consts:
  - DIR = str := "./shared"

imports:
  - "[header]({{ consts.DIR }}/header.tmpl.md)"

params: []
---
body"#;
    let (fm, _) = parse_frontmatter(source).unwrap();
    assert_eq!(fm.imports.len(), 1);
    assert_eq!(fm.imports[0].stem, "header");
    #[cfg(feature = "std")]
    assert_eq!(fm.imports[0].path, PathBuf::from("./shared/header.tmpl.md"));
    #[cfg(not(feature = "std"))]
    assert_eq!(fm.imports[0].path, "./shared/header.tmpl.md");
}
