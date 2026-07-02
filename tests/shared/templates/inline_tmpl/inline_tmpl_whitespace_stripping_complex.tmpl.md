---
params: [x = str]
---

> {% tmpl tag %}

---

params: [t = str]
---

[{{ t }}]

> {% /tmpl %}

Start{# comment #}{%- include tag with t=x -%}End
