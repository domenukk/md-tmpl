# md-tmpl — Shared Conformance Corpus

One **language-neutral** corpus of test cases, replayed by **all four** backends
so that the independent implementations are proven to agree, case-for-case:

| Backend    | Harness                                                   |
| ---------- | --------------------------------------------------------- |
| Rust       | `crates/md-tmpl-core/tests/conformance.rs`                |
| TypeScript | `crates/md-tmpl-typescript/src/tests/conformance.test.ts` |
| Go         | `go/md_tmpl/conformance_test.go`                          |
| Python     | `crates/md-tmpl-python/python/tests/test_conformance.py`  |

Run every harness at once:

```bash
just test-conformance
```

Each harness reports the offending case **`name`** on failure so drift is
immediately diagnosable.

## Files

| File                 | What it asserts                                        | Cases |
| -------------------- | ------------------------------------------------------ | ----- |
| `render.toml`        | `source` + `params` → exact rendered `output`          | 42    |
| `frontmatter.toml`   | `source` → `defaults()` object                         | 18    |
| `errors.toml`        | `source` (+`params`) → an error at a given phase       | 14    |
| `escapes.toml`       | string-escape (`S`) matrix → `defaults()`              | 8     |
| `comments.toml`      | trailing-comment (`C`) matrix → `defaults()`           | 5     |
| `interpolation.toml` | `{{ }}` interpolation inside statement string contexts | 2     |
| `literals.toml`      | literals usable in every general expression position   | 32    |

Total: **121** cases, every one satisfied by **all four** backends.

## Format: TOML

The corpus is **TOML** — parsed by the `toml` crate (Rust), `smol-toml` (TS),
`BurntSushi/toml` (Go), and stdlib `tomllib` (Python). Each file is a list of
`[[cases]]`; `\n` inside a `source` string encodes a multi-line template.

### The `null` / option-`None` sentinel

TOML has no `null`. Option-`None` (in `params` and `defaults`) is therefore
encoded as the sentinel inline table:

```toml
o = { __none__ = true }
```

Every harness decodes this back to its language's null (`null` / `nil` / `None`)
on load, mirroring the `__kind__` tag used for enum struct variants.

## Case schema

```toml
[[cases]]
name = "interp_single"
note = "single {{ }} interpolation"
source = "---\nparams:\n  - name = str\n---\nHello {{ name }}!"
params = { name = "World" }
expect = { kind = "render", output = "Hello World!" }
```

Common fields:

- `name` — unique, stable identifier. **Harnesses MUST report this on failure.**
- `note` — one-line human description / executable documentation.
- `source` — the full template text (frontmatter + body).
- `params` — the render context (present for `render` and some `error` cases).
- `env` — _(optional, `render` only)_ compile-time `env:` values, e.g.
  `{ REGION = "eu" }`. Maps to `fromSourceWithEnv` (TS) / `FromSourceWithEnv`
  (Go) / `from_source_with_env` (Python) / `CompileOptions::env` (Rust).

`expect.kind` is one of:

| kind      | extra fields                                           | meaning                                                           |
| --------- | ------------------------------------------------------ | ----------------------------------------------------------------- |
| `render`  | `output` (string)                                      | exact rendered output                                             |
| `default` | `defaults` (table)                                     | `defaults()` projected to the canonical value encoding            |
| `error`   | `phase` (`compile`\|`render`\|`any`), `error_contains` | the named stage MUST fail with a message containing the substring |

`phase = "any"` accepts a failure at **either** compile or render (used for
leak-safety cases whose phase legitimately differs between backends).

## Value encoding (canonical projection)

Expected values (`params`, `defaults`, `output`) use the engines' **native
value projection** — lossless and emitted identically by all backends:

| Type                    | Encoding                                                                            |
| ----------------------- | ----------------------------------------------------------------------------------- |
| str/int/float/bool      | TOML scalar                                                                         |
| list                    | TOML array                                                                          |
| struct                  | TOML (inline) table                                                                 |
| option `None`           | `{ __none__ = true }` (see the sentinel above)                                      |
| option `Some x`         | transparent — just `x`                                                              |
| enum **unit** variant   | the bare variant name as a string, e.g. `"Active"`                                  |
| enum **struct** variant | table tagged with `__kind__`, e.g. `{ __kind__ = "Confirmed", evidence = "found" }` |

> [!NOTE]
> A unit enum value is indistinguishable from a plain string at the value layer.
> This is intentional and true of **all** engines, so harnesses compare with a
> plain deep-equal. Enum _inputs_ in `params` follow the same convention.

## Authoring rules (enforced by the engine)

These are not style choices — the engine enforces them, so cases must follow:

- **Blank line before a new frontmatter section.** A block list (`consts:`,
  `types:`, `env:`) must be followed by a blank line before the next key.
- **Blockquote statement tags need breathing room.** A standalone `> {% ... %}`
  tag line must be followed by a blank line _or_ another `> {%...%}` tag line.
- **Params require an explicit type.** `- x := "hi"` is rejected; write
  `- x = str := "hi"`.
- **`list(struct(...))` is rejected** in favour of the field shorthand
  `list(name = str, ...)`.
- Available **filters**: `upper`, `lower`, `trim`, `fixed(n)`, `join(sep)`,
  `limit(n)`, `add(n)`, `sub(n)`.
- Available **functions**: `len(...)`, `kind(Enum.Variant)`, `kinds(Enum)`,
  `has(option)`.

## Adding a case

1. Add a `[[cases]]` block to the relevant file with a unique `name`.
2. Encode option-`None` as `{ __none__ = true }` (never a bare `null`).
3. Run `just test-conformance` — the case must pass in **all four** backends. A
   divergence is a real bug to fix, not a case to special-case.
