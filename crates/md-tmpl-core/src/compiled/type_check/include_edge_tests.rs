//! Type-check tests for include edge cases across control flow and nesting.

use super::{test_helpers::*, *};
use crate::{
    compiled::{CompiledInclude, CompiledInlineTemplate, Segment},
    scope::{CompiledExpr, CompiledPath},
    types::{VarDecl, VarType},
};

// -- Include inside control flow ------------------------------------

#[test]
fn include_inside_for_loop() {
    let inc = make_include(
        "row.tmpl.md",
        vec![("label", "item.label")],
        None,
        vec![str_decl("label")],
        vec![Segment::Expr {
            expr: CompiledExpr::compile("label").unwrap(),
            filters: vec![],
        }],
    );
    let parent_decls = vec![VarDecl {
        name: "items".to_string(),
        var_type: VarType::List(vec![str_decl("label")]),
        default_value: None,
    }];
    let segments = vec![Segment::ForLoop {
        binding: "item".into(),
        list_expr: CompiledExpr::compile("items").unwrap(),
        filters: vec![],
        body: vec![Segment::Include(inc)],
        else_body: vec![],
    }];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "include inside for loop should type-check: {errors:?}"
    );
}

#[test]
fn include_inside_match_arm() {
    let enum_type = VarType::Enum(vec![
        variant("Confirmed", vec![("evidence", VarType::Str)]),
        unit_variant("NotConfirmed"),
    ]);
    let inc = make_include(
        "detail.tmpl.md",
        vec![("proof", "outcome.evidence")],
        None,
        vec![str_decl("proof")],
        vec![Segment::Expr {
            expr: CompiledExpr::compile("proof").unwrap(),
            filters: vec![],
        }],
    );
    let parent_decls = vec![VarDecl {
        name: "outcome".to_string(),
        var_type: enum_type,
        default_value: None,
    }];
    let segments = vec![Segment::Match {
        expr: CompiledPath::compile("outcome"),
        arms: vec![crate::compiled::MatchArm {
            variants: vec!["Confirmed".into()],
            guard: None,
            body: vec![Segment::Include(inc)],
        }],
        is_option: false,
    }];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "include inside match arm should type-check: {errors:?}"
    );
}

// -- Multiple includes of same template -----------------------------

#[test]
fn multiple_includes_same_template_ok() {
    let inc1 = make_include(
        "child.tmpl.md",
        vec![("msg", "a")],
        None,
        vec![str_decl("msg")],
        vec![],
    );
    let inc2 = make_include(
        "child.tmpl.md",
        vec![("msg", "b")],
        None,
        vec![str_decl("msg")],
        vec![],
    );
    let parent_decls = vec![str_decl("a"), str_decl("b")];
    let segments = vec![Segment::Include(inc1), Segment::Include(inc2)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "multiple includes of same template: {errors:?}"
    );
}

// -- Include with extra with vars -----------------------------------

#[test]
fn include_extra_with_vars_ok() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("msg", "msg"), ("extra", "extra")],
        None,
        vec![str_decl("msg")], // child only declares msg
        vec![],
    );
    let parent_decls = vec![str_decl("msg"), str_decl("extra")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "extra with vars should be OK: {errors:?}"
    );
}

// -- Same-named inline templates from different files -------------------
// These test the pointer-based identity fix: same name but different
// Arc<[Segment]> should be walked independently.

#[test]
fn same_name_different_content_both_type_checked() {
    use std::sync::Arc;
    // Two includes both named "helper" but with different bodies/declarations.
    // The second body references an undeclared var — both should be walked.
    let inc1 = CompiledInclude {
        path: std::borrow::Cow::Borrowed("helper"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("msg"),
            std::borrow::Cow::Borrowed("a"),
        )],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: Arc::from(vec![Segment::Expr {
                expr: CompiledExpr::compile("msg").unwrap(),
                filters: vec![],
            }]),
            declarations: Arc::from(vec![str_decl("msg")]),
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };
    let inc2 = CompiledInclude {
        path: std::borrow::Cow::Borrowed("helper"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("msg"),
            std::borrow::Cow::Borrowed("b"),
        )],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            // This body references "ghost" which is NOT declared.
            segments: Arc::from(vec![
                Segment::Expr {
                    expr: CompiledExpr::compile("msg").unwrap(),
                    filters: vec![],
                },
                Segment::Expr {
                    expr: CompiledExpr::compile("ghost").unwrap(),
                    filters: vec![],
                },
            ]),
            declarations: Arc::from(vec![str_decl("msg")]),
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };

    let parent_decls = vec![str_decl("a"), str_decl("b")];
    let segments = vec![Segment::Include(inc1), Segment::Include(inc2)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert_eq!(
        errors.len(),
        1,
        "second helper's 'ghost' should be caught: {errors:?}"
    );
    assert!(
        errors[0].contains("ghost"),
        "should report undeclared 'ghost': {errors:?}"
    );
}

#[test]
fn same_arc_deduplicates_body_walk() {
    use std::sync::Arc;
    // Two includes using the SAME Arc (same file) should only walk once.
    let shared_segments: Arc<[Segment]> = Arc::from(vec![Segment::Expr {
        expr: CompiledExpr::compile("msg").unwrap(),
        filters: vec![],
    }]);
    let shared_decls: Arc<[VarDecl]> = Arc::from(vec![str_decl("msg")]);

    let inc1 = CompiledInclude {
        path: std::borrow::Cow::Borrowed("shared.tmpl.md"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("msg"),
            std::borrow::Cow::Borrowed("a"),
        )],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: Arc::clone(&shared_segments),
            declarations: Arc::clone(&shared_decls),
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };
    let inc2 = CompiledInclude {
        path: std::borrow::Cow::Borrowed("shared.tmpl.md"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("msg"),
            std::borrow::Cow::Borrowed("b"),
        )],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: Arc::clone(&shared_segments),
            declarations: Arc::clone(&shared_decls),
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };

    let parent_decls = vec![str_decl("a"), str_decl("b")];
    let segments = vec![Segment::Include(inc1), Segment::Include(inc2)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(errors.is_empty(), "shared arc dedup: {errors:?}");
}

// -- Include with inline_compiled = None (no cross-boundary checks) -----

#[test]
fn include_no_inline_compiled_validates_parent_paths() {
    // When inline_compiled is None, cross-boundary checks are skipped,
    // but parent-scope validation of `with` expressions should still happen.
    let inc = CompiledInclude {
        path: std::borrow::Cow::Borrowed("unknown.tmpl.md"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("msg"),
            std::borrow::Cow::Borrowed("ghost"),
        )],
        for_each: None,
        inline_compiled: None, // Not resolved
    };
    let parent_decls = vec![str_decl("a")]; // 'ghost' is NOT declared
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert_eq!(
        errors.len(),
        1,
        "should catch undeclared 'ghost': {errors:?}"
    );
    assert!(
        errors[0].contains("ghost"),
        "should mention 'ghost': {errors:?}"
    );
}

#[test]
fn include_no_inline_compiled_skips_contract() {
    // When inline_compiled is None, contract checking is impossible
    // (we don't know what the included template declares).
    let inc = CompiledInclude {
        path: std::borrow::Cow::Borrowed("unknown.tmpl.md"),
        with_vars: vec![],
        for_each: None,
        inline_compiled: None,
    };
    let parent_decls = vec![str_decl("a")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "no inline_compiled = no contract check: {errors:?}"
    );
}

// -- Include inside {% if %} (always type-checked, even false branch) ---

#[test]
fn include_inside_if_false_branch_still_checked() {
    // An include with a missing param inside an if branch should still
    // produce an error — type checking is exhaustive, not conditional.
    let inc = make_include(
        "child.tmpl.md",
        vec![], // missing required param
        None,
        vec![str_decl("msg")],
        vec![],
    );
    let parent_decls = vec![bool_decl("flag")];
    let segments = vec![Segment::If {
        branches: vec![(
            crate::compiled::Condition::Truthy(
                crate::scope::ConditionOperand::compile("flag").unwrap(),
            ),
            vec![Segment::Include(inc)],
        )],
        else_body: vec![],
    }];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        !errors.is_empty(),
        "include inside if-branch should still be checked"
    );
    assert!(
        errors[0].contains("msg"),
        "should mention missing 'msg': {errors:?}"
    );
}

// -- Include with both for_each AND with_vars --------------------------

#[test]
fn include_for_each_plus_with_type_mismatch() {
    let inc = make_include(
        "row.tmpl.md",
        vec![("extra", "my_int")],
        Some(("item", "items")),
        vec![str_decl("item"), str_decl("extra")], // extra expects str
        vec![],
    );
    let parent_decls = vec![
        VarDecl {
            name: "items".to_string(),
            var_type: VarType::List(vec![]),
            default_value: None,
        },
        int_decl("my_int"), // int, but child expects str
    ];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        !errors.is_empty(),
        "int→str mismatch should be caught: {errors:?}"
    );
    assert!(
        errors[0].contains("type mismatch"),
        "should mention type mismatch: {errors:?}"
    );
}

// -- With var referencing undeclared parent variable --------------------

#[test]
fn include_with_var_references_undeclared_parent() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("msg", "nonexistent")],
        None,
        vec![str_decl("msg")],
        vec![],
    );
    let parent_decls = vec![str_decl("other")]; // 'nonexistent' not declared
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        !errors.is_empty(),
        "undeclared parent var should error: {errors:?}"
    );
    assert!(
        errors[0].contains("nonexistent"),
        "should mention 'nonexistent': {errors:?}"
    );
}

// -- Same template, different types at different sites ------------------

#[test]
fn same_template_different_sites() {
    use std::sync::Arc;
    let child_decls: Arc<[VarDecl]> = Arc::from(vec![str_decl("msg")]);
    let child_segments: Arc<[Segment]> = Arc::from(vec![Segment::Expr {
        expr: CompiledExpr::compile("msg").unwrap(),
        filters: vec![],
    }]);

    // Site 1: msg=my_str (str→str, OK)
    let inc1 = CompiledInclude {
        path: std::borrow::Cow::Borrowed("child.tmpl.md"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("msg"),
            std::borrow::Cow::Borrowed("my_str"),
        )],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: Arc::clone(&child_segments),
            declarations: Arc::clone(&child_decls),
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };
    // Site 2: msg=my_int (int→str, ERROR)
    let inc2 = CompiledInclude {
        path: std::borrow::Cow::Borrowed("child.tmpl.md"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("msg"),
            std::borrow::Cow::Borrowed("my_int"),
        )],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: Arc::clone(&child_segments),
            declarations: Arc::clone(&child_decls),
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };

    let parent_decls = vec![str_decl("my_str"), int_decl("my_int")];
    let segments = vec![Segment::Include(inc1), Segment::Include(inc2)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert_eq!(
        errors.len(),
        1,
        "exactly one type mismatch (second site): {errors:?}"
    );
    assert!(
        errors[0].contains("type mismatch"),
        "should report type mismatch: {errors:?}"
    );
}

// -- Container type mismatches (list→dict, dict→list) -------------------

#[test]
fn include_list_dict_cross_kind_mismatch() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("data", "my_list")],
        None,
        vec![VarDecl {
            name: "data".to_string(),
            var_type: VarType::Struct(vec![str_decl("key")]),
            default_value: None,
        }],
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "my_list".to_string(),
        var_type: VarType::List(vec![str_decl("key")]),
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        !errors.is_empty(),
        "list→dict mismatch should error: {errors:?}"
    );
}

// -- Float and bool include type checking ------------------------------

#[test]
fn include_float_type_match_ok() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("val", "score")],
        None,
        vec![VarDecl {
            name: "val".to_string(),
            var_type: VarType::Float,
            default_value: None,
        }],
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "score".to_string(),
        var_type: VarType::Float,
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(errors.is_empty(), "float→float should be OK: {errors:?}");
}

#[test]
fn include_bool_type_mismatch() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("flag", "name")],
        None,
        vec![VarDecl {
            name: "flag".to_string(),
            var_type: VarType::Bool,
            default_value: None,
        }],
        vec![],
    );
    let parent_decls = vec![str_decl("name")]; // str != bool
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        !errors.is_empty(),
        "str→bool mismatch should error: {errors:?}"
    );
}

// -- for_each binding name collides with a with var --------------------

#[test]
fn include_for_each_binding_collides_with_var() {
    // Both `for item in items` and `with item="override"` provide "item".
    // The contract should be satisfied (item IS provided).
    let inc = make_include(
        "child.tmpl.md",
        vec![("item", "\"override\"")],
        Some(("item", "items")),
        vec![str_decl("item")],
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "items".to_string(),
        var_type: VarType::List(vec![]),
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    // Contract satisfied — "item" is provided by both for_each and with.
    assert!(
        errors.is_empty(),
        "binding + with for same name should be OK: {errors:?}"
    );
}

// -- Grandchild include type mismatch ----------------------------------

#[test]
fn nested_include_grandchild_type_error() {
    use std::sync::Arc;
    // Parent → child → grandchild. Grandchild has an undeclared var.
    let grandchild_segments: Arc<[Segment]> = Arc::from(vec![Segment::Expr {
        expr: CompiledExpr::compile("undefined_in_grandchild").unwrap(),
        filters: vec![],
    }]);
    let grandchild_decls: Arc<[VarDecl]> = Arc::from(vec![str_decl("msg")]);

    let child_inc = CompiledInclude {
        path: std::borrow::Cow::Borrowed("grandchild.tmpl.md"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("msg"),
            std::borrow::Cow::Borrowed("child_msg"),
        )],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: grandchild_segments,
            declarations: grandchild_decls,
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };

    let child_segments: Arc<[Segment]> = Arc::from(vec![
        Segment::Expr {
            expr: CompiledExpr::compile("child_msg").unwrap(),
            filters: vec![],
        },
        Segment::Include(child_inc),
    ]);
    let child_decls: Arc<[VarDecl]> = Arc::from(vec![str_decl("child_msg")]);

    let parent_inc = CompiledInclude {
        path: std::borrow::Cow::Borrowed("child.tmpl.md"),
        with_vars: vec![(
            std::borrow::Cow::Borrowed("child_msg"),
            std::borrow::Cow::Borrowed("parent_msg"),
        )],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: child_segments,
            declarations: child_decls,
            consts: std::sync::Arc::new(crate::compat::HashMap::new()),
            imported_consts: std::sync::Arc::new(crate::compat::HashMap::new()),
        }),
    };

    let parent_decls = vec![str_decl("parent_msg")];
    let segments = vec![Segment::Include(parent_inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert_eq!(errors.len(), 1, "grandchild's undeclared var: {errors:?}");
    assert!(
        errors[0].contains("undefined_in_grandchild"),
        "should report grandchild error: {errors:?}"
    );
}
