//! FFI unit tests, grouped by topic.

mod context;
mod metadata;
mod options;
mod render;

/// Typed error payloads cross the FFI boundary as `<kind>\u{1f}<message>\u{1f}<json>`.
#[test]
fn error_details_json_encodes_typed_payload() {
    use md_tmpl::{SyntaxError, TemplateError};

    let syn = TemplateError::Syntax(SyntaxError::new("bad").at_line(6, "{{ \"q\" | x }}"));
    assert_eq!(
        crate::error_details_json(&syn.details()).as_deref(),
        Some(r#"{"line":6,"snippet":"{{ \"q\" | x }}"}"#)
    );
    let missing = TemplateError::MissingParams(vec!["a".into(), "b".into()]);
    assert_eq!(
        crate::error_details_json(&missing.details()).as_deref(),
        Some(r#"{"missing":["a","b"]}"#)
    );
    assert_eq!(
        crate::error_details_json(&TemplateError::Panic("x".into()).details()),
        None
    );
}
