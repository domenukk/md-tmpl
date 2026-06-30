# md-tmpl-wasm

WebAssembly bindings for the
[md-tmpl](https://github.com/domenukk/md-tmpl) engine.
Wraps the full Rust engine via `wasm-bindgen` for use in Node.js and
browser environments.

Implements the same `ITemplate` interface as the pure-TypeScript
`md-tmpl` package — swap backends without changing application
code.

## Building

```bash
wasm-pack build --target nodejs --out-dir pkg

# For browser:
wasm-pack build --target web --out-dir pkg
```

## Quick Start

```ts
import { Template } from "./pkg/md_tmpl_wasm";

const tmpl = Template.fromSource(`---
params:
  - name = str
  - role = str
---
You are {{ role }}. Hello {{ name }}!`);

console.log(tmpl.render({ name: "Alice", role: "an AI assistant" }));
// → "You are an AI assistant. Hello Alice!"
```

## API

### Constructors

```ts
Template.fromSource(source: string): Template
Template.fromSourceAllowingUnused(source: string): Template
Template.fromSourceWithBaseDir(source: string, baseDir: string): Template
```

### Rendering

```ts
tmpl.render(params: object): string              // type-validated
tmpl.renderUnchecked(params: object): string      // allows extra params
tmpl.renderJson(jsonStr: string): string          // avoids per-field WASM crossings
tmpl.renderUncheckedJson(jsonStr: string): string
tmpl.renderFlexbuffers(buffer: Uint8Array): string        // zero-copy binary
tmpl.renderUncheckedFlexbuffers(buffer: Uint8Array): string
```

### Metadata

```ts
tmpl.body(): string
tmpl.defaults(): object
tmpl.consts(): object
tmpl.importedConsts(): object
tmpl.declarations(): [string, string][]
tmpl.sourceHash(): number
```

## Serialization Tiers

| Method                | Input        | Overhead                         |
| --------------------- | ------------ | -------------------------------- |
| `render()`            | JS object    | serde-wasm-bindgen per field     |
| `renderJson()`        | JSON string  | One string copy, JSON parse      |
| `renderFlexbuffers()` | `Uint8Array` | Zero-copy binary deserialization |

For small templates, `render()` is simplest. For high-throughput
scenarios, `renderFlexbuffers()` eliminates serialization overhead.

## ITemplate Compatibility

```ts
import type { ITemplate } from "md-tmpl";

function renderGreeting(tmpl: ITemplate, name: string): string {
  return tmpl.render({ name });
}

import { Template as WasmTemplate } from "md-tmpl-wasm";
import { Template as TsTemplate } from "md-tmpl";

renderGreeting(WasmTemplate.fromSource(src), "Alice");
renderGreeting(TsTemplate.fromSource(src), "Alice");
```

## Performance

### WASM vs Pure-TypeScript

([source](benchmarks/bench.mjs))

| Scenario               |    WASM | Pure-TS |   Winner |
| ---------------------- | ------: | ------: | -------: |
| **render simple**      |  2.4 µs |  406 ns |  TS 5.9× |
| **render multi-param** |  4.2 µs |  1.1 µs |  TS 3.8× |
| **render list/for**    |  6.8 µs |  2.7 µs |  TS 2.5× |
| **render complex**     | 12.4 µs |  6.0 µs |  TS 2.1× |
| **declarations()**     |   44 ns |  585 ns | WASM 13× |
| **renderJson complex** |  6.1 µs |  6.0 µs |     ~tie |

Pure-TS is faster for rendering due to JS↔WASM serialization overhead.
WASM wins on metadata access. Use WASM when you need exact Rust-engine
parity.

## License

Apache-2.0 OR MIT
