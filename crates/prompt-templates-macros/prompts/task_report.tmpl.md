---
name: task_report
description: A task report template with types
types:
  - Priority = enum<Critical, High, Medium, Low>
params:
  - title = str
  - priority = Priority
  - tasks = list<name = str, urgency = Priority>
---

# Task Report: {{ title }}

Priority: {{ priority }}

> {% for task in tasks %}

- {{ task.name }} ({{ task.urgency }})

  > {% /for %}
