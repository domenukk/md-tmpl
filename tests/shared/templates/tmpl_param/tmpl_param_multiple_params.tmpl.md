---
params:
  - label = str
  - count = int
---

> {% tmpl widget %}

---

params:

- label = str
- count = int

---

{{ label }}: {{ count }}

> {% /tmpl %}

> {% include widget with label=label, count=count %}
