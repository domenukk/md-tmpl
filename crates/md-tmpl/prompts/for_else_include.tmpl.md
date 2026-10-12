---
name: for_else_include
description: Includes a file from a for-loop else body
params: [items = list(str)]
---

> {% for item in items %}

- {{ item }}

> {% else %}
> {% include [empty_note](./for_else_empty_note.tmpl.md) %}
> {% /for %}
