use super::*;

#[test]
fn pascal_case_via_core_crate() {
    assert_eq!(to_pascal_case("code_review"), "CodeReview");
    assert_eq!(to_pascal_case("simple_greeting"), "SimpleGreeting");
    assert_eq!(to_pascal_case("task-report"), "TaskReport");
    assert_eq!(to_pascal_case("single"), "Single");
    assert_eq!(to_pascal_case(""), "");
    assert_eq!(to_pascal_case("already_PascalCase"), "AlreadyPascalCase");
}

#[test]
fn vartype_annotations() {
    // Scalar types ignore parent/field context.
    assert_eq!(
        vartype_to_python_annotation(&VarType::Str, "Parent", "field"),
        "str"
    );
    assert_eq!(
        vartype_to_python_annotation(&VarType::Int, "Parent", "field"),
        "int"
    );
    assert_eq!(
        vartype_to_python_annotation(&VarType::Float, "Parent", "field"),
        "float"
    );
    assert_eq!(
        vartype_to_python_annotation(&VarType::Bool, "Parent", "field"),
        "bool"
    );

    // Empty compound types.
    assert_eq!(
        vartype_to_python_annotation(&VarType::List(vec![]), "Parent", "items"),
        "list"
    );
    assert_eq!(
        vartype_to_python_annotation(&VarType::Struct(vec![]), "Parent", "config"),
        "dict"
    );

    // Non-empty list → list[ParentItemsItem].
    assert_eq!(
        vartype_to_python_annotation(
            &VarType::List(vec![VarDecl {
                name: "x".into(),
                var_type: VarType::Str,
                default_value: None,
            }]),
            "Params",
            "items"
        ),
        "list[ParamsItemsItem]"
    );

    // Non-empty struct → PascalCase(field_name).
    assert_eq!(
        vartype_to_python_annotation(
            &VarType::Struct(vec![VarDecl {
                name: "x".into(),
                var_type: VarType::Str,
                default_value: None,
            }]),
            "Params",
            "config"
        ),
        "Config"
    );

    // Enum → PascalCase(field_name).
    assert_eq!(
        vartype_to_python_annotation(&VarType::Enum(vec![]), "Params", "status"),
        "Status"
    );

    // Tmpl stays opaque.
    assert_eq!(
        vartype_to_python_annotation(&VarType::Tmpl(vec![]), "Params", "body"),
        "object"
    );
}

#[test]
fn source_gen_enum() {
    let source = source_gen_enum_class(
        "Status",
        &[
            VariantDecl {
                name: "Approved".into(),
                fields: vec![],
            },
            VariantDecl {
                name: "Rejected".into(),
                fields: vec![],
            },
            VariantDecl {
                name: "NeedsChanges".into(),
                fields: vec![VarDecl {
                    name: "reason".into(),
                    var_type: VarType::Str,
                    default_value: None,
                }],
            },
        ],
        &[],
    );
    assert!(source.contains("class Status(Variants):"));
    assert!(source.contains("Approved = ()"));
    assert!(source.contains("Rejected = ()"));
    assert!(source.contains("NeedsChanges = {\"reason\": str}"));
}

#[test]
fn source_gen_model() {
    let source = source_gen_model_class(
        "ReviewItem",
        &[
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
        ],
        &[],
    );
    assert!(source.contains("@dataclass"));
    assert!(source.contains("class ReviewItem:"));
    assert!(source.contains("    name: str"));
    assert!(source.contains("    score: int"));
}

#[test]
fn source_gen_params() {
    let mut out = String::new();
    source_gen_params_class(
        &mut out,
        "ReviewParams",
        &[
            VarDecl {
                name: "reviewer".into(),
                var_type: VarType::Str,
                default_value: None,
            },
            VarDecl {
                name: "score".into(),
                var_type: VarType::Float,
                default_value: None,
            },
        ],
        "prompts/review.tmpl.md",
        &[],
    );
    assert!(out.contains("@dataclass"));
    assert!(out.contains("class ReviewParams:"));
    assert!(out.contains("    reviewer: str"));
    assert!(out.contains("    score: float"));
}

#[test]
fn source_gen_params_has_render_method() {
    let mut out = String::new();
    source_gen_params_class(
        &mut out,
        "Greeting",
        &[VarDecl {
            name: "name".into(),
            var_type: VarType::Str,
            default_value: None,
        }],
        "prompts/greeting.tmpl.md",
        &[],
    );
    assert!(
        out.contains("def render(self, template: Template | None = None) -> str:"),
        "missing render method, got: {out}"
    );
    assert!(
        out.contains("Template.from_file("),
        "render should load from file, got: {out}"
    );
    assert!(
        out.contains("render_dict(dataclasses.asdict(self))"),
        "render should use render_dict, got: {out}"
    );
}

#[test]
fn source_gen_params_empty_still_has_render() {
    let mut out = String::new();
    source_gen_params_class(&mut out, "Empty", &[], "empty.tmpl.md", &[]);
    assert!(
        out.contains("def render(self"),
        "empty params class should still have render(), got: {out}"
    );
}

#[test]
fn source_gen_params_with_defaults() {
    let mut out = String::new();
    source_gen_params_class(
        &mut out,
        "Greeting",
        &[
            VarDecl {
                name: "name".into(),
                var_type: VarType::Str,
                default_value: Some(md_tmpl::Value::Str("World".into())),
            },
            VarDecl {
                name: "count".into(),
                var_type: VarType::Int,
                default_value: Some(md_tmpl::Value::Int(1)),
            },
        ],
        "greeting.tmpl.md",
        &[],
    );
    assert!(
        out.contains("name: str = field(default=\"World\")"),
        "missing default for name, got: {out}"
    );
    assert!(
        out.contains("count: int = field(default=1)"),
        "missing default for count, got: {out}"
    );
}

#[test]
fn source_gen_params_required_before_optional() {
    let mut out = String::new();
    source_gen_params_class(
        &mut out,
        "Mixed",
        &[
            VarDecl {
                name: "optional_first".into(),
                var_type: VarType::Str,
                default_value: Some(md_tmpl::Value::Str("default".into())),
            },
            VarDecl {
                name: "required".into(),
                var_type: VarType::Str,
                default_value: None,
            },
        ],
        "mixed.tmpl.md",
        &[],
    );
    // Required fields must come before optional in the output.
    let req_pos = out.find("required: str").expect("should have required");
    let opt_pos = out
        .find("optional_first: str = field")
        .expect("should have optional");
    assert!(
        req_pos < opt_pos,
        "required field must come before optional, got: {out}"
    );
}

#[test]
fn source_gen_option_type_annotation() {
    assert_eq!(
        vartype_to_python_annotation(&VarType::Option(Box::new(VarType::Str)), "Parent", "name"),
        "Optional[str]"
    );
    assert_eq!(
        vartype_to_python_annotation(&VarType::Option(Box::new(VarType::Int)), "Parent", "count"),
        "Optional[int]"
    );
}

#[test]
fn decls_need_optional_detects_option() {
    assert!(decls_need_optional(&[VarDecl {
        name: "x".into(),
        var_type: VarType::Option(Box::new(VarType::Str)),
        default_value: None,
    }]));
    assert!(!decls_need_optional(&[VarDecl {
        name: "x".into(),
        var_type: VarType::Str,
        default_value: None,
    }]));
}

#[test]
fn decls_need_field_detects_defaults() {
    assert!(decls_need_field(&[VarDecl {
        name: "x".into(),
        var_type: VarType::Str,
        default_value: Some(md_tmpl::Value::Str("hi".into())),
    }]));
    assert!(!decls_need_field(&[VarDecl {
        name: "x".into(),
        var_type: VarType::Str,
        default_value: None,
    }]));
}

#[test]
fn default_to_python_repr_scalars() {
    use md_tmpl::Value;
    assert_eq!(
        default_to_python_repr(&Value::Str("hello".into())),
        "\"hello\""
    );
    assert_eq!(default_to_python_repr(&Value::Int(42)), "42");
    assert_eq!(default_to_python_repr(&Value::Float(2.72)), "2.72");
    assert_eq!(default_to_python_repr(&Value::Bool(true)), "True");
    assert_eq!(default_to_python_repr(&Value::Bool(false)), "False");
    assert_eq!(default_to_python_repr(&Value::None), "None");
}

#[test]
fn default_to_python_repr_collections() {
    use std::sync::Arc;

    use md_tmpl::Value;

    assert_eq!(
        default_to_python_repr(&Value::List(Arc::new(vec![Value::Int(1), Value::Int(2),]))),
        "[1, 2]"
    );
    assert_eq!(default_to_python_repr(&Value::List(Arc::new(vec![]))), "[]");
}
