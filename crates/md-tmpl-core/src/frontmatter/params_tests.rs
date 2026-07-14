use super::*;
use crate::{
    compat::HashMap,
    types::{VarDecl, VarType},
    value::Value,
};

/// Helper: parse a type annotation with empty aliases/imports.
fn parse_type(s: &str) -> Result<VarType, String> {
    let aliases = HashMap::new();
    let imports = HashMap::new();
    parse_type_annotation(s, &aliases, &imports)
}

/// Helper: parse declarations (params, not constants) with empty aliases/imports.
fn parse_decls(rest: &str) -> Result<Vec<VarDecl>, crate::error::TemplateError> {
    let aliases = HashMap::new();
    let imports = HashMap::new();
    let consts = HashMap::new();
    parse_declarations(rest, &aliases, &imports, false, &consts)
}

/// Helper: parse constant declarations with empty aliases/imports.
fn parse_consts(rest: &str) -> Result<Vec<VarDecl>, crate::error::TemplateError> {
    let aliases = HashMap::new();
    let imports = HashMap::new();
    let consts = HashMap::new();
    parse_declarations(rest, &aliases, &imports, true, &consts)
}

// =========================================================================
// join_continuation_lines
// =========================================================================

#[test]
fn join_normal_lines() {
    let block = "line1\nline2\nline3";
    let result = join_continuation_lines(block);
    assert_eq!(result, vec!["line1", "line2", "line3"]);
}

#[test]
fn join_indented_continuation() {
    let block = "key:\n  continued\n  more";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "key: continued more");
}

#[test]
fn join_tab_continuation() {
    let block = "key:\n\tcontinued\n\tmore";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "key: continued more");
}

#[test]
fn join_first_line_indented() {
    // If the very first line is indented, there's no previous line to join to,
    // so it becomes its own logical line.
    let block = "  indented_first\nsecond";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "  indented_first");
    assert_eq!(result[1], "second");
}

#[test]
fn join_multiple_groups() {
    let block = "key1: val1\n  continued1\nkey2: val2\n  continued2";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "key1: val1 continued1");
    assert_eq!(result[1], "key2: val2 continued2");
}

#[test]
fn join_empty_block() {
    let result = join_continuation_lines("");
    assert!(result.is_empty());
}

#[test]
fn join_no_continuations() {
    let block = "a\nb\nc";
    let result = join_continuation_lines(block);
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn join_blank_line_between_entries_not_dropped() {
    // REGRESSION: a blank line between two top-level entries must not create an
    // empty logical line nor drop the second entry. Previously the blank line
    // was emitted as its own logical line, orphaning everything after it.
    let block = "- FIRST\n\n- SECOND";
    let result = join_continuation_lines(block);
    assert_eq!(result, vec!["- FIRST", "- SECOND"]);
}

#[test]
fn join_skips_full_line_comments() {
    // REGRESSION: full-line `#` comments are documentation only and must be
    // skipped without terminating an in-progress block list.
    let block = "- A\n# a comment\n- B";
    let result = join_continuation_lines(block);
    assert_eq!(result, vec!["- A", "- B"]);
}

#[test]
fn join_skips_blank_lines_within_continuation() {
    // A blank line inside a multi-line value continuation is skipped, and the
    // following indented line still appends to the same logical line.
    let block = "key: val\n  cont1\n\n  cont2";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "key: val cont1 cont2");
}

#[test]
fn join_multiline_value_with_blank_and_comment() {
    // REGRESSION: mirrors the ARTIST SEVERITY_LADDER const — a section whose
    // entry value spans multiple indented lines, with a blank line and a `#`
    // comment interleaved between entries. All entries must survive and the
    // multi-line value must be joined onto a single logical line.
    let block = "consts:\n  - A = str := \"x\"\n\n  # doc comment\n  - B = list(n = int) :=\n    [{n = 1},\n    {n = 2}]";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0],
        "consts: - A = str := \"x\" - B = list(n = int) := [{n = 1}, {n = 2}]"
    );
}

// =========================================================================
// split_at_depth_zero
// =========================================================================

#[test]
fn split_simple_comma() {
    let result = split_at_depth_zero("a, b, c");
    assert_eq!(result, vec!["a", " b", " c"]);
}

#[test]
fn split_nested_angle_brackets_preserved() {
    let result = split_at_depth_zero("name = str, items = list<label = str, count = int>");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "name = str");
    assert_eq!(result[1], " items = list<label = str, count = int>");
}

#[test]
fn split_nested_parens() {
    let result = split_at_depth_zero("A(x = str, y = int), B");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "A(x = str, y = int)");
    assert_eq!(result[1], " B");
}

#[test]
fn split_empty_input() {
    let result = split_at_depth_zero("");
    assert_eq!(result, vec![""]);
}

#[test]
fn split_single_entry() {
    let result = split_at_depth_zero("only_one");
    assert_eq!(result, vec!["only_one"]);
}

#[test]
fn split_nested_braces() {
    let result = split_at_depth_zero("{a: 1, b: 2}, c");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "{a: 1, b: 2}");
    assert_eq!(result[1], " c");
}

#[test]
fn split_deeply_nested() {
    let result = split_at_depth_zero("list<list<a = str, b = list<c = int>>>, x = bool");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "list<list<a = str, b = list<c = int>>>");
    assert_eq!(result[1], " x = bool");
}

#[test]
fn split_ignores_comma_inside_double_quotes() {
    // REGRESSION: commas inside a double-quoted string must not be treated as
    // field separators (e.g. `mem_example = "Crash, panic, sanitizer report"`).
    let result = split_at_depth_zero("msg = \"a, b, c\", n = 1");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "msg = \"a, b, c\"");
    assert_eq!(result[1], " n = 1");
}

#[test]
fn split_ignores_comma_inside_single_quotes() {
    let result = split_at_depth_zero("a = 'x, y', b = 2");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "a = 'x, y'");
    assert_eq!(result[1], " b = 2");
}

#[test]
fn split_ignores_brackets_inside_quotes() {
    // Brackets/braces inside a quoted string must not affect depth tracking.
    let result = split_at_depth_zero("a = \"x [y, z] {q}\", b = 2");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "a = \"x [y, z] {q}\"");
    assert_eq!(result[1], " b = 2");
}

// =========================================================================
// find_char_at_depth_zero
// =========================================================================

#[test]
fn find_equals_at_depth_zero() {
    let result = find_char_at_depth_zero("name = str", '=');
    assert_eq!(result, Some(5));
}

#[test]
fn find_skips_inside_angle_brackets() {
    let result = find_char_at_depth_zero("list<a = str>", '=');
    assert_eq!(result, None, "= inside <> should not be found at depth 0");
}

#[test]
fn find_returns_none_when_not_found() {
    let result = find_char_at_depth_zero("no_target_here", '=');
    assert_eq!(result, None);
}

#[test]
fn find_first_occurrence_at_depth_zero() {
    let result = find_char_at_depth_zero("a = b = c", '=');
    assert_eq!(result, Some(2));
}

#[test]
fn find_inside_parens_skipped() {
    let result = find_char_at_depth_zero("fn(x = 1)", '=');
    assert_eq!(result, None);
}

#[test]
fn find_after_brackets() {
    let result = find_char_at_depth_zero("list<a = str> = val", '=');
    assert_eq!(result, Some(14));
}

#[test]
fn find_on_empty_input() {
    assert_eq!(find_char_at_depth_zero("", '='), None);
}

// =========================================================================
// find_assign_default_at_depth_zero (internal, tested via parse_declarations)
// =========================================================================

#[test]
fn find_assign_default_basic() {
    let result = find_assign_default_at_depth_zero("str := hello");
    assert_eq!(result, Some(4));
}

#[test]
fn find_assign_default_skips_inside_brackets() {
    let result = find_assign_default_at_depth_zero("list<str := x>");
    assert_eq!(result, None);
}

#[test]
fn find_assign_default_not_found() {
    let result = find_assign_default_at_depth_zero("str");
    assert_eq!(result, None);
}

#[test]
fn find_assign_default_colon_without_equals() {
    // A bare `:` without `=` should not match.
    let result = find_assign_default_at_depth_zero("a: b");
    assert_eq!(result, None);
}

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
    let decls = parse_declarations(
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
    let decls = parse_declarations(
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
