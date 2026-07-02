---
params: []
---

> {% tmpl needs_stuff %}

---

params:

- title = str
- count = int

---

{{ title }} ({{ count }})

> {% /tmpl %}

> {% include needs_stuff %}
