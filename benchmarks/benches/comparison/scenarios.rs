//! Template definitions and expected outputs for comparison benchmarks.

// ==========================================================================
// Scenario 1 — Simple variable substitution
// ==========================================================================

pub mod simple {
    pub const MD_TMPL: &str = "\
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

pub mod loop_scenario {
    pub const MD_TMPL: &str = "\
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

pub mod conditional {
    pub const MD_TMPL: &str = "\
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

pub mod hero {
    pub const MD_TMPL: &str = "\
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
// Scenario 5 — Mega: large data, deep nesting, idx, filters
// ==========================================================================

pub mod mega {
    pub const MD_TMPL: &str = "\
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
