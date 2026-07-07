use super::*;

fn make_context() -> Context {
    let mut ctx = Context::new();
    ctx.set("name", "Alice");
    ctx.set("count", 3_i64);
    ctx
}

// -- simple resolution --

#[test]
fn resolve_from_context() {
    let ctx = make_context();
    let scope = Scope::new(&ctx);
    assert_eq!(scope.resolve("name"), Some(&Value::Str("Alice".into())));
    assert_eq!(scope.resolve("count"), Some(&Value::Int(3)));
}

#[test]
fn resolve_missing_returns_none() {
    let ctx = Context::new();
    let scope = Scope::new(&ctx);
    assert_eq!(scope.resolve("nope"), None);
}

// -- layered resolution --

#[test]
fn resolve_from_pushed_layer() {
    let ctx = make_context();
    let mut scope = Scope::new(&ctx);
    let layer = scope.push_layer();
    layer.insert("index".into(), Value::Int(1));
    layer.insert("item".into(), Value::Str("task-a".into()));

    assert_eq!(scope.resolve("index"), Some(&Value::Int(1)));
    assert_eq!(scope.resolve("item"), Some(&Value::Str("task-a".into())));
    // Context values still accessible.
    assert_eq!(scope.resolve("name"), Some(&Value::Str("Alice".into())));
}

#[test]
fn pop_layer_restores_previous() {
    let ctx = make_context();
    let mut scope = Scope::new(&ctx);
    let layer = scope.push_layer();
    layer.insert("name".into(), Value::Str("shadowed".into()));
    assert_eq!(scope.resolve("name"), Some(&Value::Str("shadowed".into())));

    scope.pop_layer();
    assert_eq!(scope.resolve("name"), Some(&Value::Str("Alice".into())));
}

// -- shadowing --

#[test]
fn inner_layer_shadows_outer() {
    let ctx = make_context();
    let mut scope = Scope::new(&ctx);

    let layer1 = scope.push_layer();
    layer1.insert("x".into(), Value::Int(10));

    let layer2 = scope.push_layer();
    layer2.insert("x".into(), Value::Int(20));

    assert_eq!(scope.resolve("x"), Some(&Value::Int(20)));

    scope.pop_layer();
    assert_eq!(scope.resolve("x"), Some(&Value::Int(10)));

    scope.pop_layer();
    assert_eq!(scope.resolve("x"), None);
}

// -- dotted path resolution --

#[test]
fn resolve_path_simple() {
    let ctx = make_context();
    let scope = Scope::new(&ctx);
    let val = scope.resolve_path_str("name").unwrap();
    assert_eq!(val, &Value::Str("Alice".into()));
}

#[test]
fn resolve_path_dotted() {
    let mut ctx = Context::new();
    let inner = Value::Struct(Arc::new(
        [("label".into(), Value::Str("important".into()))]
            .into_iter()
            .collect(),
    ));
    ctx.set("task", inner);

    let scope = Scope::new(&ctx);
    let val = scope.resolve_path_str("task.label").unwrap();
    assert_eq!(val, &Value::Str("important".into()));
}

#[test]
fn resolve_path_deeply_nested() {
    let mut ctx = Context::new();
    let deep = Value::Struct(Arc::new(
        [(
            "a".into(),
            Value::Struct(Arc::new(
                [(
                    "b".into(),
                    Value::Struct(Arc::new(
                        [("c".into(), Value::Int(42))].into_iter().collect(),
                    )),
                )]
                .into_iter()
                .collect(),
            )),
        )]
        .into_iter()
        .collect(),
    ));
    ctx.set("root", deep);

    let scope = Scope::new(&ctx);
    assert_eq!(
        scope.resolve_path_str("root.a.b.c").unwrap(),
        &Value::Int(42)
    );
}

#[test]
fn resolve_path_missing_root() {
    let ctx = Context::new();
    let scope = Scope::new(&ctx);
    let err = scope.resolve_path_str("absent").unwrap_err();
    assert!(matches!(err, TemplateError::UndefinedVariable(_)));
}

#[test]
fn resolve_path_missing_field() {
    let mut ctx = Context::new();
    ctx.set(
        "item",
        Value::Struct(Arc::new(
            [("name".into(), Value::Str("x".into()))]
                .into_iter()
                .collect(),
        )),
    );
    let scope = Scope::new(&ctx);
    let err = scope.resolve_path_str("item.missing").unwrap_err();
    assert!(matches!(err, TemplateError::UndefinedVariable(_)));
}

#[test]
fn resolve_path_field_on_non_dict() {
    let mut ctx = Context::new();
    ctx.set("val", 10_i64);
    let scope = Scope::new(&ctx);
    let err = scope.resolve_path_str("val.field").unwrap_err();
    assert!(matches!(err, TemplateError::UndefinedVariable(_)));
}

// -- dotted path in layers --

#[test]
fn resolve_path_through_layer() {
    let ctx = Context::new();
    let mut scope = Scope::new(&ctx);
    let layer = scope.push_layer();
    layer.insert(
        "item".into(),
        Value::Struct(Arc::new(
            [("name".into(), Value::Str("from-layer".into()))]
                .into_iter()
                .collect(),
        )),
    );

    let val = scope.resolve_path_str("item.name").unwrap();
    assert_eq!(val, &Value::Str("from-layer".into()));
}

#[test]
fn test_layer_allocation_reuse() {
    let ctx = Context::new();
    let mut scope = Scope::new(&ctx);

    // Initially empty
    assert_eq!(scope.layers.len(), 0);
    assert_eq!(scope.active_len, 0);

    // Push 1
    {
        let layer = scope.push_layer();
        layer.insert("k1".into(), Value::Int(100));
    }
    assert_eq!(scope.layers.len(), 1);
    assert_eq!(scope.active_len, 1);
    assert_eq!(scope.resolve("k1"), Some(&Value::Int(100)));

    // Pop 1
    scope.pop_layer();
    assert_eq!(scope.layers.len(), 1); // Allocation kept
    assert_eq!(scope.active_len, 0);
    assert_eq!(scope.resolve("k1"), None); // k1 should not resolve because active_len is 0

    // Push again - should reuse
    {
        let layer = scope.push_layer();
        // Verify it was cleared! It shouldn't contain "k1" anymore.
        assert!(layer.is_empty());
        layer.insert("k2".into(), Value::Int(200));
    }
    assert_eq!(scope.layers.len(), 1); // Still 1! Reused!
    assert_eq!(scope.active_len, 1);
    assert_eq!(scope.resolve("k1"), None);
    assert_eq!(scope.resolve("k2"), Some(&Value::Int(200)));
}

// -- kind() function tests --

#[test]
fn kind_extracts_enum_variant_name() {
    let tmpl = crate::Template::from_source(
        r"---
params: [outcome = struct(evidence = str)]
---
{{ kind(outcome) }}",
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set(
        "outcome",
        Value::Struct(Arc::new(HashMap::from([
            (
                crate::consts::ENUM_TAG_KEY.into(),
                Value::Str("Confirmed".into()),
            ),
            ("evidence".into(), Value::Str("confirmed finding".into())),
        ]))),
    );
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Confirmed");
}

#[test]
fn kind_rejects_non_dict() {
    let tmpl = crate::Template::from_source(
        r"---
params: [count = int]
---
{{ kind(count) }}",
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set("count", 42);
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    assert!(
        err.to_string().contains("enum"),
        "should mention enum requirement: {err}"
    );
}

#[test]
fn kind_rejects_dict_without_variant_tag() {
    let tmpl = crate::Template::from_source(
        r"---
params: [data = struct(name = str)]
---
{{ kind(data) }}",
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set(
        "data",
        Value::Struct(Arc::new(HashMap::from([(
            "name".into(),
            Value::Str("x".into()),
        )]))),
    );
    let err = tmpl.render_ctx(&ctx).unwrap_err();
    assert!(
        err.to_string().contains("enum"),
        "should mention enum requirement: {err}"
    );
}

#[test]
fn kind_key_not_accessible_via_dot_path() {
    // The internal __kind__ key must be rejected at compile time,
    // not at render time.
    let err = crate::Template::from_source(
        r"---
params: [outcome = struct(evidence = str)]
---
{{ outcome.__kind__ }}",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("not allowed") || err.to_string().contains("internal key"),
        "__kind__ should be rejected at compile time: {err}"
    );
}

#[test]
fn kind_key_rejected_in_condition() {
    // __kind__ access in conditions should also be rejected at compile time.
    let err = crate::Template::from_source(
        "---\nparams: [x = struct(name = str)]\n---\n\
         > {% if x.__kind__ == \"foo\" %}\n\n\
         yes\n\n\
         > {% /if %}",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("not allowed") || err.to_string().contains("internal key"),
        "__kind__ in condition should be rejected at compile time: {err}"
    );
}

#[test]
fn get_field_unchecked_returns_normal_fields() {
    let dict = Value::Struct(Arc::new(HashMap::from([
        ("name".into(), Value::Str("Alice".into())),
        ("score".into(), Value::Int(95)),
    ])));
    assert_eq!(
        dict.get_field_unchecked("name"),
        Some(&Value::Str("Alice".into()))
    );
    assert_eq!(dict.get_field_unchecked("score"), Some(&Value::Int(95)));
    assert_eq!(dict.get_field_unchecked("missing"), None);
}

#[test]
fn get_field_unchecked_on_non_struct_returns_none() {
    assert_eq!(Value::Str("x".into()).get_field_unchecked("any"), None);
    assert_eq!(Value::Int(1).get_field_unchecked("any"), None);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "ENUM_TAG_KEY")]
fn get_field_unchecked_debug_asserts_on_tag_key() {
    let dict = Value::Struct(Arc::new(HashMap::from([(
        crate::consts::ENUM_TAG_KEY.into(),
        Value::Str("Variant".into()),
    )])));
    // This should trigger the debug_assert.
    let _ = dict.get_field_unchecked(crate::consts::ENUM_TAG_KEY);
}

#[test]
fn user_field_named_tag_does_not_collide() {
    // A user field named "tag" must not collide with the internal __kind__ key.
    let tmpl = crate::Template::from_source(
        r"---
params: [entry = struct(tag = str)]
---
{{ kind(entry) }}: {{ entry.tag }}",
    )
    .unwrap();
    let mut ctx = crate::Context::new();
    ctx.set(
        "entry",
        Value::Struct(Arc::new(HashMap::from([
            (
                crate::consts::ENUM_TAG_KEY.into(),
                Value::Str("Woche".into()),
            ),
            ("tag".into(), Value::Str("Montag".into())),
        ]))),
    );
    assert_eq!(tmpl.render_ctx(&ctx).unwrap(), "Woche: Montag");
}

// -- parse_function_call edge cases --

#[test]
fn parse_function_call_valid() {
    let result = parse_function_call("idx(item)");
    assert_eq!(result, Some(("idx", "item")));
}

#[test]
fn parse_function_call_empty_func_returns_none() {
    // `(arg)` — empty function name.
    assert_eq!(parse_function_call("(arg)"), None);
}

#[test]
fn parse_function_call_empty_arg_returns_none() {
    // `func()` — empty argument.
    assert_eq!(parse_function_call("func()"), None);
}

#[test]
fn parse_function_call_no_parens_returns_none() {
    assert_eq!(parse_function_call("just_a_name"), None);
}

#[test]
fn parse_function_call_dotted_name_returns_none() {
    // Dotted names are not valid function identifiers.
    assert_eq!(parse_function_call("foo.bar(x)"), None);
}

// -- ConditionOperand compile & resolve --

#[test]
fn resolve_value_or_literal_string_literal() {
    let ctx = Context::new();
    let scope = Scope::new(&ctx);
    let operand = ConditionOperand::compile("\"hello\"").unwrap();
    let val = operand.resolve(&scope).unwrap();
    assert_eq!(*val, Value::Str("hello".into()));
}

#[test]
fn resolve_value_or_literal_bool_true() {
    let ctx = Context::new();
    let scope = Scope::new(&ctx);
    let operand = ConditionOperand::compile("true").unwrap();
    assert_eq!(*operand.resolve(&scope).unwrap(), Value::Bool(true));
}

#[test]
fn resolve_value_or_literal_integer() {
    let ctx = Context::new();
    let scope = Scope::new(&ctx);
    let operand = ConditionOperand::compile("42").unwrap();
    assert_eq!(*operand.resolve(&scope).unwrap(), Value::Int(42));
}

#[test]
fn resolve_value_or_literal_float() {
    let ctx = Context::new();
    let scope = Scope::new(&ctx);
    let operand = ConditionOperand::compile("2.78").unwrap();
    assert_eq!(*operand.resolve(&scope).unwrap(), Value::Float(2.78));
}

#[test]
fn resolve_value_or_literal_empty_token_returns_error() {
    let err = ConditionOperand::compile("").unwrap_err();
    assert!(matches!(err, TemplateError::Syntax(_)));
}

// -- include depth tracking --

#[test]
fn enter_include_enforces_max_depth() {
    let ctx = Context::new();
    let mut scope = Scope::new(&ctx).with_max_include_depth(2);
    scope.enter_include().unwrap();
    scope.enter_include().unwrap();
    // Third should exceed depth of 2.
    let err = scope.enter_include().unwrap_err();
    assert!(err.to_string().contains("maximum include depth"));
}

#[test]
fn exit_include_decrements_and_allows_reentry() {
    let ctx = Context::new();
    let mut scope = Scope::new(&ctx).with_max_include_depth(1);
    scope.enter_include().unwrap();
    scope.exit_include();
    // After exiting, re-entering should succeed.
    scope.enter_include().unwrap();
}

// -- pop_layer on empty scope --

#[test]
fn pop_layer_on_empty_scope_is_noop() {
    let ctx = Context::new();
    let mut scope = Scope::new(&ctx);
    // Should not panic.
    scope.pop_layer();
    scope.pop_layer();
    assert_eq!(scope.resolve("anything"), None);
}

// -- constants resolution --

#[test]
fn consts_take_priority_over_context() {
    let mut ctx = Context::new();
    ctx.set("x", "from_ctx");
    let mut scope = Scope::new(&ctx);
    let consts = Arc::new(HashMap::from([(
        "x".into(),
        Value::Str("from_const".into()),
    )]));
    let imported = Arc::new(HashMap::new());
    scope.set_consts(&consts, &imported);
    // Constants should shadow context values.
    assert_eq!(scope.resolve("x"), Some(&Value::Str("from_const".into())));
}

#[test]
fn push_pop_consts_restores_context_value() {
    let mut ctx = Context::new();
    ctx.set("y", "original");
    let mut scope = Scope::new(&ctx);
    scope.push_consts(
        HashMap::from([("y".into(), Value::Str("overridden".into()))]),
        HashMap::new(),
    );
    assert_eq!(scope.resolve("y"), Some(&Value::Str("overridden".into())));
    scope.pop_consts();
    assert_eq!(scope.resolve("y"), Some(&Value::Str("original".into())));
}

#[test]
fn push_loop_binding_reuses_string_alloc() {
    let ctx = Context::new();
    let mut scope = Scope::new(&ctx);

    // First iteration allocates a new string.
    let val1 = Value::Str("hello".into());
    scope.push_loop_binding("item", &val1);
    assert_eq!(scope.resolve("item"), Some(&Value::Str("hello".into())));
    scope.pop_loop_binding();

    // Second iteration should reuse the allocation (no new alloc).
    let val2 = Value::Str("world".into());
    scope.push_loop_binding("item", &val2);
    assert_eq!(scope.resolve("item"), Some(&Value::Str("world".into())));
    scope.pop_loop_binding();

    // Mixed types: string → int → string — should still work.
    let val_int = Value::Int(42);
    scope.push_loop_binding("item", &val_int);
    assert_eq!(scope.resolve("item"), Some(&Value::Int(42)));
    scope.pop_loop_binding();

    let val3 = Value::Str("back to string".into());
    scope.push_loop_binding("item", &val3);
    assert_eq!(
        scope.resolve("item"),
        Some(&Value::Str("back to string".into()))
    );
    scope.pop_loop_binding();
}

#[test]
fn for_loop_string_items_render_correctly() {
    // End-to-end test: loop over strings renders each one correctly.
    let tmpl = crate::Template::from_source(
        "---\nparams: [items = list(str)]\n---\n\
         > {% for item in items %}\n\n\
         - {{ item }}\n\n\
         > {% /for %}",
    )
    .unwrap();
    let mut ctx = Context::new();
    ctx.set(
        "items",
        Value::List(std::sync::Arc::new(vec![
            Value::Str("alpha".into()),
            Value::Str("beta".into()),
            Value::Str("gamma".into()),
        ])),
    );
    let output = tmpl.render_ctx(&ctx).unwrap();
    assert!(output.contains("- alpha"), "missing alpha: {output}");
    assert!(output.contains("- beta"), "missing beta: {output}");
    assert!(output.contains("- gamma"), "missing gamma: {output}");
}
