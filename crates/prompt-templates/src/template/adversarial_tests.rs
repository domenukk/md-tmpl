//! Adversarial and edge-case tests for the template engine.
//!
//! Covers:
//! - Include depth limits (self-recursive, chain at boundary, custom limit)
//! - Circular import detection (2-cycle, 3-cycle)
//! - Frontmatter collision rules (R1–R4 positive and negative)
//! - Adversarial inputs (huge templates, empty containers, filter chains)
//! - Whitespace control edge cases (`{%-`, `-%}`)
//! - Match/case edge cases (non-enum, no-match, case-after-default)

use crate::{Context, Template, Value};

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
        format!("---\nname: t{depth}\nparams: []\n---\nLEAF"),
    )
    .unwrap();

    // Build intermediate templates from depth-1 down to 0.
    for i in (0..depth).rev() {
        let next_name = format!("t{}.tmpl.md", i + 1);
        let this_name = format!("t{i}.tmpl.md");
        let source = format!(
            "---\nname: t{i}\nparams: []\n---\n{i}+{{% include [t{}]({next_name}) %}}",
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
    let result = tmpl.render(&Context::new()).unwrap();
    // Should contain all intermediate levels and the leaf.
    assert!(
        result.contains("LEAF"),
        "chain should render leaf: {result}"
    );
    assert!(result.contains("0+"), "chain should contain root: {result}");
}

/// 16-level include chain renders successfully (exactly at default limit).
#[test]
fn depth_limit_16_renders_ok() {
    let dir = tempfile::tempdir().unwrap();
    let root = build_include_chain(dir.path(), 16);
    let tmpl = Template::from_file(&root).unwrap();
    let result = tmpl.render(&Context::new()).unwrap();
    assert!(
        result.contains("LEAF"),
        "chain should render leaf: {result}"
    );
}

/// 17-level include chain exceeds default depth limit (16) and errors.
#[test]
fn depth_limit_17_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = build_include_chain(dir.path(), 17);
    let tmpl = Template::from_file(&root).unwrap();
    let err = tmpl
        .render(&Context::new())
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
        "---\nname: a\nparams: []\n---\nA> {% include [b](b.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "---\nname: b\nparams: []\n---\nB> {% include [a](a.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let err = tmpl
        .render(&Context::new())
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
        "---\nname: a\nparams: []\n---\nA> {% include [b](b.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.tmpl.md"),
        "---\nname: b\nparams: []\n---\nB> {% include [c](c.tmpl.md) %}",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("c.tmpl.md"),
        "---\nname: c\nparams: []\n---\nC> {% include [a](a.tmpl.md) %}",
    )
    .unwrap();

    let tmpl = Template::from_file(&dir.path().join("a.tmpl.md")).unwrap();
    let err = tmpl
        .render(&Context::new())
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
    let source = "---\nparams: [name = str, name = str]\n---\n{{ name }}\n";
    let err = Template::from_source(source).expect_err("duplicate param should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains("name"),
        "error should mention duplicate param 'name': {msg}"
    );
}

#[test]
fn collision_distinct_params_ok() {
    let source =
        "---\nparams: [first_name = str, last_name = str]\n---\n{{ first_name }} {{ last_name }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("first_name", "Alice");
    ctx.set("last_name", "Smith");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("Alice Smith"), "got: {result}");
}

// --- Rule: Reserved keyword as param name ---

#[test]
fn collision_reserved_keyword_param_rejected() {
    let source = "---\nparams: [list = str]\n---\n{{ list }}\n";
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
    let source = "---\nparams: [my_list = str]\n---\n{{ my_list }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("my_list", "items");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("items"), "got: {result}");
}

// --- Rule: Duplicate type alias ---

#[test]
fn collision_duplicate_type_alias_rejected() {
    let source = "---\ntypes:\n  - Foo = enum<A, B>\n  - Foo = enum<X, Y>\nparams: [x = Foo]\n---\n{{ x }}\n";
    let err = Template::from_source(source).expect_err("duplicate type alias should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains("Foo"),
        "error should mention duplicate type alias 'Foo': {msg}"
    );
}

#[test]
fn collision_distinct_type_aliases_ok() {
    let source = "---\ntypes:\n  - Priority = enum<High, Low>\n  - Status = enum<Active, Paused>\nparams: [p = Priority, s = Status]\n---\n{{ p }} {{ s }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("p", "High");
    ctx.set("s", "Active");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("High"), "got: {result}");
}

// --- Rule: Type alias shadows builtin ---

#[test]
fn collision_type_alias_shadows_builtin_rejected() {
    let source = "---\ntypes:\n  - Str = enum<A, B>\nparams: [x = Str]\n---\n{{ x }}\n";
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
    let source = "---\ntypes:\n  - Priority = enum<High, Low>\nparams: [level = Priority]\n---\n{{ level }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("level", "High");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("High"), "got: {result}");
}

// --- Rule: Type alias shadows import stem (R2) ---

#[test]
fn collision_type_alias_shadows_import_rejected() {
    // R2: Type alias name must exactly match import stem (case-sensitive).
    // Import stem is "mylib" from mylib.tmpl.md, and type alias is also "mylib".
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mylib.tmpl.md"),
        "---\nname: mylib\nparams: []\n---\n",
    )
    .unwrap();

    let source = "---\nname: main\nimports:\n  - [mylib](mylib.tmpl.md)\ntypes:\n  - mylib = enum<A, B>\nparams: [x = mylib]\n---\n{{ x }}\n";
    let err = Template::from_source_with_base_dir(source, dir.path())
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
        "---\nname: mylib\nparams: []\n---\n",
    )
    .unwrap();

    let source = "---\nname: main\nimports:\n  - [mylib](mylib.tmpl.md)\ntypes:\n  - Priority = enum<High, Low>\nparams: [x = Priority]\n---\n{{ x }}\n";
    let tmpl = Template::from_source_with_base_dir(source, dir.path()).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "High");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("High"), "got: {result}");
}

// --- Rule: Param name (PascalCase) shadows import stem (R2b) ---

#[test]
fn collision_param_shadows_import_rejected() {
    // R2b: PascalCase of param name must exactly match import stem.
    // Import stem "Abc" from Abc.tmpl.md, param "abc" → PascalCase "Abc".
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Abc.tmpl.md"),
        "---\nname: Abc\nparams: []\n---\n",
    )
    .unwrap();

    let source =
        "---\nname: main\nimports:\n  - [Abc](Abc.tmpl.md)\nparams: [abc = str]\n---\n{{ abc }}\n";
    let err = Template::from_source_with_base_dir(source, dir.path())
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
        "---\nname: Abc\nparams: []\n---\n",
    )
    .unwrap();

    let source =
        "---\nname: main\nimports:\n  - [Abc](Abc.tmpl.md)\nparams: [msg = str]\n---\n{{ msg }}\n";
    let tmpl = Template::from_source_with_base_dir(source, dir.path()).unwrap();
    let mut ctx = Context::new();
    ctx.set("msg", "hello");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("hello"), "got: {result}");
}

// --- Rule: Type alias vs param/const name collision (R1) ---

#[test]
fn collision_type_param_conflict_rejected() {
    // Param "priority" in PascalCase is "Priority", conflicting with type alias
    // "Priority" when the param's type is NOT that alias.
    let source = "---\ntypes:\n  - Priority = enum<High, Low>\nparams: [priority = str]\n---\n{{ priority }}\n";
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
    let source = "---\ntypes:\n  - Priority = enum<High, Low>\nparams: [priority = Priority]\n---\n{{ priority }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("priority", "High");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("High"), "got: {result}");
}

// --- Rule: Unused type alias (R4) ---

#[test]
fn collision_unused_type_alias_rejected() {
    let source = "---\ntypes:\n  - Unused = enum<A, B>\nparams: [x = str]\n---\n{{ x }}\n";
    let err = Template::from_source(source).expect_err("unused type alias should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("unused") && msg.contains("Unused"),
        "error should mention unused type alias: {msg}"
    );
}

#[test]
fn collision_used_type_alias_ok() {
    let source =
        "---\ntypes:\n  - Status = enum<Active, Paused>\nparams: [s = Status]\n---\n{{ s }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("s", "Active");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("Active"), "got: {result}");
}

// --- Rule: Duplicate constant name ---

#[test]
fn collision_duplicate_const_rejected() {
    let source = "---\nconsts:\n  - X = int := 1\n  - X = int := 2\n---\n{{ X }}\n";
    let err = Template::from_source(source).expect_err("duplicate constant should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("duplicate") && msg.contains('X'),
        "error should mention duplicate constant 'X': {msg}"
    );
}

#[test]
fn collision_distinct_consts_ok() {
    let source = "---\nconsts:\n  - X = int := 1\n  - Y = int := 2\n---\n{{ X }} {{ Y }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let result = tmpl.render(&Context::new()).unwrap();
    assert!(
        result.contains('1') && result.contains('2'),
        "got: {result}"
    );
}

// --- Rule: Param and const with same name (R3) ---

#[test]
fn collision_param_const_conflict_rejected() {
    let source = "---\nparams: [x = str]\nconsts:\n  - x = str := \"fixed\"\n---\n{{ x }}\n";
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
    let source = "---\nparams: [user_input = str]\nconsts:\n  - VERSION = str := \"1.0\"\n---\n{{ user_input }} v{{ VERSION }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("user_input", "hello");
    let result = tmpl.render(&ctx).unwrap();
    assert!(
        result.contains("hello") && result.contains("v1.0"),
        "got: {result}"
    );
}

// --- Rule: Reserved keyword as import stem ---

#[test]
fn collision_reserved_keyword_import_stem_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("enum.tmpl.md"),
        "---\nname: enum\nparams: []\n---\n",
    )
    .unwrap();

    let source = "---\nname: main\nimports:\n  - [enum](enum.tmpl.md)\nparams: []\n---\nhello\n";
    let err = Template::from_source_with_base_dir(source, dir.path())
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
        "---\nparams: [{}]\n---\n{}\n",
        param_decls.join(", "),
        body_refs.join(" ")
    );

    let tmpl = Template::from_source(&source).unwrap();
    let mut ctx = Context::new();
    for i in 0..100 {
        ctx.set(format!("v{i}"), format!("val{i}"));
    }
    let result = tmpl.render(&ctx).unwrap();
    assert!(
        result.contains("val0"),
        "should contain first var: {result}"
    );
    assert!(
        result.contains("val99"),
        "should contain last var: {result}"
    );
}

/// A for-loop over an empty list should produce no output.
#[test]
fn adversarial_empty_list_for_loop() {
    let source = "---\nparams: [items = list<name = str>]\n---\n> {% for item in items %}{{ item.name }}{% /for %}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("items", Value::List(vec![]));
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(result.trim(), "", "empty list should produce no output");
}

/// A chained filter pipeline: trim → upper → lower should work.
#[test]
fn adversarial_filter_chain() {
    let source = "---\nparams: [text = str]\n---\n{{ text | trim | upper }}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("text", "  hello world  ");
    let result = tmpl.render(&ctx).unwrap();
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
    let source = format!("---\nparams: [flag = bool]\n---\n{open}DEEP{close}\n");
    let tmpl = Template::from_source(&source).unwrap();
    let mut ctx = Context::new();
    ctx.set("flag", Value::Bool(true));
    let result = tmpl.render(&ctx).unwrap();
    assert!(
        result.contains("DEEP"),
        "nested ifs should render: {result}"
    );
}

/// Template with only frontmatter and no body should produce empty output.
#[test]
fn adversarial_empty_body() {
    let source = "---\nparams: []\n---\n";
    let tmpl = Template::from_source(source).unwrap();
    let result = tmpl.render(&Context::new()).unwrap();
    assert_eq!(result, "", "empty body should produce empty output");
}

/// Template body is only whitespace — should be preserved.
#[test]
fn adversarial_whitespace_only_body() {
    let source = "---\nparams: []\n---\n   \n";
    let tmpl = Template::from_source(source).unwrap();
    let result = tmpl.render(&Context::new()).unwrap();
    assert!(
        result.trim().is_empty(),
        "whitespace body should produce whitespace: {result:?}"
    );
}

/// Unicode in variable values and template body.
#[test]
fn adversarial_unicode_content() {
    let source = "---\nparams: [msg = str]\n---\n🎯 {{ msg }} 日本語\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("msg", "こんにちは 🦀");
    let result = tmpl.render(&ctx).unwrap();
    assert!(result.contains("🎯"), "should contain emoji: {result}");
    assert!(
        result.contains("こんにちは 🦀"),
        "should contain unicode: {result}"
    );
    assert!(result.contains("日本語"), "should contain kanji: {result}");
}

// ============================================================================
// E. Whitespace Control Edge Cases
// ============================================================================

/// `{%-` trims trailing whitespace from preceding text.
#[test]
fn whitespace_trim_before_tag() {
    let source = "---\nparams: [show = bool]\n---\nhello   {%- if show %}yes{% /if %}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = tmpl.render(&ctx).unwrap();
    // Trailing whitespace after "hello" should be trimmed by `{%-`.
    assert!(
        result.contains("helloyes"),
        "whitespace before tag should be trimmed: {result:?}"
    );
}

/// `-%}` trims leading whitespace (through newline) from following text.
#[test]
fn whitespace_trim_after_tag() {
    let source = "---\nparams: [show = bool]\n---\n> {% if show -%}\nhello\n> {% /if %}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("show", Value::Bool(true));
    let result = tmpl.render(&ctx).unwrap();
    // `-%}` should strip the newline after the tag.
    assert!(
        result.starts_with("hello"),
        "whitespace after tag should be trimmed: {result:?}"
    );
}

/// Expression trimming: `{{-` and `-}}`.
#[test]
fn whitespace_trim_expression() {
    let source = "---\nparams: [x = str]\n---\nbefore   {{- x -}}   after\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("x", "MID");
    let result = tmpl.render(&ctx).unwrap();
    // `{{-` trims trailing whitespace before expr; `-}}` trims leading after.
    assert!(
        result.contains("beforeMID"),
        "trim-before should work: {result:?}"
    );
}

// ============================================================================
// F. Match/Case Edge Cases
// ============================================================================

/// Match on a non-enum value (integer) should error at runtime.
#[test]
fn match_on_non_enum_value_errors() {
    let source = "---\nparams: [count = int]\n---\n> {% match count %}\n> {% case One %}\none\n> {% /match %}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("count", Value::Int(42));
    let err = tmpl
        .render(&ctx)
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
    let source = "---\nparams: [status = str]\n---\n> {% match status %}\n> {% case Active %}\nRunning\n> {% case Paused %}\nPaused\n> {% /match %}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("status", "Unknown");
    let result = tmpl.render(&ctx).unwrap();
    assert_eq!(
        result.trim(),
        "",
        "no matching arm should produce empty output"
    );
}

/// `{% case %}` after `{% default %}` should be a compile error.
#[test]
fn match_case_after_default_rejected() {
    let source = "---\nparams: [s = str]\n---\n> {% match s %}\n> {% default %}\nfallback\n> {% case Active %}\nactive\n> {% /match %}\n";
    let err = Template::from_source(source).expect_err("case after default should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("case") && msg.contains("default"),
        "error should mention case after default: {msg}"
    );
}

/// Match with `{% default %}` arm catches unmatched variants.
#[test]
fn match_default_arm_catches_unmatched() {
    let source = "---\nparams: [status = str]\n---\n> {% match status %}\n> {% case Active %}\nRunning\n> {% default %}\nOther\n> {% /match %}\n";
    let tmpl = Template::from_source(source).unwrap();
    let mut ctx = Context::new();
    ctx.set("status", "Stopped");
    let result = tmpl.render(&ctx).unwrap();
    assert!(
        result.contains("Other"),
        "default arm should catch unmatched: {result}"
    );
    assert!(!result.contains("Running"), "matched arm should not appear");
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
    let result = tmpl.render(&ctx).unwrap();
    assert!(
        result.contains("OuterA"),
        "outer match should work: {result}"
    );
    assert!(
        result.contains("InnerX"),
        "inner match should work: {result}"
    );
    assert!(!result.contains("OuterB"), "wrong arm should not appear");
}
