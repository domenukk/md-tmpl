//! Criterion benchmarks for the `prompt-templates` crate.
//!
//! Groups:
//! - **compile**: `Template::from_source` / `Template::try_from_source` on
//!   realistic template strings.
//! - **render**: Pre-compiled template rendering at three complexity tiers.
//! - **`round_trip`**: End-to-end source→render in a single call.
//! - **filters**: Individual filter application throughput.
//! - **conditions**: Condition evaluation overhead.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use prompt_templates::{Template, ctx};

// ---------------------------------------------------------------------------
// Template sources
// ---------------------------------------------------------------------------

const SMALL_TEMPLATE: &str = r"---
params:
  - name = str
  - place = str
---
Hello {{ name }}, welcome to {{ place }}!";

const MEDIUM_TEMPLATE: &str = "\
---
params:
  - title = str
  - status = str
  - score = float
  - show_footer = bool
  - items = list<label = str, value = str>
---
# Report for {{ title | upper }}

Status: {{ status }}
Score: {{ score | fixed(2) }}

## Items

> {% for item in items %}

- {{ item.label }}: {{ item.value }}

> {% /for %}

> {% if show_footer %}

---
Generated for {{ title }}.

> {% /if %}";

const LARGE_TEMPLATE: &str = "\
---
params:
  - title = str
  - sections = list<heading = str, entries = list<name = str, active = bool, score = float, tags = list<label = str>>>
  - notes = str
---
# {{ title | upper }}

> {% for section in sections %}

## {{ section.heading }}

> {% for entry in section.entries %}

### {{ entry.name | trim }}

> {% if entry.active %}

- Status: **active**
- Score: {{ entry.score | fixed(1) }}

> {% elif entry.score > 0 %}

- Status: inactive (score {{ entry.score }})

> {% else %}

- Status: inactive

> {% /if %}

> {% for tag in entry.tags %}

  - tag: {{ tag.label | lower }}

> {% /for %}

> {% /for %}

> {% /for %}

> {% if notes %}

## Notes

{{ notes }}

> {% /if %}";

// ---------------------------------------------------------------------------
// Context builders
// ---------------------------------------------------------------------------

fn small_context() -> prompt_templates::Context {
    ctx! {
        name: "Alice",
        place: "Wonderland",
    }
}

fn medium_context() -> prompt_templates::Context {
    ctx! {
        title: "Monthly",
        status: "complete",
        score: 87.456_f64,
        show_footer: true,
        items: [
            { label: "Alpha", value: "100" },
            { label: "Beta",  value: "200" },
            { label: "Gamma", value: "" },
            { label: "Delta", value: "400" },
            { label: "Epsilon", value: "500" },
        ],
    }
}

fn large_context() -> prompt_templates::Context {
    use std::sync::Arc;

    use prompt_templates::Value;

    let make_entry = |name: &str, active: bool, score: f64, tags: &[&str]| -> Value {
        Value::new_struct([
            ("name", Value::from(name)),
            ("active", Value::from(active)),
            ("score", Value::Float(score)),
            (
                "tags",
                Value::List(Arc::new(
                    tags.iter()
                        .map(|t| Value::new_struct([("label", Value::from(*t))]))
                        .collect(),
                )),
            ),
        ])
    };

    let make_section = |heading: &str, n: u16| -> Value {
        let entries: Vec<Value> = (0..n)
            .map(|i| {
                make_entry(
                    &format!("Entry-{i}"),
                    i % 3 == 0,
                    f64::from(i) * 1.5,
                    &["rust", "bench", "template"],
                )
            })
            .collect();
        Value::new_struct([
            ("heading", Value::from(heading)),
            ("entries", Value::List(Arc::new(entries))),
        ])
    };

    ctx! {
        title: "Benchmark Report",
        sections: [
            (make_section("Section A", 10)),
            (make_section("Section B", 10)),
            (make_section("Section C", 10)),
        ],
        notes: "End of report.",
    }
}

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

fn bench_compile(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile");

    group.bench_function("small", |b| {
        b.iter(|| Template::from_source(black_box(SMALL_TEMPLATE)).unwrap());
    });

    group.bench_function("medium", |b| {
        b.iter(|| Template::from_source(black_box(MEDIUM_TEMPLATE)).unwrap());
    });

    group.bench_function("large", |b| {
        b.iter(|| Template::from_source(black_box(LARGE_TEMPLATE)).unwrap());
    });

    group.finish();
}

fn bench_render(c: &mut Criterion) {
    let small = Template::from_source(SMALL_TEMPLATE).unwrap();
    let medium = Template::from_source(MEDIUM_TEMPLATE).unwrap();
    let large = Template::from_source(LARGE_TEMPLATE).unwrap();

    let small_ctx = small_context();
    let medium_ctx = medium_context();
    let large_ctx = large_context();

    let mut group = c.benchmark_group("render");

    group.bench_function("small", |b| {
        b.iter(|| small.render(black_box(&small_ctx)).unwrap());
    });

    group.bench_function("medium", |b| {
        b.iter(|| medium.render(black_box(&medium_ctx)).unwrap());
    });

    group.bench_function("large", |b| {
        b.iter(|| large.render(black_box(&large_ctx)).unwrap());
    });

    group.finish();
}

fn bench_round_trip(c: &mut Criterion) {
    let small_ctx = small_context();
    let medium_ctx = medium_context();
    let large_ctx = large_context();

    let mut group = c.benchmark_group("round_trip");

    group.bench_function("small", |b| {
        b.iter(|| {
            let tmpl = Template::from_source(black_box(SMALL_TEMPLATE)).unwrap();
            tmpl.render(black_box(&small_ctx)).unwrap()
        });
    });

    group.bench_function("medium", |b| {
        b.iter(|| {
            let tmpl = Template::from_source(black_box(MEDIUM_TEMPLATE)).unwrap();
            tmpl.render(black_box(&medium_ctx)).unwrap()
        });
    });

    group.bench_function("large", |b| {
        b.iter(|| {
            let tmpl = Template::from_source(black_box(LARGE_TEMPLATE)).unwrap();
            tmpl.render(black_box(&large_ctx)).unwrap()
        });
    });

    group.finish();
}

fn bench_filters(c: &mut Criterion) {
    let mut group = c.benchmark_group("filters");

    // Upper filter
    group.bench_function("upper", |b| {
        let tmpl = Template::from_source(
            r"---
params: [val = str]
---
{{ val | upper }}",
        )
        .unwrap();
        let ctx = ctx! { val: "hello world benchmark string" };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    // Lower filter
    group.bench_function("lower", |b| {
        let tmpl = Template::from_source(
            r"---
params: [val = str]
---
{{ val | lower }}",
        )
        .unwrap();
        let ctx = ctx! { val: "HELLO WORLD BENCHMARK STRING" };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    // Trim filter
    group.bench_function("trim", |b| {
        let tmpl = Template::from_source(
            r"---
params: [val = str]
---
{{ val | trim }}",
        )
        .unwrap();
        let ctx = ctx! { val: "   lots of whitespace   " };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    // Fixed filter
    group.bench_function("fixed", |b| {
        let tmpl = Template::from_source(
            r"---
params: [val = float]
---
{{ val | fixed(3) }}",
        )
        .unwrap();
        let ctx = ctx! { val: 3.15_f64 };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    // Chained filters
    group.bench_function("chain_trim_upper", |b| {
        let tmpl = Template::from_source(
            r"---
params: [val = str]
---
{{ val | trim | upper }}",
        )
        .unwrap();
        let ctx = ctx! { val: "  mixed Case Input  " };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    group.finish();
}

fn bench_conditions(c: &mut Criterion) {
    let mut group = c.benchmark_group("conditions");

    // Simple truthiness
    group.bench_function("truthy", |b| {
        let tmpl = Template::from_source(
            r"---
params: [flag = bool]
---
> {% if flag %}

yes

> {% else %}

no

> {% /if %}",
        )
        .unwrap();
        let ctx = ctx! { flag: true };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    // String equality comparison
    group.bench_function("string_eq", |b| {
        let tmpl = Template::from_source(
            r#"---
params: [status = str]
---
> {% if status == "active" %}

on

> {% else %}

off

> {% /if %}"#,
        )
        .unwrap();
        let ctx = ctx! { status: "active" };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    // Numeric comparison
    group.bench_function("numeric_gt", |b| {
        let tmpl = Template::from_source(
            r"---
params: [count = int]
---
> {% if count > 5 %}

many

> {% else %}

few

> {% /if %}",
        )
        .unwrap();
        let ctx = ctx! { count: 10_i64 };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    // elif chain
    group.bench_function("elif_chain", |b| {
        let tmpl = Template::from_source(
            r#"---
params: [level = str]
---
> {% if level == "high" %}

H

> {% elif level == "medium" %}

M

> {% elif level == "low" %}

L

> {% else %}

?

> {% /if %}"#,
        )
        .unwrap();
        // Hit the last elif branch to exercise full scan
        let ctx = ctx! { level: "low" };
        b.iter(|| tmpl.render(black_box(&ctx)).unwrap());
    });

    group.finish();
}

fn bench_render_into(c: &mut Criterion) {
    let small = Template::from_source(SMALL_TEMPLATE).unwrap();
    let medium = Template::from_source(MEDIUM_TEMPLATE).unwrap();
    let large = Template::from_source(LARGE_TEMPLATE).unwrap();

    let small_ctx = small_context();
    let medium_ctx = medium_context();
    let large_ctx = large_context();

    let mut group = c.benchmark_group("render_into");

    group.bench_function("small", |b| {
        let mut buf = String::with_capacity(256);
        b.iter(|| {
            buf.clear();
            small.render_into(black_box(&small_ctx), &mut buf).unwrap();
            black_box(&buf);
        });
    });

    group.bench_function("medium", |b| {
        let mut buf = String::with_capacity(1024);
        b.iter(|| {
            buf.clear();
            medium
                .render_into(black_box(&medium_ctx), &mut buf)
                .unwrap();
            black_box(&buf);
        });
    });

    group.bench_function("large", |b| {
        let mut buf = String::with_capacity(4096);
        b.iter(|| {
            buf.clear();
            large.render_into(black_box(&large_ctx), &mut buf).unwrap();
            black_box(&buf);
        });
    });

    group.finish();
}

fn bench_flexbuffers(c: &mut Criterion) {
    use serde::Serialize;

    // Pre-serialize contexts to FlexBuffers for deserialization benchmarks.
    #[derive(Serialize)]
    struct SmallParams {
        name: String,
        place: String,
    }

    #[derive(Serialize)]
    struct MediumItem {
        label: String,
        value: String,
    }

    #[derive(Serialize)]
    struct MediumParams {
        title: String,
        status: String,
        score: f64,
        show_footer: bool,
        items: Vec<MediumItem>,
    }

    let small_fb = flexbuffers::to_vec(&SmallParams {
        name: "Alice".into(),
        place: "Wonderland".into(),
    })
    .unwrap();

    let medium_fb = flexbuffers::to_vec(&MediumParams {
        title: "Monthly".into(),
        status: "complete".into(),
        score: 87.456,
        show_footer: true,
        items: vec![
            MediumItem {
                label: "Alpha".into(),
                value: "100".into(),
            },
            MediumItem {
                label: "Beta".into(),
                value: "200".into(),
            },
            MediumItem {
                label: "Gamma".into(),
                value: String::new(),
            },
            MediumItem {
                label: "Delta".into(),
                value: "400".into(),
            },
            MediumItem {
                label: "Epsilon".into(),
                value: "500".into(),
            },
        ],
    })
    .unwrap();

    let small = Template::from_source(SMALL_TEMPLATE).unwrap();
    let medium = Template::from_source(MEDIUM_TEMPLATE).unwrap();

    let mut group = c.benchmark_group("flexbuffers");

    // Measure deserialization only (FlexBuffers → Context).
    group.bench_function("deser_small", |b| {
        b.iter(|| prompt_templates::Context::from_flexbuffers(black_box(&small_fb)).unwrap());
    });

    group.bench_function("deser_medium", |b| {
        b.iter(|| prompt_templates::Context::from_flexbuffers(black_box(&medium_fb)).unwrap());
    });

    // Measure full FlexBuffers → render path vs pre-built Context → render.
    group.bench_function("render_small_from_flexbuffers", |b| {
        b.iter(|| {
            let ctx = prompt_templates::Context::from_flexbuffers(black_box(&small_fb)).unwrap();
            small.render(black_box(&ctx)).unwrap()
        });
    });

    group.bench_function("render_medium_from_flexbuffers", |b| {
        b.iter(|| {
            let ctx = prompt_templates::Context::from_flexbuffers(black_box(&medium_fb)).unwrap();
            medium.render(black_box(&ctx)).unwrap()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_compile,
    bench_render,
    bench_render_into,
    bench_round_trip,
    bench_filters,
    bench_conditions,
    bench_flexbuffers,
);
criterion_main!(benches);
