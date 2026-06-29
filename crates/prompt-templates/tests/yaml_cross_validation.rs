//! Cross-validate our custom frontmatter parser against `serde_yaml`.
//!
//! Our parser uses a lightweight line-based approach instead of a full YAML
//! library.  These tests verify that the param strings our parser extracts
//! match what a real YAML parser produces, for both simple and complex
//! (multiline, nested) type declarations.

use prompt_templates::parse_frontmatter;

/// Extract the raw `params` strings from a YAML block using `serde_yaml`.
///
/// Returns each `- value` entry as a trimmed string, exactly as a real YAML
/// parser sees it.
fn serde_yaml_params(yaml_block: &str) -> Vec<String> {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml_block).expect("serde_yaml parse failed");
    let mapping = doc.as_mapping().expect("top-level should be a mapping");
    let params = mapping
        .get(serde_yaml::Value::String("params".into()))
        .expect("missing 'params' key");
    let seq = params.as_sequence().expect("'params' should be a sequence");
    seq.iter()
        .map(|v| {
            v.as_str()
                .expect("each param should be a string")
                .to_string()
        })
        .collect()
}

/// Extract the raw param names+types from our custom parser.
fn our_parser_params(source: &str) -> Vec<(String, String)> {
    let (fm, _body) = parse_frontmatter(source).expect("our parser failed");
    fm.declarations
        .iter()
        .map(|d| (d.name.clone(), format!("{}", d.var_type)))
        .collect()
}

/// Parse the YAML block with `serde_yaml`, then feed each param string through
/// our type parser to get comparable (name, type) pairs.
fn serde_yaml_parsed_params(yaml_block: &str) -> Vec<(String, String)> {
    let raw_params = serde_yaml_params(yaml_block);
    // Each raw param is "name = type" — parse the same way our frontmatter does.
    // We re-use the public API by constructing a full template source.
    let params_inline: Vec<&str> = raw_params.iter().map(String::as_str).collect();
    let source = format!("---\nparams: [{}]\n---\nbody", params_inline.join(", "));
    let (fm, _) = parse_frontmatter(&source).expect("re-parse through our parser failed");
    fm.declarations
        .iter()
        .map(|d| (d.name.clone(), format!("{}", d.var_type)))
        .collect()
}

/// Helper: build a template source from a YAML frontmatter block + minimal body.
fn source_from_yaml(yaml_block: &str) -> String {
    format!("---\n{yaml_block}\n---\nbody")
}

// ---------------------------------------------------------------------------
// Simple types
// ---------------------------------------------------------------------------

#[test]
fn simple_str_param() {
    let yaml = "params:\n  - name = str";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "simple str param mismatch");
}

#[test]
fn multiple_simple_params() {
    let yaml = "params:\n  - name = str\n  - count = int\n  - enabled = bool\n  - ratio = float";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiple simple params mismatch");
}

// ---------------------------------------------------------------------------
// List types
// ---------------------------------------------------------------------------

#[test]
fn list_type_single_line() {
    let yaml = "params:\n  - items = list(name = str, score = int)";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "list type mismatch");
}

#[test]
fn list_type_multiline() {
    let yaml = "params:\n  - items = list(\n      name = str,\n      score = int,\n    )";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiline list type mismatch");
}

// ---------------------------------------------------------------------------
// Enum types
// ---------------------------------------------------------------------------

#[test]
fn enum_type_single_line() {
    let yaml = "params:\n  - severity = enum(Critical, High, Medium, Low)";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum type mismatch");
}

#[test]
fn enum_type_with_fields_single_line() {
    let yaml = "params:\n  - outcome = enum(Confirmed(evidence = str), Rejected, NeedsWork)";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum with fields mismatch");
}

#[test]
fn enum_type_multiline() {
    let yaml = "\
params:
  - outcome = enum(
      Confirmed(evidence = str),
      Rejected,
      NeedsWork,
    )";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiline enum mismatch");
}

#[test]
fn enum_type_multiple_fields_per_variant() {
    let yaml = "params:\n  - result = enum(Success(code = int, msg = str), Failure(reason = str, retryable = bool))";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(
        ours, theirs,
        "enum with multiple fields per variant mismatch"
    );
}

#[test]
fn enum_type_multiple_fields_multiline() {
    let yaml = "\
params:
  - result = enum(
      Success(code = int, msg = str),
      Failure(reason = str, retryable = bool),
      Pending,
    )";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiline enum with multiple fields mismatch");
}

#[test]
fn enum_with_nested_list_field() {
    let yaml = "params:\n  - action = enum(Batch(items = list(name = str)), Single(name = str))";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum with nested list field mismatch");
}

#[test]
fn multiple_enum_params() {
    let yaml = "\
params:
  - priority = enum(High, Medium, Low)
  - status = enum(Open, Closed, InProgress)
  - severity = enum(Critical(reason = str), Warning, Info)";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiple enum params mismatch");
}

// ---------------------------------------------------------------------------
// Nested / complex types
// ---------------------------------------------------------------------------

#[test]
fn nested_list_with_enum_single_line() {
    let yaml = "params:\n  - agent_name = str\n  - tasks = list(title = str, severity = enum(Critical(reason = str), High, Medium, Low))";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "nested list+enum mismatch (single-line)");
}

#[test]
fn nested_list_with_enum_multiline() {
    let yaml = "\
params:
  - agent_name = str
  - tasks = list(
      title = str,
      severity = enum(Critical(reason = str), High, Medium, Low),
    )";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "nested list+enum mismatch (multiline)");
}

#[test]
fn struct_type_single_line() {
    let yaml = "params:\n  - metadata = struct(key = str, value = int)";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "struct type mismatch");
}

#[test]
fn struct_type_multiline() {
    let yaml = "\
params:
  - metadata = struct(
      key = str,
      value = int,
    )";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiline struct type mismatch");
}

#[test]
fn deeply_nested_multiline() {
    let yaml = "\
params:
  - reports = list(
      title = str,
      findings = list(
        description = str,
        severity = enum(Critical(reason = str), High, Medium, Low),
      ),
    )";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "deeply nested multiline mismatch");
}

// ---------------------------------------------------------------------------
// Mixed: simple + complex params together
// ---------------------------------------------------------------------------

#[test]
fn mixed_simple_and_complex_multiline() {
    let yaml = "\
params:
  - name = str
  - verbose = bool
  - items = list(
      label = str,
      tags = list(name = str),
    )
  - status = enum(Active, Inactive)";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "mixed simple+complex mismatch");
}

// ---------------------------------------------------------------------------
// End-to-end: parse + render_ctx the exact README hero example
// ---------------------------------------------------------------------------

#[test]
fn readme_hero_example_renders_correctly() {
    use prompt_templates::{Context, Template, Value};

    let source = "\
---

params:
  - agent_name = str
  - tasks = list(
      title = str,
      severity = enum(Critical(reason = str), High, Medium, Low),
    )
---

# Report — {{ agent_name }}

> {% for task in tasks %}

- **{{ task.title }}**

> {% match task.severity %}
> {% case Critical %}

  🔴 Immediate action required: `{{ task.severity.reason }}`

> {% case High %}

  🟠 High priority.

> {% case Medium | Low %}

  🟢 Normal priority.

> {% /match %}

> {% /for %}";

    let tmpl = Template::from_source(source).expect("README hero example should parse");

    let tag_key = prompt_templates::consts::ENUM_TAG_KEY;
    let mut ctx = Context::new();
    ctx.set("agent_name", "TaskBot");
    ctx.set(
        "tasks",
        vec![
            Value::new_struct([
                ("title", Value::from("Update dependencies")),
                (
                    "severity",
                    Value::new_struct([
                        (tag_key, Value::from("Critical")),
                        ("reason", Value::from("blocking release")),
                    ]),
                ),
            ]),
            Value::new_struct([
                ("title", Value::from("Fix CI pipeline")),
                (
                    "severity",
                    Value::new_struct([(tag_key, Value::from("High"))]),
                ),
            ]),
            Value::new_struct([
                ("title", Value::from("Update README")),
                (
                    "severity",
                    Value::new_struct([(tag_key, Value::from("Low"))]),
                ),
            ]),
        ],
    );

    let output = tmpl
        .render_ctx(&ctx)
        .expect("README hero example should render");
    assert_eq!(
        output,
        "\n# Report — TaskBot\n\
         - **Update dependencies**\n\
         \x20\x20🔴 Immediate action required: `blocking release`\n\
         - **Fix CI pipeline**\n\
         \x20\x20🟠 High priority.\n\
         - **Update README**\n\
         \x20\x20🟢 Normal priority.\n"
    );
}

// ---------------------------------------------------------------------------
// Nested complex types inside enum variants
// ---------------------------------------------------------------------------

#[test]
fn enum_with_nested_list_field_crossval() {
    let yaml = "\
params:
  - action = enum(
      Batch(items = list(name = str, priority = int)),
      Single(name = str),
    )";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(
        ours, theirs,
        "enum with nested list field (multiline) mismatch"
    );
}

#[test]
fn enum_with_nested_struct_field_crossval() {
    let yaml = "\
params:
  - response = enum(
      Success(metadata = struct(key = str, value = str)),
      Error(code = int, details = str),
    )";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum with nested struct field mismatch");
}

#[test]
fn enum_with_nested_enum_field_crossval() {
    let yaml = "params:\n  - result = enum(Done(status = enum(Pass, Fail)), Pending)";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum with nested enum field mismatch");
}

// ---------------------------------------------------------------------------
// Multiline types with defaults
//
// NOTE: `name = type := default` uses ` := ` as the default separator.
// Our custom parser handles this correctly because it does its own
// continuation-line joining.  These tests verify our parser works, and
// document the known YAML divergence.
// ---------------------------------------------------------------------------

#[test]
fn simple_param_with_default() {
    let yaml = "params:\n  - name = str := \"world\"";
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert_eq!(fm.declarations[0].name, "name");
    assert!(
        fm.declarations[0].default_value.is_some(),
        "default value should be set"
    );
}

#[test]
fn yaml_divergence_default_becomes_mapping() {
    // Demonstrate: serde_yaml sees `- x = str := val` as a string containing `:=`.
    let yaml = "params:\n  - priority = enum(High, Medium, Low) := Medium";
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).expect("serde_yaml parse");
    let seq = doc["params"].as_sequence().expect("sequence");
    assert!(
        seq[0].is_string(),
        "YAML interprets ` := ` as part of a string — no longer a divergence"
    );
}

#[test]
fn our_parser_handles_defaults_correctly() {
    // Our custom parser correctly interprets ` := ` as a default value separator.
    let source = "\
---

params:
  - items = list(
      name = str,
      score = int,
    )
  - label = str := \"untitled\"
---
body";
    let (fm, _) = parse_frontmatter(source).expect("parse failed");
    assert_eq!(fm.declarations.len(), 2, "should have 2 params");
    assert_eq!(fm.declarations[0].name, "items");
    assert!(
        fm.declarations[0].default_value.is_none(),
        "list should have no default"
    );
    assert_eq!(fm.declarations[1].name, "label");
    assert!(
        fm.declarations[1].default_value.is_some(),
        "label should have default"
    );
}

#[test]
fn our_parser_handles_enum_default_correctly() {
    let source = "\
---

params:
  - priority = enum(High, Medium, Low) := Medium
  - verbose = bool := true
---
body";
    let (fm, _) = parse_frontmatter(source).expect("parse failed");
    assert_eq!(fm.declarations.len(), 2);
    assert!(
        fm.declarations[0].default_value.is_some(),
        "priority should have default"
    );
    assert!(
        fm.declarations[1].default_value.is_some(),
        "verbose should have default",
    );
}

// ---------------------------------------------------------------------------
// Default values — serde_yaml cross-validation
//
// These tests verify that every default value syntax in the SPEC is valid
// YAML.  Each test parses the frontmatter with both serde_yaml (proving
// YAML validity) and our custom parser (proving correct semantics).
// ---------------------------------------------------------------------------

/// Helper: assert that a YAML block is valid YAML and that each `- item`
/// is parsed as a plain string by `serde_yaml`.
fn assert_yaml_params_are_strings(yaml: &str) -> Vec<String> {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(yaml).expect("serde_yaml parse failed — not valid YAML");
    let mapping = doc.as_mapping().expect("top-level should be a mapping");
    let params = mapping
        .get(serde_yaml::Value::String("params".into()))
        .expect("missing 'params' key");
    let seq = params.as_sequence().expect("'params' should be a sequence");
    seq.iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("each param should be a YAML string, got: {v:?}"))
                .to_string()
        })
        .collect()
}

#[test]
fn yaml_valid_string_default() {
    let yaml = r#"params:
  - name = str := "World""#;
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains(":="), "should contain := separator");
    // Also verify our parser handles it
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_int_default() {
    let yaml = "params:\n  - count = int := 42";
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains(":= 42"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_float_default() {
    let yaml = "params:\n  - threshold = float := 0.95";
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains(":= 0.95"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_bool_default() {
    let yaml = "params:\n  - verbose = bool := false";
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains(":= false"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_enum_unit_default() {
    let yaml = "params:\n  - status = enum(Active, Paused) := Active";
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains(":= Active"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_enum_struct_variant_default() {
    let yaml = r#"params:
  - outcome = enum(Confirmed(evidence = str), Rejected) := Confirmed(evidence = "found it")"#;
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains("Confirmed(evidence"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_option_none_default() {
    let yaml = "params:\n  - label = option(str) := None";
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains(":= None"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_option_some_default() {
    let yaml = r#"params:
  - label = option(str) := "hello""#;
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains(":= \"hello\""));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_struct_default() {
    let yaml = r#"params:
  - config = struct(timeout = int, label = str) := {timeout = 10, label = "fast"}"#;
    let strings = assert_yaml_params_are_strings(yaml);
    // YAML sees the whole thing including `{...}` as a plain string
    assert!(strings[0].contains("{timeout = 10"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_list_default_block_format() {
    // Block list format: `- tags = list(str) := ["rust", "go"]`
    // This is valid YAML because `[` is not the first character of the value.
    let yaml = r#"params:
  - tags = list(str) := ["rust", "go", "python"]"#;
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(
        strings[0].contains(r#"["rust""#),
        "YAML should preserve list literal as part of the plain string"
    );
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_list_of_structs_default() {
    let yaml = r#"params:
  - items = list(name = str, score = int) := [{name = "a", score = 10}]"#;
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains("[{name"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_const_reference_default() {
    let yaml = "\
consts:
  - MAX_RETRIES = int := 3

params:
  - retries = int := MAX_RETRIES";
    // Verify YAML validity for the whole block
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).expect("serde_yaml parse failed");
    let mapping = doc.as_mapping().expect("mapping");
    assert!(mapping.contains_key(serde_yaml::Value::String("consts".into())));
    assert!(mapping.contains_key(serde_yaml::Value::String("params".into())));
    // Verify our parser resolves the const reference
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_url_in_string_default() {
    let yaml = r#"params:
  - endpoint = str := "https://api.example.com/v1/data?key=abc&format=json""#;
    let strings = assert_yaml_params_are_strings(yaml);
    assert!(strings[0].contains("https://"));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(fm.declarations[0].default_value.is_some());
}

#[test]
fn yaml_valid_multiline_type_with_default() {
    // Multiline type declaration followed by a default on the next param.
    // YAML handles continuation lines via folding.
    let yaml = "\
params:
  - items = list(
      name = str,
      score = int,
    )
  - label = str := \"fallback\"";
    // serde_yaml should parse the continuation lines correctly
    let strings = assert_yaml_params_are_strings(yaml);
    assert_eq!(strings.len(), 2, "should have 2 params");
    assert!(
        strings[0].contains("name = str") && strings[0].contains("score = int"),
        "multiline list should be joined: {:?}",
        strings[0]
    );
    assert!(strings[1].contains(":= \"fallback\""));
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert_eq!(fm.declarations.len(), 2);
    assert!(fm.declarations[1].default_value.is_some());
}

#[test]
fn yaml_valid_mixed_defaults_and_no_defaults() {
    let yaml = r#"params:
  - name = str
  - greeting = str := "Hello"
  - count = int := 5
  - items = list(label = str)
  - verbose = bool := true"#;
    let strings = assert_yaml_params_are_strings(yaml);
    assert_eq!(strings.len(), 5);
    let source = source_from_yaml(yaml);
    let (fm, _) = parse_frontmatter(&source).expect("parse failed");
    assert!(
        fm.declarations[0].default_value.is_none(),
        "name has no default"
    );
    assert!(
        fm.declarations[1].default_value.is_some(),
        "greeting has default"
    );
    assert!(
        fm.declarations[2].default_value.is_some(),
        "count has default"
    );
    assert!(
        fm.declarations[3].default_value.is_none(),
        "items has no default"
    );
    assert!(
        fm.declarations[4].default_value.is_some(),
        "verbose has default"
    );
}

#[test]
fn yaml_invalid_inline_with_nested_brackets() {
    // Inline `params: [x := [...]]` breaks because YAML interprets the
    // nested `[` as a flow sequence.
    let yaml = r#"params: [tags = list(str) := ["a", "b"]]"#;
    let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "inline params with nested brackets should be invalid YAML",
    );
}

#[test]
fn yaml_inline_splits_compound_types_on_comma() {
    // Any compound type with commas (enum, list fields, struct fields)
    // is split into multiple items by YAML when using inline format.
    // This documents why inline params only work for simple scalars.

    // enum(A, B) — YAML sees two items: "status = enum(A" and "B)"
    let yaml = "params: [status = enum(Active, Paused)]";
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
    let seq = doc["params"].as_sequence().unwrap();
    assert!(seq.len() > 1, "YAML should split enum on comma: {seq:?}");

    // list(name = str, score = int) — same issue
    let yaml2 = "params: [items = list(name = str, score = int)]";
    let doc2: serde_yaml::Value = serde_yaml::from_str(yaml2).unwrap();
    let seq2 = doc2["params"].as_sequence().unwrap();
    assert!(
        seq2.len() > 1,
        "YAML should split list fields on comma: {seq2:?}",
    );
}

#[test]
fn yaml_inline_works_for_simple_scalars() {
    // Inline params DO work for simple scalar types without commas.
    let yaml = "params: [name = str, count = int, verbose = bool]";
    let doc: serde_yaml::Value =
        serde_yaml::from_str(yaml).expect("simple inline should be valid YAML");
    let seq = doc["params"].as_sequence().unwrap();
    assert_eq!(seq.len(), 3);
    assert_eq!(seq[0].as_str().unwrap(), "name = str");
    assert_eq!(seq[1].as_str().unwrap(), "count = int");
    assert_eq!(seq[2].as_str().unwrap(), "verbose = bool");
}
