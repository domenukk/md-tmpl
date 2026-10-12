/**
 * WASM binding tests: Template construction, rendering modes, and metadata accessors.
 *
 * Uses Node.js built-in test runner (`node:test`) and `node:assert/strict`.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Template as WasmTemplate } from "../pkg/md_tmpl_wasm.js";

// =========================================================================
// 1. Template.fromSource
// =========================================================================

describe("Template.fromSource", () => {
  it("basic string param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    const result = t.render({ name: "world" });
    assert.equal(result, "Hello world!");
  });

  it("int param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
Value is {{ n }}`,
    );
    assert.equal(t.render({ n: 42 }), "Value is 42");
  });

  it("float param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - pi = float
---
Pi is {{ pi }}`,
    );
    assert.equal(t.render({ pi: 3.14159 }), "Pi is 3.14159");
  });

  it("bool param", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - flag = bool
---
Flag is {{ flag }}`,
    );
    assert.equal(t.render({ flag: true }), "Flag is true");
  });

  it("throws on syntax error", () => {
    assert.throws(() => {
      WasmTemplate.fromSource(`---
params:
  - = invalid
---
Hello`);
    });
  });

  it("throws on missing frontmatter delimiter", () => {
    assert.throws(() => {
      WasmTemplate.fromSource("no frontmatter here");
    });
  });
});

// =========================================================================
// 2. Template.fromSourceAllowingUnused
// =========================================================================

describe("Template.fromSourceAllowingUnused", () => {
  it("accepts unused declared params", () => {
    const source = `---
params:
  - used = str
  - unused = str
---
Hello {{ used }}!`;
    // Should NOT throw
    const t = WasmTemplate.fromSourceAllowingUnused(source);
    assert.equal(t.render({ used: "world", unused: "ignored" }), "Hello world!");
  });

  it("strict mode (fromSource) rejects unused params", () => {
    const source = `---
params:
  - used = str
  - unused = str
---
Hello {{ used }}!`;
    assert.throws(() => {
      WasmTemplate.fromSource(source);
    });
  });
});

// =========================================================================
// 3. Template.render (strict mode)
// =========================================================================

describe("Template.render (strict mode)", () => {
  it("renders correctly with all params", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
---
{{ name }}: {{ count }}`,
    );
    assert.equal(t.render({ name: "Alice", count: 10 }), "Alice: 10");
  });

  it("throws on missing param (error mentions param name)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    try {
      t.render({});
      assert.fail("Expected render to throw");
    } catch (err) {
      assert.ok(
        String(err).toLowerCase().includes("name"),
        `Error should mention "name", got: ${err}`,
      );
    }
  });

  it("throws on type mismatch (string where int expected)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - n = int
---
{{ n }}`,
    );
    assert.throws(() => {
      t.render({ n: "not_a_number" });
    });
  });

  it("throws on extra undeclared params", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    assert.throws(() => {
      t.render({ name: "world", extra: "oops" });
    });
  });

  it("renders with multiple params (str, int, float, bool)", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
  - score = float
  - enabled = bool
---
{{ name }}: count={{ count }}, score={{ score }}, enabled={{ enabled }}`,
    );
    assert.equal(
      t.render({ name: "Alice", count: 42, score: 9.5, enabled: true }),
      "Alice: count=42, score=9.5, enabled=true",
    );
  });
});

// =========================================================================
// 4. Template.renderUnchecked
// =========================================================================

describe("Template.renderUnchecked", () => {
  it("allows extra params", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    // Should NOT throw with extra params
    const result = t.renderUnchecked({ name: "world", extra: "ok" });
    assert.equal(result, "Hello world!");
  });

  it("allows extra undeclared params without throwing", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
Hello {{ name }}!`,
    );
    // renderUnchecked allows extra params that aren't declared
    const result = t.renderUnchecked({ name: "world", bonus: "extra" });
    assert.equal(result, "Hello world!");
  });

  it("still renders correctly", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - a = str
  - b = int
---
{{ a }}-{{ b }}`,
    );
    assert.equal(t.renderUnchecked({ a: "foo", b: 5 }), "foo-5");
  });
});

// =========================================================================
// 5. Template.body()
// =========================================================================

describe("Template.body()", () => {
  it("returns body text without frontmatter", () => {
    const source = `---
params:
  - name = str
---
Hello {{ name }}!`;
    const t = WasmTemplate.fromSource(source);
    const body = t.body();
    assert.ok(!body.includes(`---`), "Body should not contain frontmatter delimiters");
    assert.ok(!body.includes("params:"), "Body should not contain frontmatter content");
    assert.ok(
      body.includes("Hello {{ name }}!"),
      "Body should contain the template body",
    );
  });
});

// =========================================================================
// 6. Template.consts()
// =========================================================================

describe("Template.consts()", () => {
  it("returns constants as plain object", () => {
    const t = WasmTemplate.fromSource(
      `---
consts:
  - MAX = int := 100

params: []
---
Max is {{ MAX }}`,
    );
    const consts = t.consts();
    assert.deepEqual(consts, { MAX: 100 });
  });

  it("returns empty object when no consts", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Static text`,
    );
    const consts = t.consts();
    assert.deepEqual(consts, {});
  });
});

// =========================================================================
// 7. Template.declarations()
// =========================================================================

describe("Template.declarations()", () => {
  it("returns array of [name, typeString] tuples", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
---
{{ name }} {{ count }}`,
    );
    const decls = t.declarations();
    assert.ok(Array.isArray(decls), "declarations() should return an array");
    assert.equal(decls.length, 2);
    // Each element is [name, typeString]
    assert.equal(decls[0][0], "name");
    assert.equal(decls[0][1], "str");
    assert.equal(decls[1][0], "count");
    assert.equal(decls[1][1], "int");
  });

  it("handles complex types (list, struct, enum)", () => {
    const t = WasmTemplate.fromSourceAllowingUnused(
      `---
params:
  - items = list(name = str)
  - meta = struct(author = str, version = int)
  - status = enum(Active, Paused)
---
> {% for item in items %}{{ item.name }}{% /for %}{{ meta.author }}{{ status }}`,
    );
    const decls = t.declarations();
    assert.equal(decls.length, 3);
    // Check the type strings exist
    assert.ok(decls[0][1].includes("list"), `Expected list type, got: ${decls[0][1]}`);
    assert.ok(decls[1][1].includes("struct"), `Expected struct type, got: ${decls[1][1]}`);
    assert.ok(decls[2][1].includes("enum"), `Expected enum type, got: ${decls[2][1]}`);
  });
});

// =========================================================================
// 8. Template.defaults()
// =========================================================================

describe("Template.defaults()", () => {
  it("returns defaults object", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - greeting = str := "Hi"
---
{{ greeting }}!`,
    );
    const defaults = t.defaults();
    assert.equal(defaults.greeting, "Hi");
  });

  it("missing defaults returns empty object", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
---
{{ name }}`,
    );
    const defaults = t.defaults();
    assert.deepEqual(defaults, {});
  });

  it("renders using defaults when params omitted", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - greeting = str := "Hi"
  - name = str
---
{{ greeting }}, {{ name }}!`,
    );
    assert.equal(t.render({ name: "World" }), "Hi, World!");
  });
});

// =========================================================================
// 9. Template.sourceHash()
// =========================================================================

describe("Template.sourceHash()", () => {
  const source1 = `---
params:
  - x = str
---
{{ x }}`;
  const source2 = `---
params:
  - y = str
---
{{ y }}`;

  it("same source → same hash", () => {
    const t1 = WasmTemplate.fromSource(source1);
    const t2 = WasmTemplate.fromSource(source1);
    assert.equal(t1.sourceHash(), t2.sourceHash());
  });

  it("different source → different hash", () => {
    const t1 = WasmTemplate.fromSource(source1);
    const t2 = WasmTemplate.fromSource(source2);
    assert.notEqual(t1.sourceHash(), t2.sourceHash());
  });

  it("returns a number (u32)", () => {
    const t = WasmTemplate.fromSource(source1);
    const hash = t.sourceHash();
    assert.equal(typeof hash, "number");
    assert.ok(Number.isInteger(hash), "Hash should be an integer");
    assert.ok(hash >= 0, "Hash should be non-negative");
    assert.ok(hash <= 0xffffffff, "Hash should fit in u32");
  });
});

// =========================================================================
// 10. Template.importedConsts()
// =========================================================================

describe("Template.importedConsts()", () => {
  it("returns empty object when no imports", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Static content`,
    );
    const imported = t.importedConsts();
    assert.deepEqual(imported, {});
  });
});

// =========================================================================
// 23. Metadata caching (consts, declarations, defaults, importedConsts)
// =========================================================================

describe("Metadata caching", () => {
  it("consts() returns same reference on repeated calls", () => {
    const t = WasmTemplate.fromSource(
      `---
consts:
  - MAX = int := 100

params: []
---
Max is {{ MAX }}`,
    );
    const a = t.consts();
    const b = t.consts();
    assert.deepEqual(a, b);
    assert.deepEqual(a, { MAX: 100 });
  });

  it("declarations() returns same reference on repeated calls", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - name = str
  - count = int
---
{{ name }} {{ count }}`,
    );
    const a = t.declarations();
    const b = t.declarations();
    assert.deepEqual(a, b);
    assert.equal(a.length, 2);
  });

  it("defaults() returns same reference on repeated calls", () => {
    const t = WasmTemplate.fromSource(
      `---
params:
  - greeting = str := "Hi"
---
{{ greeting }}!`,
    );
    const a = t.defaults();
    const b = t.defaults();
    assert.deepEqual(a, b);
    assert.equal(a.greeting, "Hi");
  });

  it("importedConsts() returns same reference on repeated calls", () => {
    const t = WasmTemplate.fromSource(
      `---
params: []
---
Static content`,
    );
    const a = t.importedConsts();
    const b = t.importedConsts();
    assert.deepEqual(a, b);
    assert.deepEqual(a, {});
  });
});
