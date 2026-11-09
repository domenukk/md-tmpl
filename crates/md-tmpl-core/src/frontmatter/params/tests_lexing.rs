//! Line joining and depth-aware splitting/search tests for declaration parsing.

use super::*;

// =========================================================================
// join_continuation_lines
// =========================================================================

#[test]
fn join_normal_lines() {
    let block = "line1\nline2\nline3";
    let result = join_continuation_lines(block);
    assert_eq!(result, vec!["line1", "line2", "line3"]);
}

#[test]
fn join_indented_continuation() {
    let block = "key:\n  continued\n  more";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "key: continued more");
}

#[test]
fn join_tab_continuation() {
    let block = "key:\n\tcontinued\n\tmore";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "key: continued more");
}

#[test]
fn join_first_line_indented() {
    // If the very first line is indented, there's no previous line to join to,
    // so it becomes its own logical line.
    let block = "  indented_first\nsecond";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "  indented_first");
    assert_eq!(result[1], "second");
}

#[test]
fn join_multiple_groups() {
    let block = "key1: val1\n  continued1\nkey2: val2\n  continued2";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "key1: val1 continued1");
    assert_eq!(result[1], "key2: val2 continued2");
}

#[test]
fn join_empty_block() {
    let result = join_continuation_lines("");
    assert!(result.is_empty());
}

#[test]
fn join_no_continuations() {
    let block = "a\nb\nc";
    let result = join_continuation_lines(block);
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn join_blank_line_between_entries_not_dropped() {
    // REGRESSION: a blank line between two top-level entries must not create an
    // empty logical line nor drop the second entry. Previously the blank line
    // was emitted as its own logical line, orphaning everything after it.
    let block = "- FIRST\n\n- SECOND";
    let result = join_continuation_lines(block);
    assert_eq!(result, vec!["- FIRST", "- SECOND"]);
}

#[test]
fn join_skips_full_line_comments() {
    // REGRESSION: full-line `#` comments are documentation only and must be
    // skipped without terminating an in-progress block list.
    let block = "- A\n# a comment\n- B";
    let result = join_continuation_lines(block);
    assert_eq!(result, vec!["- A", "- B"]);
}

#[test]
fn join_skips_blank_lines_within_continuation() {
    // A blank line inside a multi-line value continuation is skipped, and the
    // following indented line still appends to the same logical line.
    let block = "key: val\n  cont1\n\n  cont2";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "key: val cont1 cont2");
}

#[test]
fn join_multiline_value_with_blank_and_comment() {
    // REGRESSION: mirrors the ARTIST SEVERITY_LADDER const — a section whose
    // entry value spans multiple indented lines, with a blank line and a `#`
    // comment interleaved between entries. All entries must survive and the
    // multi-line value must be joined onto a single logical line.
    let block = "consts:\n  - A = str := \"x\"\n\n  # doc comment\n  - B = list(n = int) :=\n    [{n = 1},\n    {n = 2}]";
    let result = join_continuation_lines(block);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0],
        "consts: - A = str := \"x\" - B = list(n = int) := [{n = 1}, {n = 2}]"
    );
}

// =========================================================================
// split_at_depth_zero
// =========================================================================

#[test]
fn split_simple_comma() {
    let result = split_at_depth_zero("a, b, c");
    assert_eq!(result, vec!["a", " b", " c"]);
}

#[test]
fn split_nested_angle_brackets_preserved() {
    let result = split_at_depth_zero("name = str, items = list<label = str, count = int>");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "name = str");
    assert_eq!(result[1], " items = list<label = str, count = int>");
}

#[test]
fn split_nested_parens() {
    let result = split_at_depth_zero("A(x = str, y = int), B");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "A(x = str, y = int)");
    assert_eq!(result[1], " B");
}

#[test]
fn split_empty_input() {
    let result = split_at_depth_zero("");
    assert_eq!(result, vec![""]);
}

#[test]
fn split_single_entry() {
    let result = split_at_depth_zero("only_one");
    assert_eq!(result, vec!["only_one"]);
}

#[test]
fn split_nested_braces() {
    let result = split_at_depth_zero("{a: 1, b: 2}, c");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "{a: 1, b: 2}");
    assert_eq!(result[1], " c");
}

#[test]
fn split_deeply_nested() {
    let result = split_at_depth_zero("list<list<a = str, b = list<c = int>>>, x = bool");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "list<list<a = str, b = list<c = int>>>");
    assert_eq!(result[1], " x = bool");
}

#[test]
fn split_ignores_comma_inside_double_quotes() {
    // REGRESSION: commas inside a double-quoted string must not be treated as
    // field separators (e.g. `mem_example = "Crash, panic, sanitizer report"`).
    let result = split_at_depth_zero("msg = \"a, b, c\", n = 1");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "msg = \"a, b, c\"");
    assert_eq!(result[1], " n = 1");
}

#[test]
fn split_ignores_comma_inside_single_quotes() {
    let result = split_at_depth_zero("a = 'x, y', b = 2");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "a = 'x, y'");
    assert_eq!(result[1], " b = 2");
}

#[test]
fn split_ignores_brackets_inside_quotes() {
    // Brackets/braces inside a quoted string must not affect depth tracking.
    let result = split_at_depth_zero("a = \"x [y, z] {q}\", b = 2");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "a = \"x [y, z] {q}\"");
    assert_eq!(result[1], " b = 2");
}

#[test]
fn split_two_quoted_strings_each_with_commas() {
    // T1 low-level: two quoted list elements, each containing a comma. Only the
    // top-level comma between the two literals is a separator.
    let result = split_at_depth_zero("\"a, b\", \"c, d\"");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "\"a, b\"");
    assert_eq!(result[1], " \"c, d\"");
}

#[test]
fn split_ignores_parens_inside_quotes() {
    // Parentheses inside a quoted string must not affect depth tracking.
    let result = split_at_depth_zero("a = \"g(h)i\", b = 2");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "a = \"g(h)i\"");
    assert_eq!(result[1], " b = 2");
}

#[test]
fn split_ignores_multiple_embedded_delimiters_in_one_string() {
    // A single quoted string containing many commas and every bracket family
    // must remain one logical field; only the trailing top-level comma splits.
    let result = split_at_depth_zero("note = \"p, q, [r], {s}, (t), <u>\", n = 1");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "note = \"p, q, [r], {s}, (t), <u>\"");
    assert_eq!(result[1], " n = 1");
}

#[test]
fn split_single_quoted_strings_each_with_commas() {
    // T5 low-level: single-quoted elements, each containing a comma.
    let result = split_at_depth_zero("'a, b', 'c'");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "'a, b'");
    assert_eq!(result[1], " 'c'");
}

#[test]
fn split_unterminated_quote_never_splits_midquote() {
    // Edge case: once a quote opens and is never closed, the remainder of the
    // input stays a single (unsplit) segment — commas after the opening quote
    // must NOT create new fields.
    let result = split_at_depth_zero("a = \"x, y, z");
    assert_eq!(result, vec!["a = \"x, y, z"]);
}

#[test]
fn split_no_quotes_control_still_splits_on_commas() {
    // Control: without quotes the same commas DO split into separate fields.
    let result = split_at_depth_zero("x, y, z");
    assert_eq!(result, vec!["x", " y", " z"]);
}

#[test]
fn split_single_quote_inside_double_quoted_string() {
    // E1 low-level: an apostrophe inside a double-quoted string does not open a
    // new quote context, so the embedded comma is still protected.
    let result = split_at_depth_zero("\"it's, fine\", \"ok\"");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "\"it's, fine\"");
    assert_eq!(result[1], " \"ok\"");
}

#[test]
fn split_double_quotes_inside_single_quoted_string() {
    // E2 low-level: double quotes inside a single-quoted string are literal, so
    // the embedded comma inside the single-quoted field is protected.
    let result = split_at_depth_zero("'say \"hi\", bye', \"z\"");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "'say \"hi\", bye'");
    assert_eq!(result[1], " \"z\"");
}

#[test]
fn split_nested_list_with_quoted_commas() {
    // E3 low-level: only the top-level comma between the two inner lists splits;
    // commas inside the quoted elements of each inner list are protected.
    let result = split_at_depth_zero("[\"a, b\"], [\"c, d\", \"e\"]");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "[\"a, b\"]");
    assert_eq!(result[1], " [\"c, d\", \"e\"]");
}

#[test]
fn split_preserves_whitespace_inside_quotes() {
    // E4 low-level: leading/trailing spaces inside a quoted field are part of
    // the field slice and are not consumed by the splitter.
    let result = split_at_depth_zero("\" a, b \"");
    assert_eq!(result, vec!["\" a, b \""]);
}

// =========================================================================
// find_char_at_depth_zero
// =========================================================================

#[test]
fn find_equals_at_depth_zero() {
    let result = find_char_at_depth_zero("name = str", '=');
    assert_eq!(result, Some(5));
}

#[test]
fn find_skips_inside_angle_brackets() {
    let result = find_char_at_depth_zero("list<a = str>", '=');
    assert_eq!(result, None, "= inside <> should not be found at depth 0");
}

#[test]
fn find_returns_none_when_not_found() {
    let result = find_char_at_depth_zero("no_target_here", '=');
    assert_eq!(result, None);
}

#[test]
fn find_first_occurrence_at_depth_zero() {
    let result = find_char_at_depth_zero("a = b = c", '=');
    assert_eq!(result, Some(2));
}

#[test]
fn find_inside_parens_skipped() {
    let result = find_char_at_depth_zero("fn(x = 1)", '=');
    assert_eq!(result, None);
}

#[test]
fn find_after_brackets() {
    let result = find_char_at_depth_zero("list<a = str> = val", '=');
    assert_eq!(result, Some(14));
}

#[test]
fn find_on_empty_input() {
    assert_eq!(find_char_at_depth_zero("", '='), None);
}

#[test]
fn find_returns_key_separator_before_quoted_equals() {
    // For a struct field entry like `msg = "a = b"`, the key/value separator is
    // the first `=` at depth 0. Because the search returns the first match, the
    // `=` embedded in the quoted value never becomes the split point.
    let result = find_char_at_depth_zero("msg = \"a = b\"", '=');
    assert_eq!(result, Some(4));
}

// =========================================================================
// find_assign_default_at_depth_zero (internal, tested via parse_declarations)
// =========================================================================

#[test]
fn find_assign_default_basic() {
    let result = find_assign_default_at_depth_zero("str := hello");
    assert_eq!(result, Some(4));
}

#[test]
fn find_assign_default_skips_inside_brackets() {
    let result = find_assign_default_at_depth_zero("list<str := x>");
    assert_eq!(result, None);
}

#[test]
fn find_assign_default_not_found() {
    let result = find_assign_default_at_depth_zero("str");
    assert_eq!(result, None);
}

#[test]
fn find_assign_default_colon_without_equals() {
    // A bare `:` without `=` should not match.
    let result = find_assign_default_at_depth_zero("a: b");
    assert_eq!(result, None);
}
