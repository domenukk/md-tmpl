---
params: [name = str]
---

> {% tmpl counter %}

---

params: [count = int]
---

{{ count }}

> {% /tmpl %}

> {% include counter with count=name %}
