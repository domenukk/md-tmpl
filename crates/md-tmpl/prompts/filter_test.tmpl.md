---
name: filter_test
description: Tests all filters including those with numeric args (parsed_num codegen)
params:
  - name = str
  - score = float
  - count = int
  - items = list(label = str)
---

Upper: {{ name | upper }}
Lower: {{ name | lower }}
Trim: {{ name | trim }}
Fixed: {{ score | fixed(2) }}
Added: {{ count | add(10) }}
Subtracted: {{ count | sub(3) }}
Items: {% for item in items %}{{ item.label | upper }}{% /for %}
