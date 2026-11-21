"""Auto-generated typed stubs for tests/golden_stubs/fixtures/advanced_codegen.tmpl.md.

Do not edit — regenerate with ``generate_types_source()``.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Optional

from md_tmpl import Template, Variants

class Stage(Variants):
    Draft = ()
    Ready = ()

@dataclass
class AdvancedCodegen:
    """Typed parameters for template ``tests/golden_stubs/fixtures/advanced_codegen.tmpl.md``."""

    tags: list[str]
    note: str | None
    limit_opt: int | None
    header: object
    stage: Stage
    my_param: str = field(default="default", metadata={"wire": "my-param"})

    def render(self, template: Template | None = None) -> str:
        """Render this params object into its template."""
        if template is None:
            template = Template.from_file("tests/golden_stubs/fixtures/advanced_codegen.tmpl.md")
        import dataclasses
        def _wire(obj: Any) -> Any:
            if dataclasses.is_dataclass(obj) and not isinstance(obj, type):
                return {f.metadata.get("wire", f.name): _wire(getattr(obj, f.name)) for f in dataclasses.fields(obj)}
            if isinstance(obj, list): return [_wire(x) for x in obj]
            return obj
        return template.render_dict(_wire(self))

__all__ = ["AdvancedCodegen", "Stage"]
