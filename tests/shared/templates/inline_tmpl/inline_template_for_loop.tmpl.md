---
params: [items = list(str)]
---

> {% tmpl item %}

---

params: [it = str]
---

- {{ it }}

> {% /tmpl %}

> {% include item for it in items %}
