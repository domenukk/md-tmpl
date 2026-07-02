---
params: []
---

> {% tmpl versioned %}

---

params: []
consts:

- V = str := "2.0"

---

v{{ V }}

> {% /tmpl %}
> {% include versioned %}
