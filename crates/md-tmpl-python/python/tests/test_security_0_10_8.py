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
