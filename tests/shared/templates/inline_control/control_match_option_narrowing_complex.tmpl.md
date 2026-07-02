---
params: [config = option(struct(host = str, port = int))]
---

> {% match config %}

> {% case Some %}

{{ config.host }}:{{ config.port }}

> {% case None %}

default:80

> {% /match %}
