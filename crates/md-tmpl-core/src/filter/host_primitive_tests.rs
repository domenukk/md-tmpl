//! Expectations for the host sanitization primitives, mirrored verbatim by the
//! TypeScript (`filter_parity.test.ts`), Python, and Go binding tests.

use alloc::{string::ToString, vec, vec::Vec};

use super::*;

/// Input exercising every decodable entity, a double-escaped entity, and unknown entities.
const DECODE_INPUT: &str = "a &lt;b&gt; &amp;lt; &quot;q&quot; &#39;s&apos; &unknown; &";
const DECODE_EXPECTED: &str = "a <b> &lt; \"q\" 's' &unknown; &";

const ATTR_INPUT: &str = "a&b<c>\"d'e\nf\rg\0h\u{1}i\u{7f}j\u{85}k";
const ATTR_EXPECTED: &str = "a&amp;b&lt;c&gt;&quot;d&apos;e\\nf\\rg\\0h i j k";

const BODY_INPUT: &str = "a&b<c>\"d'\n\0";
const BODY_EXPECTED: &str = "a&amp;b&lt;c&gt;\"d'\n\\0";

#[test]
fn decode_xml_str_single_pass() {
    assert_eq!(decode_xml_str(DECODE_INPUT), DECODE_EXPECTED);
}

#[test]
fn escape_xml_attr_str_escapes_markup_and_controls() {
    assert_eq!(escape_xml_attr_str(ATTR_INPUT), ATTR_EXPECTED);
}

#[test]
fn escape_xml_body_str_escapes_markup_and_nul() {
    assert_eq!(escape_xml_body_str(BODY_INPUT), BODY_EXPECTED);
}

fn letters(items: &[&str]) -> Vec<Value> {
    items.iter().map(|s| Value::Str((*s).to_string())).collect()
}

#[test]
fn truncate_middle_list_markers() {
    let items = letters(&["a", "b", "c", "d", "e"]);
    assert_eq!(
        truncate_middle_list(&items, 2, None),
        letters(&["a", "[TRUNCATED: 3 items omitted]", "e"])
    );
    assert_eq!(
        truncate_middle_list(&items, 2, Some("")),
        letters(&["a", "e"])
    );
    assert_eq!(
        truncate_middle_list(&items, 3, Some("..{skipped}..")),
        letters(&["a", "..2..", "d", "e"])
    );
    assert_eq!(truncate_middle_list(&items, 0, None), vec![]);
    assert_eq!(truncate_middle_list(&items, 5, None), items);
}
