//! Criterion benchmarks comparing md-tmpl against Tera, MiniJinja,
//! and Handlebars across five scenarios of increasing complexity.
//!
//! **Scenarios**:
//! 1. **Simple** — plain variable substitution
//! 2. **Loop** — iterating over a list of items
//! 3. **Conditional** — if / elif / else branching
//! 4. **Hero** — nested loops + conditionals (realistic)
//! 5. **Mega** — deep nesting, large lists, idx, filters
//!
//! Each scenario benchmarks:
//! - `md_tmpl_macro`: compile-time `include_template!` / `template!` macro (`Params::render()`)
//! - `md_tmpl`: runtime template engine (`Template::render_ctx`) with full parameter & schema validation
//! - `tera`: Tera template engine with pre-built context
//! - `minijinja`: MiniJinja template engine with direct serialization
//! - `handlebars`: Handlebars template engine with JSON context
//!
//! Plus `hero_e2e` and `mega_e2e` measuring end-to-end (context construction + render) pipelines.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

#[path = "comparison/engines.rs"]
mod engines;
#[path = "comparison/models.rs"]
mod models;
#[path = "comparison/scenarios.rs"]
mod scenarios;

use engines::{assert_engines_match, HandlebarsEngine, MdTmplEngine, MiniJinjaEngine, TeraEngine};
use models::*;
use scenarios::*;

md_tmpl_macros::template!(
    r#"---
params:
  - name = str
  - place = str
---
Hello {{ name }}, welcome to {{ place }}!"# => simple_macro
);

md_tmpl_macros::template!(
    r#"---
params:
  - items = list(label = str, value = int)
---
> {% for item in items %}

- {{ item.label }}: {{ item.value }}

> {% /for %}"# => loop_macro
);

md_tmpl_macros::template!(
    r#"---
params:
  - level = str
  - score = int
---
> {% if level == "high" %}

Rating: Excellent

> {% elif level == "medium" %}

Rating: Good (score {{ score }})

> {% else %}

Rating: Needs Improvement

> {% /if %}"# => conditional_macro
);

md_tmpl_macros::template!(
    r#"---
params:
  - title = str
  - sections = list(heading = str, entries = list(name = str, active = bool, score = float, tags = list(label = str)))
---
# {{ title }}

> {% for section in sections %}

## {{ section.heading }}

> {% for entry in section.entries %}

### {{ entry.name }}

> {% if entry.active %}

- Status: active
- Score: {{ entry.score | fixed(1) }}

> {% elif entry.score > 0 %}

- Status: inactive (score {{ entry.score | fixed(1) }})

> {% else %}

- Status: inactive

> {% /if %}
> {% for tag in entry.tags %}

  - tag: {{ tag.label }}

> {% /for %}
> {% /for %}
> {% /for %}"# => hero_macro
);

md_tmpl_macros::include_template!("templates/mega_macro.tmpl.md");

fn build_hero_macro_data(shared_data: &HeroReport) -> hero_macro::Params {
    hero_macro::Params {
        title: shared_data.title.clone(),
        sections: shared_data
            .sections
            .iter()
            .map(|s| hero_macro::ParamsSectionsItem {
                heading: s.heading.clone(),
                entries: s
                    .entries
                    .iter()
                    .map(|e| hero_macro::ParamsSectionsItemEntriesItem {
                        name: e.name.clone(),
                        active: e.active,
                        score: e.score,
                        tags: e
                            .tags
                            .iter()
                            .map(|t| hero_macro::ParamsSectionsItemEntriesItemTagsItem {
                                label: t.label.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn build_mega_macro_data(shared_data: &MegaReport) -> mega_macro::Params {
    mega_macro::Params {
        org: shared_data.org.clone(),
        teams: shared_data
            .teams
            .iter()
            .map(|t| mega_macro::ParamsTeamsItem {
                name: t.name.clone(),
                lead: t.lead.clone(),
                active: t.active,
                idx: t.idx.unwrap_or(0) as i64,
                members: t
                    .members
                    .iter()
                    .map(|m| mega_macro::ParamsTeamsItemMembersItem {
                        name: m.name.clone(),
                        role: m.role.clone(),
                        score: m.score,
                        skills: m
                            .skills
                            .iter()
                            .map(|s| mega_macro::ParamsTeamsItemMembersItemSkillsItem {
                                name: s.name.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

// ==========================================================================
// Scenario 1 — Simple variable substitution
// ==========================================================================

fn bench_simple(c: &mut Criterion) {
    let pt = MdTmplEngine::compile(simple::MD_TMPL);
    let tera = TeraEngine::compile(simple::TERA);
    let mj = MiniJinjaEngine::compile(simple::MINIJINJA);
    let hbs = HandlebarsEngine::compile(simple::HANDLEBARS);

    let data = simple_data();
    let macro_data = simple_macro::Params {
        name: data.name.clone(),
        place: data.place.clone(),
    };
    let pt_ctx = md_tmpl::Context::from_serialize(&data).unwrap();
    let tera_ctx = TeraEngine::context(&data);
    let json_ctx = serde_json::to_value(&data).unwrap();

    assert_engines_match(
        "simple",
        &pt.render_ctx(&pt_ctx),
        &tera.render_ctx(&tera_ctx),
        &mj.render(&data),
        &hbs.render(&json_ctx),
        Some(simple::EXPECTED),
    );
    assert_eq!(
        engines::normalize(&macro_data.render().unwrap()),
        engines::normalize(simple::EXPECTED)
    );

    let mut group = c.benchmark_group("simple");
    group.bench_function("md_tmpl_macro", |b| {
        b.iter(|| black_box(&macro_data).render().unwrap());
    });
    group.bench_function("md_tmpl", |b| {
        b.iter(|| pt.render_ctx(black_box(&pt_ctx)));
    });
    group.bench_function("tera", |b| {
        b.iter(|| tera.render_ctx(black_box(&tera_ctx)));
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| hbs.render(black_box(&json_ctx)));
    });
    group.finish();
}

// ==========================================================================
// Scenario 2 — Loop over a list
// ==========================================================================

fn bench_loop(c: &mut Criterion) {
    let pt = MdTmplEngine::compile(loop_scenario::MD_TMPL);
    let tera = TeraEngine::compile(loop_scenario::TERA);
    let mj = MiniJinjaEngine::compile(loop_scenario::MINIJINJA);
    let hbs = HandlebarsEngine::compile(loop_scenario::HANDLEBARS);

    let data = loop_data();
    let macro_data = loop_macro::Params {
        items: data
            .items
            .iter()
            .map(|i| loop_macro::ParamsItemsItem {
                label: i.label.clone(),
                value: i.value,
            })
            .collect(),
    };
    let pt_ctx = md_tmpl::Context::from_serialize(&data).unwrap();
    let tera_ctx = TeraEngine::context(&data);
    let json_ctx = serde_json::to_value(&data).unwrap();

    assert_engines_match(
        "loop",
        &pt.render_ctx(&pt_ctx),
        &tera.render_ctx(&tera_ctx),
        &mj.render(&data),
        &hbs.render(&json_ctx),
        Some(loop_scenario::EXPECTED),
    );
    assert_eq!(
        engines::normalize(&macro_data.render().unwrap()),
        engines::normalize(loop_scenario::EXPECTED)
    );

    let mut group = c.benchmark_group("loop");
    group.bench_function("md_tmpl_macro", |b| {
        b.iter(|| black_box(&macro_data).render().unwrap());
    });
    group.bench_function("md_tmpl", |b| {
        b.iter(|| pt.render_ctx(black_box(&pt_ctx)));
    });
    group.bench_function("tera", |b| {
        b.iter(|| tera.render_ctx(black_box(&tera_ctx)));
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| hbs.render(black_box(&json_ctx)));
    });
    group.finish();
}

// ==========================================================================
// Scenario 3 — Conditional branching
// ==========================================================================

fn bench_conditional(c: &mut Criterion) {
    let pt = MdTmplEngine::compile(conditional::MD_TMPL);
    let tera = TeraEngine::compile(conditional::TERA);
    let mj = MiniJinjaEngine::compile(conditional::MINIJINJA);
    let hbs = HandlebarsEngine::compile(conditional::HANDLEBARS);

    let shared_data = conditional_data();
    let strict_data = conditional_strict_data();
    let macro_data = conditional_macro::Params {
        level: strict_data.level.clone(),
        score: strict_data.score,
    };

    let pt_ctx = md_tmpl::Context::from_serialize(&strict_data).unwrap();
    let tera_ctx = TeraEngine::context(&shared_data);
    let json_ctx = serde_json::to_value(&shared_data).unwrap();

    assert_engines_match(
        "conditional",
        &pt.render_ctx(&pt_ctx),
        &tera.render_ctx(&tera_ctx),
        &mj.render(&shared_data),
        &hbs.render(&json_ctx),
        Some(conditional::EXPECTED),
    );
    assert_eq!(
        engines::normalize(&macro_data.render().unwrap()),
        engines::normalize(conditional::EXPECTED)
    );

    let mut group = c.benchmark_group("conditional");
    group.bench_function("md_tmpl_macro", |b| {
        b.iter(|| black_box(&macro_data).render().unwrap());
    });
    group.bench_function("md_tmpl", |b| {
        b.iter(|| pt.render_ctx(black_box(&pt_ctx)));
    });
    group.bench_function("tera", |b| {
        b.iter(|| tera.render_ctx(black_box(&tera_ctx)));
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&shared_data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| hbs.render(black_box(&json_ctx)));
    });
    group.finish();
}

// ==========================================================================
// Scenario 4 — Hero: nested loops + conditionals
// ==========================================================================

fn bench_hero(c: &mut Criterion) {
    let pt = MdTmplEngine::compile(hero::MD_TMPL);
    let tera = TeraEngine::compile(hero::TERA);
    let mj = MiniJinjaEngine::compile(hero::MINIJINJA);
    let hbs = HandlebarsEngine::compile(hero::HANDLEBARS);

    let shared_data = hero_data();
    let strict_data = hero_strict_data();
    let macro_data = build_hero_macro_data(&shared_data);

    let pt_ctx = md_tmpl::Context::from_serialize(&strict_data).unwrap();
    let tera_ctx = TeraEngine::context(&shared_data);
    let json_ctx = serde_json::to_value(&shared_data).unwrap();

    let rendered = pt.render_ctx(&pt_ctx);
    assert_engines_match(
        "hero",
        &rendered,
        &tera.render_ctx(&tera_ctx),
        &mj.render(&shared_data),
        &hbs.render(&json_ctx),
        None,
    );
    assert_eq!(
        engines::normalize(&macro_data.render().unwrap()),
        engines::normalize(&rendered)
    );

    let mut group = c.benchmark_group("hero");
    group.bench_function("md_tmpl_macro", |b| {
        b.iter(|| black_box(&macro_data).render().unwrap());
    });
    group.bench_function("md_tmpl", |b| {
        b.iter(|| pt.render_ctx(black_box(&pt_ctx)));
    });
    group.bench_function("tera", |b| {
        b.iter(|| tera.render_ctx(black_box(&tera_ctx)));
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&shared_data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| hbs.render(black_box(&json_ctx)));
    });
    group.finish();
}

// ==========================================================================
// Scenario 5 — Mega: large data, deep nesting, idx, filters
// ==========================================================================

fn bench_mega(c: &mut Criterion) {
    let pt = MdTmplEngine::compile(mega::MD_TMPL);
    let tera = TeraEngine::compile(mega::TERA);
    let mj = MiniJinjaEngine::compile(mega::MINIJINJA);
    let hbs = HandlebarsEngine::compile(mega::HANDLEBARS);

    let shared_data = mega_data();
    let strict_data = mega_strict_data();
    let macro_data = build_mega_macro_data(&shared_data);

    let pt_ctx = md_tmpl::Context::from_serialize(&strict_data).unwrap();
    let tera_ctx = TeraEngine::context(&shared_data);
    let json_ctx = serde_json::to_value(&shared_data).unwrap();

    let rendered = pt.render_ctx(&pt_ctx);
    assert_engines_match(
        "mega",
        &rendered,
        &tera.render_ctx(&tera_ctx),
        &mj.render(&shared_data),
        &hbs.render(&json_ctx),
        None,
    );
    assert_eq!(
        engines::normalize(&macro_data.render().unwrap()),
        engines::normalize(&rendered)
    );

    let mut group = c.benchmark_group("mega");
    group.bench_function("md_tmpl_macro", |b| {
        b.iter(|| black_box(&macro_data).render().unwrap());
    });
    group.bench_function("md_tmpl", |b| {
        b.iter(|| pt.render_ctx(black_box(&pt_ctx)));
    });
    group.bench_function("tera", |b| {
        b.iter(|| tera.render_ctx(black_box(&tera_ctx)));
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&shared_data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| hbs.render(black_box(&json_ctx)));
    });
    group.finish();
}

// ==========================================================================
// End-to-end benchmarks — include context construction in the hot loop.
// ==========================================================================

fn bench_hero_e2e(c: &mut Criterion) {
    let pt = MdTmplEngine::compile(hero::MD_TMPL);
    let tera = TeraEngine::compile(hero::TERA);
    let mj = MiniJinjaEngine::compile(hero::MINIJINJA);
    let hbs = HandlebarsEngine::compile(hero::HANDLEBARS);

    let shared_data = hero_data();
    let strict_data = hero_strict_data();
    let macro_data = build_hero_macro_data(&shared_data);

    let mut group = c.benchmark_group("hero_e2e");
    group.bench_function("md_tmpl_macro", |b| {
        b.iter(|| black_box(&macro_data).render().unwrap());
    });
    group.bench_function("md_tmpl", |b| {
        b.iter(|| {
            let ctx = md_tmpl::Context::from_serialize(black_box(&strict_data)).unwrap();
            pt.render_ctx(&ctx)
        });
    });
    group.bench_function("tera", |b| {
        b.iter(|| {
            let ctx = TeraEngine::context(black_box(&shared_data));
            tera.render_ctx(&ctx)
        });
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&shared_data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| {
            let json = serde_json::to_value(black_box(&shared_data)).unwrap();
            hbs.render(&json)
        });
    });
    group.finish();
}

fn bench_mega_e2e(c: &mut Criterion) {
    let pt = MdTmplEngine::compile(mega::MD_TMPL);
    let tera = TeraEngine::compile(mega::TERA);
    let mj = MiniJinjaEngine::compile(mega::MINIJINJA);
    let hbs = HandlebarsEngine::compile(mega::HANDLEBARS);

    let shared_data = mega_data();
    let strict_data = mega_strict_data();
    let macro_data = build_mega_macro_data(&shared_data);

    let mut group = c.benchmark_group("mega_e2e");
    group.bench_function("md_tmpl_macro", |b| {
        b.iter(|| black_box(&macro_data).render().unwrap());
    });
    group.bench_function("md_tmpl", |b| {
        b.iter(|| {
            let ctx = md_tmpl::Context::from_serialize(black_box(&strict_data)).unwrap();
            pt.render_ctx(&ctx)
        });
    });
    group.bench_function("tera", |b| {
        b.iter(|| {
            let ctx = TeraEngine::context(black_box(&shared_data));
            tera.render_ctx(&ctx)
        });
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&shared_data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| {
            let json = serde_json::to_value(black_box(&shared_data)).unwrap();
            hbs.render(&json)
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_simple,
    bench_loop,
    bench_conditional,
    bench_hero,
    bench_mega,
    bench_hero_e2e,
    bench_mega_e2e
);
criterion_main!(benches);
