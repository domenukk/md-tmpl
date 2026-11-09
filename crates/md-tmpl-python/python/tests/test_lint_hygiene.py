"""Adversarial Gate R tests for `scripts/lint_hygiene.py` in `md-tmpl`.

Verifies that `lint_hygiene.py` unconditionally rejects Rust module-path
override attributes across both `crates/` and `benchmarks/` and `json!(` macro
invocations in production `crates/*/src/` (outside `#[cfg(test)]`), including
with inline `NOLINT` comments or single-item `#[cfg(test)] use` guards, while
passing on `#[cfg(test)]` blocks, integration `tests/`, and the real workspace.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest

sys.dont_write_bytecode = True

REPO_ROOT = Path(__file__).resolve().parents[4]
LINT_SCRIPT = REPO_ROOT / "scripts" / "lint_hygiene.py"

OUTER_PATH = "#" + "[path"
INNER_PATH = "#!" + "[path"
SPACED_PATH = "#" + "[ path"
NOLINT_COMMENT = "// NOL" + "INT: legacy benchmark module layout"


def _load_lint_module():
    spec = importlib.util.spec_from_file_location("md_tmpl_lint_hygiene", LINT_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Unable to load lint module from {LINT_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    sys.modules["md_tmpl_lint_hygiene"] = module
    spec.loader.exec_module(module)
    return module


LINT = _load_lint_module()


@pytest.mark.parametrize(
    ("rel_path", "content"),
    [
        (
            "benchmarks/benches/comparison.rs",
            f'{OUTER_PATH} = "comparison/models.rs"]\nmod models;\n',
        ),
        (
            "benchmarks/benches/comparison/main.rs",
            f'{NOLINT_COMMENT}\n{OUTER_PATH} = "../extra.rs"]\nmod extra;\n',
        ),
        (
            "crates/md-tmpl-core/src/compiled/render/mod.rs",
            f'#[cfg(test)]\n{OUTER_PATH} = "../render_tests.rs"]\nmod render_tests;\n',
        ),
        (
            "benchmarks/examples/spaced.rs",
            f'{SPACED_PATH} = "helper.rs"]\nmod helper;\n',
        ),
        (
            "crates/demo/src/lib.rs",
            f'{INNER_PATH} = "other.rs"]\n',
        ),
    ],
)
def test_gate_r_rejects_path_overrides_in_crates_and_benchmarks(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], rel_path: str, content: str
) -> None:
    target = tmp_path / rel_path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")

    failed = LINT.check_workspace(tmp_path, check_packaging=False)
    captured = capsys.readouterr().out
    assert failed is True, f"Expected violation in {rel_path}, got output:\n{captured}"
    assert rel_path in captured


@pytest.mark.parametrize(
    ("rel_path", "content"),
    [
        (
            "crates/md-tmpl-core/src/payload.rs",
            "pub fn build() {\n    let v = json!({});\n    std::hint::black_box(v);\n}\n",
        ),
        (
            "crates/md-tmpl-core/src/bracket.rs",
            "pub fn build() {\n    let v = json![1, 2];\n    std::hint::black_box(v);\n}\n",
        ),
        (
            "crates/md-tmpl/src/lib.rs",
            (
                "#[cfg(test)]\n"
                "use std::collections::BTreeMap;\n\n"
                f"{NOLINT_COMMENT}\n"
                'pub fn wire() { let v = serde_json::json!({"ok": true}); std::hint::black_box(v); }\n'
            ),
        ),
    ],
)
def test_gate_r_rejects_production_json_macro_in_crate_src(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], rel_path: str, content: str
) -> None:
    target = tmp_path / rel_path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")

    failed = LINT.check_workspace(tmp_path, check_packaging=False)
    captured = capsys.readouterr().out
    assert (
        failed is True
    ), f"Expected json! violation in {rel_path}, got output:\n{captured}"
    assert rel_path in captured
    assert "json!" in captured


def test_gate_r_allows_json_macro_in_cfg_test_and_integration_tests(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    lib_rs = tmp_path / "crates/md-tmpl-core/src/lib.rs"
    lib_rs.parent.mkdir(parents=True, exist_ok=True)
    lib_rs.write_text(
        "pub fn typed_only() -> u32 {\n"
        "    7\n"
        "}\n\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    #[test]\n"
        "    fn fixture_allows_json() {\n"
        '        let v = serde_json::json!({"ok": true});\n'
        "        assert!(v.is_object());\n"
        "    }\n"
        "}\n",
        encoding="utf-8",
    )
    integ_rs = tmp_path / "crates/md-tmpl-core/tests/integration.rs"
    integ_rs.parent.mkdir(parents=True, exist_ok=True)
    integ_rs.write_text(
        "#[test]\nfn it_works() {\n    let v = json!({});\n    assert!(v.is_object());\n}\n",
        encoding="utf-8",
    )

    failed = LINT.check_workspace(tmp_path, check_packaging=False)
    captured = capsys.readouterr().out
    assert failed is False, f"Unexpected violation on test-only json!:\n{captured}"


def test_gate_r_rejects_spaced_and_inner_allow_or_expect_suppressions(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    spaced_allow = "#" + " [allow(dead_code)]"
    inner_allow = "#!" + "[allow(dead_code)]"
    spaced_expect = "#" + " [expect(dead_code)]"
    consts_rs = tmp_path / "crates/md-tmpl-core/src/consts.rs"
    consts_rs.parent.mkdir(parents=True, exist_ok=True)
    consts_rs.write_text(
        f"{spaced_allow}\n"
        f"{inner_allow}\n"
        f"{spaced_expect}\n"
        "pub const X: u32 = 1;\n",
        encoding="utf-8",
    )
    failed = LINT.check_workspace(tmp_path, check_packaging=False)
    captured = capsys.readouterr().out
    assert failed is True, f"Expected allow/expect violation, got:\n{captured}"
    assert "crates/md-tmpl-core/src/consts.rs:1" in captured
    assert "crates/md-tmpl-core/src/consts.rs:2" in captured
    assert "crates/md-tmpl-core/src/consts.rs:3" in captured


def test_real_workspace_passes_hygiene_gate() -> None:
    assert LINT.check_workspace(REPO_ROOT, check_packaging=True) is False
