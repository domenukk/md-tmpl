//! Collision and scope tests for prompt-templates.
//!
//! Covers name collision rules (R1–R3, R11–R13), scope isolation of inline
//! templates (`{% tmpl %}`), and edge cases around constants, imports,
//! type aliases, and for-loop bindings.

use crate::{CompileOptions, Context, Template};

// ============================================================================
// 1. Const as param default — SUPPORTED (resolved via available_consts)
// ============================================================================

/// Local const reference used as default value for a param should succeed.
///
/// The parser resolves const names in the default position by looking them
/// up in the available constants map built from `consts:` declarations.
#[test]
fn const_ref_as_param_default_succeeds() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - MAX = int := 10\n",
        "params:\n",
        "  - count = int := MAX\n",
        "---\n",
        "{{ count }}",
    );
    let tmpl = Template::from_source(src)
        .expect("const name as param default should resolve to the const value");
    // Render without passing `count` — should use the default from the const.
    let ctx = Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "10");
}

/// Imported const reference as default should also succeed.
#[test]
fn imported_const_ref_as_param_default_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("lib.tmpl.md"),
        concat!(
            "---\n",
            "name: lib\n",
            "consts:\n",
            "  - LIMIT = int := 50\n",
            "---\n",
        ),
    )
    .unwrap();

    let src = concat!(
        "---\n",
        "imports:\n",
        "  - [lib](lib.tmpl.md)\n",
        "params:\n",
        "  - count = int := lib.LIMIT\n",
        "---\n",
        "{{ count }}",
    );
    let (tmpl, _fm) = Template::compile(src, CompileOptions::default().base_dir(dir.path()))
        .expect("imported const ref as default should resolve");
    let ctx = Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "50");
}

// ============================================================================
// 2. Param name shadows import stem — REJECTED (Rule 2b)
// ============================================================================

/// Param whose `PascalCase` matches an import stem → rejected.
#[test]
fn param_pascal_shadows_import_stem_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Helper.tmpl.md"),
        concat!("---\n", "name: Helper\n", "params: []\n", "---\n",),
    )
    .unwrap();

    let src = concat!(
        "---\n",
        "imports:\n",
        "  - [Helper](Helper.tmpl.md)\n",
        "params:\n",
        "  - helper = str\n",
        "---\n",
        "{{ helper }}",
    );
    let err = Template::compile(src, CompileOptions::default().base_dir(dir.path()))
        .expect_err("param 'helper' (PascalCase 'Helper') should shadow import 'Helper'");
    let msg = err.to_string();
    assert!(
        msg.contains("shadows import"),
        "expected import shadow error, got: {msg}",
    );
}

/// Const whose `PascalCase` matches an import stem → rejected.
#[test]
fn const_pascal_shadows_import_stem_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Config.tmpl.md"),
        concat!("---\n", "name: Config\n", "params: []\n", "---\n",),
    )
    .unwrap();

    let src = concat!(
        "---\n",
        "imports:\n",
        "  - [Config](Config.tmpl.md)\n",
        "consts:\n",
        "  - config = int := 1\n",
        "---\n",
        "{{ config }}",
    );
    let err = Template::compile(src, CompileOptions::default().base_dir(dir.path()))
        .expect_err("const 'config' (PascalCase 'Config') should shadow import 'Config'");
    let msg = err.to_string();
    assert!(
        msg.contains("shadows import"),
        "expected import shadow error, got: {msg}",
    );
}

// ============================================================================
// 3. Param name shadows type alias — REJECTED (Rule 1) unless type matches
// ============================================================================

/// Param shadows type alias with a DIFFERENT type → rejected.
#[test]
fn param_shadows_type_alias_different_type_rejected() {
    let src = concat!(
        "---\n",
        "types:\n",
        "  - Level = enum<High, Low>\n",
        "params:\n",
        "  - level = str\n",
        "---\n",
        "{{ level }}",
    );
    let err = Template::from_source(src)
        .expect_err("param 'level' (PascalCase 'Level') with different type should conflict");
    let msg = err.to_string();
    assert!(
        msg.contains("conflicts with type alias"),
        "expected type-param conflict error, got: {msg}",
    );
}

/// Param shadows type alias with the SAME type → allowed (R1 exception).
#[test]
fn param_shadows_type_alias_same_type_allowed() {
    let src = concat!(
        "---\n",
        "types:\n",
        "  - Level = enum<High, Low>\n",
        "params:\n",
        "  - level = Level\n",
        "---\n",
        "{{ level }}",
    );
    let tmpl = Template::from_source(src)
        .expect("param type IS the alias → R1 exception should allow this");
    let mut ctx = Context::new();
    ctx.set("level", "High");
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "High");
}

/// Const shadows type alias with a different type → rejected.
#[test]
fn const_shadows_type_alias_different_type_rejected() {
    let src = concat!(
        "---\n",
        "types:\n",
        "  - Status = enum<Active, Paused>\n",
        "consts:\n",
        "  - status = str := \"override\"\n",
        "---\n",
        "{{ status }}",
    );
    let err = Template::from_source(src)
        .expect_err("const 'status' (PascalCase 'Status') with different type should conflict");
    let msg = err.to_string();
    assert!(
        msg.contains("conflicts with type alias"),
        "expected type-const conflict error, got: {msg}",
    );
}

// ============================================================================
// 4. Param name shadows const name — REJECTED (Rule 3)
// ============================================================================

/// Param and const with the same name → rejected (param declared first).
#[test]
fn param_then_const_same_name_rejected() {
    let src = concat!(
        "---\n",
        "params:\n",
        "  - level = str\n",
        "consts:\n",
        "  - level = str := \"fixed\"\n",
        "---\n",
        "{{ level }}",
    );
    let err = Template::from_source(src)
        .expect_err("param and const with same name 'level' should conflict");
    let msg = err.to_string();
    assert!(
        msg.contains("parameter name conflicts with constant name"),
        "expected param-const conflict error, got: {msg}",
    );
}

/// Const then param with same name → also rejected (order reversed).
#[test]
fn const_then_param_same_name_rejected() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - level = str := \"fixed\"\n",
        "params:\n",
        "  - level = str\n",
        "---\n",
        "{{ level }}",
    );
    let err = Template::from_source(src)
        .expect_err("const then param with same name 'level' should conflict");
    let msg = err.to_string();
    assert!(
        msg.contains("parameter name conflicts with constant name"),
        "expected param-const conflict error, got: {msg}",
    );
}

// ============================================================================
// 5. Import stem shadows type alias — REJECTED (Rule 2)
// ============================================================================

/// Type alias name equals an import stem → rejected.
#[test]
fn type_alias_shadows_import_stem_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Utils.tmpl.md"),
        concat!("---\n", "name: Utils\n", "params: []\n", "---\n",),
    )
    .unwrap();

    let src = concat!(
        "---\n",
        "imports:\n",
        "  - [Utils](Utils.tmpl.md)\n",
        "types:\n",
        "  - Utils = enum<A, B>\n",
        "params:\n",
        "  - x = Utils\n",
        "---\n",
        "{{ x }}",
    );
    let err = Template::compile(src, CompileOptions::default().base_dir(dir.path()))
        .expect_err("type alias 'Utils' shadowing import stem 'Utils' should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("shadow") && msg.contains("import"),
        "expected import shadow error, got: {msg}",
    );
}

// ============================================================================
// 6. Inline tmpl name shadows param — REJECTED (Rule 12)
// ============================================================================

/// Inline template with the same name as a param → rejected.
#[test]
fn inline_tmpl_shadows_param_rejected() {
    let src = concat!(
        "---\n",
        "params: [widget = str]\n",
        "---\n",
        "> {% tmpl widget %}\n",
        "---\n",
        "params: []\n",
        "---\n",
        "inner\n",
        "\n",
        "> {% /tmpl %}\n",
        "\n",
        "{{ widget }}",
    );
    let err = Template::from_source(src)
        .expect_err("inline template 'widget' should conflict with param 'widget'");
    let msg = err.to_string();
    assert!(
        msg.contains("inline template name 'widget'")
            && msg.contains("conflicts with a declared parameter or constant"),
        "expected param/tmpl collision error, got: {msg}",
    );
}

/// Inline template with the same name as a const → rejected.
#[test]
fn inline_tmpl_shadows_const_rejected() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - widget = str := \"fixed\"\n",
        "---\n",
        "> {% tmpl widget %}\n",
        "---\n",
        "params: []\n",
        "---\n",
        "inner\n",
        "\n",
        "> {% /tmpl %}\n",
        "\n",
        "{{ widget }}",
    );
    let err = Template::from_source(src)
        .expect_err("inline template 'widget' should conflict with const 'widget'");
    let msg = err.to_string();
    assert!(
        msg.contains("inline template name 'widget'")
            && msg.contains("conflicts with a declared parameter or constant"),
        "expected const/tmpl collision error, got: {msg}",
    );
}

// ============================================================================
// 7. Inline tmpl name shadows import stem — REJECTED (Rule 11)
// ============================================================================

/// Inline template with the same name as an import stem → rejected.
#[test]
fn inline_tmpl_shadows_import_stem_rejected() {
    let src = concat!(
        "---\n",
        "imports: [[shared](shared.tmpl.md)]\n",
        "params: [x = str]\n",
        "allow_unused: true\n",
        "---\n",
        "> {% tmpl shared %}\n",
        "\n",
        "---\n",
        "params: []\n",
        "---\n",
        "inner\n",
        "\n",
        "> {% /tmpl %}\n",
        "\n",
        "{{ x }}",
    );
    let err = Template::from_source(src)
        .expect_err("inline template 'shared' should conflict with import stem 'shared'");
    let msg = err.to_string();
    assert!(
        msg.contains("import stem") && msg.contains("conflicts with inline template"),
        "expected import/tmpl collision error, got: {msg}",
    );
}

// ============================================================================
// 8. For-loop binding shadows declared name — REJECTED (Rule 13)
// ============================================================================

/// For-loop binding `item` shadows param named `item` → rejected.
#[test]
fn for_binding_shadows_param_rejected() {
    let src = concat!(
        "---\n",
        "params:\n",
        "  - items = list<name = str>\n",
        "  - item = str\n",
        "---\n",
        "> {% for item in items %}{{ item.name }}\n",
        "\n",
        "> {% /for %}\n",
        "\n",
        "{{ item }}",
    );
    let err =
        Template::from_source(src).expect_err("for binding 'item' should shadow param 'item'");
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'item'"),
        "expected for-binding shadow error, got: {msg}",
    );
}

/// For-loop binding shadows const → rejected.
#[test]
fn for_binding_shadows_const_rejected() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - item = str := \"fixed\"\n",
        "params:\n",
        "  - items = list<name = str>\n",
        "---\n",
        "> {% for item in items %}{{ item.name }}\n",
        "\n",
        "> {% /for %}",
    );
    let err =
        Template::from_source(src).expect_err("for binding 'item' should shadow const 'item'");
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'item'"),
        "expected for-binding shadow const error, got: {msg}",
    );
}

/// For-loop binding shadows import stem → rejected.
#[test]
fn for_binding_shadows_import_stem_rejected() {
    let src = concat!(
        "---\n",
        "imports:\n",
        "  - \"[lib](lib.tmpl.md)\"\n",
        "params:\n",
        "  - items = list<name = str>\n",
        "allow_unused: true\n",
        "---\n",
        "> {% for lib in items %}{{ lib.name }}\n",
        "\n",
        "> {% /for %}",
    );
    let err =
        Template::from_source(src).expect_err("for binding 'lib' should shadow import stem 'lib'");
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'lib'"),
        "expected for-binding shadow import error, got: {msg}",
    );
}

/// For-loop binding shadows inline template name → rejected.
#[test]
fn for_binding_shadows_inline_tmpl_rejected() {
    let src = concat!(
        "---\n",
        "params: [items = list<name = str>]\n",
        "allow_unused: true\n",
        "---\n",
        "> {% tmpl card %}\n",
        "---\n",
        "params: [title = str]\n",
        "---\n",
        "{{ title }}\n",
        "\n",
        "> {% /tmpl %}\n",
        "> {% for card in items %}{{ card.name }}\n",
        "> {% /for %}",
    );
    let err = Template::from_source(src)
        .expect_err("for binding 'card' should shadow inline template 'card'");
    let msg = err.to_string();
    assert!(
        msg.contains("for loop binding shadows") && msg.contains("'card'"),
        "expected for-binding shadow tmpl error, got: {msg}",
    );
}

// ============================================================================
// 9. Nested tmpl accessing parent types and imports — ISOLATED scope
// ============================================================================

/// Inline templates inherit parent's type aliases.
///
/// A child `{% tmpl %}` can reference types defined in the parent's frontmatter.
#[test]
fn nested_tmpl_inherits_parent_type_aliases() {
    let src = concat!(
        "---\n",
        "types:\n",
        "  - Level = enum<High, Low>\n",
        "params:\n",
        "  - priority = Level\n",
        "---\n",
        "> {% tmpl child %}\n",
        "---\n",
        "params:\n",
        "  - val = Level\n",
        "---\n",
        "{{ val }}\n",
        "\n",
        "> {% /tmpl %}\n",
        "\n",
        "{{ priority }}",
    );
    // Inline templates inherit the parent's types — this should compile.
    let tmpl =
        Template::from_source(src).expect("child template should inherit parent's type aliases");
    let mut ctx = Context::new();
    ctx.set("priority", "High");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("High"),
        "expected parent type to work, got: {output}",
    );
}

/// Inline templates inherit parent constants.
///
/// Constants defined in the parent scope ARE accessible in the child template.
#[test]
fn nested_tmpl_inherits_parent_consts() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - VERSION = str := \"1.0\"\n",
        "params: []\n",
        "---\n",
        "> {% tmpl child %}\n",
        "---\n",
        "params: []\n",
        "---\n",
        "{{ VERSION }}\n",
        "\n",
        "> {% /tmpl %}\n",
        "\n",
        "{{ VERSION }}",
    );
    // The parent's VERSION should be visible to the child.
    let tmpl =
        Template::from_source(src).expect("child template should inherit parent's constants");
    let ctx = Context::new();
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("1.0"),
        "expected parent const to render, got: {output}",
    );
}

/// Positive case: nested tmpl can define and use its own types.
#[test]
fn nested_tmpl_can_define_own_types() {
    let src = concat!(
        "---\n",
        "params: [name = str]\n",
        "---\n",
        "> {% tmpl greeting %}\n",
        "---\n",
        "params: [who = str]\n",
        "---\n",
        "Hello {{ who }}!\n",
        "\n",
        "> {% /tmpl %}\n",
        "\n",
        "> {% include greeting with who = name %}\n",
    );
    let tmpl = Template::from_source(src).expect("nested tmpl with its own params should compile");
    let mut ctx = Context::new();
    ctx.set("name", "World");
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("Hello World!"),
        "expected nested template output, got: {output}",
    );
}

/// Inline templates' own consts are propagated into the render scope —
/// the template renders successfully using its own const values.
#[test]
fn nested_tmpl_own_consts_in_render_scope() {
    let src = concat!(
        "---\n",
        "params: []\n",
        "---\n",
        "> {% tmpl versioned %}\n",
        "---\n",
        "consts:\n",
        "  - V = str := \"2.0\"\n",
        "params: []\n",
        "---\n",
        "v{{ V }}\n",
        "\n",
        "> {% /tmpl %}\n",
        "\n",
        "> {% include versioned %}\n",
    );
    let tmpl = Template::from_source(src).expect("inline tmpl with own consts should compile");
    let ctx = Context::new();
    // Render succeeds: inline template's own consts are now properly
    // injected into the include render scope.
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(
        output.contains("v2.0"),
        "expected inline const to render, got: {output}",
    );
}

// ============================================================================
// 10. Duplicate const names — REJECTED
// ============================================================================

/// Two constants with the same name → rejected.
#[test]
fn duplicate_const_names_rejected() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - X = int := 1\n",
        "  - X = int := 2\n",
        "---\n",
        "{{ X }}",
    );
    let err = Template::from_source(src).expect_err("duplicate constant name should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains('X'),
        "expected duplicate const error, got: {msg}",
    );
}

/// Duplicate const names with different types → also rejected.
#[test]
fn duplicate_const_names_different_types_rejected() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - LABEL = str := \"hello\"\n",
        "  - LABEL = int := 42\n",
        "---\n",
        "{{ LABEL }}",
    );
    let err = Template::from_source(src)
        .expect_err("duplicate constant with different types should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains("LABEL"),
        "expected duplicate const error, got: {msg}",
    );
}

/// Three consts, two share a name → rejected.
#[test]
fn duplicate_const_among_multiple_rejected() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - A = int := 1\n",
        "  - B = int := 2\n",
        "  - A = int := 3\n",
        "---\n",
        "{{ A }} {{ B }}",
    );
    let err = Template::from_source(src)
        .expect_err("duplicate 'A' among multiple consts should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains('A'),
        "expected duplicate const error, got: {msg}",
    );
}

// ============================================================================
// 11. Const name conflicts with type alias — more edge cases
// ============================================================================

/// Const with `PascalCase` name matching a type alias (different type) → rejected.
#[test]
fn const_pascal_conflicts_with_type_alias_rejected() {
    let src = concat!(
        "---\n",
        "types:\n",
        "  - Stage = enum<Design, Build>\n",
        "consts:\n",
        "  - Stage = str := \"override\"\n",
        "params: []\n",
        "---\n",
        "{{ Stage }}",
    );
    let err = Template::from_source(src)
        .expect_err("const 'Stage' conflicting with type alias 'Stage' should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("conflicts with type alias") || msg.contains("conflict"),
        "expected type/const conflict error, got: {msg}",
    );
}

/// Const named `my_phase` where `MyPhase` doesn't exist as type → OK.
#[test]
fn const_pascal_no_matching_type_alias_ok() {
    let src = concat!(
        "---\n",
        "consts:\n",
        "  - my_val = int := 42\n",
        "params: []\n",
        "---\n",
        "{{ my_val }}",
    );
    let tmpl =
        Template::from_source(src).expect("const with no matching type alias should compile fine");
    let ctx = Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "42");
}

/// Const whose `PascalCase` matches type alias but types are the same → allowed
/// (R1 exception applies to consts too).
#[test]
fn const_pascal_matches_type_alias_same_type_allowed() {
    let src = concat!(
        "---\n",
        "types:\n",
        "  - DefaultLevel = enum<High, Low>\n",
        "consts:\n",
        "  - default_level = DefaultLevel := High\n",
        "params: []\n",
        "---\n",
        "{{ kind(default_level) }}",
    );
    let tmpl = Template::from_source(src)
        .expect("const whose PascalCase matches alias with same type should be allowed");
    let ctx = Context::new();
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "High");
}

/// Multiple type aliases and consts — only the conflicting pair is rejected.
#[test]
fn multiple_types_and_consts_only_conflict_rejected() {
    let src = concat!(
        "---\n",
        "types:\n",
        "  - Color = enum<Red, Blue>\n",
        "  - Size = enum<Big, Small>\n",
        "consts:\n",
        "  - color = str := \"override\"\n",
        "params: []\n",
        "---\n",
        "{{ color }}",
    );
    let err = Template::from_source(src)
        .expect_err("const 'color' (PascalCase 'Color') with different type should conflict");
    let msg = err.to_string();
    assert!(
        msg.contains("conflicts with type alias") && msg.contains("Color"),
        "expected specific conflict with 'Color', got: {msg}",
    );
}
