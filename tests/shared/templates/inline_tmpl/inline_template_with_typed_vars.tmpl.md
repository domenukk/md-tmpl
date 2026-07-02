---
params:
  - label = str
  - count = int
---

> {% tmpl row %}

---

params:

- label = str
- count = int

---

- {{ label }}: {{ count }}

> {% /tmpl %}

> {% include row with label=label, count=count %}
