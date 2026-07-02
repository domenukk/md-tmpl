---
params: [val = str]
---

> {% tmpl l3 %}

---

params: [v = str]
---

L3:{{ v }}

> {% /tmpl %}
> {% tmpl l2 %}

---

params: [v = str]
---

L2->{%- include l3 with v=v -%}

> {% /tmpl %}
> {% tmpl l1 %}

---

params: [v = str]
---

L1->{%- include l2 with v=v -%}

> {% /tmpl %}
> {%- include l1 with v=val -%}
