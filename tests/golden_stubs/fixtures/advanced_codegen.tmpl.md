---
name: advanced_codegen
description: Template exercising scalar lists, options, sub-templates, imports, and hyphenated names
imports:
  - "[shared_types](./shared_types.tmpl.md)"

params:
  - tags = list(str)
  - note = option(str)
  - limit_opt = option(int)
  - header = tmpl(title = str)
  - stage = shared_types.Stage
  - my-param = str := "default"

allow_unused: true
---

Tags: {{ tags | join(", ") }}

> {% if has(note) %}

Note: {{ note }}

> {% /if %}

> {% if has(limit_opt) %}

Limit: {{ limit_opt }}

> {% /if %}

Stage: {{ kind(stage) }}

> {% include header with title="Summary" %}
