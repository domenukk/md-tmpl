---
params: []
---

> {% tmpl a %}

---

params: []
---

> {% include b %}

> {% /tmpl %}
> {% tmpl b %}

---

params: []
---

> {% include a %}

> {% /tmpl %}
> {% include a %}
