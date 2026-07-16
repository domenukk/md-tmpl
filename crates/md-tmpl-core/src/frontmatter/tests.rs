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
        .map(|(decls, _)| decls)
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
fn empty_frontmatter_block_declares_nothing() {
    let source = r"---
---
body";
    let (fm, body) = parse_frontmatter(source).unwrap();
    assert!(fm.declarations.is_empty());
    assert!(!fm.has_params);
    assert_eq!(body, "body");
}

#[test]
fn empty_frontmatter_block_with_blank_lines() {
    let source = r"---

---
body";
    let (fm, body) = parse_frontmatter(source).unwrap();
    assert!(fm.declarations.is_empty());
    assert_eq!(body, "body");
}

#[test]
fn empty_frontmatter_block_crlf() {
    let source = r"---
---
body"
        .replace('\n', "\r\n");
    let (fm, body) = parse_frontmatter(&source).unwrap();
    assert!(fm.declarations.is_empty());
    assert_eq!(body, "body");
}

#[test]
fn empty_frontmatter_block_at_eof() {
    let source = r"---
---";
    let (fm, body) = parse_frontmatter(source).unwrap();
    assert!(fm.declarations.is_empty());
    assert_eq!(body, "");
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

// =========================================================================
// Regression: delimiters inside quoted strings in frontmatter defaults
//
// Commas and the bracket family `()[]{}<>` that appear inside a `"..."` or
// `'...'` string literal in a `:= ...` default value must be treated as
// literal characters, not as element/field separators. These end-to-end
// tests parse a full template via the public `parse_frontmatter` API and
// assert the resulting default `Value`s (matrix cases T1–T8).
// =========================================================================

/// Helper: extract the sole declaration's default value from a template whose
/// `params:` line is `params: [<decl>]`.
fn default_of(decl: &str) -> Value {
    let source = format!("---\nparams: [{decl}]\n---\nbody");
    let (fm, _) = parse_frontmatter(&source)
        .unwrap_or_else(|e| panic!("failed to parse frontmatter for `{decl}`: {e}"));
    fm.declarations[0]
        .default_value
        .clone()
        .unwrap_or_else(|| panic!("declaration `{decl}` had no default value"))
}

#[test]
fn t1_list_of_str_commas_in_double_quotes() {
    // T1: `list(str) := ["a, b", "c, d"]` → 2 items, commas preserved.
    match default_of(r#"x = list(str) := ["a, b", "c, d"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 2, "commas inside quotes must not split items");
            assert_eq!(items[0], Value::Str("a, b".to_string()));
            assert_eq!(items[1], Value::Str("c, d".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t2_struct_field_with_comma_in_quotes() {
    // T2: `struct(msg = str, n = int) := {msg = "a, b", n = 1}`.
    match default_of(r#"x = struct(msg = str, n = int) := {msg = "a, b", n = 1}"#) {
        Value::Struct(map) => {
            assert_eq!(map.get("msg"), Some(&Value::Str("a, b".to_string())));
            assert_eq!(map.get("n"), Some(&Value::Int(1)));
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

#[test]
fn t3_list_of_records_note_field_with_commas() {
    // T3: list of records; first record's `note` holds a comma-laden string.
    match default_of(
        r#"x = list(name = str, note = str) := [{name = "x", note = "p, q, r"}, {name = "y", note = "s"}]"#,
    ) {
        Value::List(items) => {
            assert_eq!(items.len(), 2, "record separators must be top-level only");
            match &items[0] {
                Value::Struct(m) => {
                    assert_eq!(m.get("name"), Some(&Value::Str("x".to_string())));
                    assert_eq!(m.get("note"), Some(&Value::Str("p, q, r".to_string())));
                }
                other => panic!("Expected record 0 Struct, got {other:?}"),
            }
            match &items[1] {
                Value::Struct(m) => {
                    assert_eq!(m.get("name"), Some(&Value::Str("y".to_string())));
                    assert_eq!(m.get("note"), Some(&Value::Str("s".to_string())));
                }
                other => panic!("Expected record 1 Struct, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t4_list_of_str_brackets_in_quotes() {
    // T4: brackets/braces/parens inside quoted list elements stay intact.
    match default_of(r#"x = list(str) := ["a[b]c", "d{e}f", "g(h)i"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::Str("a[b]c".to_string()));
            assert_eq!(items[1], Value::Str("d{e}f".to_string()));
            assert_eq!(items[2], Value::Str("g(h)i".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t5_list_of_str_single_quotes_with_comma() {
    // T5: single-quoted elements with an embedded comma.
    match default_of("x = list(str) := ['a, b', 'c']") {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("a, b".to_string()));
            assert_eq!(items[1], Value::Str("c".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t6_list_of_str_unicode_em_dash_and_emoji() {
    // T6: multi-byte unicode (em-dash, emoji) intact, and a comma inside a
    // quoted element with unicode must not split it.
    match default_of("x = list(str) := [\"Theory — not a finding\", \"✅ done, ok\"]") {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("Theory — not a finding".to_string()));
            assert_eq!(items[1], Value::Str("✅ done, ok".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t7_list_of_records_with_empty_strings() {
    // T7: empty quoted strings parse, and a comma inside a later field stays.
    match default_of(
        r#"x = list(name = str, note = str) := [{name = "", note = ""}, {name = "a", note = "y, z"}]"#,
    ) {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            match &items[0] {
                Value::Struct(m) => {
                    assert_eq!(m.get("name"), Some(&Value::Str(String::new())));
                    assert_eq!(m.get("note"), Some(&Value::Str(String::new())));
                }
                other => panic!("Expected record 0 Struct, got {other:?}"),
            }
            match &items[1] {
                Value::Struct(m) => {
                    assert_eq!(m.get("name"), Some(&Value::Str("a".to_string())));
                    assert_eq!(m.get("note"), Some(&Value::Str("y, z".to_string())));
                }
                other => panic!("Expected record 1 Struct, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t8_severity_ladder_shape_prose_with_delimiters_intact() {
    // T8: real-world SEVERITY_LADDER shape — a list of multi-field records
    // whose prose fields contain commas, em-dashes, and emoji. The
    // comma-bearing prose field must survive parsing intact.
    let default = default_of(
        "ladder = list(tier = str, short = str, proves = str) := \
         [{tier = \"L1\", short = \"Memory safety\", \
         proves = \"Crash, panic, or sanitizer report — reliably reproduced\"}, \
         {tier = \"L2\", short = \"Logic bug\", \
         proves = \"Incorrect output ✅, exploitable\"}]",
    );
    match default {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            match &items[0] {
                Value::Struct(m) => {
                    assert_eq!(m.get("tier"), Some(&Value::Str("L1".to_string())));
                    assert_eq!(
                        m.get("short"),
                        Some(&Value::Str("Memory safety".to_string()))
                    );
                    assert_eq!(
                        m.get("proves"),
                        Some(&Value::Str(
                            "Crash, panic, or sanitizer report — reliably reproduced".to_string()
                        )),
                        "comma/em-dash prose must be intact",
                    );
                }
                other => panic!("Expected record 0 Struct, got {other:?}"),
            }
            match &items[1] {
                Value::Struct(m) => {
                    assert_eq!(m.get("tier"), Some(&Value::Str("L2".to_string())));
                    assert_eq!(
                        m.get("proves"),
                        Some(&Value::Str("Incorrect output ✅, exploitable".to_string())),
                        "emoji + comma prose must be intact",
                    );
                }
                other => panic!("Expected record 1 Struct, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

// =========================================================================
// Regression: additional delimiter/quoting edge cases (E1–E5)
// =========================================================================

#[test]
fn e1_double_quoted_string_with_apostrophe_and_comma() {
    // E1: an apostrophe inside a double-quoted element must not end the quote,
    // so the embedded comma stays literal.
    match default_of(r#"x = list(str) := ["it's, fine", "ok"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("it's, fine".to_string()));
            assert_eq!(items[1], Value::Str("ok".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn e2_single_quoted_string_with_double_quotes_and_comma() {
    // E2: double quotes inside a single-quoted element are literal, so the
    // embedded comma is preserved.
    match default_of(r#"x = list(str) := ['say "hi", bye', "z"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("say \"hi\", bye".to_string()));
            assert_eq!(items[1], Value::Str("z".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn e3_nested_list_of_lists_with_embedded_commas() {
    // E3: list(list(str)) — commas inside quoted innermost elements must not
    // split the inner lists.
    match default_of(r#"x = list(list(str)) := [["a, b"], ["c, d", "e"]]"#) {
        Value::List(outer) => {
            assert_eq!(outer.len(), 2);
            match &outer[0] {
                Value::List(inner) => {
                    assert_eq!(inner.len(), 1);
                    assert_eq!(inner[0], Value::Str("a, b".to_string()));
                }
                other => panic!("Expected inner list 0, got {other:?}"),
            }
            match &outer[1] {
                Value::List(inner) => {
                    assert_eq!(inner.len(), 2);
                    assert_eq!(inner[0], Value::Str("c, d".to_string()));
                    assert_eq!(inner[1], Value::Str("e".to_string()));
                }
                other => panic!("Expected inner list 1, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn e4_whitespace_inside_quotes_preserved() {
    // E4: leading/trailing spaces inside a quoted string are part of the value
    // and must NOT be trimmed.
    match default_of(r#"x = list(str) := [" a, b "]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0], Value::Str(" a, b ".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

// =========================================================================
// Escape-sequence matrix (S1–S5): backslash escapes in string literals.
// See scratch/md_tmpl_escapes_spec.md. Only `\\`, `\"`, `\'` are unescaped;
// every other `\X` is preserved verbatim (backward-compatible).
// =========================================================================

#[test]
fn s1_escaped_both_quote_types() {
    // S1: escaped double quotes inside a double-quoted literal, plus a bare
    // apostrophe, all decode to the literal characters.
    assert_eq!(
        default_of(r#"x = str := "he said \"hi\" it's ok""#),
        Value::Str(r#"he said "hi" it's ok"#.to_string()),
    );
}

#[test]
fn s2_escaped_single_quote_in_single_quoted() {
    // S2: `\'` inside a single-quoted literal decodes to `'`; the embedded
    // comma stays literal because it is inside the (still-open) quote.
    assert_eq!(
        default_of(r"x = str := 'it\'s, fine'"),
        Value::Str("it's, fine".to_string()),
    );
}

#[test]
fn s3_escaped_backslash() {
    // S3: `\\` decodes to a single backslash.
    assert_eq!(
        default_of(r#"x = str := "a\\b""#),
        Value::Str("a\\b".to_string()),
    );
}

#[test]
fn s4_backslash_before_delimiter_does_not_split_list() {
    // S4: an escaped quote (`\"`) must not close the string, so the following
    // comma stays inside the first element instead of splitting the list.
    match default_of(r#"x = list(str) := ["a\", b", "c"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 2, "escaped quote must not split the list");
            assert_eq!(items[0], Value::Str(r#"a", b"#.to_string()));
            assert_eq!(items[1], Value::Str("c".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn s5_unknown_escape_preserved_verbatim() {
    // S5: unrecognized escapes (`\p`, `\n`) keep both the backslash and the
    // following char — this pass does NOT interpret C-style whitespace escapes.
    assert_eq!(
        default_of(r#"x = str := "c:\path\n""#),
        Value::Str("c:\\path\\n".to_string()),
    );
}

// =========================================================================
// YAML-consistent `#` comments (C1–C4) + cross-validation against serde_yaml.
// md-tmpl frontmatter list-item scalars must be extracted identically to how a
// real YAML parser extracts them. See scratch/md_tmpl_escapes_spec.md §Change 2.
// =========================================================================

/// Extract md-tmpl's logical scalar for a single block list item `- {item}`,
/// mirroring the full frontmatter pipeline: inline-comment stripping
/// (`join_continuation_lines`) followed by outer YAML-quote removal +
/// unescaping (as `parse_declarations` does).
fn mdtmpl_list_scalar(item: &str) -> String {
    let block = format!("- {item}");
    let logical = crate::frontmatter::params::join_continuation_lines(&block);
    let line = logical
        .first()
        .expect("md-tmpl produced no logical line for list item");
    let scalar = line
        .strip_prefix("- ")
        .expect("logical line should retain its list marker")
        .trim();
    match crate::consts::strip_string_literal(scalar) {
        Some(inner) => crate::consts::unescape_string_literal(inner),
        None => scalar.to_string(),
    }
}

/// Extract the scalar a real YAML parser sees for `- {item}`.
fn yaml_list_scalar(item: &str) -> String {
    let doc = format!("- {item}");
    let seq: Vec<String> =
        serde_yaml::from_str(&doc).expect("serde_yaml failed to parse the sequence");
    seq.into_iter()
        .next()
        .expect("serde_yaml produced an empty sequence")
}

#[test]
fn cross_validation_matches_serde_yaml_scalar_extraction() {
    // A corpus covering: plain commas/brackets (unchanged), an inline ` #`
    // comment (stripped), a `#` with no leading space (kept), and an
    // outer-quoted decl that protects its `#`. md-tmpl must agree with YAML
    // on every one.
    let corpus = [
        // Plain scalar with a comma — no comment, unchanged.
        "a = str := hello, world",
        // Plain scalar with brackets and a comma — unchanged.
        "x = list(str) := [a, b]",
        // Inline comment: ` #` preceded by whitespace is stripped.
        "x = int := 3 # the retry count",
        // `#` with no leading whitespace is a literal character, kept.
        "x = str := a#b,c",
        // Outer YAML double-quoted scalar protects the inner `#`.
        r#""x = str := \"a # b, c\"""#,
    ];
    for item in corpus {
        assert_eq!(
            mdtmpl_list_scalar(item),
            yaml_list_scalar(item),
            "md-tmpl and serde_yaml disagree on scalar extraction for `{item}`",
        );
    }
}

#[test]
fn c1_space_hash_truncates_like_yaml() {
    // C1: ` #` (hash preceded by whitespace) starts a YAML comment even inside
    // what looks like a quoted md-tmpl literal — the scalar is truncated at the
    // comment. md-tmpl must extract exactly what YAML extracts.
    let item = r#"x = str := "a # b""#;
    assert_eq!(mdtmpl_list_scalar(item), yaml_list_scalar(item));
    assert_eq!(mdtmpl_list_scalar(item), r#"x = str := "a"#);
}

#[test]
fn c2_hash_without_leading_space_is_literal() {
    // C2: a `#` not preceded by whitespace is an ordinary character and is
    // preserved by both md-tmpl and YAML.
    let item = r#"x = str := "a#b,c""#;
    assert_eq!(mdtmpl_list_scalar(item), yaml_list_scalar(item));
    assert_eq!(mdtmpl_list_scalar(item), r#"x = str := "a#b,c""#);
}

#[test]
fn c3_outer_yaml_quotes_protect_hash() {
    // C3: an outer YAML double-quoted scalar protects an inner `#`, so the full
    // md-tmpl declaration (including `# b, c`) is recovered intact.
    let item = r#""x = str := \"a # b, c\"""#;
    assert_eq!(mdtmpl_list_scalar(item), yaml_list_scalar(item));
    assert_eq!(mdtmpl_list_scalar(item), r#"x = str := "a # b, c""#);
}

#[test]
fn c4_trailing_comment_stripped_and_default_parses() {
    // C4: a trailing explanatory comment is stripped so the numeric default
    // still parses to its value.
    let source = "---\nparams:\n  - x = int := 3 # the retry count\n---\nbody";
    let (fm, _) =
        parse_frontmatter(source).expect("frontmatter with trailing comment should parse");
    assert_eq!(fm.declarations[0].default_value, Some(Value::Int(3)));
}
