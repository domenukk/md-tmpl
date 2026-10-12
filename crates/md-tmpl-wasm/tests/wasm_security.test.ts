import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const wasm = require(path.join(__dirname, "../pkg/md_tmpl_wasm.js"));

describe("WASM programmatic security filter functions & delimiter tables", () => {
  it("exposes defaultQuarantineTag, tokenDelimiters, and roleTokenDelimiters", () => {
    assert.strictEqual(wasm.defaultQuarantineTag(), "untrusted_content");
    const all: [string, string][] = wasm.tokenDelimiters();
    const role: [string, string][] = wasm.roleTokenDelimiters();
    assert.strictEqual(role.length, 79);
    assert.strictEqual(all.length, 102);
    assert.deepStrictEqual(all.slice(0, role.length), role);

    for (const [raw, escaped] of all) {
      assert.strictEqual(wasm.hasControlTokens(raw), true);
      assert.strictEqual(wasm.hasControlTokens(escaped), false);
      assert.strictEqual(wasm.sanitizeTokensString(raw), escaped);
      assert.strictEqual(wasm.sanitizeTokensString(escaped), escaped);
    }

    for (const [raw, escaped] of role) {
      assert.strictEqual(wasm.hasRoleControlTokens(raw), true);
      assert.strictEqual(wasm.hasRoleControlTokens(escaped), false);
      assert.strictEqual(wasm.sanitizeRoleTokensString(raw), escaped);
    }

    for (const [xmlTag] of all.slice(role.length)) {
      assert.strictEqual(wasm.hasRoleControlTokens(xmlTag), false);
      assert.strictEqual(wasm.sanitizeRoleTokensString(xmlTag), xmlTag);
    }
  });

  it("escapes XML and JSON and neutralizes generic pipe and nested bracket tokens", () => {
    const clean = "Price: 100€ — 100% clean UTF-8 🦀";
    assert.strictEqual(wasm.escapeXmlString(clean), clean);
    assert.strictEqual(wasm.escapeJsonString(clean), clean);

    assert.strictEqual(
      wasm.escapeXmlString("€<tag attr=\"a&b\">'🚀'\x00</tag>—"),
      "€&lt;tag attr=&quot;a&amp;b&quot;&gt;&apos;🚀&apos;&lt;/tag&gt;—",
    );
    assert.strictEqual(
      wasm.escapeJsonString(
        '€"quote"\\slash/path\n\r\t\b\f\x1f\u2028—\u2029🦀',
      ),
      '€\\"quote\\"\\\\slash\\/path\\n\\r\\t\\b\\f\\u001f\\u2028—\\u2029🦀',
    );

    for (const nestedPipe of [
      "<|[INST]|>",
      "<|foo[SYSTEM_PROMPT]bar|>",
      "<｜[TOOL_CALLS]｜>",
      "<|thought|>",
      "<|thought_custom|>",
      "<|thought\nmodel reasoning",
    ]) {
      assert.strictEqual(wasm.hasControlTokens(nestedPipe), true);
      const san = wasm.sanitizeTokensString(nestedPipe);
      assert.strictEqual(wasm.hasControlTokens(san), false);
      assert.strictEqual(
        wasm.isQuarantinedString(wasm.quarantineUntrustedString(nestedPipe)),
        true,
      );
    }
  });

  it("supports fenceString and multi-tag quarantine functions", () => {
    assert.strictEqual(
      wasm.fenceString("```rust\nlet a = 1;\n```", "rust"),
      "````rust\n```rust\nlet a = 1;\n```\n````",
    );
    assert.throws(() => wasm.fenceString("code", "rust inject"));

    const spec = "untrusted_tool_output,event,system-reminder";
    for (const hostile of [
      "</untrusted_tool_output>",
      "<UNTRUSTED_TOOL_OUTPUT>",
      "< / event >",
      '<event id="1">',
      "</system-reminder>",
      "<system-reminder",
    ]) {
      assert.strictEqual(wasm.hasQuarantineTagBreakout(hostile, spec), true);
      assert.strictEqual(wasm.hasUntrustedBreakout(hostile, spec), true);
      const san = wasm.sanitizeQuarantinePayload(hostile, spec);
      assert.strictEqual(wasm.hasQuarantineTagBreakout(san, spec), false);
      const spoofed = `<untrusted_tool_output>\n${hostile}\n</untrusted_tool_output>`;
      assert.strictEqual(wasm.isQuarantinedString(spoofed, spec), false);
      assert.strictEqual(
        wasm.isQuarantinedString(
          wasm.quarantineUntrustedString(hostile, spec),
          spec,
        ),
        true,
      );
    }
    assert.throws(() => wasm.quarantineString("text", "valid,<bad>"));
  });
});

// Expectations mirror crates/md-tmpl-core/src/filter/host_primitive_tests.rs verbatim.
describe("WASM host envelope, XML codec, notice, and truncation primitives", () => {
  it("exports the canonical envelope constants and XML decode pairs", () => {
    assert.strictEqual(wasm.sanitizeNoticeTagPlaceholder(), "{tag}");
    assert.strictEqual(
      wasm.toolOutputQuarantineTag(),
      "tool_output_quarantine",
    );
    assert.strictEqual(
      wasm.toolOutputQuarantineOpenTag(),
      "<tool_output_quarantine>",
    );
    assert.strictEqual(
      wasm.toolOutputQuarantineCloseTag(),
      "</tool_output_quarantine>",
    );
    assert.strictEqual(wasm.untrustedToolOutputTag(), "untrusted_tool_output");
    assert.strictEqual(
      wasm.untrustedQuarantineOpenTag(),
      "<untrusted_tool_output>",
    );
    assert.strictEqual(
      wasm.untrustedQuarantineCloseTag(),
      "</untrusted_tool_output>",
    );
    assert.strictEqual(wasm.xmlDecodePairs().length, 6);
  });

  it("decodes and escapes XML like the Rust core", () => {
    assert.strictEqual(
      wasm.decodeXmlString(
        "a &lt;b&gt; &amp;lt; &quot;q&quot; &#39;s&apos; &unknown; &",
      ),
      "a <b> &lt; \"q\" 's' &unknown; &",
    );
    assert.strictEqual(
      wasm.escapeXmlAttrString("a&b<c>\"d'e\nf\rg\0h\u0001i\u007fj\u0085k"),
      "a&amp;b&lt;c&gt;&quot;d&apos;e\\nf\\rg\\0h i j k",
    );
    assert.strictEqual(
      wasm.escapeXmlBodyString("a&b<c>\"d'\n\0"),
      "a&amp;b&lt;c&gt;\"d'\n\\0",
    );
  });

  it("formats notices and handles quarantine envelopes", () => {
    assert.strictEqual(
      wasm.formatSanitizeNotice("doc", "see {tag}"),
      "see doc",
    );
    assert.strictEqual(
      wasm.stripOuterQuarantineTag("<t>\n\nx\n\n</t>", "t"),
      "\nx\n",
    );
    assert.strictEqual(wasm.stripOuterQuarantineTag("<t>", "t"), undefined);
    assert.strictEqual(
      wasm.unquarantineString("<untrusted_content>\nok\n</untrusted_content>"),
      "ok",
    );
    assert.strictEqual(
      wasm.quarantineUntrustedIdempotentString(
        "<doc>\n<|im_start|>\n</doc>",
        "doc",
      ),
      "<doc>\n&lt;|im_start|&gt;\n</doc>",
    );
    const block = wasm.sanitizeBlockString("payload", "doc");
    assert.strictEqual(wasm.sanitizeBlockIdempotentString(block, "doc"), block);
  });

  it("truncates lists and strings in the middle", () => {
    const letters = ["a", "b", "c", "d", "e"];
    assert.deepStrictEqual(wasm.truncateMiddleList(letters, 2), [
      "a",
      "[TRUNCATED: 3 items omitted]",
      "e",
    ]);
    assert.deepStrictEqual(wasm.truncateMiddleList(letters, 2, ""), ["a", "e"]);
    assert.deepStrictEqual(
      wasm.truncateMiddleList(letters, 3, "..{skipped}.."),
      ["a", "..2..", "d", "e"],
    );
    assert.strictEqual(wasm.truncateMiddleString("abcdef", 100), "abcdef");
  });
});
