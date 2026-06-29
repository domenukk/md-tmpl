---
name: option_test
params:
  - name = str
  - nickname = option(str)
  - age = option(int)
---

Hello {{ name }}!

> {% if has(nickname) %}

Nickname: {{ nickname }}

> {% /if %}

> {% if has(age) %}

Age: {{ age }}

> {% /if %}
