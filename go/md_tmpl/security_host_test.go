package md_tmpl

import (
	"reflect"
	"strings"
	"testing"
)

// Expectations mirror crates/md-tmpl-core/src/filter/host_primitive_tests.rs verbatim.
const (
	hostDecodeInput    = "a &lt;b&gt; &amp;lt; &quot;q&quot; &#39;s&apos; &unknown; &"
	hostDecodeExpected = "a <b> &lt; \"q\" 's' &unknown; &"
	hostAttrInput      = "a&b<c>\"d'e\nf\rg\x00h\x01i\x7fj\u0085k"
	hostAttrExpected   = "a&amp;b&lt;c&gt;&quot;d&apos;e\\nf\\rg\\0h i j k"
	hostBodyInput      = "a&b<c>\"d'\n\x00"
	hostBodyExpected   = "a&amp;b&lt;c&gt;\"d'\n\\0"
)

func TestHostConstantsMatchCore(t *testing.T) {
	core := SecurityConstants()
	want := map[string]string{
		"DEFAULT_QUARANTINE_TAG":           DefaultQuarantineTag,
		"DEFAULT_SANITIZE_TAG":             DefaultSanitizeTag,
		"DEFAULT_SANITIZE_NOTICE":          DefaultSanitizeNotice,
		"SANITIZE_NOTICE_TAG_PLACEHOLDER":  SanitizeNoticeTagPlaceholder,
		"TOOL_OUTPUT_QUARANTINE_TAG":       ToolOutputQuarantineTag,
		"TOOL_OUTPUT_QUARANTINE_OPEN_TAG":  ToolOutputQuarantineOpenTag,
		"TOOL_OUTPUT_QUARANTINE_CLOSE_TAG": ToolOutputQuarantineCloseTag,
		"UNTRUSTED_TOOL_OUTPUT_TAG":        UntrustedToolOutputTag,
		"UNTRUSTED_QUARANTINE_OPEN_TAG":    UntrustedQuarantineOpenTag,
		"UNTRUSTED_QUARANTINE_CLOSE_TAG":   UntrustedQuarantineCloseTag,
		"DEFAULT_LIST_TRUNCATE_MARKER":     DefaultListTruncateMarker,
		"DEFAULT_TRUNCATE_MARKER":          DefaultTruncateMarker,
		"DEFAULT_TRUNCATE_MARKER_COMPACT":  DefaultTruncateMarkerCompact,
	}
	for name, goValue := range want {
		if core[name] != goValue {
			t.Errorf("%s: Go constant %q != core %q", name, goValue, core[name])
		}
	}
	if len(XMLDecodePairs()) != 6 {
		t.Errorf("XMLDecodePairs: got %v", XMLDecodePairs())
	}
}

func TestHostXMLCodec(t *testing.T) {
	if got := DecodeXML(hostDecodeInput); got != hostDecodeExpected {
		t.Errorf("DecodeXML = %q", got)
	}
	if got := EscapeXMLAttr(hostAttrInput); got != hostAttrExpected {
		t.Errorf("EscapeXMLAttr = %q", got)
	}
	if got := EscapeXMLBody(hostBodyInput); got != hostBodyExpected {
		t.Errorf("EscapeXMLBody = %q", got)
	}
}

func TestHostNoticeAndEnvelopes(t *testing.T) {
	notice, err := FormatSanitizeNotice("doc")
	if err != nil || notice != strings.ReplaceAll(DefaultSanitizeNotice, SanitizeNoticeTagPlaceholder, "doc") {
		t.Errorf("FormatSanitizeNotice = %q, %v", notice, err)
	}
	custom, err := FormatSanitizeNoticeWithTemplate("doc", "see {tag}")
	if err != nil || custom != "see doc" {
		t.Errorf("FormatSanitizeNoticeWithTemplate = %q, %v", custom, err)
	}
	if got, ok := StripOuterQuarantineTag("<t>\n\nx\n\n</t>", "t"); !ok || got != "\nx\n" {
		t.Errorf("StripOuterQuarantineTag = %q, %v", got, ok)
	}
	if _, ok := StripOuterQuarantineTag("<t>", "t"); ok {
		t.Error("StripOuterQuarantineTag accepted an unwrapped string")
	}
	if got, ok := Unquarantine("<untrusted_content>\nok\n</untrusted_content>", ""); !ok || got != "ok" {
		t.Errorf("Unquarantine = %q, %v", got, ok)
	}
	if _, ok := Unquarantine("<doc>\n</doc>\n</doc>", "doc"); ok {
		t.Error("Unquarantine accepted a dirty envelope")
	}
	rewrapped, err := QuarantineUntrustedIdempotent("<doc>\n<|im_start|>\n</doc>", "doc")
	if err != nil || rewrapped != "<doc>\n&lt;|im_start|&gt;\n</doc>" {
		t.Errorf("QuarantineUntrustedIdempotent = %q, %v", rewrapped, err)
	}
	block, err := SanitizeBlock("payload", "doc")
	if err != nil {
		t.Fatal(err)
	}
	if again, err := SanitizeBlockIdempotent(block, "doc"); err != nil || again != block {
		t.Errorf("SanitizeBlockIdempotent = %q, %v", again, err)
	}
	if _, err := QuarantineUntrustedIdempotent("x", "#bad"); err == nil {
		t.Error("QuarantineUntrustedIdempotent accepted an invalid tag")
	}
}

func TestHostTruncation(t *testing.T) {
	letters := []any{"a", "b", "c", "d", "e"}
	cases := []struct {
		got  func() ([]any, error)
		want []any
	}{
		{func() ([]any, error) { return TruncateMiddleList(letters, 2) }, []any{"a", "[TRUNCATED: 3 items omitted]", "e"}},
		{func() ([]any, error) { return TruncateMiddleListWithMarker(letters, 2, "") }, []any{"a", "e"}},
		{func() ([]any, error) { return TruncateMiddleListWithMarker(letters, 3, "..{skipped}..") }, []any{"a", "..2..", "d", "e"}},
	}
	for i, c := range cases {
		got, err := c.got()
		if err != nil || !reflect.DeepEqual(got, c.want) {
			t.Errorf("case %d: got %v, %v; want %v", i, got, err, c.want)
		}
	}
	if got, err := TruncateMiddle("abcdef", 100); err != nil || got != "abcdef" {
		t.Errorf("TruncateMiddle = %q, %v", got, err)
	}
}
