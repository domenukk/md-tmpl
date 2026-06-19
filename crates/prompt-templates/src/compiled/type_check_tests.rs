use super::*;
use crate::{
    compiled::CompiledInlineTemplate,
    types::{VarDecl, VarType, VariantDecl},
};

/// Helper: build declarations from a shorthand.
fn enum_decl(name: &str, variants: Vec<VariantDecl>) -> VarDecl {
    VarDecl {
        name: name.to_string(),
        var_type: VarType::Enum(variants),
        default_value: None,
    }
}

fn variant(name: &str, fields: Vec<(&str, VarType)>) -> VariantDecl {
    VariantDecl {
        name: name.to_string(),
        fields: fields
            .into_iter()
            .map(|(n, t)| VarDecl {
                name: n.to_string(),
                var_type: t,
                default_value: None,
            })
            .collect(),
    }
}

fn unit_variant(name: &str) -> VariantDecl {
    variant(name, vec![])
}

fn compile_and_check(template: &str, decls: &[VarDecl]) -> Vec<String> {
    let (_fm, body) = crate::parse_frontmatter(template).expect("parse");
    let empty_aliases = crate::compat::HashMap::new();
    let (segments, _) = crate::compiled::compile(body, &empty_aliases).expect("compile");
    validate_field_accesses(&segments, decls)
}

// -- Variant name validation -----------------------------------------

#[test]
fn match_valid_variant_names() {
    let decls = vec![enum_decl(
        "outcome",
        vec![unit_variant("Confirmed"), unit_variant("NotConfirmed")],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed, NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confirmed %}yes{% case NotConfirmed %}no{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}

#[test]
fn match_unknown_variant_name() {
    let decls = vec![enum_decl(
        "outcome",
        vec![unit_variant("Confirmed"), unit_variant("NotConfirmed")],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed, NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confrimed %}yes{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected 1 error, got: {errors:?}");
    assert!(
        errors[0].contains("unknown variant 'Confrimed'"),
        "got: {}",
        errors[0]
    );
    assert!(
        errors[0].contains("Confirmed, NotConfirmed"),
        "should list valid variants: {}",
        errors[0]
    );
}

// -- Field access outside match (must be on ALL variants) ------------

#[test]
fn field_on_all_variants_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("reason", VarType::Str)]),
            variant("Rejected", vec![("reason", VarType::Str)]),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(reason = str), Rejected(reason = str)>\n---\n\
                     {{ outcome.reason }}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "unexpected: {errors:?}");
}

#[test]
fn field_not_on_all_variants_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     {{ outcome.evidence }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected 1 error, got: {errors:?}");
    assert!(
        errors[0].contains("not available on variant") && errors[0].contains("NotConfirmed"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn tag_field_is_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     {{ outcome.tag }}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, ".tag should be an error: {errors:?}");
    assert!(
        errors[0].contains("tag") && errors[0].contains("does not exist"),
        "got: {}",
        errors[0]
    );
}

// -- Field access inside match arm (narrowed) ------------------------

#[test]
fn field_in_matching_arm_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confirmed %}{{ outcome.evidence }}{% case NotConfirmed %}none{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "narrowed access should work: {errors:?}");
}

#[test]
fn field_in_wrong_arm_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     > {% match outcome %}{% case NotConfirmed %}{{ outcome.evidence }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("evidence") && errors[0].contains("NotConfirmed"),
        "got: {}",
        errors[0]
    );
}

// -- Multi-variant case: intersection of fields ----------------------

#[test]
fn multi_variant_shared_field_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant(
                "Confirmed",
                vec![("reason", VarType::Str), ("evidence", VarType::Str)],
            ),
            variant("ConfirmedWithCaveats", vec![("reason", VarType::Str)]),
            unit_variant("Rejected"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(reason = str, evidence = str), ConfirmedWithCaveats(reason = str), Rejected>\n---\n\
                     > {% match outcome %}{% case Confirmed | ConfirmedWithCaveats %}{{ outcome.reason }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "shared field 'reason' should work: {errors:?}"
    );
}

#[test]
fn multi_variant_non_shared_field_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant(
                "Confirmed",
                vec![("reason", VarType::Str), ("evidence", VarType::Str)],
            ),
            variant("ConfirmedWithCaveats", vec![("reason", VarType::Str)]),
            unit_variant("Rejected"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(reason = str, evidence = str), ConfirmedWithCaveats(reason = str), Rejected>\n---\n\
                     > {% match outcome %}{% case Confirmed | ConfirmedWithCaveats %}{{ outcome.evidence }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("evidence") && errors[0].contains("ConfirmedWithCaveats"),
        "got: {}",
        errors[0]
    );
}

// -- Inline match case -----------------------------------------------

#[test]
fn inline_match_case_field_ok() {
    let decls = vec![enum_decl(
        "vt",
        vec![
            variant("Known", vec![("label", VarType::Str)]),
            unit_variant("Unknown"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - vt = enum<Known(label = str), Unknown>\n---\n\
                     > {% match vt case Known %}{{ vt.label }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "inline narrowing: {errors:?}");
}

// -- Nested match ----------------------------------------------------

#[test]
fn nested_match_narrows_independently() {
    let decls = vec![
        enum_decl(
            "a",
            vec![
                variant("X", vec![("x_val", VarType::Str)]),
                unit_variant("Y"),
            ],
        ),
        enum_decl(
            "b",
            vec![
                variant("P", vec![("p_val", VarType::Str)]),
                unit_variant("Q"),
            ],
        ),
    ];
    let tmpl = "---\nname: t\nparams:\n  - a = enum<X(x_val = str), Y>\n  - b = enum<P(p_val = str), Q>\n---\n\
                     > {% match a %}{% case X %}\
                     > {% match b %}{% case P %}{{ a.x_val }} {{ b.p_val }}{% /match %}\
                     {% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "nested narrowing: {errors:?}");
}

// -- Match on non-enum rejects at compile time ----------------------

#[test]
fn match_on_str_is_compile_error() {
    let decls = vec![VarDecl {
        name: "status".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl = "---\nname: t\nparams:\n  - status = str\n---\n\
                     > {% match status %}{% case Active %}active{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("expected enum") && errors[0].contains("str"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn match_on_int_is_compile_error() {
    let decls = vec![VarDecl {
        name: "count".to_string(),
        var_type: VarType::Int,
        default_value: None,
    }];
    let tmpl = "---\nname: t\nparams:\n  - count = int\n---\n\
                     > {% match count %}{% case One %}one{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("expected enum") && errors[0].contains("int"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn match_on_bool_is_compile_error() {
    let decls = vec![VarDecl {
        name: "flag".to_string(),
        var_type: VarType::Bool,
        default_value: None,
    }];
    let tmpl = "---\nname: t\nparams:\n  - flag = bool\n---\n\
                     > {% match flag %}{% case True %}yes{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("expected enum") && errors[0].contains("bool"),
        "got: {}",
        errors[0]
    );
}

// -- Condition paths inside if inside match --------------------------

#[test]
fn condition_inside_match_arm_validated() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confirmed %}{% if outcome.evidence %}yes{% /if %}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "condition in arm: {errors:?}");
}

// -- Enum comparison rejection ----------------------------------------

#[test]
fn eq_on_unit_enum_is_compile_error() {
    let decls = vec![enum_decl(
        "role",
        vec![
            unit_variant("Builder"),
            unit_variant("Analyst"),
            unit_variant("Chainer"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - role = enum<Builder, Analyst, Chainer>\n---\n\
                     > {% if role == Builder %}yes{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot compare enum") && errors[0].contains("match"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn eq_on_struct_enum_is_compile_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("Rejected"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), Rejected>\n---\n\
                     > {% if outcome == Confirmed %}yes{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot compare enum"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn ne_on_enum_is_compile_error() {
    let decls = vec![enum_decl(
        "status",
        vec![unit_variant("Active"), unit_variant("Inactive")],
    )];
    let tmpl = "---\nname: t\nparams:\n  - status = enum<Active, Inactive>\n---\n\
                     > {% if status != Active %}no{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot compare enum"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn eq_enum_string_literal_is_compile_error() {
    // Even comparing an enum to a string literal should be rejected.
    let decls = vec![enum_decl(
        "role",
        vec![unit_variant("Builder"), unit_variant("Analyst")],
    )];
    let tmpl = "---\nname: t\nparams:\n  - role = enum<Builder, Analyst>\n---\n\
                     > {% if role == \"Builder\" %}yes{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("cannot compare enum") && errors[0].contains("match"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn elif_on_enum_is_compile_error() {
    // Full if/elif chain on an enum should be rejected.
    let decls = vec![enum_decl(
        "role",
        vec![
            unit_variant("Builder"),
            unit_variant("Analyst"),
            unit_variant("Support"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - role = enum<Builder, Analyst, Support>\n---\n\
                     > {% if role == Builder %}b{% elif role == Analyst %}a{% /if %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 2, "expected 2 errors (if + elif): {errors:?}");
    assert!(
        errors.iter().all(|e| e.contains("cannot compare enum")),
        "got: {errors:?}",
    );
}

// -- Field doesn't exist on any variant ------------------------------

#[test]
fn field_nonexistent_on_all_variants() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("A", vec![("x", VarType::Str)]),
            variant("B", vec![("y", VarType::Str)]),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<A(x = str), B(y = str)>\n---\n\
                     > {% match outcome %}{% case A %}{{ outcome.z }}{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].contains("'z'") && errors[0].contains("does not exist"),
        "got: {}",
        errors[0]
    );
}

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
    let tmpl = "---\nname: t\nparams:\n  - tasks = list<title = str, vt = enum<Known(label = str), Unknown>>\n---\n\
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
    let tmpl = "---\nname: t\nparams:\n  - tasks = list<title = str, vt = enum<Known(label = str), Unknown>>\n---\n\
                     > {% for task in tasks %}{{ task.vt.label }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(
        errors[0].contains("label") && errors[0].contains("Unknown"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn undeclared_variable_in_match_is_error() {
    let decls = vec![];
    let tmpl = "---\nname: t\nparams:\n---\n\
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
    let tmpl = "---\nname: t\nparams:\n---\n{{ ghost.field }}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.iter().any(|e| e.contains("undeclared")),
        "expected undeclared error: {errors:?}"
    );
}

// -- Exhaustiveness --------------------------------------------------

#[test]
fn multi_arm_match_exhaustive_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("NotConfirmed"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), NotConfirmed>\n---\n\
                     > {% match outcome %}{% case Confirmed %}yes{% case NotConfirmed %}no{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "exhaustive match: {errors:?}");
}

#[test]
fn multi_arm_match_non_exhaustive_error() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("A", vec![("x", VarType::Str)]),
            unit_variant("B"),
            unit_variant("C"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<A(x = str), B, C>\n---\n\
                     > {% match outcome %}{% case A %}a{% case B %}b{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert_eq!(errors.len(), 1, "expected exhaustiveness error: {errors:?}");
    assert!(
        errors[0].contains("non-exhaustive") && errors[0].contains('C'),
        "got: {}",
        errors[0]
    );
}

#[test]
fn single_arm_inline_not_exhaustive_ok() {
    let decls = vec![enum_decl(
        "outcome",
        vec![unit_variant("A"), unit_variant("B")],
    )];
    // Single inline arm — intentionally non-exhaustive guard.
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<A, B>\n---\n\
                     > {% match outcome case A %}a{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "inline guard: {errors:?}");
}

#[test]
fn match_with_no_arms_is_error() {
    // Construct directly since the parser might not produce this.
    let decls = vec![enum_decl("x", vec![unit_variant("A"), unit_variant("B")])];
    let segments = vec![Segment::Match {
        expr: CompiledPath::compile("x"),
        arms: vec![],
    }];
    let errors = validate_field_accesses(&segments, &decls);
    assert_eq!(errors.len(), 1, "expected error: {errors:?}");
    assert!(errors[0].contains("no case arms"), "got: {}", errors[0]);
}

// -- Default arm (catch-all) -----------------------------------------

#[test]
fn default_arm_satisfies_exhaustiveness() {
    let decls = vec![enum_decl(
        "outcome",
        vec![
            variant("Confirmed", vec![("evidence", VarType::Str)]),
            unit_variant("Rejected"),
            unit_variant("Pending"),
        ],
    )];
    let tmpl = "---\nname: t\nparams:\n  - outcome = enum<Confirmed(evidence = str), Rejected, Pending>\n---\n\
                     > {% match outcome %}{% case Confirmed %}yes{% default %}other{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "default should satisfy exhaustiveness: {errors:?}"
    );
}

#[test]
fn default_alone_satisfies_exhaustiveness() {
    let decls = vec![enum_decl(
        "status",
        vec![unit_variant("A"), unit_variant("B"), unit_variant("C")],
    )];
    let tmpl = "---\nname: t\nparams:\n  - status = enum<A, B, C>\n---\n\
                     > {% match status %}{% default %}fallback{% /match %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors.is_empty(),
        "default-only should be valid: {errors:?}"
    );
}

#[test]
fn default_arm_renders_correctly() {
    let tmpl = crate::Template::from_source(
        "---\nparams:\n  - status = enum<A, B, C>\n---\n\
             > {% match status %}{% case A %}alpha{% default %}other{% /match %}",
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set("status", "B");
    assert_eq!(tmpl.render(&ctx).unwrap(), "other");
    ctx.set("status", "A");
    assert_eq!(tmpl.render(&ctx).unwrap(), "alpha");
}

// -- For-loop on non-list --------------------------------------------

#[test]
fn for_loop_on_str_is_error() {
    let decls = vec![VarDecl {
        name: "name".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let tmpl = "---\nname: t\nparams:\n  - name = str\n---\n\
                     > {% for c in name %}{{ c }}{% /for %}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("expected list") && e.contains("str")),
        "expected for-loop type error: {errors:?}"
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
    let tmpl = "---\nname: t\nparams:\n  - name = str\n---\n{{ name.length }}";
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
    let tmpl = "---\nname: t\nparams:\n  - count = int\n---\n{{ count.abs }}";
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
    let tmpl = "---\nname: t\nparams:\n  - config = struct<host = str>\n---\n{{ config.port }}";
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
    let tmpl = "---\nname: t\nparams:\n  - config = struct<host = str>\n---\n{{ config.host }}";
    let errors = compile_and_check(tmpl, &decls);
    assert!(errors.is_empty(), "declared field: {errors:?}");
}

// -- Include contract tests ------------------------------------------

fn make_include(
    path: &str,
    with_vars: Vec<(&str, &str)>,
    for_each: Option<(&str, &str)>,
    declarations: Vec<VarDecl>,
    segments: Vec<Segment>,
) -> CompiledInclude {
    use std::sync::Arc;
    CompiledInclude {
        path: std::borrow::Cow::Owned(path.to_string()),
        with_vars: with_vars
            .into_iter()
            .map(|(k, v)| {
                (
                    std::borrow::Cow::Owned(k.to_string()),
                    std::borrow::Cow::Owned(v.to_string()),
                )
            })
            .collect(),
        for_each: for_each.map(|(b, l)| {
            (
                std::borrow::Cow::Owned(b.to_string()),
                std::borrow::Cow::Owned(l.to_string()),
            )
        }),
        inline_compiled: Some(CompiledInlineTemplate {
            segments: Arc::from(segments),
            declarations: Arc::from(declarations),
        }),
    }
}

fn str_decl(name: &str) -> VarDecl {
    VarDecl {
        name: name.to_string(),
        var_type: VarType::Str,
        default_value: None,
    }
}

fn int_decl(name: &str) -> VarDecl {
    VarDecl {
        name: name.to_string(),
        var_type: VarType::Int,
        default_value: None,
    }
}

#[test]
fn include_missing_required_params_error() {
    let inc = make_include(
        "child.tmpl.md",
        vec![],
        None,
        vec![str_decl("msg"), int_decl("count")],
        vec![],
    );
    let parent_decls: Vec<VarDecl> = vec![];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.iter().any(|e| e.contains("missing required param")),
        "expected missing params error: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("msg") && e.contains("count")),
        "should mention both missing params: {errors:?}"
    );
}

#[test]
fn include_all_params_provided_ok() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("msg", "my_msg"), ("count", "my_count")],
        None,
        vec![str_decl("msg"), int_decl("count")],
        vec![],
    );
    let parent_decls = vec![str_decl("my_msg"), int_decl("my_count")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "all params provided should be OK: {errors:?}"
    );
}

#[test]
fn include_for_binding_counts_as_provided() {
    let inc = make_include(
        "row.tmpl.md",
        vec![],
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
    assert!(
        errors.is_empty(),
        "for binding should satisfy param: {errors:?}"
    );
}

#[test]
fn include_no_declarations_always_ok() {
    let inc = make_include("static.tmpl.md", vec![], None, vec![], vec![]);
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.is_empty(),
        "no declarations should always be OK: {errors:?}"
    );
}

// -- Include type-check tests ----------------------------------------

#[test]
fn include_type_match_str_ok() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("name", "parent_name")],
        None,
        vec![str_decl("name")],
        vec![],
    );
    let parent_decls = vec![str_decl("parent_name")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(errors.is_empty(), "matching types should be OK: {errors:?}");
}

#[test]
fn include_type_mismatch_error() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("count", "name")],
        None,
        vec![int_decl("count")],
        vec![],
    );
    let parent_decls = vec![str_decl("name")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.iter().any(|e| e.contains("type mismatch")),
        "should report type mismatch: {errors:?}"
    );
}

#[test]
fn include_dotted_path_type_resolution() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("label", "config.host")],
        None,
        vec![str_decl("label")],
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "config".to_string(),
        var_type: VarType::Struct(vec![str_decl("host")]),
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "dotted path type should resolve: {errors:?}"
    );
}

#[test]
fn include_dotted_path_type_mismatch() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("count", "config.host")],
        None,
        vec![int_decl("count")], // expects int
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "config".to_string(),
        var_type: VarType::Struct(vec![str_decl("host")]), // host is str
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.iter().any(|e| e.contains("type mismatch")),
        "dotted path type mismatch: {errors:?}"
    );
}

#[test]
fn include_literal_value_skips_type_check() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("msg", "\"hello\"")],
        None,
        vec![str_decl("msg")],
        vec![],
    );
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.is_empty(),
        "literals should skip type check: {errors:?}"
    );
}

#[test]
fn include_enum_type_forwarding() {
    let enum_variants = vec![
        variant("Active", vec![("label", VarType::Str)]),
        unit_variant("Stopped"),
    ];
    let enum_type = VarType::Enum(enum_variants.clone());
    let inc = make_include(
        "child.tmpl.md",
        vec![("status", "status")],
        None,
        vec![VarDecl {
            name: "status".to_string(),
            var_type: enum_type.clone(),
            default_value: None,
        }],
        vec![],
    );
    let parent_decls = vec![VarDecl {
        name: "status".to_string(),
        var_type: enum_type,
        default_value: None,
    }];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "enum forwarding should be OK: {errors:?}"
    );
}

// -- Include body validation tests ------------------------------------

#[test]
fn include_body_undeclared_var_error() {
    let inc = make_include(
        "child.tmpl.md",
        vec![],
        None,
        vec![], // no declarations in child
        vec![Segment::Expr {
            expr: CompiledExpr::compile("ghost").unwrap(),
            filters: vec![],
        }],
    );
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.iter().any(|e| e.contains("undeclared")),
        "should catch undeclared var in included body: {errors:?}"
    );
}

#[test]
fn include_body_field_on_scalar_error() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("name", "parent_name")],
        None,
        vec![str_decl("name")],
        vec![Segment::Expr {
            expr: CompiledExpr::compile("name.length").unwrap(),
            filters: vec![],
        }],
    );
    let parent_decls = vec![str_decl("parent_name")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot access field") && e.contains("str")),
        "should catch field access on scalar in included body: {errors:?}"
    );
}

#[test]
fn include_body_valid_references_ok() {
    let inc = make_include(
        "child.tmpl.md",
        vec![("msg", "parent_msg")],
        None,
        vec![str_decl("msg")],
        vec![Segment::Expr {
            expr: CompiledExpr::compile("msg").unwrap(),
            filters: vec![],
        }],
    );
    let parent_decls = vec![str_decl("parent_msg")];
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.is_empty(),
        "valid references in body should be OK: {errors:?}"
    );
}

// -- Cycle detection tests -------------------------------------------

#[test]
fn self_recursive_include_no_infinite_loop() {
    // A template that includes itself — should not infinite loop.
    // Boundary is checked, body is walked once.
    let inc = make_include(
        "self.tmpl.md",
        vec![],
        None,
        vec![],
        // Body includes itself again (with the same path).
        vec![Segment::Include(CompiledInclude {
            path: "self.tmpl.md".into(),
            with_vars: vec![],
            for_each: None,
            inline_compiled: Some(CompiledInlineTemplate {
                segments: std::sync::Arc::from(vec![Segment::Static("leaf".into())]),
                declarations: std::sync::Arc::from(vec![]),
            }),
        })],
    );
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    // Should complete without hanging. No errors expected (no params).
    assert!(
        errors.is_empty(),
        "self-recursive with no params should be OK: {errors:?}"
    );
}

#[test]
fn flip_flop_includes_no_infinite_loop() {
    // A→B→A cycle — should not infinite loop.
    let inc_b = CompiledInclude {
        path: "a.tmpl.md".into(),
        with_vars: vec![],
        for_each: None,
        inline_compiled: Some(CompiledInlineTemplate {
            segments: std::sync::Arc::from(vec![Segment::Static("leaf".into())]),
            declarations: std::sync::Arc::from(vec![]),
        }),
    };
    let inc_a = make_include(
        "b.tmpl.md",
        vec![],
        None,
        vec![],
        vec![Segment::Include(inc_b)],
    );
    let segments = vec![Segment::Include(inc_a)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.is_empty(),
        "flip-flop cycle with no params should be OK: {errors:?}"
    );
}

#[test]
fn self_recursive_include_with_type_mismatch_error() {
    // Self-recursive include with wrong type at boundary.
    let inc = make_include(
        "self.tmpl.md",
        vec![("name", "name")],
        None,
        vec![int_decl("name")], // child expects int
        vec![Segment::Include(CompiledInclude {
            path: "self.tmpl.md".into(),
            with_vars: vec![(
                std::borrow::Cow::Borrowed("name"),
                std::borrow::Cow::Borrowed("name"),
            )],
            for_each: None,
            inline_compiled: Some(CompiledInlineTemplate {
                segments: std::sync::Arc::from(vec![]),
                declarations: std::sync::Arc::from(vec![int_decl("name")]),
            }),
        })],
    );
    let parent_decls = vec![str_decl("name")]; // parent has str
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &parent_decls);
    assert!(
        errors.iter().any(|e| e.contains("type mismatch")),
        "should catch type mismatch at recursive boundary: {errors:?}"
    );
}

#[test]
fn self_recursive_include_contract_missing_params_error() {
    // Self-recursive include missing required params at boundary.
    let inc = make_include(
        "self.tmpl.md",
        vec![], // no with vars at outer call
        None,
        vec![str_decl("msg")], // child requires msg
        vec![Segment::Include(CompiledInclude {
            path: "self.tmpl.md".into(),
            with_vars: vec![],
            for_each: None,
            inline_compiled: Some(CompiledInlineTemplate {
                segments: std::sync::Arc::from(vec![]),
                declarations: std::sync::Arc::from(vec![str_decl("msg")]),
            }),
        })],
    );
    let segments = vec![Segment::Include(inc)];
    let errors = validate_field_accesses(&segments, &[]);
    assert!(
        errors.iter().any(|e| e.contains("missing required param")),
        "should catch missing params at recursive boundary: {errors:?}"
    );
}

// -- types_compatible tests ------------------------------------------

#[test]
fn types_compatible_scalars() {
    assert!(types_compatible(&VarType::Str, &VarType::Str));
    assert!(types_compatible(&VarType::Int, &VarType::Int));
    assert!(types_compatible(&VarType::Float, &VarType::Float));
    assert!(types_compatible(&VarType::Bool, &VarType::Bool));
    assert!(!types_compatible(&VarType::Str, &VarType::Int));
    assert!(!types_compatible(&VarType::Int, &VarType::Bool));
}

#[test]
fn types_compatible_untyped_containers() {
    // Untyped list is compatible with typed list.
    assert!(types_compatible(
        &VarType::List(vec![]),
        &VarType::List(vec![str_decl("x")])
    ));
    assert!(types_compatible(
        &VarType::List(vec![str_decl("x")]),
        &VarType::List(vec![])
    ));
    // Same typed lists are compatible.
    assert!(types_compatible(
        &VarType::List(vec![str_decl("x")]),
        &VarType::List(vec![str_decl("x")])
    ));
    // Different typed lists are not.
    assert!(!types_compatible(
        &VarType::List(vec![str_decl("x")]),
        &VarType::List(vec![int_decl("x")])
    ));
}

#[test]
fn types_compatible_cross_kind() {
    assert!(!types_compatible(&VarType::Str, &VarType::List(vec![])));
    assert!(!types_compatible(&VarType::Struct(vec![]), &VarType::Int));
}

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
        list_path: CompiledPath::compile("items"),
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
        arms: vec![(vec!["Confirmed".into()], vec![Segment::Include(inc)])],
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
    let parent_decls = vec![str_decl("flag")];
    let segments = vec![Segment::If {
        branches: vec![(
            super::Condition::Truthy(crate::scope::ConditionOperand::compile("flag").unwrap()),
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

// -- Complex type aliases in types: block ----------------------------------

/// Like `compile_and_check`, but uses the frontmatter's own declarations
/// (including type-alias-resolved params) instead of externally passed decls.
fn compile_and_check_self(template: &str) -> Vec<String> {
    let (fm, body) = crate::parse_frontmatter(template).expect("parse");
    let (segments, _) = crate::compiled::compile(body, &fm.type_aliases).expect("compile");
    validate_field_accesses(&segments, &fm.declarations)
}

#[test]
fn list_type_alias_in_types_block() {
    let tmpl = "---\nname: t\ntypes:\n  - TaskList = list<title = str, score = int>\nparams:\n  - tasks = TaskList\n---\n\
                     > {% for b in tasks %}{{ b.title }}: {{ b.score }}\n> {% /for %}";
    let errors = compile_and_check_self(tmpl);
    assert!(errors.is_empty(), "list type alias should work: {errors:?}");
}

#[test]
fn chained_type_alias_enum_in_list() {
    let tmpl = "---\nname: t\ntypes:\n  - Severity = enum<High, Medium, Low>\n  - TaskReport = list<title = str, severity = Severity>\nparams:\n  - tasks = TaskReport\n---\n\
                     > {% for b in tasks %}{{ b.title }} {% match b.severity %}{% case High %}🔴{% case Medium %}🟡{% case Low %}🟢{% /match %}\n> {% /for %}";
    let errors = compile_and_check_self(tmpl);
    assert!(
        errors.is_empty(),
        "chained type alias should work: {errors:?}"
    );
}

// -- Opaque roots (import stems / consts) --------------------------------

fn compile_and_check_with_opaque(
    template: &str,
    decls: &[VarDecl],
    opaque: &[&str],
) -> Vec<String> {
    let (_fm, body) = crate::parse_frontmatter(template).expect("parse");
    let empty_aliases = crate::compat::HashMap::new();
    let (segments, _) = crate::compiled::compile(body, &empty_aliases).expect("compile");
    let opaque_set: HashSet<&str> = opaque.iter().copied().collect();
    validate_field_accesses_with_opaque(&segments, decls, &opaque_set)
}

#[test]
fn opaque_root_skips_undeclared_error() {
    // `imported.NOTEBOOK_FILENAME` should not error when `imported` is opaque.
    let errors = compile_and_check_with_opaque(
        "---\nparams: []\n---\n{{ imported.NOTEBOOK_FILENAME }}",
        &[],
        &["imported"],
    );
    assert!(
        errors.is_empty(),
        "opaque root should skip validation: {errors:?}"
    );
}

#[test]
fn opaque_root_dotted_path_deep() {
    // Deep dotted path on opaque root should also be skipped.
    let errors = compile_and_check_with_opaque(
        "---\nparams: []\n---\n{{ config.PHASES.EXPLORE }}",
        &[],
        &["config"],
    );
    assert!(
        errors.is_empty(),
        "deep opaque path should skip: {errors:?}"
    );
}

#[test]
fn opaque_root_bare_reference() {
    // Bare reference to opaque root (no dotted path) should also be valid.
    let errors = compile_and_check_with_opaque(
        "---\nparams: []\n---\n{{ MAX_RETRIES }}",
        &[],
        &["MAX_RETRIES"],
    );
    assert!(
        errors.is_empty(),
        "bare opaque reference should work: {errors:?}"
    );
}

#[test]
fn non_opaque_unknown_variable_still_errors() {
    // Unknown variable that is NOT opaque should still error.
    let errors = compile_and_check_with_opaque(
        "---\nparams: []\n---\n{{ unknown_var }}",
        &[],
        &["imported"],
    );
    assert_eq!(errors.len(), 1, "non-opaque unknown should error");
    assert!(errors[0].contains("undeclared variable"));
}

#[test]
fn opaque_root_in_conditional() {
    // Opaque roots used in if conditions should not error.
    let errors = compile_and_check_with_opaque(
        "---\nparams: []\n---\n> {% if imported.ENABLED %}yes> {% /if %}",
        &[],
        &["imported"],
    );
    assert!(errors.is_empty(), "opaque root in conditional: {errors:?}");
}

#[test]
fn opaque_root_coexists_with_typed_params() {
    // Opaque roots and typed params should coexist.
    let decls = vec![VarDecl {
        name: "name".to_string(),
        var_type: VarType::Str,
        default_value: None,
    }];
    let errors = compile_and_check_with_opaque(
        "---\nparams:\n  - name = str\n---\n{{ name }} {{ imported.CONST }}",
        &decls,
        &["imported"],
    );
    assert!(
        errors.is_empty(),
        "mixed opaque and typed should work: {errors:?}"
    );
}

#[test]
fn multiple_opaque_roots() {
    // Multiple opaque roots should all be recognized.
    let errors = compile_and_check_with_opaque(
        "---\nparams: []\n---\n{{ imported.X }} {{ config.Y }} {{ MAX }}",
        &[],
        &["imported", "config", "MAX"],
    );
    assert!(errors.is_empty(), "multiple opaque roots: {errors:?}");
}

#[test]
fn empty_opaque_set_behaves_like_normal() {
    // Empty opaque set = normal validation.
    let errors = compile_and_check_with_opaque("---\nparams: []\n---\n{{ unknown }}", &[], &[]);
    assert_eq!(errors.len(), 1, "empty opaque set should not help");
}
