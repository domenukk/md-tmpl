---
params: [nums = list(int)]
---

[X{%- for n in nums -%}{%- if n > 0 -%}{{ n }}{%- /if -%}{%- /for -%}Y]
