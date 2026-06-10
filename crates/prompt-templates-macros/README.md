# prompt-templates-macros: Compile-time validation and code generation for prompt-templates

Proc macros for compile-time template validation, pre-parsing, and typed parameter struct generation in the `prompt-templates` template engine.

These macros ensure that template files are valid and that the variable references match their Rust representations before your code even compiles.

## Macros

### `include_template!`

Reads, parses, and validates a `.tmpl.md` template file at compile time.

The file path is resolved relative to the calling crate's `CARGO_MANIFEST_DIR`. At compile time, this macro:

1. Reads the template file.
2. Parses its frontmatter (template name, description, typed variable declarations).
3. Validates that all `{{ var }}` expressions in the body reference declared variables (when frontmatter declares a `params:` block).
4. Verifies that all type annotations are syntactically valid.

If any checks fail, a **compile error** is emitted. At runtime, the returned `Template` has **zero parsing overhead** — all parsing and validation was done at compile time. Only rendering runs.

Additionally, this macro registers the template file as a dependency using `include_str!` internally, ensuring that Cargo automatically rebuilds your crate whenever the template file is modified.

```rust
use prompt_templates_macros::include_template;

// Pre-parsed and validated at compile time.
let tmpl = include_template!("prompts/simple_greeting.tmpl.md");

// At runtime, just render — zero parsing overhead.
let mut ctx = prompt_templates::Context::new();
ctx.set("name", "world");
let output = tmpl.render(&ctx).unwrap();
```

### `validate_template!`

Like `include_template!`, but only performs compile-time validation. It does not produce a runtime `Template` value.

This is useful for static assertions in test modules or build scripts to guarantee template validity without loading them into memory.

```rust
// Fails the build if the template is invalid.
prompt_templates_macros::validate_template!("prompts/simple_greeting.tmpl.md");
```

### `template_params_struct!`

Generates a typed parameter Rust struct from a `.tmpl.md` template's frontmatter variable declarations.

It reads the template at compile time, inspects its variable declarations, and generates a struct with matching field names and mapped Rust types. The generated struct provides:

- **`to_context(&self) -> Context`**: Converts the struct's fields into a rendering context.
- **`validate_template(tmpl: &Template) -> Result<(), TemplateError>`**: Checks that a template's declarations match this struct (essential for hot-reloading safety).
- **`render(&self, tmpl: &Template) -> Result<String, TemplateError>`**: Validates the template and renders it using the struct's fields.

#### Type Mapping

| Frontmatter Type          | Rust Type                                                  |
| :------------------------ | :--------------------------------------------------------- |
| `str`                     | `String`                                                   |
| `int`                     | `i64`                                                      |
| `float`                   | `f64`                                                      |
| `bool`                    | `bool`                                                     |
| `list<field = type, ...>` | `Vec<{StructName}{Field}Item>` (auto-generated sub-struct) |
| `list` (untyped)          | `Vec<prompt_templates::Value>`                             |
| `dict<field = type, ...>` | `{StructName}{Field}` (auto-generated sub-struct)          |
| _(untyped)_               | `prompt_templates::Value`                                  |

#### Example

```rust
// Given `prompts/greeting.tmpl.md` with the following frontmatter:
// ---
// params:
//   - name = str
//   - count = int
//   - items = list<label = str>
// ---
// Hello {{ name }}!

prompt_templates_macros::template_params_struct!("prompts/greeting.tmpl.md" => GreetingParams);

// This generates:
//
// pub struct GreetingParams {
//     pub name: String,
//     pub count: i64,
//     pub items: Vec<GreetingParamsItemsItem>,
// }
//
// pub struct GreetingParamsItemsItem {
//     pub label: String,
// }

let tmpl = prompt_templates_macros::include_template!("prompts/greeting.tmpl.md");
let output = GreetingParams {
    name: "Alice".to_string(),
    count: 42,
    items: vec![GreetingParamsItemsItem {
        label: "hello".to_string(),
    }],
}.render(&tmpl).unwrap();
```

## Hot-Reloading and Type Safety

You can combine compile-time type-safety with dynamic loading (e.g. for fast local iteration of prompts without recompilation). Load the template from the filesystem at runtime, and validate it against the generated parameter struct:

```rust
use prompt_templates::Template;

// Struct generated at compile time:
prompt_templates_macros::template_params_struct!("prompts/greeting.tmpl.md" => GreetingParams);

// At runtime, load template from disk:
let tmpl = Template::from_file(std::path::Path::new("prompts/greeting.tmpl.md")).unwrap();

// Ensure the reloaded file has not diverged from the compiled struct:
GreetingParams::validate_template(&tmpl).unwrap();

// Safely render:
let output = GreetingParams {
    name: "Bob".to_string(),
    count: 1,
    items: vec![],
}.render(&tmpl).unwrap();
```
