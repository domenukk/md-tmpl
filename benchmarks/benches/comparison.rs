//! Criterion benchmarks comparing prompt-templates against Tera, MiniJinja,
//! and Handlebars across four scenarios of increasing complexity.
//!
//! **Scenarios**:
//! 1. **Simple** — plain variable substitution
//! 2. **Loop** — iterating over a list of items
//! 3. **Conditional** — if / elif / else branching
//! 4. **Hero** — nested loops + conditionals (realistic)
//!
//! Each scenario pre-compiles templates and benchmarks only render time.
//! Before benchmarking, a correctness test asserts all four engines produce
//! identical output.
//!
//! ## Methodology notes
//!
//! - **MiniJinja** accepts `&impl Serialize` directly, so its `.render(&data)`
//!   re-serializes the data struct on every iteration, while the other engines
//!   use pre-built context objects. The `hero_e2e` / `mega_e2e` benchmarks
//!   include context construction in the hot loop for all engines to level
//!   this playing field.
//!
//! - **Handlebars** lacks string equality, numeric comparisons, and filters.
//!   The data structs include pre-computed boolean flags (e.g. `is_high`,
//!   `has_positive_score`) and pre-formatted strings (e.g. `score_fmt`)
//!   so Handlebars can produce equivalent output — this means Handlebars
//!   does slightly less per-render work than the other engines.
//!
//! - **prompt-templates** uses `render_ctx_allowing_extra()` because the
//!   shared data structs carry fields for other engines. This skips strict
//!   unknown-field rejection but still performs type validation.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use handlebars::Handlebars;
use minijinja::Environment;
use prompt_templates::Template;

// ==========================================================================
// Constants: template name used for registration
// ==========================================================================

const TEMPLATE_NAME: &str = "bench";

// ==========================================================================
// Scenario 1 — Simple variable substitution
// ==========================================================================

mod simple {
    pub const PROMPT_TEMPLATES: &str = "\
---
params:
  - name = str
  - place = str
---
Hello {{ name }}, welcome to {{ place }}!";

    pub const TERA: &str = "Hello {{ name }}, welcome to {{ place }}!";

    pub const MINIJINJA: &str = "Hello {{ name }}, welcome to {{ place }}!";

    // Triple-stache to avoid HTML escaping.
    pub const HANDLEBARS: &str = "Hello {{{name}}}, welcome to {{{place}}}!";

    pub const EXPECTED: &str = "Hello Alice, welcome to Wonderland!";
}

// ==========================================================================
// Scenario 2 — Loop over a list
// ==========================================================================

mod loop_scenario {
    pub const PROMPT_TEMPLATES: &str = "\
---
params:
  - items = list(label = str, value = int)
---
> {% for item in items %}

- {{ item.label }}: {{ item.value }}

> {% /for %}";

    pub const TERA: &str = "\
{% for item in items %}\
- {{ item.label }}: {{ item.value }}
{% endfor %}";

    pub const MINIJINJA: &str = "\
{% for item in items %}\
- {{ item.label }}: {{ item.value }}
{% endfor %}";

    // Triple-stache to avoid HTML escaping.
    pub const HANDLEBARS: &str = "\
{{#each items}}\
- {{{this.label}}}: {{{this.value}}}
{{/each}}";

    pub const EXPECTED: &str = "\
- Alpha: 10
- Beta: 20
- Gamma: 30
";
}

// ==========================================================================
// Scenario 3 — Conditional branching (if / elif / else)
// ==========================================================================

mod conditional {
    pub const PROMPT_TEMPLATES: &str = "\
---
params:
  - level = str
  - score = int
---
> {% if level == \"high\" %}

Rating: Excellent

> {% elif level == \"medium\" %}

Rating: Good (score {{ score }})

> {% else %}

Rating: Needs Improvement

> {% /if %}";

    pub const TERA: &str = "\
{% if level == \"high\" %}\
Rating: Excellent
{% elif level == \"medium\" %}\
Rating: Good (score {{ score }})
{% else %}\
Rating: Needs Improvement
{% endif %}";

    pub const MINIJINJA: &str = "\
{% if level == \"high\" %}\
Rating: Excellent
{% elif level == \"medium\" %}\
Rating: Good (score {{ score }})
{% else %}\
Rating: Needs Improvement
{% endif %}";

    // Handlebars has no elif — use nested if/else.
    pub const HANDLEBARS: &str = "\
{{#if is_high}}\
Rating: Excellent
{{else}}{{#if is_medium}}\
Rating: Good (score {{{score}}})
{{else}}\
Rating: Needs Improvement
{{/if}}{{/if}}";

    pub const EXPECTED: &str = "Rating: Good (score 75)\n";
}

// ==========================================================================
// Scenario 4 — Hero: nested loops + conditionals
// ==========================================================================

mod hero {
    pub const PROMPT_TEMPLATES: &str = "\
---
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
> {% /for %}";

    pub const TERA: &str = "\
# {{ title }}

{% for section in sections %}\
## {{ section.heading }}

{% for entry in section.entries %}\
### {{ entry.name }}

{% if entry.active %}\
- Status: active
- Score: {{ entry.score_fmt }}
{%- elif entry.has_positive_score %}\
- Status: inactive (score {{ entry.score_fmt }})
{%- else %}\
- Status: inactive
{%- endif %}
{%- for tag in entry.tags %}
  - tag: {{ tag.label }}
{%- endfor %}
{% endfor %}\
{% endfor %}";

    pub const MINIJINJA: &str = "\
# {{ title }}

{% for section in sections %}\
## {{ section.heading }}

{% for entry in section.entries %}\
### {{ entry.name }}

{% if entry.active %}\
- Status: active
- Score: {{ entry.score_fmt }}
{%- elif entry.has_positive_score %}\
- Status: inactive (score {{ entry.score_fmt }})
{%- else %}\
- Status: inactive
{%- endif %}
{%- for tag in entry.tags %}
  - tag: {{ tag.label }}
{%- endfor %}
{% endfor %}\
{% endfor %}";

    // Handlebars: no elif, no filters — pass pre-formatted scores.
    // Handlebars `~` whitespace control strips too aggressively, so we
    // rely on `no_escape` mode and careful line breaks instead.
    pub const HANDLEBARS: &str =
        "# {{title}}\n\n\
         {{#each sections}}## {{this.heading}}\n\n\
         {{#each this.entries}}### {{this.name}}\n\n\
         {{#if this.active}}\
         - Status: active\n\
         - Score: {{this.score_fmt}}\n\
         {{else}}\
         {{#if this.has_positive_score}}\
         - Status: inactive (score {{this.score_fmt}})\n\
         {{else}}\
         - Status: inactive\n\
         {{/if}}\
         {{/if}}\
         {{#each this.tags}}  - tag: {{this.label}}\n{{/each}}\
         {{/each}}\
         {{/each}}";
}

// ==========================================================================
// Context builders
// ==========================================================================

// ==========================================================================
// Shared data models — one struct per scenario, used by all engines via serde
// ==========================================================================

// -- Simple --

#[derive(serde::Serialize)]
struct SimpleData {
    name: String,
    place: String,
}

fn simple_data() -> SimpleData {
    SimpleData {
        name: "Alice".into(),
        place: "Wonderland".into(),
    }
}

// -- Loop --

#[derive(serde::Serialize)]
struct LoopData {
    items: Vec<LoopItem>,
}

#[derive(serde::Serialize)]
struct LoopItem {
    label: String,
    value: i64,
}

fn loop_data() -> LoopData {
    LoopData {
        items: vec![
            LoopItem { label: "Alpha".into(), value: 10 },
            LoopItem { label: "Beta".into(),  value: 20 },
            LoopItem { label: "Gamma".into(), value: 30 },
        ],
    }
}

// -- Conditional --

#[derive(serde::Serialize)]
struct ConditionalData {
    level: String,
    score: i64,
    /// Handlebars can't do string equality — needs boolean flags.
    is_high: bool,
    is_medium: bool,
}

fn conditional_data() -> ConditionalData {
    ConditionalData {
        level: "medium".into(),
        score: 75,
        is_high: false,
        is_medium: true,
    }
}

// -- Hero --

#[derive(serde::Serialize)]
struct HeroReport {
    title: String,
    sections: Vec<HeroSection>,
}

#[derive(serde::Serialize)]
struct HeroSection {
    heading: String,
    entries: Vec<HeroEntry>,
}

#[derive(serde::Serialize)]
struct HeroEntry {
    name: String,
    active: bool,
    score: f64,
    /// Pre-formatted score for engines without `fixed()` filter.
    score_fmt: String,
    /// Pre-computed flag for engines without numeric comparison.
    has_positive_score: bool,
    tags: Vec<HeroTag>,
}

#[derive(serde::Serialize)]
struct HeroTag {
    label: String,
}

impl HeroEntry {
    fn new(name: &str, active: bool, score: f64, tags: &[&str]) -> Self {
        Self {
            name: name.into(),
            active,
            score,
            score_fmt: format!("{score:.1}"),
            has_positive_score: score > 0.0,
            tags: tags.iter().map(|t| HeroTag { label: t.to_string() }).collect(),
        }
    }
}

fn hero_data() -> HeroReport {
    HeroReport {
        title: "System Report".into(),
        sections: vec![
            HeroSection {
                heading: "Overview".into(),
                entries: vec![
                    HeroEntry::new("Service-A", true, 98.7, &["prod", "critical"]),
                    HeroEntry::new("Service-B", false, 45.2, &["staging"]),
                    HeroEntry::new("Service-C", false, 0.0, &["deprecated"]),
                ],
            },
            HeroSection {
                heading: "Metrics".into(),
                entries: vec![
                    HeroEntry::new("Latency", true, 12.3, &["p99"]),
                    HeroEntry::new("Throughput", false, 0.0, &["batch"]),
                ],
            },
        ],
    }
}

// ==========================================================================
// Engine wrappers — pre-compiled template holders
// ==========================================================================

struct PromptTemplatesEngine {
    template: Template,
}

impl PromptTemplatesEngine {
    fn compile(source: &str) -> Self {
        Self {
            template: Template::from_source(source)
                .expect("prompt-templates: failed to compile template"),
        }
    }

    fn render(&self, ctx: &prompt_templates::Context) -> String {
        // Use render_allowing_extra since shared structs may carry fields
        // needed by other engines (e.g. score_fmt, is_high).
        self.template
            .render_ctx_allowing_extra(ctx)
            .expect("prompt-templates: render failed")
    }
}

struct TeraEngine {
    engine: tera::Tera,
}

impl TeraEngine {
    fn compile(source: &str) -> Self {
        let mut engine = tera::Tera::default();
        engine
            .add_raw_template(TEMPLATE_NAME, source)
            .expect("tera: failed to compile template");
        Self { engine }
    }

    /// Build a reusable Tera context from any Serialize type.
    fn context(data: &impl serde::Serialize) -> tera::Context {
        tera::Context::from_serialize(data).expect("tera: failed to serialize context")
    }

    /// Render with a pre-built context (used in benchmark loops).
    fn render_ctx(&self, ctx: &tera::Context) -> String {
        self.engine
            .render(TEMPLATE_NAME, ctx)
            .expect("tera: render failed")
    }
}

struct MiniJinjaEngine {
    env: Environment<'static>,
}

impl MiniJinjaEngine {
    fn compile(source: &'static str) -> Self {
        let mut env = Environment::new();
        env.add_template_owned(TEMPLATE_NAME.to_owned(), source.to_owned())
            .expect("minijinja: failed to compile template");
        Self { env }
    }

    /// Render directly from any Serialize type — MiniJinja's optimal path.
    fn render(&self, data: &impl serde::Serialize) -> String {
        let tmpl = self
            .env
            .get_template(TEMPLATE_NAME)
            .expect("minijinja: template not found");
        tmpl.render(data)
            .expect("minijinja: render failed")
    }
}

struct HandlebarsEngine {
    registry: Handlebars<'static>,
}

impl HandlebarsEngine {
    fn compile(source: &str) -> Self {
        let mut registry = Handlebars::new();
        registry.set_strict_mode(true);
        // Disable HTML escaping — we produce plain text.
        registry.register_escape_fn(handlebars::no_escape);
        registry
            .register_template_string(TEMPLATE_NAME, source)
            .expect("handlebars: failed to compile template");
        Self { registry }
    }

    fn render(&self, data: &serde_json::Value) -> String {
        self.registry
            .render(TEMPLATE_NAME, data)
            .expect("handlebars: render failed")
    }
}

// ==========================================================================
// Correctness assertions
// ==========================================================================

fn normalize(s: &str) -> String {
    s.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Assert that all four engines produce the same output for a scenario.
/// `expected` is optional — if `Some`, also verify against the known value.
fn assert_engines_match(
    scenario: &str,
    pt_output: &str,
    tera_output: &str,
    mj_output: &str,
    hbs_output: &str,
    expected: Option<&str>,
) {
    let pt = normalize(pt_output);
    let tera = normalize(tera_output);
    let mj = normalize(mj_output);
    let hbs = normalize(hbs_output);

    assert_eq!(
        pt, tera,
        "[{scenario}] prompt-templates vs tera mismatch:\nPT:\n{pt}\n\nTERA:\n{tera}"
    );
    assert_eq!(
        pt, mj,
        "[{scenario}] prompt-templates vs minijinja mismatch:\nPT:\n{pt}\n\nMJ:\n{mj}"
    );
    assert_eq!(
        pt, hbs,
        "[{scenario}] prompt-templates vs handlebars mismatch:\nPT:\n{pt}\n\nHBS:\n{hbs}"
    );

    if let Some(exp) = expected {
        let exp_norm = normalize(exp);
        assert_eq!(
            pt, exp_norm,
            "[{scenario}] output does not match expected:\nGOT:\n{pt}\n\nEXPECTED:\n{exp_norm}"
        );
    }
}

// ==========================================================================
// Benchmark groups
// ==========================================================================

fn bench_simple(c: &mut Criterion) {
    let pt = PromptTemplatesEngine::compile(simple::PROMPT_TEMPLATES);
    let tera = TeraEngine::compile(simple::TERA);
    let mj = MiniJinjaEngine::compile(simple::MINIJINJA);
    let hbs = HandlebarsEngine::compile(simple::HANDLEBARS);

    let data = simple_data();
    let pt_ctx = prompt_templates::Context::from_serialize(&data).unwrap();
    let tera_ctx = TeraEngine::context(&data);
    let json_ctx = serde_json::to_value(&data).unwrap();

    assert_engines_match(
        "simple",
        &pt.render(&pt_ctx),
        &tera.render_ctx(&tera_ctx),
        &mj.render(&data),
        &hbs.render(&json_ctx),
        Some(simple::EXPECTED),
    );

    let mut group = c.benchmark_group("simple");
    group.bench_function("prompt_templates", |b| {
        b.iter(|| pt.render(black_box(&pt_ctx)));
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

fn bench_loop(c: &mut Criterion) {
    let pt = PromptTemplatesEngine::compile(loop_scenario::PROMPT_TEMPLATES);
    let tera = TeraEngine::compile(loop_scenario::TERA);
    let mj = MiniJinjaEngine::compile(loop_scenario::MINIJINJA);
    let hbs = HandlebarsEngine::compile(loop_scenario::HANDLEBARS);

    let data = loop_data();
    let pt_ctx = prompt_templates::Context::from_serialize(&data).unwrap();
    let tera_ctx = TeraEngine::context(&data);
    let json_ctx = serde_json::to_value(&data).unwrap();

    assert_engines_match(
        "loop",
        &pt.render(&pt_ctx),
        &tera.render_ctx(&tera_ctx),
        &mj.render(&data),
        &hbs.render(&json_ctx),
        Some(loop_scenario::EXPECTED),
    );

    let mut group = c.benchmark_group("loop");
    group.bench_function("prompt_templates", |b| {
        b.iter(|| pt.render(black_box(&pt_ctx)));
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

fn bench_conditional(c: &mut Criterion) {
    let pt = PromptTemplatesEngine::compile(conditional::PROMPT_TEMPLATES);
    let tera = TeraEngine::compile(conditional::TERA);
    let mj = MiniJinjaEngine::compile(conditional::MINIJINJA);
    let hbs = HandlebarsEngine::compile(conditional::HANDLEBARS);

    let data = conditional_data();
    let pt_ctx = prompt_templates::Context::from_serialize(&data).unwrap();
    let tera_ctx = TeraEngine::context(&data);
    let json_ctx = serde_json::to_value(&data).unwrap();

    assert_engines_match(
        "conditional",
        &pt.render(&pt_ctx),
        &tera.render_ctx(&tera_ctx),
        &mj.render(&data),
        &hbs.render(&json_ctx),
        Some(conditional::EXPECTED),
    );

    let mut group = c.benchmark_group("conditional");
    group.bench_function("prompt_templates", |b| {
        b.iter(|| pt.render(black_box(&pt_ctx)));
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

fn bench_hero(c: &mut Criterion) {
    let pt = PromptTemplatesEngine::compile(hero::PROMPT_TEMPLATES);
    let tera = TeraEngine::compile(hero::TERA);
    let mj = MiniJinjaEngine::compile(hero::MINIJINJA);
    let hbs = HandlebarsEngine::compile(hero::HANDLEBARS);

    let data = hero_data();
    let pt_ctx = prompt_templates::Context::from_serialize(&data).unwrap();
    let tera_ctx = TeraEngine::context(&data);
    let json_ctx = serde_json::to_value(&data).unwrap();

    assert_engines_match(
        "hero",
        &pt.render(&pt_ctx),
        &tera.render_ctx(&tera_ctx),
        &mj.render(&data),
        &hbs.render(&json_ctx),
        None,
    );

    let mut group = c.benchmark_group("hero");
    group.bench_function("prompt_templates", |b| {
        b.iter(|| pt.render(black_box(&pt_ctx)));
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
// Scenario 5 — Mega: large data, deep nesting, idx, filters
// ==========================================================================

mod mega {
    pub const PROMPT_TEMPLATES: &str = "\
---
params:
  - org = str
  - teams = list(name = str, lead = str, active = bool, idx = int, members = list(name = str, role = str, score = float, skills = list(name = str)))
---
# {{ org }} Organization Report

> {% for team in teams %}

## {{ team.idx }}. {{ team.name }}

Lead: {{ team.lead }}

> {% if team.active %}

Status: ACTIVE

> {% else %}

Status: INACTIVE

> {% /if %}

> {% for member in team.members %}

### {{ member.name }} ({{ member.role }})

Score: {{ member.score | fixed(1) }}

> {% if member.score > 90 %}

Rating: Outstanding

> {% elif member.score > 70 %}

Rating: Good

> {% elif member.score > 50 %}

Rating: Average

> {% else %}

Rating: Needs Improvement

> {% /if %}

Skills:

> {% for skill in member.skills %}

  - {{ skill.name }}

> {% /for %}
> {% /for %}

---
> {% /for %}";

    pub const TERA: &str = "\
# {{ org }} Organization Report

{% for team in teams %}\
## {{ loop.index }}. {{ team.name }}

Lead: {{ team.lead }}
{% if team.active %}\
Status: ACTIVE
{% else %}\
Status: INACTIVE
{% endif %}
{% for member in team.members %}\
### {{ member.name }} ({{ member.role }})

Score: {{ member.score_fmt }}
{% if member.rating == \"outstanding\" %}\
Rating: Outstanding
{% elif member.rating == \"good\" %}\
Rating: Good
{% elif member.rating == \"average\" %}\
Rating: Average
{% else %}\
Rating: Needs Improvement
{% endif %}
Skills:
{%- for skill in member.skills %}
  - {{ skill.name }}
{%- endfor %}
{% endfor %}\
---
{% endfor %}";

    pub const MINIJINJA: &str = "\
# {{ org }} Organization Report

{% for team in teams %}\
## {{ loop.index }}. {{ team.name }}

Lead: {{ team.lead }}
{% if team.active %}\
Status: ACTIVE
{% else %}\
Status: INACTIVE
{% endif %}
{% for member in team.members %}\
### {{ member.name }} ({{ member.role }})

Score: {{ member.score_fmt }}
{% if member.rating == \"outstanding\" %}\
Rating: Outstanding
{% elif member.rating == \"good\" %}\
Rating: Good
{% elif member.rating == \"average\" %}\
Rating: Average
{% else %}\
Rating: Needs Improvement
{% endif %}
Skills:
{%- for skill in member.skills %}
  - {{ skill.name }}
{%- endfor %}
{% endfor %}\
---
{% endfor %}";


    pub const HANDLEBARS: &str = "\
# {{{org}}} Organization Report

{{#each teams}}\
## {{{this.idx}}}. {{{this.name}}}

Lead: {{{this.lead}}}
{{#if this.active}}\
Status: ACTIVE
{{else}}\
Status: INACTIVE
{{/if}}
{{#each this.members}}\
### {{{this.name}}} ({{{this.role}}})

Score: {{{this.score_fmt}}}
{{#if this.is_outstanding}}\
Rating: Outstanding
{{else}}{{#if this.is_good}}\
Rating: Good
{{else}}{{#if this.is_average}}\
Rating: Average
{{else}}\
Rating: Needs Improvement
{{/if}}{{/if}}{{/if}}
Skills:
{{#each this.skills}}
  - {{{this.name}}}
{{/each}}
{{/each}}\
---
{{/each}}";
}

/// Shared data model — used by all engines via serde.
#[derive(serde::Serialize, Clone)]
struct MegaReport {
    org: String,
    teams: Vec<MegaTeam>,
}

#[derive(serde::Serialize, Clone)]
struct MegaTeam {
    name: String,
    lead: String,
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    idx: Option<usize>,
    members: Vec<MegaMember>,
}

#[derive(serde::Serialize, Clone)]
struct MegaMember {
    name: String,
    role: String,
    score: f64,
    /// Pre-formatted score for engines without `fixed()` filter.
    score_fmt: String,
    /// Pre-computed rating string for engines without numeric comparison.
    rating: String,
    /// Handlebars-friendly boolean flags.
    is_outstanding: bool,
    is_good: bool,
    is_average: bool,
    skills: Vec<MegaSkill>,
}

#[derive(serde::Serialize, Clone)]
struct MegaSkill {
    name: String,
}

impl MegaMember {
    fn new(name: &str, role: &str, score: f64, skills: &[&str]) -> Self {
        let rating = if score > 90.0 {
            "outstanding"
        } else if score > 70.0 {
            "good"
        } else if score > 50.0 {
            "average"
        } else {
            "needs_improvement"
        };
        Self {
            name: name.into(),
            role: role.into(),
            score,
            score_fmt: format!("{score:.1}"),
            rating: rating.into(),
            is_outstanding: rating == "outstanding",
            is_good: rating == "good",
            is_average: rating == "average",
            skills: skills.iter().map(|s| MegaSkill { name: s.to_string() }).collect(),
        }
    }
}

impl MegaTeam {
    fn new(name: &str, lead: &str, active: bool, idx: usize, members: Vec<MegaMember>) -> Self {
        Self {
            name: name.into(),
            lead: lead.into(),
            active,
            idx: Some(idx),
            members,
        }
    }
}

fn mega_data() -> MegaReport {
    MegaReport {
        org: "Acme Corp".into(),
        teams: vec![
            MegaTeam::new("Backend", "Alice", true, 1, vec![
                MegaMember::new("Bob", "Senior", 95.5, &["Rust", "Go", "SQL"]),
                MegaMember::new("Carol", "Mid", 78.3, &["Python", "Docker"]),
                MegaMember::new("Dave", "Junior", 62.1, &["JavaScript", "HTML"]),
                MegaMember::new("Eve", "Senior", 91.0, &["Java", "Kotlin", "gRPC"]),
                MegaMember::new("Frank", "Intern", 45.0, &["Python"]),
            ]),
            MegaTeam::new("Frontend", "Grace", true, 2, vec![
                MegaMember::new("Heidi", "Senior", 88.7, &["React", "TypeScript", "CSS"]),
                MegaMember::new("Ivan", "Mid", 71.2, &["Vue", "JavaScript"]),
                MegaMember::new("Judy", "Junior", 55.0, &["HTML", "CSS"]),
                MegaMember::new("Karl", "Senior", 92.4, &["Angular", "RxJS", "SCSS"]),
                MegaMember::new("Liam", "Mid", 68.9, &["Svelte"]),
            ]),
            MegaTeam::new("SRE", "Mallory", false, 3, vec![
                MegaMember::new("Nancy", "Senior", 97.1, &["Kubernetes", "Terraform", "Go"]),
                MegaMember::new("Oscar", "Mid", 73.5, &["Ansible", "Bash"]),
                MegaMember::new("Peggy", "Junior", 51.2, &["Linux"]),
                MegaMember::new("Quentin", "Senior", 89.3, &["Prometheus", "Grafana"]),
                MegaMember::new("Ruth", "Intern", 38.0, &["Python"]),
            ]),
        ],
    }
}

fn bench_mega(c: &mut Criterion) {
    let pt = PromptTemplatesEngine::compile(mega::PROMPT_TEMPLATES);
    let tera = TeraEngine::compile(mega::TERA);
    let mj = MiniJinjaEngine::compile(mega::MINIJINJA);
    let hbs = HandlebarsEngine::compile(mega::HANDLEBARS);

    let data = mega_data();
    let pt_ctx = prompt_templates::Context::from_serialize(&data)
        .expect("prompt-templates: serde context failed");
    let tera_ctx = TeraEngine::context(&data);
    let json_ctx = serde_json::to_value(&data)
        .expect("serde_json: serialization failed");

    // Correctness — all engines must produce the same output.
    assert_engines_match(
        "mega",
        &pt.render(&pt_ctx),
        &tera.render_ctx(&tera_ctx),
        &mj.render(&data),
        &hbs.render(&json_ctx),
        None,
    );

    let mut group = c.benchmark_group("mega");
    group.bench_function("prompt_templates", |b| {
        b.iter(|| pt.render(black_box(&pt_ctx)));
    });

    // --- Macro (Compile-time) Benchmark ---
    // The consolidated `include_template!` generates both the pre-compiled
    // template AND the `Params` struct that directly converts its fields
    // into a `Context` without using Serde, which is faster.
    prompt_templates_macros::include_template!("templates/mega_macro.tmpl.md");
    let macro_tmpl = mega_macro::template();

    // Map our mega_data into the macro-generated struct.
    let macro_data = mega_macro::Params {
        org: data.org.clone(),
        teams: data.teams.iter().map(|t| mega_macro::ParamsTeamsItem {
            name: t.name.clone(),
            lead: t.lead.clone(),
            active: t.active,
            idx: t.idx.unwrap_or(0) as i64,
            members: t.members.iter().map(|m| mega_macro::ParamsTeamsItemMembersItem {
                name: m.name.clone(),
                role: m.role.clone(),
                score: m.score,
                skills: m.skills.iter().map(|s| mega_macro::ParamsTeamsItemMembersItemSkillsItem {
                    name: s.name.clone(),
                }).collect(),
            }).collect(),
        }).collect(),
    };

    let macro_ctx = macro_data.to_context();
    group.bench_function("prompt_templates_macro", |b| {
        b.iter(|| macro_tmpl.render_ctx(black_box(&macro_ctx)));
    });
    // -------------------------------------

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
// End-to-end benchmarks — include context construction in the hot loop.
//
// These measure the full pipeline: struct → context → render → String.
// MiniJinja already does this (it takes &impl Serialize directly), so
// these benchmarks level the playing field for all engines.
// ==========================================================================

fn bench_hero_e2e(c: &mut Criterion) {
    let pt = PromptTemplatesEngine::compile(hero::PROMPT_TEMPLATES);
    let tera = TeraEngine::compile(hero::TERA);
    let mj = MiniJinjaEngine::compile(hero::MINIJINJA);
    let hbs = HandlebarsEngine::compile(hero::HANDLEBARS);
    let data = hero_data();

    let mut group = c.benchmark_group("hero_e2e");
    group.bench_function("prompt_templates", |b| {
        b.iter(|| {
            let ctx = prompt_templates::Context::from_serialize(black_box(&data)).unwrap();
            pt.render(&ctx)
        });
    });
    group.bench_function("tera", |b| {
        b.iter(|| {
            let ctx = TeraEngine::context(black_box(&data));
            tera.render_ctx(&ctx)
        });
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| {
            let json = serde_json::to_value(black_box(&data)).unwrap();
            hbs.render(&json)
        });
    });
    group.finish();
}

fn bench_mega_e2e(c: &mut Criterion) {
    let pt = PromptTemplatesEngine::compile(mega::PROMPT_TEMPLATES);
    let tera = TeraEngine::compile(mega::TERA);
    let mj = MiniJinjaEngine::compile(mega::MINIJINJA);
    let hbs = HandlebarsEngine::compile(mega::HANDLEBARS);
    let data = mega_data();

    let mut group = c.benchmark_group("mega_e2e");
    group.bench_function("prompt_templates", |b| {
        b.iter(|| {
            let ctx = prompt_templates::Context::from_serialize(black_box(&data)).unwrap();
            pt.render(&ctx)
        });
    });
    group.bench_function("tera", |b| {
        b.iter(|| {
            let ctx = TeraEngine::context(black_box(&data));
            tera.render_ctx(&ctx)
        });
    });
    group.bench_function("minijinja", |b| {
        b.iter(|| mj.render(black_box(&data)));
    });
    group.bench_function("handlebars", |b| {
        b.iter(|| {
            let json = serde_json::to_value(black_box(&data)).unwrap();
            hbs.render(&json)
        });
    });
    group.finish();
}

criterion_group!(benches, bench_simple, bench_loop, bench_conditional, bench_hero, bench_mega, bench_hero_e2e, bench_mega_e2e);
criterion_main!(benches);

// ==========================================================================
// Tests — run via `cargo test --benches`
// ==========================================================================

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    /// Helper: build pt context + tera context + json value from a shared
    /// Serialize struct.
    #[allow(dead_code)]
    fn contexts<T: serde::Serialize>(
        data: &T,
    ) -> (prompt_templates::Context, tera::Context, serde_json::Value) {
        let pt = prompt_templates::Context::from_serialize(data).unwrap();
        let tera = TeraEngine::context(data);
        let json = serde_json::to_value(data).unwrap();
        (pt, tera, json)
    }

    #[test]
    fn simple_output_matches() {
        let pt = PromptTemplatesEngine::compile(simple::PROMPT_TEMPLATES);
        let tera = TeraEngine::compile(simple::TERA);
        let mj = MiniJinjaEngine::compile(simple::MINIJINJA);
        let hbs = HandlebarsEngine::compile(simple::HANDLEBARS);
        let data = simple_data();
        let (pt_ctx, tera_ctx, json_ctx) = contexts(&data);

        assert_engines_match(
            "simple",
            &pt.render(&pt_ctx),
            &tera.render_ctx(&tera_ctx),
            &mj.render(&data),
            &hbs.render(&json_ctx),
            Some(simple::EXPECTED),
        );
    }

    #[test]
    fn loop_output_matches() {
        let pt = PromptTemplatesEngine::compile(loop_scenario::PROMPT_TEMPLATES);
        let tera = TeraEngine::compile(loop_scenario::TERA);
        let mj = MiniJinjaEngine::compile(loop_scenario::MINIJINJA);
        let hbs = HandlebarsEngine::compile(loop_scenario::HANDLEBARS);
        let data = loop_data();
        let (pt_ctx, tera_ctx, json_ctx) = contexts(&data);

        assert_engines_match(
            "loop",
            &pt.render(&pt_ctx),
            &tera.render_ctx(&tera_ctx),
            &mj.render(&data),
            &hbs.render(&json_ctx),
            Some(loop_scenario::EXPECTED),
        );
    }

    #[test]
    fn conditional_output_matches() {
        let pt = PromptTemplatesEngine::compile(conditional::PROMPT_TEMPLATES);
        let tera = TeraEngine::compile(conditional::TERA);
        let mj = MiniJinjaEngine::compile(conditional::MINIJINJA);
        let hbs = HandlebarsEngine::compile(conditional::HANDLEBARS);
        let data = conditional_data();
        let (pt_ctx, tera_ctx, json_ctx) = contexts(&data);

        assert_engines_match(
            "conditional",
            &pt.render(&pt_ctx),
            &tera.render_ctx(&tera_ctx),
            &mj.render(&data),
            &hbs.render(&json_ctx),
            Some(conditional::EXPECTED),
        );
    }

    #[test]
    fn hero_output_matches() {
        let pt = PromptTemplatesEngine::compile(hero::PROMPT_TEMPLATES);
        let tera = TeraEngine::compile(hero::TERA);
        let mj = MiniJinjaEngine::compile(hero::MINIJINJA);
        let hbs = HandlebarsEngine::compile(hero::HANDLEBARS);
        let data = hero_data();
        let (pt_ctx, tera_ctx, json_ctx) = contexts(&data);

        assert_engines_match(
            "hero",
            &pt.render(&pt_ctx),
            &tera.render_ctx(&tera_ctx),
            &mj.render(&data),
            &hbs.render(&json_ctx),
            None,
        );
    }

    #[test]
    fn mega_output_matches() {
        let pt = PromptTemplatesEngine::compile(mega::PROMPT_TEMPLATES);
        let tera = TeraEngine::compile(mega::TERA);
        let mj = MiniJinjaEngine::compile(mega::MINIJINJA);
        let hbs = HandlebarsEngine::compile(mega::HANDLEBARS);
        let data = mega_data();
        let (pt_ctx, tera_ctx, json_ctx) = contexts(&data);

        assert_engines_match(
            "mega",
            &pt.render(&pt_ctx),
            &tera.render_ctx(&tera_ctx),
            &mj.render(&data),
            &hbs.render(&json_ctx),
            None,
        );
    }
}
