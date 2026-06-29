//! Shared cross-backend test runner.
//!
//! Reads test case definitions from `tests/shared/inline_tmpl_tests.json`
//! and `tests/shared/include_tests.json`, and runs them against the Rust
//! template engine.
//!
//! These same fixtures are consumed by the TypeScript backend's test runner,
//! ensuring behavioral parity between implementations.
//!
//! Templates use `template_lines` / `parent_template_lines` (arrays of
//! strings joined with `\n`) so the JSON stays readable without inline `\n`.

use crate::{CompileOptions, Context, Template, Value};

/// Minimal JSON value parser for test fixtures.
/// Avoids adding `serde_json` as a dependency.
#[derive(Debug, Clone)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::Str(s) => Some(s),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            JsonValue::Object(o) => Some(o),
            _ => None,
        }
    }

    fn get(&self, key: &str) -> Option<&JsonValue> {
        self.as_object()
            .and_then(|o| o.iter().find(|(k, _)| k == key).map(|(_, v)| v))
    }

    /// Join an array of strings with `\n`, or return a plain string.
    fn join_lines(&self) -> String {
        match self {
            JsonValue::Array(items) => items
                .iter()
                .map(|v| v.as_str().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n"),
            JsonValue::Str(s) => s.clone(),
            _ => String::new(),
        }
    }

    fn to_value(&self) -> Value {
        match self {
            JsonValue::Null => Value::Str(String::new()),
            JsonValue::Bool(b) => Value::Bool(*b),
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::float_cmp
            )]
            JsonValue::Number(n) => {
                let as_int = *n as i64;
                if *n == as_int as f64 {
                    Value::Int(as_int)
                } else {
                    Value::Float(*n)
                }
            }
            JsonValue::Str(s) => Value::Str(s.clone()),
            JsonValue::Array(items) => Value::List(std::sync::Arc::new(
                items.iter().map(JsonValue::to_value).collect(),
            )),
            JsonValue::Object(fields) => {
                let map: crate::compat::HashMap<String, Value> = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_value()))
                    .collect();
                Value::Struct(std::sync::Arc::new(map))
            }
        }
    }

    fn to_context(&self) -> Context {
        let mut ctx = Context::new();
        if let Some(obj) = self.as_object() {
            for (key, val) in obj {
                ctx.set(key, val.to_value());
            }
        }
        ctx
    }
}

/// Simple recursive JSON parser.
fn parse_json(input: &str) -> JsonValue {
    let input = input.trim();
    let (val, _) = parse_json_value(input);
    val
}

fn parse_json_value(input: &str) -> (JsonValue, &str) {
    let input = input.trim();
    if input.starts_with('"') {
        parse_json_string(input)
    } else if input.starts_with('{') {
        parse_json_object(input)
    } else if input.starts_with('[') {
        parse_json_array(input)
    } else if let Some(rest) = input.strip_prefix("true") {
        (JsonValue::Bool(true), rest)
    } else if let Some(rest) = input.strip_prefix("false") {
        (JsonValue::Bool(false), rest)
    } else if let Some(rest) = input.strip_prefix("null") {
        (JsonValue::Null, rest)
    } else {
        // Number
        let end = input
            .find(|c: char| {
                !c.is_ascii_digit() && c != '.' && c != '-' && c != '+' && c != 'e' && c != 'E'
            })
            .unwrap_or(input.len());
        let num: f64 = input[..end].parse().unwrap_or(0.0);
        (JsonValue::Number(num), &input[end..])
    }
}

fn parse_json_string(input: &str) -> (JsonValue, &str) {
    assert!(input.starts_with('"'));
    let mut result = String::new();
    let bytes = input.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'"' => result.push('"'),
                b'\\' => result.push('\\'),
                b'n' => result.push('\n'),
                b'r' => result.push('\r'),
                b't' => result.push('\t'),
                b'/' => result.push('/'),
                other => {
                    result.push('\\');
                    result.push(other as char);
                }
            }
            i += 2;
        } else if bytes[i] == b'"' {
            return (JsonValue::Str(result), &input[i + 1..]);
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    (JsonValue::Str(result), "")
}

fn parse_json_object(input: &str) -> (JsonValue, &str) {
    assert!(input.starts_with('{'));
    let mut rest = input[1..].trim();
    let mut fields = Vec::new();
    if let Some(after) = rest.strip_prefix('}') {
        return (JsonValue::Object(fields), after);
    }
    loop {
        rest = rest.trim();
        let (key, after_key) = parse_json_string(rest);
        let key_str = key.as_str().unwrap().to_string();
        rest = after_key.trim();
        assert!(rest.starts_with(':'), "expected ':' after key, got: {rest}");
        rest = rest[1..].trim();
        let (val, after_val) = parse_json_value(rest);
        fields.push((key_str, val));
        rest = after_val.trim();
        if rest.starts_with(',') {
            rest = &rest[1..];
        } else if let Some(after) = rest.strip_prefix('}') {
            return (JsonValue::Object(fields), after);
        } else {
            return (JsonValue::Object(fields), rest);
        }
    }
}

fn parse_json_array(input: &str) -> (JsonValue, &str) {
    assert!(input.starts_with('['));
    let mut rest = input[1..].trim();
    let mut items = Vec::new();
    if let Some(after) = rest.strip_prefix(']') {
        return (JsonValue::Array(items), after);
    }
    loop {
        rest = rest.trim();
        let (val, after_val) = parse_json_value(rest);
        items.push(val);
        rest = after_val.trim();
        if rest.starts_with(',') {
            rest = &rest[1..];
        } else if let Some(after) = rest.strip_prefix(']') {
            return (JsonValue::Array(items), after);
        } else {
            return (JsonValue::Array(items), rest);
        }
    }
}

// ---------------------------------------------------------------------------
// Shared inline template tests
// ---------------------------------------------------------------------------

#[test]
fn shared_inline_tmpl_tests() {
    let json_str = include_str!("../../../../tests/shared/inline_tmpl_tests.json");
    let root = parse_json(json_str);
    let tests = root.get("tests").unwrap().as_array().unwrap();

    for tc in tests {
        let name = tc.get("name").unwrap().as_str().unwrap();
        let template_src = tc.get("template_lines").unwrap().join_lines();
        let params = tc.get("params").unwrap();

        if let Some(expected_output) = tc.get("expected_output") {
            let expected = expected_output.as_str().unwrap();
            let tmpl = Template::from_source(&template_src)
                .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            let ctx = params.to_context();
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_error) = tc.get("expected_error") {
            let expected_substr = expected_error.as_str().unwrap();
            let result = Template::from_source(&template_src).and_then(|tmpl| {
                let ctx = params.to_context();
                tmpl.render_ctx(&ctx)
            });
            let err = result.unwrap_err();
            assert!(
                err.to_string()
                    .to_lowercase()
                    .contains(&expected_substr.to_lowercase()),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared file-based include tests
// ---------------------------------------------------------------------------

#[test]
fn shared_include_tests() {
    let json_str = include_str!("../../../../tests/shared/include_tests.json");
    let root = parse_json(json_str);
    let tests = root.get("tests").unwrap().as_array().unwrap();

    for tc in tests {
        let name = tc.get("name").unwrap().as_str().unwrap();
        let parent_template = tc.get("parent_template_lines").unwrap().join_lines();
        let files = tc.get("files").unwrap().as_object().unwrap();
        let params = tc.get("params").unwrap();

        // Create temp dir with include files.
        let dir = tempfile::tempdir().unwrap();
        for (filename, content) in files {
            let content_str = content.join_lines();
            std::fs::write(dir.path().join(filename), content_str).unwrap();
        }

        if let Some(expected_output) = tc.get("expected_output") {
            let expected = expected_output.as_str().unwrap();
            let (tmpl, _fm) = Template::compile(
                &parent_template,
                CompileOptions::default().base_dir(dir.path()),
            )
            .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            let ctx = params.to_context();
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_error) = tc.get("expected_error") {
            let expected_substr = expected_error.as_str().unwrap();
            let result = Template::compile(
                &parent_template,
                CompileOptions::default().base_dir(dir.path()),
            )
            .and_then(|(tmpl, _fm)| {
                let ctx = params.to_context();
                tmpl.render_ctx(&ctx)
            });
            let err = result.unwrap_err();
            assert!(
                err.to_string()
                    .to_lowercase()
                    .contains(&expected_substr.to_lowercase()),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared inline control flow tests (if, for, match, etc.)
// ---------------------------------------------------------------------------

#[test]
fn shared_inline_control_tests() {
    let json_str = include_str!("../../../../tests/shared/inline_control_tests.json");
    let root = parse_json(json_str);
    let tests = root.get("tests").unwrap().as_array().unwrap();

    for tc in tests {
        let name = tc.get("name").unwrap().as_str().unwrap();
        let template_src = tc.get("template_lines").unwrap().join_lines();
        let params = tc.get("params").unwrap();

        if let Some(expected_output) = tc.get("expected_output") {
            let expected = expected_output.as_str().unwrap();
            let tmpl = Template::from_source(&template_src)
                .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            let ctx = params.to_context();
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_error) = tc.get("expected_error") {
            let expected_substr = expected_error.as_str().unwrap();
            let result = Template::from_source(&template_src).and_then(|tmpl| {
                let ctx = params.to_context();
                tmpl.render_ctx(&ctx)
            });
            let err = result.unwrap_err();
            assert!(
                err.to_string()
                    .to_lowercase()
                    .contains(&expected_substr.to_lowercase()),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared tmpl() parameter tests
// ---------------------------------------------------------------------------

#[test]
fn shared_tmpl_param_tests() {
    let json_str = include_str!("../../../../tests/shared/tmpl_param_tests.json");
    let root = parse_json(json_str);
    let tests = root.get("tests").unwrap().as_array().unwrap();

    for tc in tests {
        let name = tc.get("name").unwrap().as_str().unwrap();
        let template_src = tc.get("template_lines").unwrap().join_lines();
        let params = tc.get("params").unwrap();

        if let Some(expected_output) = tc.get("expected_output") {
            let expected = expected_output.as_str().unwrap();
            let tmpl = Template::from_source(&template_src)
                .unwrap_or_else(|e| panic!("[{name}] parse failed: {e}"));
            let ctx = params.to_context();
            let output = tmpl
                .render_ctx(&ctx)
                .unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            assert_eq!(output, expected, "[{name}] output mismatch");
        } else if let Some(expected_error) = tc.get("expected_error") {
            let expected_substr = expected_error.as_str().unwrap();
            let result = Template::from_source(&template_src).and_then(|tmpl| {
                let ctx = params.to_context();
                tmpl.render_ctx(&ctx)
            });
            let err = result.unwrap_err();
            assert!(
                err.to_string()
                    .to_lowercase()
                    .contains(&expected_substr.to_lowercase()),
                "[{name}] expected error containing \"{expected_substr}\", got: \"{err}\""
            );
        }
    }
}
