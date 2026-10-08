//! Quoting, escaping, and `#` comment regression tests for frontmatter defaults.

use super::{
    test_helpers::{default_of, mdtmpl_list_scalar, yaml_list_scalar},
    *,
};
use crate::value::Value;

// Regression: delimiters inside quoted strings in frontmatter defaults
//
// Commas and the bracket family `()[]{}<>` that appear inside a `"..."` or
// `'...'` string literal in a `:= ...` default value must be treated as
// literal characters, not as element/field separators. These end-to-end
// tests parse a full template via the public `parse_frontmatter` API and
// assert the resulting default `Value`s (matrix cases T1–T8).
// =========================================================================

#[test]
fn t1_list_of_str_commas_in_double_quotes() {
    // T1: `list(str) := ["a, b", "c, d"]` → 2 items, commas preserved.
    match default_of(r#"x = list(str) := ["a, b", "c, d"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 2, "commas inside quotes must not split items");
            assert_eq!(items[0], Value::Str("a, b".to_string()));
            assert_eq!(items[1], Value::Str("c, d".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t2_struct_field_with_comma_in_quotes() {
    // T2: `struct(msg = str, n = int) := {msg = "a, b", n = 1}`.
    match default_of(r#"x = struct(msg = str, n = int) := {msg = "a, b", n = 1}"#) {
        Value::Struct(map) => {
            assert_eq!(map.get("msg"), Some(&Value::Str("a, b".to_string())));
            assert_eq!(map.get("n"), Some(&Value::Int(1)));
        }
        other => panic!("Expected Struct, got {other:?}"),
    }
}

#[test]
fn t3_list_of_records_note_field_with_commas() {
    // T3: list of records; first record's `note` holds a comma-laden string.
    match default_of(
        r#"x = list(name = str, note = str) := [{name = "x", note = "p, q, r"}, {name = "y", note = "s"}]"#,
    ) {
        Value::List(items) => {
            assert_eq!(items.len(), 2, "record separators must be top-level only");
            match &items[0] {
                Value::Struct(m) => {
                    assert_eq!(m.get("name"), Some(&Value::Str("x".to_string())));
                    assert_eq!(m.get("note"), Some(&Value::Str("p, q, r".to_string())));
                }
                other => panic!("Expected record 0 Struct, got {other:?}"),
            }
            match &items[1] {
                Value::Struct(m) => {
                    assert_eq!(m.get("name"), Some(&Value::Str("y".to_string())));
                    assert_eq!(m.get("note"), Some(&Value::Str("s".to_string())));
                }
                other => panic!("Expected record 1 Struct, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t4_list_of_str_brackets_in_quotes() {
    // T4: brackets/braces/parens inside quoted list elements stay intact.
    match default_of(r#"x = list(str) := ["a[b]c", "d{e}f", "g(h)i"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::Str("a[b]c".to_string()));
            assert_eq!(items[1], Value::Str("d{e}f".to_string()));
            assert_eq!(items[2], Value::Str("g(h)i".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t5_list_of_str_single_quotes_with_comma() {
    // T5: single-quoted elements with an embedded comma.
    match default_of("x = list(str) := ['a, b', 'c']") {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("a, b".to_string()));
            assert_eq!(items[1], Value::Str("c".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t6_list_of_str_unicode_em_dash_and_emoji() {
    // T6: multi-byte unicode (em-dash, emoji) intact, and a comma inside a
    // quoted element with unicode must not split it.
    match default_of("x = list(str) := [\"Theory — not a finding\", \"✅ done, ok\"]") {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("Theory — not a finding".to_string()));
            assert_eq!(items[1], Value::Str("✅ done, ok".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t7_list_of_records_with_empty_strings() {
    // T7: empty quoted strings parse, and a comma inside a later field stays.
    match default_of(
        r#"x = list(name = str, note = str) := [{name = "", note = ""}, {name = "a", note = "y, z"}]"#,
    ) {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            match &items[0] {
                Value::Struct(m) => {
                    assert_eq!(m.get("name"), Some(&Value::Str(String::new())));
                    assert_eq!(m.get("note"), Some(&Value::Str(String::new())));
                }
                other => panic!("Expected record 0 Struct, got {other:?}"),
            }
            match &items[1] {
                Value::Struct(m) => {
                    assert_eq!(m.get("name"), Some(&Value::Str("a".to_string())));
                    assert_eq!(m.get("note"), Some(&Value::Str("y, z".to_string())));
                }
                other => panic!("Expected record 1 Struct, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn t8_severity_ladder_shape_prose_with_delimiters_intact() {
    // T8: real-world SEVERITY_LADDER shape — a list of multi-field records
    // whose prose fields contain commas, em-dashes, and emoji. The
    // comma-bearing prose field must survive parsing intact.
    let default = default_of(
        "ladder = list(tier = str, short = str, proves = str) := \
         [{tier = \"L1\", short = \"Memory safety\", \
         proves = \"Crash, panic, or sanitizer report — reliably reproduced\"}, \
         {tier = \"L2\", short = \"Logic bug\", \
         proves = \"Incorrect output ✅, exploitable\"}]",
    );
    match default {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            match &items[0] {
                Value::Struct(m) => {
                    assert_eq!(m.get("tier"), Some(&Value::Str("L1".to_string())));
                    assert_eq!(
                        m.get("short"),
                        Some(&Value::Str("Memory safety".to_string()))
                    );
                    assert_eq!(
                        m.get("proves"),
                        Some(&Value::Str(
                            "Crash, panic, or sanitizer report — reliably reproduced".to_string()
                        )),
                        "comma/em-dash prose must be intact",
                    );
                }
                other => panic!("Expected record 0 Struct, got {other:?}"),
            }
            match &items[1] {
                Value::Struct(m) => {
                    assert_eq!(m.get("tier"), Some(&Value::Str("L2".to_string())));
                    assert_eq!(
                        m.get("proves"),
                        Some(&Value::Str("Incorrect output ✅, exploitable".to_string())),
                        "emoji + comma prose must be intact",
                    );
                }
                other => panic!("Expected record 1 Struct, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

// =========================================================================
// Regression: additional delimiter/quoting edge cases (E1–E5)
// =========================================================================

#[test]
fn e1_double_quoted_string_with_apostrophe_and_comma() {
    // E1: an apostrophe inside a double-quoted element must not end the quote,
    // so the embedded comma stays literal.
    match default_of(r#"x = list(str) := ["it's, fine", "ok"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("it's, fine".to_string()));
            assert_eq!(items[1], Value::Str("ok".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn e2_single_quoted_string_with_double_quotes_and_comma() {
    // E2: double quotes inside a single-quoted element are literal, so the
    // embedded comma is preserved.
    match default_of(r#"x = list(str) := ['say "hi", bye', "z"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::Str("say \"hi\", bye".to_string()));
            assert_eq!(items[1], Value::Str("z".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn e3_nested_list_of_lists_with_embedded_commas() {
    // E3: list(list(str)) — commas inside quoted innermost elements must not
    // split the inner lists.
    match default_of(r#"x = list(list(str)) := [["a, b"], ["c, d", "e"]]"#) {
        Value::List(outer) => {
            assert_eq!(outer.len(), 2);
            match &outer[0] {
                Value::List(inner) => {
                    assert_eq!(inner.len(), 1);
                    assert_eq!(inner[0], Value::Str("a, b".to_string()));
                }
                other => panic!("Expected inner list 0, got {other:?}"),
            }
            match &outer[1] {
                Value::List(inner) => {
                    assert_eq!(inner.len(), 2);
                    assert_eq!(inner[0], Value::Str("c, d".to_string()));
                    assert_eq!(inner[1], Value::Str("e".to_string()));
                }
                other => panic!("Expected inner list 1, got {other:?}"),
            }
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn e4_whitespace_inside_quotes_preserved() {
    // E4: leading/trailing spaces inside a quoted string are part of the value
    // and must NOT be trimmed.
    match default_of(r#"x = list(str) := [" a, b "]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0], Value::Str(" a, b ".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

// =========================================================================
// Escape-sequence matrix (S1–S5): backslash escapes in string literals.
// See scratch/md_tmpl_escapes_spec.md. Only `\\`, `\"`, `\'` are unescaped;
// every other `\X` is preserved verbatim (backward-compatible).
// =========================================================================

#[test]
fn s1_escaped_both_quote_types() {
    // S1: escaped double quotes inside a double-quoted literal, plus a bare
    // apostrophe, all decode to the literal characters.
    assert_eq!(
        default_of(r#"x = str := "he said \"hi\" it's ok""#),
        Value::Str(r#"he said "hi" it's ok"#.to_string()),
    );
}

#[test]
fn s2_escaped_single_quote_in_single_quoted() {
    // S2: `\'` inside a single-quoted literal decodes to `'`; the embedded
    // comma stays literal because it is inside the (still-open) quote.
    assert_eq!(
        default_of(r"x = str := 'it\'s, fine'"),
        Value::Str("it's, fine".to_string()),
    );
}

#[test]
fn s3_escaped_backslash() {
    // S3: `\\` decodes to a single backslash.
    assert_eq!(
        default_of(r#"x = str := "a\\b""#),
        Value::Str("a\\b".to_string()),
    );
}

#[test]
fn s4_backslash_before_delimiter_does_not_split_list() {
    // S4: an escaped quote (`\"`) must not close the string, so the following
    // comma stays inside the first element instead of splitting the list.
    match default_of(r#"x = list(str) := ["a\", b", "c"]"#) {
        Value::List(items) => {
            assert_eq!(items.len(), 2, "escaped quote must not split the list");
            assert_eq!(items[0], Value::Str(r#"a", b"#.to_string()));
            assert_eq!(items[1], Value::Str("c".to_string()));
        }
        other => panic!("Expected List, got {other:?}"),
    }
}

#[test]
fn s5_unknown_escape_preserved_verbatim() {
    // S5: unrecognized escapes (`\p`, `\n`) keep both the backslash and the
    // following char — this pass does NOT interpret C-style whitespace escapes.
    assert_eq!(
        default_of(r#"x = str := "c:\path\n""#),
        Value::Str("c:\\path\\n".to_string()),
    );
}

// =========================================================================
// YAML-consistent `#` comments (C1–C4) + cross-validation against serde_yaml.
// md-tmpl frontmatter list-item scalars must be extracted identically to how a
// real YAML parser extracts them. See scratch/md_tmpl_escapes_spec.md §Change 2.
// =========================================================================

#[test]
fn cross_validation_matches_serde_yaml_scalar_extraction() {
    // A corpus covering: plain commas/brackets (unchanged), an inline ` #`
    // comment (stripped), a `#` with no leading space (kept), and an
    // outer-quoted decl that protects its `#`. md-tmpl must agree with YAML
    // on every one.
    let corpus = [
        // Plain scalar with a comma — no comment, unchanged.
        "a = str := hello, world",
        // Plain scalar with brackets and a comma — unchanged.
        "x = list(str) := [a, b]",
        // Inline comment: ` #` preceded by whitespace is stripped.
        "x = int := 3 # the retry count",
        // `#` with no leading whitespace is a literal character, kept.
        "x = str := a#b,c",
        // Outer YAML double-quoted scalar protects the inner `#`.
        r#""x = str := \"a # b, c\"""#,
    ];
    for item in corpus {
        assert_eq!(
            mdtmpl_list_scalar(item),
            yaml_list_scalar(item),
            "md-tmpl and serde_yaml disagree on scalar extraction for `{item}`",
        );
    }
}

#[test]
fn c1_space_hash_truncates_like_yaml() {
    // C1: ` #` (hash preceded by whitespace) starts a YAML comment even inside
    // what looks like a quoted md-tmpl literal — the scalar is truncated at the
    // comment. md-tmpl must extract exactly what YAML extracts.
    let item = r#"x = str := "a # b""#;
    assert_eq!(mdtmpl_list_scalar(item), yaml_list_scalar(item));
    assert_eq!(mdtmpl_list_scalar(item), r#"x = str := "a"#);
}

#[test]
fn c2_hash_without_leading_space_is_literal() {
    // C2: a `#` not preceded by whitespace is an ordinary character and is
    // preserved by both md-tmpl and YAML.
    let item = r#"x = str := "a#b,c""#;
    assert_eq!(mdtmpl_list_scalar(item), yaml_list_scalar(item));
    assert_eq!(mdtmpl_list_scalar(item), r#"x = str := "a#b,c""#);
}

#[test]
fn c3_outer_yaml_quotes_protect_hash() {
    // C3: an outer YAML double-quoted scalar protects an inner `#`, so the full
    // md-tmpl declaration (including `# b, c`) is recovered intact.
    let item = r#""x = str := \"a # b, c\"""#;
    assert_eq!(mdtmpl_list_scalar(item), yaml_list_scalar(item));
    assert_eq!(mdtmpl_list_scalar(item), r#"x = str := "a # b, c""#);
}

#[test]
fn c4_trailing_comment_stripped_and_default_parses() {
    // C4: a trailing explanatory comment is stripped so the numeric default
    // still parses to its value.
    let source = "---
params:
  - x = int := 3 # the retry count
---
body";
    let (fm, _) =
        parse_frontmatter(source).expect("frontmatter with trailing comment should parse");
    assert_eq!(fm.declarations[0].default_value, Some(Value::Int(3)));
}
