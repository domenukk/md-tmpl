/**
 * Tests for the `md-tmpl/browser` entry point: lazy, on-demand template
 * loading over a simulated network (a fake {@link FetchLike}).
 *
 * These verify the core promise of the browser loader — nothing is preloaded;
 * only the files a real render reaches are fetched, then cached — across static
 * includes, dynamic (param-dependent) includes, imports (including chained,
 * const-dependent import paths), and missing-file errors.
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { Template, TemplateError } from "../index.js";
import {
  type FetchLike,
  MemoryFs,
  clearCache,
  loadTemplate,
  posixPath,
} from "../browser/index.js";

const ORIGIN = "http://localhost";

/**
 * Build a fake `fetch` backed by an in-memory URL→source map, tracking which
 * URLs were requested so tests can assert nothing extraneous is fetched.
 */
function fakeFetch(files: Record<string, string>): {
  fetch: FetchLike;
  requested: string[];
} {
  const requested: string[] = [];
  const fetch: FetchLike = (url) => {
    requested.push(url);
    const body = files[url];
    if (body === undefined) {
      return Promise.resolve({
        ok: false,
        status: 404,
        text: () => Promise.resolve(""),
      });
    }
    return Promise.resolve({
      ok: true,
      status: 200,
      text: () => Promise.resolve(body),
    });
  };
  return { fetch, requested };
}

describe("md-tmpl/browser: loadTemplate", () => {
  it("loads and renders a template with no dependencies", async () => {
    const { fetch, requested } = fakeFetch({
      [`${ORIGIN}/t1/main.tmpl.md`]: `---
params: [name = str]
---
Hello {{ name }}!`,
    });

    const tmpl = await loadTemplate("/t1/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });

    assert.strictEqual(
      await tmpl.renderAsync({ name: "World" }),
      "Hello World!",
    );
    // Only the entry file was ever fetched — nothing preloaded.
    assert.deepStrictEqual(requested, [`${ORIGIN}/t1/main.tmpl.md`]);
    // Sync render works too (no file includes to resolve).
    assert.strictEqual(tmpl.render({ name: "there" }), "Hello there!");
  });

  it("fetches a static include lazily, then serves it from cache", async () => {
    const { fetch, requested } = fakeFetch({
      [`${ORIGIN}/t2/main.tmpl.md`]: `---
params: [x = str]
---
> {% include [a](./a.tmpl.md) with x=x %}`,
      [`${ORIGIN}/t2/a.tmpl.md`]: `---
params: [x = str]
---
A:{{ x }}`,
    });

    const tmpl = await loadTemplate("/t2/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    // Only the entry is fetched at load; the include is resolved at render.
    assert.deepStrictEqual(requested, [`${ORIGIN}/t2/main.tmpl.md`]);

    assert.strictEqual(await tmpl.renderAsync({ x: "1" }), "A:1");
    assert.deepStrictEqual(requested, [
      `${ORIGIN}/t2/main.tmpl.md`,
      `${ORIGIN}/t2/a.tmpl.md`,
    ]);

    // The include is now cached: a second, synchronous render needs no fetch.
    assert.strictEqual(tmpl.render({ x: "2" }), "A:2");
    assert.strictEqual(requested.length, 2);
  });

  it("resolves dynamic (param-dependent) includes without preloading", async () => {
    const { fetch, requested } = fakeFetch({
      [`${ORIGIN}/t3/main.tmpl.md`]: `---
params: [section = str]
---
> {% include [s](./sections/{{ section }}.tmpl.md) %}`,
      [`${ORIGIN}/t3/sections/intro.tmpl.md`]: `---
params: []
---
INTRO`,
      [`${ORIGIN}/t3/sections/outro.tmpl.md`]: `---
params: []
---
OUTRO`,
    });

    const tmpl = await loadTemplate("/t3/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });

    assert.strictEqual(await tmpl.renderAsync({ section: "intro" }), "INTRO");
    // Rendering the "intro" shape must NOT have fetched the "outro" section.
    assert.ok(!requested.includes(`${ORIGIN}/t3/sections/outro.tmpl.md`));

    assert.strictEqual(await tmpl.renderAsync({ section: "outro" }), "OUTRO");
    assert.ok(requested.includes(`${ORIGIN}/t3/sections/outro.tmpl.md`));
  });

  it("fetches imports during construction", async () => {
    const { fetch, requested } = fakeFetch({
      [`${ORIGIN}/t4/main.tmpl.md`]: `---
imports:
  - "[cfg](./cfg.tmpl.md)"

params: []
---
Mode: {{ cfg.MODE }}`,
      [`${ORIGIN}/t4/cfg.tmpl.md`]: `---
name: cfg
consts: [MODE = str := "production"]
---
`,
    });

    const tmpl = await loadTemplate("/t4/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    // Import is resolved at construction, so both files are already fetched.
    assert.deepStrictEqual(requested.sort(), [
      `${ORIGIN}/t4/cfg.tmpl.md`,
      `${ORIGIN}/t4/main.tmpl.md`,
    ]);
    assert.strictEqual(await tmpl.renderAsync({}), "Mode: production");
  });

  it("resolves chained, const-dependent import paths in order", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/t6/main.tmpl.md`]: `---
imports:
  - "[env](./env.tmpl.md)"
  - "[greet]({{ env.DIR }}/greet.tmpl.md)"

params: [name = str]
---
{{ name }}:{{ greet.MSG }}`,
      [`${ORIGIN}/t6/env.tmpl.md`]: `---
name: env
consts: [DIR = str := "."]
---
`,
      [`${ORIGIN}/t6/greet.tmpl.md`]: `---
name: greet
consts: [MSG = str := "hi"]
---
`,
    });

    const tmpl = await loadTemplate("/t6/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    assert.strictEqual(await tmpl.renderAsync({ name: "World" }), "World:hi");
  });

  it("throws a clear error when a dependency is missing (HTTP 404)", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/t5/main.tmpl.md`]: `---
params: []
---
> {% include [m](./missing.tmpl.md) %}`,
    });

    const tmpl = await loadTemplate("/t5/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    await assert.rejects(
      () => tmpl.renderAsync({}),
      (err: unknown) => {
        assert.ok(err instanceof TemplateError);
        assert.match(err.message, /missing\.tmpl\.md/);
        assert.match(err.message, /404/);
        return true;
      },
    );
  });

  it("enforces the maxRounds backstop on a deep include chain", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/t7/main.tmpl.md`]: `---
params: []
---
> {% include [a](./a.tmpl.md) %}`,
      [`${ORIGIN}/t7/a.tmpl.md`]: `---
params: []
---
> {% include [b](./b.tmpl.md) %}`,
      [`${ORIGIN}/t7/b.tmpl.md`]: `---
params: []
---
DEEP`,
    });

    // Construction needs 1 round (fetch entry); rendering the a→b chain needs
    // more rounds than allowed, so the backstop trips with a clear error.
    const tmpl = await loadTemplate("/t7/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
      maxRounds: 1,
    });
    await assert.rejects(
      () => tmpl.renderAsync({}),
      (err: unknown) => {
        assert.ok(err instanceof TemplateError);
        assert.match(err.message, /exceeded 1 fetches/);
        return true;
      },
    );

    // With enough rounds, the same chain resolves fully.
    const ok = await loadTemplate("/t7/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
      maxRounds: 10,
    });
    assert.strictEqual(await ok.renderAsync({}), "DEEP");
  });

  it("passes the env option through to the template", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/t8/main.tmpl.md`]: `---
env: [MODE = str]

params: []
---
Mode: {{ MODE }}`,
    });

    const tmpl = await loadTemplate("/t8/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
      env: { MODE: "prod" },
    });
    assert.strictEqual(await tmpl.renderAsync({}), "Mode: prod");
  });

  it("resolves parent-directory includes", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/t9/pages/main.tmpl.md`]: `---
params: []
---
> {% include [s](../shared.tmpl.md) %}`,
      [`${ORIGIN}/t9/shared.tmpl.md`]: `---
params: []
---
SHARED`,
    });

    const tmpl = await loadTemplate("/t9/pages/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    assert.strictEqual(await tmpl.renderAsync({}), "SHARED");
  });

  it("accepts an absolute URL without a baseUrl", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/t10/main.tmpl.md`]: `---
params: [name = str]
---
Hi {{ name }}`,
    });

    const tmpl = await loadTemplate(`${ORIGIN}/t10/main.tmpl.md`, { fetch });
    assert.strictEqual(await tmpl.renderAsync({ name: "X" }), "Hi X");
  });

  it("wraps network (rejection) errors with context", async () => {
    const fetch: FetchLike = (url) => {
      if (url === `${ORIGIN}/t11/main.tmpl.md`) {
        return Promise.reject(new Error("network down"));
      }
      return Promise.resolve({
        ok: false,
        status: 404,
        text: () => Promise.resolve(""),
      });
    };

    await assert.rejects(
      () => loadTemplate("/t11/main.tmpl.md", { fetch, baseUrl: ORIGIN }),
      (err: unknown) => {
        assert.ok(err instanceof TemplateError);
        assert.match(err.message, /failed to fetch/);
        assert.match(err.message, /network down/);
        return true;
      },
    );
  });

  it("throws when no fetch implementation is available", async () => {
    const globalWithFetch = globalThis as { fetch?: unknown };
    const original = globalWithFetch.fetch;
    delete globalWithFetch.fetch;
    try {
      await assert.rejects(
        () => loadTemplate(`${ORIGIN}/t12/main.tmpl.md`),
        (err: unknown) => {
          assert.ok(err instanceof TemplateError);
          assert.match(err.message, /no fetch implementation/);
          return true;
        },
      );
    } finally {
      globalWithFetch.fetch = original;
    }
  });

  it("wraps non-Error fetch rejection values with String()", async () => {
    // NOLINT: intentional non-Error rejection — this test verifies the string→Error wrapping branch
    // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- intentional: testing the non-Error branch
    const fetch: FetchLike = () => Promise.reject("plain string rejection");

    await assert.rejects(
      () => loadTemplate("/t13/main.tmpl.md", { fetch, baseUrl: ORIGIN }),
      (err: unknown) => {
        assert.ok(err instanceof TemplateError);
        assert.match(err.message, /plain string rejection/);
        return true;
      },
    );
  });

  it("exposes the underlying Template via the .template getter", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/t14/main.tmpl.md`]: `---\nparams: [x = str]\n---\n{{ x }}`,
    });

    const tmpl = await loadTemplate("/t14/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    const inner = tmpl.template;
    // The inner Template has the core API (e.g. declarations, sourceHash).
    assert.ok(inner instanceof Template);
    assert.deepStrictEqual(inner.declarations(), [["x", "str"]]);
  });

  it("propagates real (non-missing-file) errors without retrying", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/t15/main.tmpl.md`]: `---\nparams: []\n---\n{{ undefined_var }}`,
    });

    await assert.rejects(
      () => loadTemplate("/t15/main.tmpl.md", { fetch, baseUrl: ORIGIN }),
      (err: unknown) => {
        // Should be a compile/syntax error, not a fetch error.
        assert.ok(err instanceof Error);
        assert.ok(
          !err.message.includes("fetch"),
          `expected non-fetch error, got: ${err.message}`,
        );
        return true;
      },
    );
  });

  it("proxies metadata methods: declarations, defaults, consts, body, sourceHash, frontmatter", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/tm/main.tmpl.md`]: `---
params: [x = str := "hi"]
consts: [N = int := 3]
---
body {{ x }}`,
    });

    const tmpl = await loadTemplate("/tm/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    assert.deepStrictEqual(tmpl.declarations(), [["x", "str"]]);
    assert.deepStrictEqual(tmpl.defaults(), { x: "hi" });
    assert.deepStrictEqual(tmpl.consts(), { N: 3 });
    assert.strictEqual(tmpl.body(), "body {{ x }}");
    assert.strictEqual(typeof tmpl.sourceHash(), "number");
    assert.ok(tmpl.frontmatter);
  });

  it("renderDictAsync fetches includes lazily", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/td/main.tmpl.md`]: `---
params: [x = str]
---
> {% include [a](./a.tmpl.md) with x=x %}`,
      [`${ORIGIN}/td/a.tmpl.md`]: `---
params: [x = str]
---
A:{{ x }}`,
    });

    const tmpl = await loadTemplate("/td/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    const out = await tmpl.renderDictAsync({ x: "val" });
    assert.strictEqual(out, "A:val");
  });

  it("renderEmptyAsync fetches includes lazily", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/te/main.tmpl.md`]: `---
params: [x = str := "dflt"]
---
> {% include [a](./a.tmpl.md) with x=x %}`,
      [`${ORIGIN}/te/a.tmpl.md`]: `---
params: [x = str]
---
A:{{ x }}`,
    });

    const tmpl = await loadTemplate("/te/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    assert.strictEqual(await tmpl.renderEmptyAsync(), "A:dflt");
  });

  it("renderUnchecked renders synchronously when cache is warm", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/tu/main.tmpl.md`]: `---
params: [x = str]
---
{{ x }}!`,
    });

    const tmpl = await loadTemplate("/tu/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    // No includes, so sync render is fine.
    assert.strictEqual(tmpl.renderUnchecked({ x: "fast" }), "fast!");
  });

  it("clearCache forces re-fetch on next load", async () => {
    const files: Record<string, string> = {
      [`${ORIGIN}/tc/main.tmpl.md`]: `---
params: []
---
v1`,
    };
    const { fetch, requested } = fakeFetch(files);

    const t1 = await loadTemplate("/tc/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    assert.strictEqual(await t1.renderAsync({}), "v1");
    const fetchesBefore = requested.length;

    // Update the "server" content.
    files[`${ORIGIN}/tc/main.tmpl.md`] = `---
params: []
---
v2`;

    // Without clearing, a new load still serves the VFS-cached v1 source.
    const t2 = await loadTemplate("/tc/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    assert.strictEqual(await t2.renderAsync({}), "v1");
    assert.strictEqual(requested.length, fetchesBefore);

    // After clearing, the source is re-fetched and we get v2.
    clearCache();
    const t3 = await loadTemplate("/tc/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    assert.strictEqual(await t3.renderAsync({}), "v2");
    assert.ok(requested.length > fetchesBefore);
  });

  it("concurrent renderAsync calls do not corrupt each other", async () => {
    const { fetch } = fakeFetch({
      [`${ORIGIN}/cc/main.tmpl.md`]: `---
params: [branch = str]
---
> {% include [b](./sections/{{ branch }}.tmpl.md) %}`,
      [`${ORIGIN}/cc/sections/alpha.tmpl.md`]: `---
params: []
---
ALPHA`,
      [`${ORIGIN}/cc/sections/beta.tmpl.md`]: `---
params: []
---
BETA`,
    });

    const tmpl = await loadTemplate("/cc/main.tmpl.md", {
      fetch,
      baseUrl: ORIGIN,
    });
    // Fire two concurrent renders that reach different dynamic includes.
    const [a, b] = await Promise.all([
      tmpl.renderAsync({ branch: "alpha" }),
      tmpl.renderAsync({ branch: "beta" }),
    ]);
    assert.strictEqual(a, "ALPHA");
    assert.strictEqual(b, "BETA");
  });
});

describe("md-tmpl/browser: posixPath", () => {
  it("resolves relative and absolute segments POSIX-style", () => {
    assert.strictEqual(posixPath.resolve("/a/b", "./c.md"), "/a/b/c.md");
    assert.strictEqual(posixPath.resolve("/a/b", "../c.md"), "/a/c.md");
    assert.strictEqual(posixPath.resolve("/a/b", "/x/y.md"), "/x/y.md");
    assert.strictEqual(posixPath.resolve("/a/b/../c//d"), "/a/c/d");
  });

  it("computes dirname POSIX-style", () => {
    assert.strictEqual(posixPath.dirname("/a/b/c.md"), "/a/b");
    assert.strictEqual(posixPath.dirname("/a"), "/");
    assert.strictEqual(posixPath.dirname("a.md"), ".");
  });

  it("preserves leading '..' segments for relative paths", () => {
    // Relative-path resolve: ".." can't be collapsed further, so it stays.
    assert.strictEqual(posixPath.resolve("../a/b"), "/a/b");
    // But from an absolute base, ".." never escapes the root.
    assert.strictEqual(posixPath.resolve("/", "../../../a"), "/a");
  });

  it("handles empty and dot-only segments", () => {
    assert.strictEqual(posixPath.resolve("/a", "", "b"), "/a/b");
    assert.strictEqual(posixPath.resolve("/a", ".", "b"), "/a/b");
    assert.strictEqual(posixPath.resolve("/a/./b/."), "/a/b");
  });

  it("resolves to / when given only empty or dot segments", () => {
    assert.strictEqual(posixPath.resolve("/", "."), "/");
  });
});

describe("md-tmpl/browser: MemoryFs", () => {
  it("records missing paths and clears them on write", () => {
    const fs = new MemoryFs();
    assert.strictEqual(
      fs.statSync("/x.md", { throwIfNoEntry: false }),
      undefined,
    );
    assert.throws(() => fs.readFileSync("/y.md", "utf-8"));
    assert.deepStrictEqual(fs.takeMissing().sort(), ["/x.md", "/y.md"]);
    // takeMissing clears the record.
    assert.deepStrictEqual(fs.takeMissing(), []);

    fs.write("/x.md", "hello");
    assert.strictEqual(fs.readFileSync("/x.md", "utf-8"), "hello");
    assert.ok(fs.has("/x.md"));
    assert.deepStrictEqual(fs.takeMissing(), []);
  });

  it("bumps mtime on overwrite so caches invalidate", () => {
    const fs = new MemoryFs();
    fs.write("/a.md", "one");
    const first = fs.statSync("/a.md", { throwIfNoEntry: false });
    fs.write("/a.md", "two");
    const second = fs.statSync("/a.md", { throwIfNoEntry: false });
    assert.ok(first && second && second.mtimeMs > first.mtimeMs);
  });

  it("write() removes the path from the missing set", () => {
    const fs = new MemoryFs();
    // Trigger a miss so the path is recorded.
    fs.statSync("/z.md", { throwIfNoEntry: false });
    assert.deepStrictEqual(fs.takeMissing(), ["/z.md"]);
    // Now record another miss, then write the file before taking.
    fs.statSync("/z.md", { throwIfNoEntry: false });
    fs.write("/z.md", "content");
    // The write should have removed /z.md from the missing set.
    assert.deepStrictEqual(fs.takeMissing(), []);
  });

  it("readFileSync throws FileNotFoundError with ENOENT code", () => {
    const fs = new MemoryFs();
    try {
      fs.readFileSync("/missing.md", "utf-8");
      assert.fail("expected error");
    } catch (err: unknown) {
      assert.ok(err instanceof Error);
      assert.strictEqual(err.name, "FileNotFoundError");
      assert.strictEqual((err as unknown as { code: string }).code, "ENOENT");
      assert.match(err.message, /missing\.md/);
    }
  });
});
