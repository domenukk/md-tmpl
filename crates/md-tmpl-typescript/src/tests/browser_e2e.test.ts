/**
 * End-to-end browser tests — a **real** HTTP server serving template files,
 * exercising the full `loadTemplate` → `fetch` → `MemoryFs` pipeline.
 *
 * Unlike the unit-level browser tests (which use a fake `FetchLike`), these
 * tests spin up a Node.js HTTP server on an ephemeral port, serve template
 * files from an in-memory tree, and call `loadTemplate` with real `fetch`.
 * This catches integration issues that fake-fetch tests cannot:
 *
 *  - URL resolution / origin handling with a real server
 *  - Relative include paths across directory boundaries
 *  - Chained includes (A → B → C) with lazy discovery
 *  - Import resolution via fetch
 *  - Error propagation from real HTTP 404s
 *  - Cross-template constant sharing
 *
 * Each `describe` block allocates its own server, and `after` tears it down.
 */

import { describe, it, before, after } from "node:test";
import assert from "node:assert/strict";
import {
  createServer,
  type Server,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";

import { TemplateError } from "../index.js";
import { loadTemplate } from "../browser/index.js";

// ---------------------------------------------------------------------------
// Test HTTP server helper
// ---------------------------------------------------------------------------

/** A minimal static file server backed by an in-memory path→source map. */
interface TestServer {
  /** e.g. "http://127.0.0.1:54321" */
  origin: string;
  /** Shut down the server. */
  close: () => Promise<void>;
  /** The underlying Node HTTP server (for inspection). */
  server: Server;
}

function startServer(files: Record<string, string>): Promise<TestServer> {
  return new Promise((resolve, reject) => {
    const handler = (req: IncomingMessage, res: ServerResponse): void => {
      const url = req.url ?? "/";
      const body = files[url];
      if (body === undefined) {
        res.writeHead(404, { "Content-Type": "text/plain" });
        res.end("Not Found");
        return;
      }
      res.writeHead(200, { "Content-Type": "text/markdown; charset=utf-8" });
      res.end(body);
    };

    const server = createServer(handler);
    server.listen(0, "127.0.0.1", () => {
      const addr = server.address();
      if (!addr || typeof addr === "string") {
        reject(new Error("unexpected server address format"));
        return;
      }
      const origin = `http://127.0.0.1:${String(addr.port)}`;
      resolve({
        origin,
        close: () =>
          new Promise<void>((r) => {
            server.close(() => {
              r();
            });
          }),
        server,
      });
    });
    server.on("error", reject);
  });
}

// The browser module uses a single shared MemoryFs across all `loadTemplate`
// calls.  Each test group uses unique URL paths, so they don't interfere.

// ---------------------------------------------------------------------------
// E2E: relative includes across directories
// ---------------------------------------------------------------------------

describe("browser E2E: relative includes across directories", () => {
  let srv: TestServer;

  before(async () => {
    srv = await startServer({
      // Root template includes a header from ../shared/ and a footer from ./parts/
      "/app/pages/home.tmpl.md": `---
params: [title = str]
---
> {% include [header](../shared/header.tmpl.md) with title=title %}

Welcome to {{ title }}.

> {% include [footer](./parts/footer.tmpl.md) %}`,

      "/app/shared/header.tmpl.md": `---
params: [title = str]
---
# {{ title }}
`,

      "/app/pages/parts/footer.tmpl.md": `---
params: []
---
---
Built with md-tmpl.`,
    });
  });

  after(async () => {
    await srv.close();
  });

  it("resolves relative includes (../ and ./) across directory boundaries", async () => {
    const tmpl = await loadTemplate("/app/pages/home.tmpl.md", {
      baseUrl: srv.origin,
    });

    const out = await tmpl.renderAsync({ title: "Home" });
    assert.ok(out.includes("# Home"), `expected header, got: ${out}`);
    assert.ok(out.includes("Welcome to Home."), `expected body, got: ${out}`);
    assert.ok(
      out.includes("Built with md-tmpl."),
      `expected footer, got: ${out}`,
    );
  });

  it("serves subsequent sync renders from cache", async () => {
    const tmpl = await loadTemplate("/app/pages/home.tmpl.md", {
      baseUrl: srv.origin,
    });
    // Warm the include cache with an async render.
    await tmpl.renderAsync({ title: "Cached" });
    // Sync render should work without any fetch.
    const out = tmpl.render({ title: "Sync" });
    assert.ok(out.includes("# Sync"));
  });
});

// ---------------------------------------------------------------------------
// E2E: chained includes (A → B → C)
// ---------------------------------------------------------------------------

describe("browser E2E: chained includes (three levels deep)", () => {
  let srv: TestServer;

  before(async () => {
    srv = await startServer({
      "/chain/root.tmpl.md": `---
params: [x = str]
---
ROOT:{{ x }}

> {% include [mid](./mid.tmpl.md) with x=x %}`,

      "/chain/mid.tmpl.md": `---
params: [x = str]
---
MID:{{ x }}

> {% include [leaf](./sub/leaf.tmpl.md) with x=x %}`,

      "/chain/sub/leaf.tmpl.md": `---
params: [x = str]
---
LEAF:{{ x }}`,
    });
  });

  after(async () => {
    await srv.close();
  });

  it("lazily discovers and fetches the full include chain", async () => {
    const tmpl = await loadTemplate("/chain/root.tmpl.md", {
      baseUrl: srv.origin,
    });

    const out = await tmpl.renderAsync({ x: "hello" });
    assert.ok(out.includes("ROOT:hello"), `expected ROOT, got: ${out}`);
    assert.ok(out.includes("MID:hello"), `expected MID, got: ${out}`);
    assert.ok(out.includes("LEAF:hello"), `expected LEAF, got: ${out}`);
  });
});

// ---------------------------------------------------------------------------
// E2E: imports across directories
// ---------------------------------------------------------------------------

describe("browser E2E: imports with constants", () => {
  let srv: TestServer;

  before(async () => {
    srv = await startServer({
      "/imports/main.tmpl.md": `---
imports:
  - "[config](../config/settings.tmpl.md)"

params: [name = str]
---
{{ name }} uses {{ config.MODEL }} with {{ config.MAX_TOKENS }} tokens.`,

      "/config/settings.tmpl.md": `---
name: config
consts:
  - MODEL = str := "gemini-2.5-pro"
  - MAX_TOKENS = int := 8192
---
`,
    });
  });

  after(async () => {
    await srv.close();
  });

  it("fetches imports at construction and resolves cross-file constants", async () => {
    const tmpl = await loadTemplate("/imports/main.tmpl.md", {
      baseUrl: srv.origin,
    });

    const out = await tmpl.renderAsync({ name: "Alice" });
    assert.strictEqual(out, "Alice uses gemini-2.5-pro with 8192 tokens.");
  });
});

// ---------------------------------------------------------------------------
// E2E: dynamic (param-dependent) include paths
// ---------------------------------------------------------------------------

describe("browser E2E: dynamic include paths", () => {
  let srv: TestServer;

  before(async () => {
    srv = await startServer({
      "/dynamic/router.tmpl.md": `---
params: [page = str]
---
> {% include [p](./pages/{{ page }}.tmpl.md) %}`,

      "/dynamic/pages/about.tmpl.md": `---
params: []
---
About us.`,

      "/dynamic/pages/contact.tmpl.md": `---
params: []
---
Contact info.`,
    });
  });

  after(async () => {
    await srv.close();
  });

  it("fetches only the include the current params reach", async () => {
    const tmpl = await loadTemplate("/dynamic/router.tmpl.md", {
      baseUrl: srv.origin,
    });

    const aboutOut = await tmpl.renderAsync({ page: "about" });
    assert.ok(
      aboutOut.includes("About us."),
      `expected about page, got: ${aboutOut}`,
    );

    const contactOut = await tmpl.renderAsync({ page: "contact" });
    assert.ok(
      contactOut.includes("Contact info."),
      `expected contact page, got: ${contactOut}`,
    );
  });
});

// ---------------------------------------------------------------------------
// E2E: env variables
// ---------------------------------------------------------------------------

describe("browser E2E: env variables", () => {
  let srv: TestServer;

  before(async () => {
    srv = await startServer({
      "/env/prompt.tmpl.md": `---
env:
  - DEPLOYMENT = str
  - DEBUG = bool := false

params: [query = str]
---
[{{ DEPLOYMENT }}] Query: {{ query }}`,
    });
  });

  after(async () => {
    await srv.close();
  });

  it("passes env values through to the template", async () => {
    const tmpl = await loadTemplate("/env/prompt.tmpl.md", {
      baseUrl: srv.origin,
      env: { DEPLOYMENT: "production" },
    });

    const out = await tmpl.renderAsync({ query: "hello" });
    assert.strictEqual(out, "[production] Query: hello");
  });
});

// ---------------------------------------------------------------------------
// E2E: real HTTP 404 error
// ---------------------------------------------------------------------------

describe("browser E2E: HTTP 404 errors", () => {
  let srv: TestServer;

  before(async () => {
    srv = await startServer({
      "/errors/main.tmpl.md": `---
params: []
---
> {% include [missing](./does_not_exist.tmpl.md) %}`,
    });
  });

  after(async () => {
    await srv.close();
  });

  it("surfaces a clear TemplateError with the 404 status", async () => {
    const tmpl = await loadTemplate("/errors/main.tmpl.md", {
      baseUrl: srv.origin,
    });

    await assert.rejects(
      () => tmpl.renderAsync({}),
      (err: unknown) => {
        assert.ok(err instanceof TemplateError);
        assert.match(err.message, /does_not_exist/);
        assert.match(err.message, /404/);
        return true;
      },
    );
  });
});

// ---------------------------------------------------------------------------
// E2E: for loop rendering over the network
// ---------------------------------------------------------------------------

describe("browser E2E: for loop rendering", () => {
  let srv: TestServer;

  before(async () => {
    srv = await startServer({
      "/iter/list.tmpl.md": `---
params: [items = list(name = str)]
---

> {% for item in items %}

- {{ item.name }}

> {% /for %}
`,
    });
  });

  after(async () => {
    await srv.close();
  });

  it("renders a for loop with list items fetched over the network", async () => {
    const tmpl = await loadTemplate("/iter/list.tmpl.md", {
      baseUrl: srv.origin,
    });

    const out = await tmpl.renderAsync({
      items: [{ name: "Alpha" }, { name: "Beta" }, { name: "Gamma" }],
    });
    assert.ok(out.includes("- Alpha"), `expected Alpha, got: ${out}`);
    assert.ok(out.includes("- Beta"), `expected Beta, got: ${out}`);
    assert.ok(out.includes("- Gamma"), `expected Gamma, got: ${out}`);
  });
});
