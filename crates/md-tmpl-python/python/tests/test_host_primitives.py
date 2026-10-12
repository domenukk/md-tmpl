"""Host sanitization primitives exposed from the Rust core.

Expectations mirror ``crates/md-tmpl-core/src/filter/host_primitive_tests.rs``
and the TypeScript ``filter_parity.test.ts`` verbatim.
"""

import md_tmpl

DECODE_INPUT = "a &lt;b&gt; &amp;lt; &quot;q&quot; &#39;s&apos; &unknown; &"
DECODE_EXPECTED = "a <b> &lt; \"q\" 's' &unknown; &"
ATTR_INPUT = "a&b<c>\"d'e\nf\rg\0h\x01i\x7fj\x85k"
ATTR_EXPECTED = "a&amp;b&lt;c&gt;&quot;d&apos;e\\nf\\rg\\0h i j k"
BODY_INPUT = "a&b<c>\"d'\n\0"
BODY_EXPECTED = "a&amp;b&lt;c&gt;\"d'\n\\0"
LETTERS = ["a", "b", "c", "d", "e"]


def test_xml_codec() -> None:
    assert md_tmpl.decode_xml(DECODE_INPUT) == DECODE_EXPECTED
    assert md_tmpl.escape_xml_attr(ATTR_INPUT) == ATTR_EXPECTED
    assert md_tmpl.escape_xml_body(BODY_INPUT) == BODY_EXPECTED
    assert len(md_tmpl.XML_DECODE_PAIRS) == 6
    assert ("&amp;", "&") in md_tmpl.XML_DECODE_PAIRS


def test_constants() -> None:
    assert md_tmpl.SANITIZE_NOTICE_TAG_PLACEHOLDER == "{tag}"
    assert md_tmpl.TOOL_OUTPUT_QUARANTINE_OPEN_TAG == "<tool_output_quarantine>"
    assert md_tmpl.TOOL_OUTPUT_QUARANTINE_CLOSE_TAG == "</tool_output_quarantine>"
    assert md_tmpl.UNTRUSTED_QUARANTINE_OPEN_TAG == "<untrusted_tool_output>"
    assert md_tmpl.UNTRUSTED_QUARANTINE_CLOSE_TAG == "</untrusted_tool_output>"
    assert md_tmpl.TOOL_OUTPUT_QUARANTINE_TAG == "tool_output_quarantine"
    assert md_tmpl.UNTRUSTED_TOOL_OUTPUT_TAG == "untrusted_tool_output"


def test_format_sanitize_notice() -> None:
    assert md_tmpl.format_sanitize_notice("doc", "see {tag}") == "see doc"
    assert md_tmpl.format_sanitize_notice("doc", "") == ""
    default = md_tmpl.format_sanitize_notice("doc")
    assert default == md_tmpl.DEFAULT_SANITIZE_NOTICE.replace("{tag}", "doc")


def test_envelope_helpers() -> None:
    assert md_tmpl.strip_outer_quarantine_tag("<t>\n\nx\n\n</t>", "t") == "\nx\n"
    assert md_tmpl.strip_outer_quarantine_tag("<t>", "t") is None
    clean = "<untrusted_content>\nok\n</untrusted_content>"
    assert md_tmpl.unquarantine(clean) == "ok"
    assert md_tmpl.unquarantine("<doc>\n</doc>\n</doc>", "doc") is None
    assert md_tmpl.quarantine_untrusted_idempotent("<doc>\nhi\n</doc>", "doc") == (
        "<doc>\nhi\n</doc>"
    )
    assert md_tmpl.quarantine_untrusted_idempotent(
        "<doc>\n<|im_start|>\n</doc>", "doc"
    ) == ("<doc>\n&lt;|im_start|&gt;\n</doc>")
    block = md_tmpl.sanitize_block("payload", "doc")
    assert md_tmpl.sanitize_block_idempotent(block, "doc") == block


def test_truncation() -> None:
    assert md_tmpl.truncate_middle_list(LETTERS, 2) == [
        "a",
        "[TRUNCATED: 3 items omitted]",
        "e",
    ]
    assert md_tmpl.truncate_middle_list(LETTERS, 2, "") == ["a", "e"]
    assert md_tmpl.truncate_middle_list(LETTERS, 3, "..{skipped}..") == [
        "a",
        "..2..",
        "d",
        "e",
    ]
    assert md_tmpl.truncate_middle_list(LETTERS, 0) == []
    assert md_tmpl.truncate_middle("abcdef", 100) == "abcdef"
