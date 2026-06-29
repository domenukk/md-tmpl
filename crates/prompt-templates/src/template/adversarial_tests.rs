//! Adversarial and edge-case tests for the template engine.
//!
//! Covers:
//! - Include depth limits (self-recursive, chain at boundary, custom limit)
//! - Circular import detection (2-cycle, 3-cycle)
//! - Frontmatter collision rules (R1–R4 positive and negative)
//! - Adversarial inputs (huge templates, empty containers, filter chains)
//! - Whitespace control edge cases (`{%-`, `-%}`)
//! - Match/case edge cases (non-enum, no-match, case-after-default)

use std::sync::Arc;

use crate::{CompileOptions, Context, Template, Value};

// ============================================================================
// A. Include Depth Limit Tests
// ============================================================================

/// Build a chain of N templates: t0 includes t1, t1 includes t2, …, t(N-1)
/// is a leaf. Returns the path to the root template.
fn build_include_chain(dir: &std::path::Path, depth: usize) -> std::path::PathBuf {
    // Leaf template at the end of the chain.
    let leaf_name = format!("t{depth}.tmpl.md");
    std::fs::write(
        dir.join(&leaf_name),
        format!(
            r"---
name: t{depth}
params: []
---
LEAF"
        ),
    )
    .unwrap();

    // Build intermediate templates from depth-1 down to 0.
    for i in (0..depth).rev() {
        let next_name = format!("./t{}.tmpl.md", i + 1);
        let this_name = format!("t{i}.tmpl.md");
        let source = format!(
            r"---
name: t{i}
params: []
---
{i}+{{% include [t{}]({next_name}) %}}",
            i + 1
        );
        std::fs::write(dir.join(&this_name), source).unwrap();
    }

    dir.join("t0.tmpl.md")
}

/// 15-level include chain renders successfully with default depth limit (16).
#[test]
fn depth_limit_15_renders_ok() {
    let dir = tempfile::tempdir().unwrap();
    let root = build_include_chain(dir.path(), 15);
    let tmpl = Template::from_file(&root).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    // Should contain all intermediate levels and the leaf.
    assert_eq!(result, "0+1+2+3+4+5+6+7+8+9+10+11+12+13+14+LEAF");
}

/// 16-level include chain renders successfully (exactly at default limit).
#[test]
fn depth_limit_16_renders_ok() {
    let dir = tempfile::tempdir().unwrap();
    let root = build_include_chain(dir.path(), 16);
    let tmpl = Template::from_file(&root).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(result, "0+1+2+3+4+5+6+7+8+9+10+11+12+13+14+15+LEAF");
}

/// 17-level include chain exceeds default depth limit (16) and errors.
#[test]
fn depth_limit_17_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = build_include_chain(dir.path(), 17);
    let tmpl = Template::from_file(&root).unwrap();
    let err = tmpl
        .render_ctx(&Context::new())
        .expect_err("17-level chain should exceed depth limit 16");
    let msg = err.to_string();
    assert!(
        msg.contains("include depth"),
        "error should mention depth limit: {msg}"
    );
}

// ============================================================================
// B. Circular Include Tests (detected via runtime depth limit)
// ============================================================================

/// Two-template mutual include cycle: A includes B, B includes A.
/// The engine catches this at render time via the include depth limit.
#[test]
fn circular_include_two_cycle_detected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.tmpl.md"),
        r"---
name: a
params: []
---
A> {% include [b](./b.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        r"---
name: b
params: []
---
B> {% include [a](./a.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let err = tmpl
        .render_ctx(&Context::new())
        .expect_err("2-cycle include should hit depth limit");
    let msg = err.to_string();
    assert!(
        msg.contains("include depth"),
        "error should mention include depth: {msg}"
    );
}

/// Three-template include cycle: A → B → C → A.
#[test]
fn circular_include_three_cycle_detected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.tmpl.md"),
        r"---
name: a
params: []
---
A> {% include [b](./b.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        r"---
name: b
params: []
---
B> {% include [c](./c.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("c.tmpl.md"),
        r"---
name: c
params: []
---
C> {% include [a](./a.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let err = tmpl
        .render_ctx(&Context::new())
        .expect_err("3-cycle include should hit depth limit");
    let msg = err.to_string();
    assert!(
        msg.contains("include depth"),
        "error should mention include depth: {msg}"
    );
}

// ============================================================================
// C. Collision Rule Tests
// ============================================================================

// --- Rule: Duplicate parameter name ---

#[test]
fn collision_duplicate_param_rejected() {
    let source = r"---
params: [name = str, name = str]
---
{{ name }}
";
    let err = Template::from_source(source).expect_err("duplicate param should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains("name"),
        "error should mention duplicate param 'name': {msg}"
    );
}

#[test]
fn collision_distinct_params_ok() {
    let source = r"---
params: [first_name = str, last_name = str]
---
{{ first_name }} {{ last_name }}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("first_name", "Alice");
    ctx.set("last_name", "Smith");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"Alice Smith
"
    );
}

// --- Rule: Reserved keyword as param name ---

#[test]
fn collision_reserved_keyword_param_rejected() {
    let source = r"---
params: [list = str]
---
{{ list }}
";
    let err =
        Template::from_source(source).expect_err("reserved keyword 'list' as param should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("reserved keyword"),
        "error should mention reserved keyword: {msg}"
    );
}

#[test]
fn collision_non_reserved_param_ok() {
    let source = r"---
params: [my_list = str]
---
{{ my_list }}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("my_list", "items");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"items
"
    );
}

// --- Rule: Duplicate type alias ---

#[test]
fn collision_duplicate_type_alias_rejected() {
    let source = r"---
types:
  - Foo = enum(A, B)
  - Foo = enum(X, Y)

params: [x = Foo]
---
{{ x }}
";
    let err = Template::from_source(source).expect_err("duplicate type alias should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains("Foo"),
        "error should mention duplicate type alias 'Foo': {msg}"
    );
}

#[test]
fn collision_distinct_type_aliases_ok() {
    let source = r"---
types:
  - Priority = enum(High, Low)
  - Status = enum(Active, Paused)

params: [p = Priority, s = Status]
---
{{ p }} {{ s }}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("p", "High");
    ctx.set("s", "Active");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"High Active
"
    );
}

// --- Rule: Type alias shadows builtin ---

#[test]
fn collision_type_alias_shadows_builtin_rejected() {
    let source = r"---
types:
  - Str = enum(A, B)

params: [x = Str]
---
{{ x }}
";
    let err =
        Template::from_source(source).expect_err("type alias shadowing builtin 'str' should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("shadow") || msg.contains("builtin"),
        "error should mention builtin shadow: {msg}"
    );
}

#[test]
fn collision_non_builtin_type_alias_ok() {
    let source = r"---
types:
  - Priority = enum(High, Low)

params: [level = Priority]
---
{{ level }}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("level", "High");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"High
"
    );
}

// --- Rule: Type alias shadows import stem (R2) ---

#[test]
fn collision_type_alias_shadows_import_rejected() {
    // R2: Type alias name must exactly match import stem (case-sensitive).
    // Import stem is "mylib" from mylib.tmpl.md, and type alias is also "mylib".
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mylib.tmpl.md"),
        r"---
name: mylib
params: []
---
",
    )
    .unwrap();

    let source = r"---
name: main
imports:
  - [mylib](./mylib.tmpl.md)

types:
  - mylib = enum(A, B)

params: [x = mylib]
---
{{ x }}
";
    let err = Template::compile(source, CompileOptions::default().base_dir(dir.path()))
        .expect_err("type alias 'mylib' shadowing import stem 'mylib' should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("shadow") && msg.contains("import"),
        "error should mention import shadow: {msg}"
    );
}

#[test]
fn collision_type_alias_not_shadowing_import_ok() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mylib.tmpl.md"),
        r"---
name: mylib
params: []
---
",
    )
    .unwrap();

    let source = r"---
name: main
imports:
  - [mylib](./mylib.tmpl.md)

types:
  - Priority = enum(High, Low)

params: [x = Priority]
---
{{ x }}
";
    let (tmpl, _fm) =
        Template::compile(source, CompileOptions::default().base_dir(dir.path())).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "High");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"High
"
    );
}

// --- Rule: Param name (PascalCase) shadows import stem (R2b) ---

#[test]
fn collision_param_shadows_import_rejected() {
    // R2b: PascalCase of param name must exactly match import stem.
    // Import stem "Abc" from Abc.tmpl.md, param "abc" → PascalCase "Abc".
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Abc.tmpl.md"),
        r"---
name: Abc
params: []
---
",
    )
    .unwrap();

    let source = r"---
name: main
imports:
  - [Abc](./Abc.tmpl.md)

params: [abc = str]
---
{{ abc }}
";
    let err = Template::compile(source, CompileOptions::default().base_dir(dir.path()))
        .expect_err("param 'abc' (PascalCase 'Abc') shadowing import stem 'Abc' should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("shadow") && msg.contains("import"),
        "error should mention import shadow: {msg}"
    );
}

#[test]
fn collision_param_not_shadowing_import_ok() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Abc.tmpl.md"),
        r"---
name: Abc
params: []
---
",
    )
    .unwrap();

    let source = r"---
name: main
imports:
  - [Abc](./Abc.tmpl.md)

params: [msg = str]
---
{{ msg }}
";
    let (tmpl, _fm) =
        Template::compile(source, CompileOptions::default().base_dir(dir.path())).unwrap();
    let mut ctx = Context::new();
    ctx.set("msg", "hello");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"hello
"
    );
}

// --- Rule: Type alias vs param/const name collision (R1) ---

#[test]
fn collision_type_param_conflict_rejected() {
    // Param "priority" in PascalCase is "Priority", conflicting with type alias
    // "Priority" when the param's type is NOT that alias.
    let source = r"---
types:
  - Priority = enum(High, Low)

params: [priority = str]
---
{{ priority }}
";
    let err = Template::from_source(source)
        .expect_err("param 'priority' conflicting with type alias 'Priority' should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("conflict") && msg.contains("Priority"),
        "error should mention type-param conflict: {msg}"
    );
}

#[test]
fn collision_type_param_same_type_ok() {
    // When param type IS the alias, this is allowed.
    let source = r"---
types:
  - Priority = enum(High, Low)

params: [priority = Priority]
---
{{ priority }}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("priority", "High");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"High
"
    );
}

// --- Rule: Unused type alias (R4) ---

#[test]
fn collision_unused_type_alias_rejected() {
    // Enum types are exempt from R4 (auto-injected as constants), so use struct.
    let source = r"---
types:
  - Unused = struct(a = str, b = int)

params: [x = str]
---
{{ x }}
";
    let err = Template::from_source(source).expect_err("unused type alias should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("unused") && msg.contains("Unused"),
        "error should mention unused type alias: {msg}"
    );
}

#[test]
fn collision_used_type_alias_ok() {
    let source = r"---
types:
  - Status = enum(Active, Paused)

params: [s = Status]
---
{{ s }}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("s", "Active");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"Active
"
    );
}

// --- Rule: Duplicate constant name ---

#[test]
fn collision_duplicate_const_rejected() {
    let source = r"---
consts:
  - X = int := 1
  - X = int := 2
---
{{ X }}
";
    let err = Template::from_source(source).expect_err("duplicate constant should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains('X'),
        "error should mention duplicate constant 'X': {msg}"
    );
}

#[test]
fn collision_distinct_consts_ok() {
    let source = r"---
consts:
  - X = int := 1
  - Y = int := 2
---
{{ X }} {{ Y }}
";
    let tmpl = Template::from_source(source).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(
        result,
        r"1 2
"
    );
}

// --- Rule: Param and const with same name (R3) ---

#[test]
fn collision_param_const_conflict_rejected() {
    let source = r#"---
params: [x = str]
consts:
  - x = str := "fixed"
---
{{ x }}
"#;
    let err =
        Template::from_source(source).expect_err("param and const with same name should conflict");
    let msg = err.to_string();
    assert!(
        msg.contains("conflict") || (msg.contains("param") && msg.contains("constant")),
        "error should mention param-const conflict: {msg}"
    );
}

#[test]
fn collision_param_const_different_names_ok() {
    let source = r#"---
params: [user_input = str]
consts:
  - VERSION = str := "1.0"
---
{{ user_input }} v{{ VERSION }}
"#;
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("user_input", "hello");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"hello v1.0
"
    );
}

// --- Rule: Reserved keyword as import stem ---

#[test]
fn collision_reserved_keyword_import_stem_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("enum.tmpl.md"),
        r"---
name: enum
params: []
---
",
    )
    .unwrap();

    let source = r"---
name: main
imports:
  - [enum](./enum.tmpl.md)

params: []
---
hello
";
    let err = Template::compile(source, CompileOptions::default().base_dir(dir.path()))
        .expect_err("reserved keyword 'enum' as import stem should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("reserved keyword"),
        "error should mention reserved keyword: {msg}"
    );
}

// ============================================================================
// D. Adversarial Input Tests
// ============================================================================

/// A template with many variables (100) should render correctly.
#[test]
fn adversarial_many_variables() {
    let mut param_decls = Vec::new();
    let mut body_refs = Vec::new();
    for i in 0..100 {
        param_decls.push(format!("v{i} = str"));
        body_refs.push(format!("{{{{ v{i} }}}}"));
    }
    let source = format!(
        r"---
params: [{}]
---
{}
",
        param_decls.join(", "),
        body_refs.join(" ")
    );

    let tmpl = Template::from_source(&source).unwrap();
    let mut ctx = Context::new();
    for i in 0..100 {
        ctx.set(format!("v{i}"), format!("val{i}"));
    }
    let result = tmpl.render_ctx(&ctx).unwrap();
    let mut expected_parts = Vec::new();
    for i in 0..100 {
        expected_parts.push(format!("val{i}"));
    }
    let expected = format!(
        r"{}
",
        expected_parts.join(" ")
    );
    assert_eq!(result, expected);
}

/// A for-loop over an empty list should produce no output.
#[test]
fn adversarial_empty_list_for_loop() {
    let source = r"---
params: [items = list(name = str)]
---
> {% for item in items %}{{ item.name }}{% /for %}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("items", Value::List(Arc::new(vec![])));
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result.trim(), "", "empty list should produce no output");
}

/// A chained filter pipeline: trim → upper → lower should work.
#[test]
fn adversarial_filter_chain() {
    let source = r"---
params: [text = str]
---
{{ text | trim | upper }}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("text", "  hello world  ");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result.trim(), "HELLO WORLD");
}

/// Deeply nested conditionals (10 levels) should render correctly.
#[test]
fn adversarial_deeply_nested_conditionals() {
    let mut open = String::new();
    let mut close = String::new();
    for _ in 0..10 {
        open.push_str("> {% if flag %}");
        close.push_str("{% /if %}");
    }
    let source = format!(
        r"---
params: [flag = bool]
---
{open}DEEP{close}
"
    );
    let tmpl = Template::from_source(&source).unwrap();
    let mut ctx = Context::new();
    ctx.set("flag", Value::Bool(true));
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(result, "> > > > > > > > > DEEP");
}

/// Template with only frontmatter and no body should produce empty output.
#[test]
fn adversarial_empty_body() {
    let source = r"---
params: []
---
";
    let tmpl = Template::from_source(source).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert_eq!(result, "", "empty body should produce empty output");
}

/// Template body is only whitespace — should be preserved.
#[test]
fn adversarial_whitespace_only_body() {
    let source = r"---
params: []
---
   
";
    let tmpl = Template::from_source(source).unwrap();
    let result = tmpl.render_ctx(&Context::new()).unwrap();
    assert!(
        result.trim().is_empty(),
        "whitespace body should produce whitespace: {result:?}"
    );
}

/// Unicode in variable values and template body.
#[test]
fn adversarial_unicode_content() {
    let source = r"---
params: [msg = str]
---
🎯 {{ msg }} 日本語
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("msg", "こんにちは 🦀");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"🎯 こんにちは 🦀 日本語
"
    );
}

// ============================================================================
// E. Whitespace Control Edge Cases
// ============================================================================

/// `{%-` trims trailing whitespace from preceding text.
#[test]
fn whitespace_trim_before_tag() {
    let source = r"---
params: [show = bool]
---
hello   {%- if show %}yes{% /if %}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = tmpl.render_ctx(&ctx).unwrap();
    // Trailing whitespace after "hello" should be trimmed by `{%-`.
    assert_eq!(result, "helloyes");
}

/// `-%}` trims leading whitespace (through newline) from following text.
#[test]
fn whitespace_trim_after_tag() {
    let source = r"---
params: [show = bool]
---
> {% if show -%}

hello

> {% /if %}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = tmpl.render_ctx(&ctx).unwrap();
    // `-%}` should strip the newline after the tag.
    assert_eq!(
        result,
        r"hello
"
    );
}

/// Expression trimming: `{{-` and `-}}`.
#[test]
fn whitespace_trim_expression() {
    let source = r"---
params: [x = str]
---
before   {{- x -}}   after
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "MID");
    let result = tmpl.render_ctx(&ctx).unwrap();
    // `{{-` trims trailing whitespace before expr; `-}}` trims leading after.
    assert_eq!(
        result,
        r"beforeMIDafter
"
    );
}

// ============================================================================
// F. Match/Case Edge Cases
// ============================================================================

/// Match on a non-enum value (integer) should error at runtime.
#[test]
fn match_on_non_enum_value_errors() {
    let source = r"---
params: [count = int]
---
> {% match count %}
> {% case One %}

one

> {% /match %}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("count", Value::Int(42));
    let err = tmpl
        .render_ctx(&ctx)
        .expect_err("matching on integer should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("not an enum"),
        "error should mention non-enum: {msg}"
    );
}

/// Match where no arm matches should produce empty output (not an error).
#[test]
fn match_no_arm_matches_produces_empty() {
    let source = r"---
params: [status = str]
---
> {% match status %}
> {% case Active %}

Running

> {% case Paused %}

Paused

> {% /match %}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("status", "Unknown");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result.trim(),
        "",
        "no matching arm should produce empty output"
    );
}

/// `{% case %}` after `{% else %}` should be a compile error.
#[test]
fn match_case_after_else_rejected() {
    let source = r"---
params: [s = str]
---
> {% match s %}
> {% else %}

fallback

> {% case Active %}

active

> {% /match %}
";
    let err = Template::from_source(source).expect_err("case after else should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("case") && msg.contains("else"),
        "error should mention case after else: {msg}"
    );
}

/// Match with `{% else %}` arm catches unmatched variants.
#[test]
fn match_else_arm_catches_unmatched() {
    let source = r"---
params: [status = str]
---
> {% match status %}
> {% case Active %}

Running

> {% else %}

Other

> {% /match %}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("status", "Stopped");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"Other
"
    );
}

/// Nested match blocks should scope correctly.
#[test]
fn match_nested_matches() {
    let source = "\
---

params: [outer = str, inner = str]
---
> {% match outer %}
> {% case A %}

OuterA

> {% match inner %}
> {% case X %}

InnerX

> {% /match %}
> {% case B %}

OuterB

> {% /match %}
";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("outer", "A");
    ctx.set("inner", "X");
    let result = tmpl.render_ctx(&ctx).unwrap();
    assert_eq!(
        result,
        r"OuterA
InnerX
"
    );
}
