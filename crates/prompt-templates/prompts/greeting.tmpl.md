---
name: greeting
description: A greeting template
params: [name = str, count = int, items = list(label = str)]
---

Hello {{ name }}! Count: {{ count }}. Items: {% for item in items %}{{ item.label }}{% /for %}
