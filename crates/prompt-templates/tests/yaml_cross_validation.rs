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
    let yaml = "params:\n  - items = list<name = str, score = int>";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "list type mismatch");
}

#[test]
fn list_type_multiline() {
    let yaml = "params:\n  - items = list<\n      name = str,\n      score = int,\n    >";
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
    let yaml = "params:\n  - severity = enum<Critical, High, Medium, Low>";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum type mismatch");
}

#[test]
fn enum_type_with_fields_single_line() {
    let yaml = "params:\n  - outcome = enum<Confirmed(evidence = str), Rejected, NeedsWork>";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum with fields mismatch");
}

#[test]
fn enum_type_multiline() {
    let yaml = "\
params:
  - outcome = enum<
      Confirmed(evidence = str),
      Rejected,
      NeedsWork,
    >";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiline enum mismatch");
}

#[test]
fn enum_type_multiple_fields_per_variant() {
    let yaml = "params:\n  - result = enum<Success(code = int, msg = str), Failure(reason = str, retryable = bool)>";
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
  - result = enum<
      Success(code = int, msg = str),
      Failure(reason = str, retryable = bool),
      Pending,
    >";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiline enum with multiple fields mismatch");
}

#[test]
fn enum_with_nested_list_field() {
    let yaml = "params:\n  - action = enum<Batch(items = list<name = str>), Single(name = str)>";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum with nested list field mismatch");
}

#[test]
fn multiple_enum_params() {
    let yaml = "\
params:
  - priority = enum<High, Medium, Low>
  - status = enum<Open, Closed, InProgress>
  - severity = enum<Critical(reason = str), Warning, Info>";
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
    let yaml = "params:\n  - agent_name = str\n  - bugs = list<title = str, severity = enum<Critical(reason = str), High, Medium, Low>>";
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
  - bugs = list<
      title = str,
      severity = enum<Critical(reason = str), High, Medium, Low>,
    >";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "nested list+enum mismatch (multiline)");
}

#[test]
fn dict_type_single_line() {
    let yaml = "params:\n  - metadata = dict<key = str, value = int>";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "dict type mismatch");
}

#[test]
fn dict_type_multiline() {
    let yaml = "\
params:
  - metadata = dict<
      key = str,
      value = int,
    >";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "multiline dict type mismatch");
}

#[test]
fn deeply_nested_multiline() {
    let yaml = "\
params:
  - reports = list<
      title = str,
      findings = list<
        description = str,
        severity = enum<Critical(reason = str), High, Medium, Low>,
      >,
    >";
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
  - items = list<
      label = str,
      tags = list<name = str>,
    >
  - status = enum<Active, Inactive>";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "mixed simple+complex mismatch");
}

// ---------------------------------------------------------------------------
// End-to-end: parse + render the exact README hero example
// ---------------------------------------------------------------------------

#[test]
fn readme_hero_example_renders_correctly() {
    use prompt_templates::{Context, Template, Value};

    let source = "\
---
params:
  - agent_name = str
  - bugs = list<
      title = str,
      severity = enum<Critical(reason = str), High, Medium, Low>,
    >
---

# Report — {{ agent_name }}

> {% for bug in bugs %}

- **{{ bug.title }}**
> {% match bug.severity %}
> {% case Critical %}
  🔴 Immediate action required: `{{ bug.severity.reason }}`
> {% case High %}
  🟠 High priority.
> {% case Medium | Low %}
  🟢 Normal priority.
> {% /match %}
> {% /for %}";

    let tmpl = Template::from_source(source).expect("README hero example should parse");

    let mut ctx = Context::new();
    ctx.set("agent_name", "SecurityBot");
    ctx.set(
        "bugs",
        vec![
            Value::dict([
                ("title", Value::from("Buffer overflow")),
                (
                    "severity",
                    Value::dict([
                        ("tag", Value::from("Critical")),
                        ("reason", Value::from("RCE in parser")),
                    ]),
                ),
            ]),
            Value::dict([
                ("title", Value::from("Missing CSRF token")),
                ("severity", Value::dict([("tag", Value::from("High"))])),
            ]),
            Value::dict([
                ("title", Value::from("Verbose logging")),
                ("severity", Value::dict([("tag", Value::from("Low"))])),
            ]),
        ],
    );

    let output = tmpl
        .render(&ctx)
        .expect("README hero example should render");
    assert!(
        output.contains("# Report — SecurityBot"),
        "should contain header: {output}"
    );
    assert!(
        output.contains("**Buffer overflow**"),
        "should contain bug title: {output}"
    );
    assert!(
        output.contains("RCE in parser"),
        "should contain Critical reason: {output}"
    );
    assert!(
        output.contains("🟠 High priority"),
        "should contain High priority: {output}"
    );
    assert!(
        output.contains("🟢 Normal priority"),
        "should contain Low priority: {output}"
    );
}

// ---------------------------------------------------------------------------
// Nested complex types inside enum variants
// ---------------------------------------------------------------------------

#[test]
fn enum_with_nested_list_field_crossval() {
    let yaml = "\
params:
  - action = enum<
      Batch(items = list<name = str, priority = int>),
      Single(name = str),
    >";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(
        ours, theirs,
        "enum with nested list field (multiline) mismatch"
    );
}

#[test]
fn enum_with_nested_dict_field_crossval() {
    let yaml = "\
params:
  - response = enum<
      Success(metadata = dict<key = str, value = str>),
      Error(code = int, details = str),
    >";
    let source = source_from_yaml(yaml);
    let ours = our_parser_params(&source);
    let theirs = serde_yaml_parsed_params(yaml);
    assert_eq!(ours, theirs, "enum with nested dict field mismatch");
}

#[test]
fn enum_with_nested_enum_field_crossval() {
    let yaml = "params:\n  - result = enum<Done(status = enum<Pass, Fail>), Pending>";
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
    let yaml = "params:\n  - name = str := world";
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
    let yaml = "params:\n  - priority = enum<High, Medium, Low> := Medium";
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
  - items = list<
      name = str,
      score = int,
    >
  - label = str := untitled
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
  - priority = enum<High, Medium, Low> := Medium
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
        "verbose should have default"
    );
}
