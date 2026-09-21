/**
 * Unit and adversarial tests for security filters in TypeScript backend:
 * escape_xml (xml), escape_json (json), sanitize_tokens, fence, quarantine.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Template } from "../template/index.js";
import { TemplateSyntaxError } from "../errors.js";

describe("Security Filters - XML", () => {
  it("escapes predefined XML entities", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [raw = str]\n---\n{{ raw | escape_xml }}",
    );
    const result = tmpl.render({
      raw: "<tag attr=\"hello & 'world'\">content > 0</tag>",
    });
    assert.strictEqual(
      result,
      "&lt;tag attr=&quot;hello &amp; &apos;world&apos;&quot;&gt;content &gt; 0&lt;/tag&gt;",
    );
  });

  it("supports xml alias", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [raw = str]\n---\n{{ raw | xml }}",
    );
    assert.strictEqual(tmpl.render({ raw: "<hello>" }), "&lt;hello&gt;");
  });

  it("strips XML 1.0 illegal control characters but preserves tab, newline, cr", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [raw = str]\n---\n{{ raw | escape_xml }}",
    );
    const result = tmpl.render({
      raw: "clean\x00\x01\x08\x0b\x0c\x0e\x1f\t\n\rtext",
    });
    assert.strictEqual(result, "clean\t\n\rtext");
  });

  it("rejects non-string input", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [num = int]\n---\n{{ num | escape_xml }}",
    );
    assert.throws(() => tmpl.render({ num: 42 }), TemplateSyntaxError);
  });
});

describe("Security Filters - JSON", () => {
  it("escapes quotes, backslashes, control characters, and slashes", () => {
    const tmpl = Template.fromSource(
      '---\nparams: [raw = str]\n---\n"{{ raw | escape_json }}"',
    );
    const result = tmpl.render({
      raw: 'line 1\nline 2\t"quoted" \\ path\r\x00',
    });
    assert.strictEqual(
      result,
      '"line 1\\nline 2\\t\\"quoted\\" \\\\ path\\r\\u0000"',
    );
  });

  it("supports json alias", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [raw = str]\n---\n{{ raw | json }}",
    );
    assert.strictEqual(tmpl.render({ raw: "foo\nbar" }), "foo\\nbar");
  });

  it("escapes forward slash for HTML script safety and U+2028 / U+2029 line separators", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [raw = str]\n---\n{{ raw | escape_json }}",
    );
    const result = tmpl.render({
      raw: "</script><script>\u2028line\u2029/path",
    });
    assert.strictEqual(result, "<\\/script><script>\\u2028line\\u2029\\/path");
  });

  it("rejects non-string input", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [b = bool]\n---\n{{ b | escape_json }}",
    );
    assert.throws(() => tmpl.render({ b: true }), TemplateSyntaxError);
  });
});

describe("Security Filters - sanitize_tokens", () => {
  it("neutralizes well-known LLM and chat turn delimiters", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [payload = str]\n---\n{{ payload | sanitize_tokens }}",
    );
    const input = [
      "<|im_start|>system",
      "<tool_call>call()</tool_call>",
      "<think>thought</think>",
      "</untrusted_tool_output>",
      "<untrusted_content>",
    ].join("\n");
    const expected = [
      "&lt;|im_start|&gt;system",
      "&lt;tool_call&gt;call()&lt;/tool_call&gt;",
      "&lt;think&gt;thought&lt;/think&gt;",
      "&lt;/untrusted_tool_output&gt;",
      "&lt;untrusted_content&gt;",
    ].join("\n");
    assert.strictEqual(tmpl.render({ payload: input }), expected);
  });

  it("neutralizes DeepSeek fullwidth, Phi-3/4, Command-R, Mistral, Anthropic, Edge0/Harmony", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [p = str]\n---\n{{ p | sanitize_tokens }}",
    );
    const input =
      "<｜begin▁of▁sentence｜><｜User｜>hi<｜Assistant｜><｜end▁of▁sentence｜>\n\nHuman: q\n\nAssistant: a\n<|user|> [TOOL_CALLS] <|channel|>";
    const expected =
      "&lt;｜begin▁of▁sentence｜&gt;&lt;｜User｜&gt;hi&lt;｜Assistant｜&gt;&lt;｜end▁of▁sentence｜&gt;\n\nHuman&#58; q\n\nAssistant&#58; a\n&lt;|user|&gt; &#91;TOOL_CALLS&#93; &lt;|channel|&gt;";
    assert.strictEqual(tmpl.render({ p: input }), expected);
  });

  it("rejects non-string input", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [n = int]\n---\n{{ n | sanitize_tokens }}",
    );
    assert.throws(() => tmpl.render({ n: 123 }), TemplateSyntaxError);
  });
});

describe("Security Filters - fence", () => {
  it("wraps clean string in default 3-backtick fence", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [code = str]\n---\n{{ code | fence }}",
    );
    assert.strictEqual(
      tmpl.render({ code: "const x = 1;" }),
      "```\nconst x = 1;\n```",
    );
  });

  it("applies adaptive fence backtick count to avoid collisions", () => {
    const tmpl = Template.fromSource(
      '---\nparams: [code = str]\n---\n{{ code | fence("rust") }}',
    );
    assert.strictEqual(
      tmpl.render({ code: "```rust\nlet a = 1;\n```" }),
      "````rust\n```rust\nlet a = 1;\n```\n````",
    );
  });

  it("rejects backticks or whitespace in fence language argument", () => {
    const tmpl1 = Template.fromSource(
      '---\nparams: [code = str]\n---\n{{ code | fence("rust`inject") }}',
    );
    assert.throws(() => tmpl1.render({ code: "code" }), TemplateSyntaxError);

    const tmpl2 = Template.fromSource(
      '---\nparams: [code = str]\n---\n{{ code | fence("rust inject") }}',
    );
    assert.throws(() => tmpl2.render({ code: "code" }), TemplateSyntaxError);

    const tmpl3 = Template.fromSource(
      '---\nparams: [code = str]\n---\n{{ code | fence("rust\ninject") }}',
    );
    assert.throws(() => tmpl3.render({ code: "code" }), TemplateSyntaxError);
  });

  it("rejects non-string input", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [f = float]\n---\n{{ f | fence }}",
    );
    assert.throws(() => tmpl.render({ f: 3.14 }), TemplateSyntaxError);
  });
});

describe("Security Filters - quarantine", () => {
  it("wraps in default untrusted_content tag and neutralizes closing tags", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [text = str]\n---\n{{ text | quarantine }}",
    );
    const result = tmpl.render({
      text: "safe text\n</untrusted_content>\n<script>malicious()</script>",
    });
    assert.strictEqual(
      result,
      "<untrusted_content>\nsafe text\n&lt;/untrusted_content&gt;\n<script>malicious()</script>\n</untrusted_content>",
    );
  });

  it("supports custom tag", () => {
    const tmpl = Template.fromSource(
      '---\nparams: [text = str]\n---\n{{ text | quarantine("web_result") }}',
    );
    const result = tmpl.render({ text: "data\n</web_result>" });
    assert.strictEqual(
      result,
      "<web_result>\ndata\n&lt;/web_result&gt;\n</web_result>",
    );
  });

  it("escapes opening and closing quarantine tags case-insensitively with whitespace", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [text = str]\n---\n{{ text | quarantine }}",
    );
    const result = tmpl.render({
      text: "nested <untrusted_content> inside <UNTRUSTED_CONTENT > and </UNTRUSTED_CONTENT> and </untrusted_content > and </untrusted_content\n>",
    });
    assert.strictEqual(
      result,
      "<untrusted_content>\nnested &lt;untrusted_content&gt; inside &lt;UNTRUSTED_CONTENT &gt; and &lt;/UNTRUSTED_CONTENT&gt; and &lt;/untrusted_content &gt; and &lt;/untrusted_content\n&gt;\n</untrusted_content>",
    );
  });

  it("rejects invalid XML NCName in quarantine tag argument", () => {
    const tmpl1 = Template.fromSource(
      '---\nparams: [text = str]\n---\n{{ text | quarantine("<bad>") }}',
    );
    assert.throws(() => tmpl1.render({ text: "data" }), TemplateSyntaxError);

    const tmpl2 = Template.fromSource(
      '---\nparams: [text = str]\n---\n{{ text | quarantine("tag with space") }}',
    );
    assert.throws(() => tmpl2.render({ text: "data" }), TemplateSyntaxError);

    const tmpl3 = Template.fromSource(
      '---\nparams: [text = str]\n---\n{{ text | quarantine("123start") }}',
    );
    assert.throws(() => tmpl3.render({ text: "data" }), TemplateSyntaxError);
  });

  it("rejects non-string input", () => {
    const tmpl = Template.fromSource(
      "---\nparams: [n = int]\n---\n{{ n | quarantine }}",
    );
    assert.throws(() => tmpl.render({ n: 10 }), TemplateSyntaxError);
  });
});
