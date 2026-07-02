---
params: [name = str]
---

> {% tmpl greeting %}

---

params: [name = str]
---

Hello {{ name }}!

> {% /tmpl %}
> {% include greeting with name=name %}
