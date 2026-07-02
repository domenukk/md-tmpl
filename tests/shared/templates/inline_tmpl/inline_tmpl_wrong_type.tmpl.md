---
params: [items = list(str)]
---

> {% tmpl row %}

---

params: [x = int]
---

{{ x }}

> {% /tmpl %}
> {% include row with x=items %}
