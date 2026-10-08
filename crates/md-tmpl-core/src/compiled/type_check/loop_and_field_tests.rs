//! Type-check tests for loops, for-else, and scalar/struct field access.

use super::{test_helpers::*, *};
use crate::types::{VarDecl, VarType};

// -- Loop binding type tracking --------------------------------------

#[test]
fn for_loop_binding_typed_from_list() {
    let decls = vec![VarDecl {
        name: "tasks".to_string(),
        var_type: VarType::List(vec![
            VarDecl {
                name: "title".to_string(),
                var_type: VarType::Str,
                default_value: None,
            },
            VarDecl {
                name: "vt".to_string(),
                var_type: VarType::Enum(vec![
                    variant("Known", vec![("label", VarType::Str)]),
                    unit_variant("Unknown"),
                ]),
                default_value: None,
            },
        ]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - tasks = list(title = str, vt = enum(Known(label = str), Unknown))
---
> {% for task in tasks %}{% match task.vt case Known %}{{ task.vt.label }}{% /match %}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "loop binding should be typed: {errors:?}"
    );
}

#[test]
fn for_loop_binding_field_access_validated() {
    let decls = vec![VarDecl {
        name: "tasks".to_string(),
        var_type: VarType::List(vec![
            VarDecl {
                name: "title".to_string(),
                var_type: VarType::Str,
                default_value: None,
            },
            VarDecl {
                name: "vt".to_string(),
                var_type: VarType::Enum(vec![
                    variant("Known", vec![("label", VarType::Str)]),
                    unit_variant("Unknown"),
                ]),
                default_value: None,
            },
        ]),
        default_value: None,
    }];
    // Access .label outside match — should fail since Unknown has no label.
    let tmpl = r"---
name: t
params:
  - tasks = list(title = str, vt = enum(Known(label = str), Unknown))
---
> {% for task in tasks %}{{ task.vt.label }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("label") && errors[0].contains("Unknown"),
        "got: {}",
        errors[0]
    );
}

// -- Loop over opaque imported constant --------------------------------

#[test]
fn for_loop_over_opaque_import_binding_is_opaque() {
    // Iterating an imported constant (an opaque root whose element type is
    // not statically known) must not flag `row.field` accesses in the body
    // as undeclared variables. Regression test for loops over
    // `<import>.CONST` such as `artist.SEVERITY_LADDER`.
    let tmpl = r"---
name: t
params:
---
> {% for row in cfg.LADDER %}

{{ row.short }}: {{ row.name }}

> {% /for %}";
    let (_fm, body) = crate::parse_frontmatter(tmpl).expect("parse");
    let empty_aliases = crate::compat::HashMap::new();
    let (segments, _) = crate::compiled::compile(body, &empty_aliases).expect("compile");
    let mut opaque = crate::compat::HashSet::new();
    opaque.insert("cfg".to_string());
    let errors = validate_field_accesses_with_opaque(&segments, &[], &opaque);
    assert!(
        errors.is_empty(),
        "loop over opaque import should type-check: {errors:?}"
    );
}

#[test]
fn for_loop_over_undeclared_non_opaque_still_errors() {
    // Guard: the opacity relaxation is scoped to opaque roots only. A loop
    // over a genuinely undeclared (non-opaque) iterable must still surface an
    // undeclared-variable error.
    let tmpl = r"---
name: t
params:
---
> {% for row in ghost %}

{{ row.short }}

> {% /for %}";
    let (_fm, body) = crate::parse_frontmatter(tmpl).expect("parse");
    let empty_aliases = crate::compat::HashMap::new();
    let (segments, _) = crate::compiled::compile(body, &empty_aliases).expect("compile");
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.iter().any(|e| e.contains("undeclared")),
        "undeclared iterable should still error: {errors:?}"
    );
}

#[test]
fn undeclared_variable_in_match_is_error() {
    let decls = vec![];
    let tmpl = r"---
name: t
params:
---
> {% match ghost %}{% case X %}x{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.iter().any(|e| e.contains("undeclared")),
        "expected undeclared error: {errors:?}"
    );
}

#[test]
fn undeclared_variable_in_expr_is_error() {
    let decls = vec![];
    let tmpl = r"---
name: t
params:
---
{{ ghost.field }}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.iter().any(|e| e.contains("undeclared")),
        "expected undeclared error: {errors:?}"
    );
}

// -- For-loop on non-list --------------------------------------------

#[test]
fn for_loop_on_str_is_error() {
    let decls = vec![VarDecl {
        name: "name".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - name = str
---
> {% for c in name %}{{ c }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("expected list") && e.contains("str")),
        "expected for-loop type error: {errors:?}"
    );
}

#[test]
fn for_loop_on_enum_suggests_kinds() {
    let decls = vec![VarDecl {
        name: "Stage".to_string(),
        var_type: VarType::Enum(vec![unit_variant("A"), unit_variant("B")]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - Stage = enum(A, B)
---
> {% for s in Stage %}{{ s }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("use kinds(Stage) to iterate over variant names")),
        "expected kinds suggestion for enum loop: {errors:?}"
    );
}

#[test]
fn for_loop_over_kinds_ok() {
    let decls = vec![VarDecl {
        name: "Stage".to_string(),
        var_type: VarType::Enum(vec![unit_variant("A"), unit_variant("B")]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - Stage = enum(A, B)
---
> {% for s in kinds(Stage) %}{{ s }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "for loop over kinds(Stage) should pass type checking: {errors:?}"
    );
}

// -- Scalar field access ---------------------------------------------

#[test]
fn field_on_str_is_error() {
    let decls = vec![VarDecl {
        name: "name".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - name = str
---
{{ name.length }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot access field") && errors[0].contains("str"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn field_on_int_is_error() {
    let decls = vec![VarDecl {
        name: "count".to_string(),
        var_type: VarType::Int,
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - count = int
---
{{ count.abs }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot access field") && errors[0].contains("int"),
        "got: {}",
        errors[0]
    );
}

// -- Struct field validation -------------------------------------------

#[test]
fn dict_unknown_field_is_error() {
    let decls = vec![VarDecl {
        name: "config".to_string(),
        var_type: VarType::Struct(vec![VarDecl {
            name: "host".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - config = struct(host = str)
---
{{ config.port }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("port") && errors[0].contains("does not exist"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn dict_known_field_ok() {
    let decls = vec![VarDecl {
        name: "config".to_string(),
        var_type: VarType::Struct(vec![VarDecl {
            name: "host".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - config = struct(host = str)
---
{{ config.host }}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "declared field: {errors:?}");
}

// -- for...else type checking -----------------------------------------

#[test]
fn for_else_body_type_checked() {
    // Variables in the else body should be type-checked.
    let decls = vec![
        VarDecl {
            name: "items".to_string(),
            var_type: VarType::List(vec![VarDecl {
                name: "title".to_string(),
                var_type: VarType::Str,
                default_value: None,
            }]),
            default_value: None,
        },
        VarDecl {
            name: "fallback".to_string(),
            var_type: VarType::Str,
            default_value: None,
        },
    ];
    let tmpl = r"---
name: t
params:
  - items = list(title = str)
  - fallback = str
---
> {% for item in items %}{{ item.title }}{% else %}{{ fallback }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "else body with valid var should pass: {errors:?}"
    );
}

#[test]
fn for_else_body_undeclared_variable_is_error() {
    // Undeclared variables in the else body should be caught.
    let decls = vec![VarDecl {
        name: "items".to_string(),
        var_type: VarType::List(vec![VarDecl {
            name: "title".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - items = list(title = str)
---
> {% for item in items %}{{ item.title }}{% else %}{{ ghost }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.iter().any(|e| e.contains("undeclared")),
        "should catch undeclared var in else body: {errors:?}"
    );
}

#[test]
fn for_else_loop_binding_not_in_else_body() {
    // The loop binding (item) should not be accessible in the else body,
    // since the else body only executes when the list is empty.
    // The type checker doesn't need to produce an error for this —
    // the loop binding is just not narrowed in the else body scope.
    // This test verifies the else body is walked without the binding.
    let decls = vec![VarDecl {
        name: "items".to_string(),
        var_type: VarType::List(vec![VarDecl {
            name: "title".to_string(),
            var_type: VarType::Str,
            default_value: None,
        }]),
        default_value: None,
    }];
    let tmpl = r"---
name: t
params:
  - items = list(title = str)
---
> {% for item in items %}{{ item.title }}{% else %}{{ item.title }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    // item is not declared as a root variable, so accessing it in else
    // should produce an undeclared error.
    assert!(
        errors.iter().any(|e| e.contains("undeclared")),
        "loop binding should not be accessible in else body: {errors:?}"
    );
}
