---
params:
  - name = str
  - greeting = str
---

> {% tmpl greeter %}

---

params:

- name = str
- greeting = str := "Hi"

---

{{ greeting }} {{ name }}!

> {% /tmpl %}

> {% include greeter with name=name, greeting=greeting %}
