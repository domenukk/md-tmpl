---
params: [name = str]
---

> {% tmpl row %}

---

params: [count = int]
---

{{ count }}

> {% /tmpl %}

> {% include row with count=name %}
