/**
 * Tests for the md-tmpl TypeScript bindings: fromSourceWithBaseDir and file-backed TemplateCache.
 *
 * Uses Node.js built-in test runner (`node:test`).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

import { Template, TemplateCache, IncludeNotFoundError } from "../index.js";

// ---------------------------------------------------------------------------
// fromSourceWithBaseDir
// ---------------------------------------------------------------------------

describe("Template.fromSourceWithBaseDir", () => {
  it("parses and renders a basic template", () => {
    const tmpl = Template.fromSourceWithBaseDir(
      `---
params: [x = str]
---
{{ x }}`,
      "/tmp",
    );
    assert.strictEqual(tmpl.render({ x: "works" }), "works");
  });

  it("sets basePath for include resolution", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-basedir-"));
    try {
      const tmpl = Template.fromSourceWithBaseDir(
        `---
params: [x = str]
---
{{ x }}`,
        dir,
      );
      assert.strictEqual(tmpl.basePath, dir);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("resolves and renders file-based includes", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-include-"));
    try {
      // Create the included template file
      fs.writeFileSync(
        path.join(dir, "header.tmpl.md"),
        `---
params: [title = str]
---
# {{ title }}`,
      );
      const src = [
        `---`,
        "params: [title = str]",
        `---`,
        "> {% include [header](./header.tmpl.md) with title=title %}",
        "",
        "Body: {{ title }}",
      ].join("\n");
      const tmpl = Template.fromSourceWithBaseDir(src, dir);
      const result = tmpl.render({ title: "Hello" });
      assert.ok(
        result.includes("# Hello"),
        `expected included header in: ${result}`,
      );
      assert.ok(result.includes("Body: Hello"), `expected body in: ${result}`);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("throws on missing include file", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-include-"));
    try {
      const src = [
        `---`,
        "params: [title = str]",
        `---`,
        "> {% include [missing](./does_not_exist.tmpl.md) with title=title %}",
        "",
        "Body: {{ title }}",
      ].join("\n");
      const tmpl = Template.fromSourceWithBaseDir(src, dir);
      assert.throws(
        () => tmpl.render({ title: "Hello" }),
        (err: Error) =>
          err instanceof IncludeNotFoundError &&
          err.include.includes("does_not_exist.tmpl.md") &&
          err.message.includes("include not found"),
      );
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});

// ---------------------------------------------------------------------------
// TemplateCache — file-based tests
// ---------------------------------------------------------------------------

describe("TemplateCache (file operations)", () => {
  it("loads and renders a template from file", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-cache-"));
    try {
      const fp = path.join(dir, "greet.tmpl.md");
      fs.writeFileSync(
        fp,
        `---
params: [name = str]
---
Hello {{ name }}!`,
      );
      const cache = new TemplateCache();
      const tmpl = cache.load(fp);
      assert.strictEqual(tmpl.render({ name: "cached" }), "Hello cached!");
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("returns cached template on repeated loads", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-cache2-"));
    try {
      const fp = path.join(dir, "test.tmpl.md");
      fs.writeFileSync(
        fp,
        `---
params: [x = str]
---
{{ x }}`,
      );
      const cache = new TemplateCache();
      const t1 = cache.load(fp);
      const t2 = cache.load(fp);
      assert.strictEqual(t1.sourceHash(), t2.sourceHash());
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("templateCount tracks loaded templates", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-count-"));
    try {
      const cache = new TemplateCache();
      assert.strictEqual(cache.templateCount(), 0);

      for (let i = 0; i < 3; i++) {
        const fp = path.join(dir, `t${String(i)}.tmpl.md`);
        fs.writeFileSync(
          fp,
          `---
params: []
---
Hi`,
        );
        cache.load(fp);
      }
      assert.strictEqual(cache.templateCount(), 3);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("clear removes all cached templates", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-clear-"));
    try {
      const fp = path.join(dir, "test.tmpl.md");
      fs.writeFileSync(
        fp,
        `---
params: []
---
Hi`,
      );
      const cache = new TemplateCache();
      cache.load(fp);
      assert.strictEqual(cache.templateCount(), 1);
      cache.clear();
      assert.strictEqual(cache.templateCount(), 0);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("throws on non-existent file", () => {
    const cache = new TemplateCache();
    assert.throws(() => cache.load("/nonexistent/path.tmpl.md"));
  });

  it("reloads when file content changes", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-reload-"));
    try {
      const fp = path.join(dir, "test.tmpl.md");
      fs.writeFileSync(
        fp,
        `---
params: []
---
Version 1`,
      );
      const cache = new TemplateCache();
      const t1 = cache.load(fp);
      const hash1 = t1.sourceHash();

      fs.writeFileSync(
        fp,
        `---
params: []
---
Version 2`,
      );
      const t2 = cache.load(fp);
      const hash2 = t2.sourceHash();

      assert.notStrictEqual(hash1, hash2);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("deduplicates include ASTs across multiple references", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-dedup-"));
    try {
      const headerPath = path.join(dir, "header.tmpl.md");
      fs.writeFileSync(
        headerPath,
        `---
params: [title = str]
---
# {{ title }}`,
      );
      const mainPath = path.join(dir, "main.tmpl.md");
      fs.writeFileSync(
        mainPath,
        `---
params: [title = str]
---
> {% include [header](./header.tmpl.md) with title=title %}

Body content

> {% include [header](./header.tmpl.md) with title=title %}`,
      );
      const cache = new TemplateCache();
      const tmpl = cache.load(mainPath);
      assert.strictEqual(cache.templateCount(), 1);
      assert.strictEqual(cache.includeCount(), 0);

      const result = tmpl.render({ title: "Welcome" });
      assert.ok(result.includes("# Welcome"));
      assert.strictEqual(cache.templateCount(), 1);
      assert.strictEqual(cache.includeCount(), 1);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("compiles include exactly once inside a loop", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-loop-"));
    try {
      const itemPath = path.join(dir, "item.tmpl.md");
      fs.writeFileSync(
        itemPath,
        `---
params: [name = str]
---
Item: {{ name }}
`,
      );
      const listPath = path.join(dir, "list.tmpl.md");
      fs.writeFileSync(
        listPath,
        `---
params: [items = list(name = str)]
---
> {% for item in items %}
> {% include [item](./item.tmpl.md) with name=item.name %}
> {% /for %}`,
      );
      const cache = new TemplateCache();
      const tmpl = cache.load(listPath);

      const items = Array.from({ length: 50 }, (_, i) => ({
        name: `Item ${String(i)}`,
      }));
      const result = tmpl.render({ items });
      assert.ok(result.includes("Item: Item 0"));
      assert.ok(result.includes("Item: Item 49"));
      assert.strictEqual(cache.includeCount(), 1);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("recompiles include when mtime changes on disk", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-mtime-"));
    try {
      const subPath = path.join(dir, "sub.tmpl.md");
      fs.writeFileSync(
        subPath,
        `---
params: []
---
Original Include`,
      );
      const mainPath = path.join(dir, "main.tmpl.md");
      fs.writeFileSync(
        mainPath,
        `---
params: []
---
> {% include [sub](./sub.tmpl.md) %}`,
      );
      const cache = new TemplateCache();
      const tmpl = cache.load(mainPath);

      const res1 = tmpl.render();
      assert.ok(res1.includes("Original Include"));
      assert.strictEqual(cache.includeCount(), 1);

      const stat1 = fs.statSync(subPath);
      const newTime = new Date(stat1.mtimeMs + 2000);
      fs.writeFileSync(
        subPath,
        `---
params: []
---
Modified Include`,
      );
      fs.utimesSync(subPath, newTime, newTime);

      const res2 = tmpl.render();
      assert.ok(res2.includes("Modified Include"));
      assert.strictEqual(cache.includeCount(), 1);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("resolves include env: vars through the shared cache", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-env-"));
    try {
      // Child declares an env var with NO default — it can only resolve if
      // the parent's compile env is threaded through the cached resolver.
      fs.writeFileSync(
        path.join(dir, "child.tmpl.md"),
        `---
env: [PROMPTS_DIR = str]
params: []
---
dir={{ PROMPTS_DIR }}`,
      );
      const mainSrc = `---
params: []
---
> {% include [child](./child.tmpl.md) %}`;
      fs.writeFileSync(path.join(dir, "main.tmpl.md"), mainSrc);

      const cache = new TemplateCache();
      const tmpl = Template.fromSourceWithOptions(mainSrc, {
        baseDir: dir,
        env: { PROMPTS_DIR: "/prompts" },
      });
      tmpl._cache = cache;

      // First render — compiles the include from disk with env threaded in.
      const out1 = tmpl.render();
      assert.ok(out1.includes("dir=/prompts"), out1);
      assert.strictEqual(cache.includeCount(), 1);

      // Second render — served from cache; env-injected const must persist.
      const out2 = tmpl.render();
      assert.strictEqual(out1, out2);
      assert.strictEqual(cache.includeCount(), 1);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("invalidates cached include when compile env changes", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-env-inval-"));
    try {
      fs.writeFileSync(
        path.join(dir, "child.tmpl.md"),
        `---
env: [PROMPTS_DIR = str]
params: []
---
dir={{ PROMPTS_DIR }}`,
      );
      const mainSrc = `---
params: []
---
> {% include [child](./child.tmpl.md) %}`;
      fs.writeFileSync(path.join(dir, "main.tmpl.md"), mainSrc);

      const cache = new TemplateCache();
      const compileWith = (value: string): Template => {
        const t = Template.fromSourceWithOptions(mainSrc, {
          baseDir: dir,
          env: { PROMPTS_DIR: value },
        });
        t._cache = cache;
        return t;
      };

      // Render with env A → caches "dir=/alpha".
      const outA = compileWith("/alpha").render();
      assert.ok(outA.includes("dir=/alpha"), outA);

      // Render with env B against the SAME cache. The file is untouched, so
      // without env-aware invalidation the mtime fast path would return the
      // stale "/alpha" entry. It must instead recompute to "/beta".
      const outB = compileWith("/beta").render();
      assert.ok(outB.includes("dir=/beta"), outB);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("clear() resets both templateCount and includeCount", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-cleardup-"));
    try {
      const subPath = path.join(dir, "sub.tmpl.md");
      fs.writeFileSync(
        subPath,
        `---
params: []
---
Sub content`,
      );
      const mainPath = path.join(dir, "main.tmpl.md");
      fs.writeFileSync(
        mainPath,
        `---
params: []
---
> {% include [sub](./sub.tmpl.md) %}`,
      );
      const cache = new TemplateCache();
      const tmpl = cache.load(mainPath);
      tmpl.render();

      assert.strictEqual(cache.templateCount(), 1);
      assert.strictEqual(cache.includeCount(), 1);

      cache.clear();
      assert.strictEqual(cache.templateCount(), 0);
      assert.strictEqual(cache.includeCount(), 0);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });

  it("resolves nested relative includes across directories in cache", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pt-nested-"));
    try {
      const dir1 = path.join(dir, "dir1");
      const dir2 = path.join(dir, "dir2");
      fs.mkdirSync(dir1);
      fs.mkdirSync(dir2);

      fs.writeFileSync(
        path.join(dir2, "nested.tmpl.md"),
        `---
params: []
---
Nested content`,
      );
      fs.writeFileSync(
        path.join(dir2, "sub.tmpl.md"),
        `---
params: []
---
Sub before {% include [nested](./nested.tmpl.md) %} Sub after`,
      );
      const mainPath = path.join(dir1, "main.tmpl.md");
      fs.writeFileSync(
        mainPath,
        `---
params: []
---
Main before {% include [sub](../dir2/sub.tmpl.md) %} Main after`,
      );

      const cache = new TemplateCache();
      const tmpl = cache.load(mainPath);
      const result = tmpl.render();
      assert.ok(result.includes("Main before"));
      assert.ok(result.includes("Sub before"));
      assert.ok(result.includes("Nested content"));
      assert.ok(result.includes("Sub after"));
      assert.ok(result.includes("Main after"));
      assert.strictEqual(cache.templateCount(), 1);
      assert.strictEqual(cache.includeCount(), 2);
    } finally {
      fs.rmSync(dir, { recursive: true });
    }
  });
});
