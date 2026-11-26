"""Regression tests for md-tmpl 0.10.8 Python bindings."""

from __future__ import annotations

import pytest

import md_tmpl
from md_tmpl import DeclarationsMutatedError, Template, TemplateSyntaxError


def test_render_dict_rejects_reserved_kind_key_without_panic() -> None:
    tmpl = Template.from_source("""---
params: []
---
ok""")
    with pytest.raises(TemplateSyntaxError, match="__kind__"):
        tmpl.render_dict({"__kind__": "Admin"}, allow_extra=True)

    with pytest.raises(TemplateSyntaxError, match="__kind__"):
        tmpl.render(__kind__="Admin")


def test_validate_declarations_rejects_str_to_trusted_str_downgrade() -> None:
    safe_tmpl = Template.from_source("""---
params:
  - x = str
---
{{ x }}""")
    downgraded = Template.from_source("""---
params:
  - x = trusted str
---
{{ x }}""")
    decls = safe_tmpl.declarations()
    safe_tmpl.validate_declarations_against(decls)
    with pytest.raises(DeclarationsMutatedError):
        downgraded.validate_declarations_against(decls)


def test_fence_strips_nul_before_counting_backticks() -> None:
    fenced = md_tmpl.fence("``\x00``\nESCAPED")
    assert fenced.startswith("`````\n")
    assert fenced.endswith("\n`````")


def test_truncate_placeholders_order_independent_validation_and_codegen_kebab_list(
    tmp_path: object,
) -> None:
    from pathlib import Path

    assert md_tmpl.TRUNCATE_PLACEHOLDER_SKIPPED == "{skipped}"
    assert md_tmpl.TRUNCATE_PLACEHOLDER_COUNT == "{count}"

    base = Template.from_source("""---
params:
  - a = int
  - doc = str | sanitize("doc", "Notice A")
---
{{ a }} {{ doc }}""")
    reordered = Template.from_source("""---
params:
  - doc = str | sanitize("doc", "Notice A")
  - a = int
---
{{ a }} {{ doc }}""")
    dropped = Template.from_source("""---
params:
  - a = int
  - doc = str
---
{{ a }} {{ doc }}""")
    reordered.validate_declarations_against(base.declarations())
    reordered.validate_against_template(base)
    with pytest.raises(DeclarationsMutatedError):
        dropped.validate_against_template(base)

    tmpl_file = Path(str(tmp_path)) / "kebab_tags.tmpl.md"
    tmpl_file.write_text(
        """---
params:
  - tags = list(str)
  - my-param = str := "dflt"
---
{{ tags | join(",") }}:{{ my-param }}""",
        encoding="utf-8",
    )
    types_ns = md_tmpl.load_types(str(tmpl_file))
    KebabTags = types_ns.KebabTags
    inst = KebabTags(tags=["x", "y"], my_param="custom")
    loaded_tmpl = Template.from_file(str(tmpl_file))
    assert inst.render(loaded_tmpl) == "x,y:custom"

    stub_src = md_tmpl.generate_types_source(str(tmpl_file))
    assert "tags: list[str]" in stub_src
    assert 'metadata={"wire": "my-param"}' in stub_src
    stub_ns: dict[str, object] = {}
    exec(stub_src, stub_ns)
    StubCls = stub_ns["KebabTags"]
    assert callable(StubCls)
    stub_inst = StubCls(tags=["x", "y"])
    assert getattr(stub_inst, "render")(loaded_tmpl) == "x,y:dflt"

    tricky_dir = Path(str(tmp_path)) / 'o\'reilly_"""_dir'
    tricky_dir.mkdir(parents=True, exist_ok=True)
    tricky_file = tricky_dir / '123-tricky"""'
    tricky_file.write_text(
        """---
params:
  - item-name = str
---
{{ item-name }}""",
        encoding="utf-8",
    )
    tricky_ns = md_tmpl.load_types(str(tricky_file))
    TrickyCls = tricky_ns.Template123Tricky
    assert TrickyCls(item_name="ok").render(None) == "ok"
    tricky_src = md_tmpl.generate_types_source(str(tricky_file))
    compiled_ns: dict[str, object] = {}
    exec(tricky_src, compiled_ns)
    assert callable(compiled_ns["Template123Tricky"])

    bad_variant_file = Path(str(tmp_path)) / "bad_variant.tmpl.md"
    bad_variant_file.write_text(
        """---
allow_unused: true
params:
  - kind_val = enum(Good, Bad = 1)
---
ok""",
        encoding="utf-8",
    )
    with pytest.raises((ValueError, md_tmpl.TemplateSyntaxError)):
        md_tmpl.load_types(str(bad_variant_file))

    child_h1 = md_tmpl.Template.from_source("""---
params:
  - heading = trusted str
---
# {{ heading }}""")
    host_h1 = md_tmpl.Template.from_source("""---
params:
  - slot = tmpl(heading = str)
  - title = str
---
> {% include slot with heading = title %}""")
    rendered_h1 = host_h1.render(slot=child_h1, title="Safe\n## Injected")
    assert "\n## Injected" not in rendered_h1
    assert rendered_h1 == "# Safe ## Injected"
