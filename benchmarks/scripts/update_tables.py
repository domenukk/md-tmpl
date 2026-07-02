#!/usr/bin/env python3
"""Update benchmark tables in README.md with fresh numbers.

Reads a unified JSON file (from parse_benchmarks.py) and replaces the benchmark
tables in-place, preserving surrounding content and formatting conventions:
  - Right-aligned numbers
  - 🏆 on winners per row
  - Proper units (ns/µs/ms)
  - Speedup ratios for Go tables
  - Comma-separated large ns values (e.g. 56,080 ns)

Usage:
  python3 benchmarks/scripts/update_tables.py --json results.json [--readme README.md]
  # Or pipe from parse_benchmarks.py:
  python3 benchmarks/scripts/parse_benchmarks.py --rust-dir ... | python3 benchmarks/scripts/update_tables.py --readme README.md
"""

from __future__ import annotations

import argparse
import json
import logging
import re
import sys
from pathlib import Path
from typing import Any

log = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Formatting helpers
# ---------------------------------------------------------------------------


def format_ns(ns: float, comma_separate: bool = False) -> str:
    """Format nanoseconds into a human-readable string.

    Uses ns for <1000ns, µs for <1000µs, ms otherwise.
    """
    if ns >= 1_000_000:
        val = ns / 1_000_000
        if val >= 100:
            return f"{val:.0f} ms"
        return f"{val:.2f} ms"
    elif ns >= 1_000:
        val = ns / 1_000
        return f"{val:.2f} µs"
    else:
        return f"{ns:.0f} ns"


def format_ns_go(ns: float) -> str:
    """Format nanoseconds for Go tables — use comma separators for readability."""
    if ns >= 1_000_000:
        val = ns / 1_000_000
        return f"{val:.2f} ms"
    # Go tables use raw ns with commas
    return f"{ns:,.0f} ns"


def format_us(us: float) -> str:
    """Format microseconds into a human-readable string."""
    if us >= 1000:
        return f"{us / 1000:.2f} ms"
    return f"{us:.2f} µs"


# ---------------------------------------------------------------------------
# Table generation — Rust
# ---------------------------------------------------------------------------

RUST_ENGINES_DISPLAY = {
    "md_tmpl": "md-tmpl",
    "tera": "Tera",
    "minijinja": "`MiniJinja`",
    "handlebars": "Handlebars",
}

RUST_ENGINES_ORDER = ["md_tmpl", "tera", "minijinja", "handlebars"]
RUST_SCENARIOS = ["simple", "loop", "conditional", "hero", "mega"]




def build_rust_table(
    data: dict[str, dict[str, float]],
    engine_display: dict[str, str] | None = None,
) -> str:
    """Build a markdown table for Rust benchmark results."""
    if engine_display is None:
        engine_display = RUST_ENGINES_DISPLAY

    header_names = [engine_display[e] for e in RUST_ENGINES_ORDER]

    # Calculate column widths
    scenario_width = max(len("Scenario"), max(len(f"**{s}**") for s in RUST_SCENARIOS))
    col_widths = [max(len(name), 10) for name in header_names]

    lines = []

    # Header
    header = f"| {'Scenario':<{scenario_width}} |"
    separator = f"| {'-' * scenario_width} |"
    for name, width in zip(header_names, col_widths):
        header += f" {name:>{width}} |"
        separator += f" {'-' * width}: |"
    lines.append(header)
    lines.append(separator)

    # Data rows
    for scenario in RUST_SCENARIOS:
        if scenario not in data:
            continue

        row_data = data[scenario]
        values = {}
        for engine in RUST_ENGINES_ORDER:
            if engine in row_data:
                values[engine] = row_data[engine]

        # Find winner (lowest ns)
        if values:
            winner = min(values, key=lambda e: values[e])
        else:
            winner = None

        row = f"| **{scenario}** {'':>{scenario_width - len(scenario) - 4}}|"
        for engine, width in zip(RUST_ENGINES_ORDER, col_widths):
            if engine in values:
                formatted = format_ns(values[engine])
                if engine == winner:
                    cell = f"**{formatted}** 🏆"
                else:
                    cell = formatted
                row += f" {cell:>{width}} |"
            else:
                row += f" {'N/A':>{width}} |"
        lines.append(row)

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Table generation — Python
# ---------------------------------------------------------------------------

PYTHON_ENGINES_ORDER = [
    "md-tmpl",
    "Jinja2",
    "Mako",
    "Chevron",
    "Django",
    "str.Tmpl",
]

PYTHON_ENGINES_README_RENDER = {
    "md-tmpl": "md-tmpl",
    "Jinja2": "Jinja2",
    "Mako": "Mako",
    "Chevron": "Chevron",
    "Django": "Django",
    "str.Tmpl": "string.Template",
}

PYTHON_ENGINES_README_E2E = {
    "md-tmpl": "md-tmpl",
    "Jinja2": "Jinja2",
    "Mako": "Mako",
    "Chevron": "Chevron",
    "Django": "Django",
    "str.Tmpl": "str.Template",
}

PYTHON_SCENARIOS = ["simple", "loop", "conditional", "hero"]


def build_python_table(
    data: dict[str, dict[str, float]],
    engine_display: dict[str, str],
    engines_order: list[str] | None = None,
) -> str:
    """Build a markdown table for Python benchmark results (values in µs)."""
    if engines_order is None:
        engines_order = PYTHON_ENGINES_ORDER

    header_names = [engine_display.get(e, e) for e in engines_order]

    scenario_width = max(len("Scenario"), max(len(f"**{s}**") for s in PYTHON_SCENARIOS))
    col_widths = [max(len(name), 12) for name in header_names]

    lines = []

    # Header
    header = f"| {'Scenario':<{scenario_width}} |"
    separator = f"| {'-' * scenario_width} |"
    for name, width in zip(header_names, col_widths):
        header += f" {name:>{width}} |"
        separator += f" {'-' * width}: |"
    lines.append(header)
    lines.append(separator)

    # Data rows
    for scenario in PYTHON_SCENARIOS:
        if scenario not in data:
            continue

        row_data = data[scenario]
        values = {}
        for engine in engines_order:
            if engine in row_data:
                values[engine] = row_data[engine]

        # Find winner (lowest µs)
        if values:
            winner = min(values, key=lambda e: values[e])
        else:
            winner = None

        row = f"| **{scenario}** {'':>{scenario_width - len(scenario) - 4}}|"
        for engine, width in zip(engines_order, col_widths):
            if engine in values:
                formatted = format_us(values[engine])
                if engine == winner:
                    cell = f"**{formatted}** 🏆"
                else:
                    cell = formatted
                row += f" {cell:>{width}} |"
            else:
                row += f" {'N/A':>{width}} |"
        lines.append(row)

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Table generation — Go
# ---------------------------------------------------------------------------

GO_SCENARIOS = ["Small", "Medium", "Large"]
GO_SCENARIO_DISPLAY = {"Small": "small", "Medium": "medium", "Large": "large"}


def build_go_table(
    pt_data: dict[str, dict[str, Any]],
    go_data: dict[str, dict[str, Any]],
    phase: str,
) -> str:
    """Build a Go comparison table for a specific phase (Render/Compile/RoundTrip).

    phase should be one of: "Render", "Compile", "RoundTrip"
    """
    scenario_width = max(len("Scenario"), 10)
    pt_width = max(len("md-tmpl"), 16)
    go_width = max(len("Go `text/template`"), 18)
    speedup_width = max(len("speedup"), 7)

    lines = []

    # Header
    lines.append(
        f"| {'Scenario':<{scenario_width}} "
        f"| {'md-tmpl':>{pt_width}} "
        f"| {'Go `text/template`':>{go_width}} "
        f"| {'speedup':>{speedup_width}} |"
    )
    lines.append(
        f"| {'-' * scenario_width} "
        f"| {'-' * pt_width}: "
        f"| {'-' * go_width}: "
        f"| {'-' * speedup_width}: |"
    )

    for scenario in GO_SCENARIOS:
        pt_key = f"{phase}{scenario}"
        go_key = f"{phase}{scenario}"

        pt_ns = pt_data.get(pt_key, {}).get("ns")
        go_ns = go_data.get(go_key, {}).get("ns")

        if pt_ns is None or go_ns is None:
            continue

        pt_formatted = format_ns_go(pt_ns)
        go_formatted = format_ns_go(go_ns)

        speedup = go_ns / pt_ns if pt_ns > 0 else 0
        if speedup >= 1.05:
            speedup_str = f"{speedup:.1f}×" if speedup >= 2 else f"{speedup:.2f}×"
        else:
            speedup_str = "~1.0×"

        # Winner marker
        if speedup >= 1.05:
            pt_cell = f"**{pt_formatted}** 🏆"
        else:
            pt_cell = pt_formatted

        display = GO_SCENARIO_DISPLAY[scenario]
        lines.append(
            f"| **{display}**{'':>{scenario_width - len(display) - 4}} "
            f"| {pt_cell:>{pt_width}} "
            f"| {go_formatted:>{go_width}} "
            f"| {speedup_str:>{speedup_width}} |"
        )

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# File updating
# ---------------------------------------------------------------------------


def find_table_range(lines: list[str], start_marker: str) -> tuple[int, int] | None:
    """Find a markdown table following a line containing start_marker.

    Returns (first_table_line, last_table_line) inclusive, or None.
    """
    marker_idx = None
    for i, line in enumerate(lines):
        if start_marker in line:
            marker_idx = i
            break

    if marker_idx is None:
        return None

    # Find the first table line (starts with |) after the marker
    table_start = None
    for i in range(marker_idx + 1, min(marker_idx + 10, len(lines))):
        if lines[i].strip().startswith("|"):
            table_start = i
            break

    if table_start is None:
        return None

    # Find the end of the table
    table_end = table_start
    for i in range(table_start + 1, len(lines)):
        if lines[i].strip().startswith("|"):
            table_end = i
        else:
            break

    return (table_start, table_end)


def replace_table(
    content: str, start_marker: str, new_table: str
) -> str:
    """Replace a markdown table in content that follows start_marker."""
    lines = content.split("\n")
    table_range = find_table_range(lines, start_marker)

    if table_range is None:
        log.warning("Could not find table after marker: %s", start_marker)
        return content

    start, end = table_range
    new_lines = lines[:start] + new_table.split("\n") + lines[end + 1 :]
    return "\n".join(new_lines)


# ---------------------------------------------------------------------------
# README.md updating
# ---------------------------------------------------------------------------


def update_readme(readme_path: Path, data: dict[str, Any]) -> bool:
    """Update benchmark tables in a Rust README.md. Returns True if changed."""
    content = readme_path.read_text()
    changes_made = False

    # Rust table
    if "rust" in data:
        rust_table = build_rust_table(data["rust"], RUST_ENGINES_DISPLAY)
        for marker in ("### Rust (render-only, pre-parsed)", "### vs Competitors"):
            new_content = replace_table(
                content,
                marker,
                rust_table,
            )
            if new_content != content:
                content = new_content
                changes_made = True
                log.info("Updated Rust table in %s after marker %s", readme_path, marker)

    if changes_made:
        readme_path.write_text(content)
    return changes_made


def update_python_readme(readme_path: Path, data: dict[str, Any]) -> bool:
    """Update benchmark tables in the Python crate README.md. Returns True if changed."""
    if "python" not in data:
        return False

    content = readme_path.read_text()
    changes_made = False

    # Render table
    if "render" in data["python"]:
        py_render_table = build_python_table(
            data["python"]["render"],
            PYTHON_ENGINES_README_RENDER,
        )
        new_content = replace_table(
            content,
            "### Render Time (pre-parsed template + data)",
            py_render_table,
        )
        if new_content != content:
            content = new_content
            changes_made = True
            log.info("Updated Python render table")

    # Compile table
    if "compile" in data["python"]:
        py_compile_table = build_python_table(
            data["python"]["compile"],
            PYTHON_ENGINES_README_RENDER,
        )
        new_content = replace_table(
            content,
            "### Parse Time (source → template object)",
            py_compile_table,
        )
        if new_content != content:
            content = new_content
            changes_made = True
            log.info("Updated Python compile table")

    # End-to-end table
    if "end_to_end" in data["python"]:
        py_e2e_table = build_python_table(
            data["python"]["end_to_end"],
            PYTHON_ENGINES_README_E2E,
        )
        new_content = replace_table(
            content,
            "### End-to-End (parse + render)",
            py_e2e_table,
        )
        if new_content != content:
            content = new_content
            changes_made = True
            log.info("Updated Python end-to-end table")

    if changes_made:
        readme_path.write_text(content)
    return changes_made


def update_go_readme(readme_path: Path, data: dict[str, Any]) -> bool:
    """Update benchmark tables in the Go README.md. Returns True if changed."""
    if "go" not in data:
        return False

    pt_data = data["go"].get("pt", {})
    go_data = data["go"].get("go", {})

    if not pt_data or not go_data:
        return False

    content = readme_path.read_text()
    changes_made = False

    # Render table
    go_render_table = build_go_table(pt_data, go_data, "Render")
    new_content = replace_table(content, "**Render** (pre-parsed template + data → output):", go_render_table)
    if new_content != content:
        content = new_content
        changes_made = True
        log.info("Updated Go render table")

    # Round-trip table
    go_rt_table = build_go_table(pt_data, go_data, "RoundTrip")
    new_content = replace_table(content, "**Round-trip** (parse + render):", go_rt_table)
    if new_content != content:
        content = new_content
        changes_made = True
        log.info("Updated Go round-trip table")

    if changes_made:
        readme_path.write_text(content)
    return changes_made

# ---------------------------------------------------------------------------
# Table generation — TypeScript (internal)
# ---------------------------------------------------------------------------

TS_INTERNAL_SCENARIOS = [
    "simple (1 str)",
    "multi-param (4 types)",
    "list (2 items)",
    "list (20 items)",
    "enum unit variant",
    "enum struct variant",
    "filters (idx+add, upper)",
    "if/elif/else",
]


def _strip_ts_prefix(name: str) -> str:
    """Strip 'render: ' or 'renderUnchecked: ' prefix from bench scenario name."""
    for prefix in ("render: ", "renderUnchecked: "):
        if name.startswith(prefix):
            return name[len(prefix):]
    return name


def _format_ns_comma(ns: float) -> str:
    """Format nanoseconds with comma separators (e.g. 1,234 ns)."""
    return f"{ns:,.0f} ns"


def build_ts_internal_table(
    render_data: dict[str, float],
    unchecked_data: dict[str, float],
) -> str:
    """Build the TS internal benchmarks table (render vs renderUnchecked)."""
    render = {_strip_ts_prefix(k): v for k, v in render_data.items()}
    unchecked = {_strip_ts_prefix(k): v for k, v in unchecked_data.items()}

    lines = []
    lines.append("| Scenario              |    render | renderUnchecked |")
    lines.append("| --------------------- | --------: | --------------: |")

    for scenario in TS_INTERNAL_SCENARIOS:
        r_ns = render.get(scenario)
        u_ns = unchecked.get(scenario)

        r_str = _format_ns_comma(r_ns) if r_ns else "N/A"
        u_str = _format_ns_comma(u_ns) if u_ns else "N/A"

        # Bold the faster one
        if r_ns and u_ns and u_ns <= r_ns:
            u_str = f"**{u_str}**"
        elif r_ns and u_ns and r_ns < u_ns:
            r_str = f"**{r_str}**"

        lines.append(f"| {scenario:<21} | {r_str:>9} | {u_str:>15} |")

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Table generation — TypeScript (comparison vs Handlebars/Mustache)
# ---------------------------------------------------------------------------

TS_COMP_SCENARIOS = ["simple", "loop (5 items)", "conditional"]


def build_ts_comparison_render_table(
    render_data: dict[str, dict[str, float]],
    unchecked_data: dict[str, dict[str, float]],
) -> str:
    """Build TS comparison render table (render + unchecked + HBS + Mustache)."""
    lines = []
    lines.append(
        "| Scenario           | render() | renderUnchecked() | Handlebars |        Mustache |"
    )
    lines.append(
        "| ------------------ | -------: | ----------------: | ---------: | --------------: |"
    )

    for scenario in TS_COMP_SCENARIOS:
        r = render_data.get(scenario, {})
        u = unchecked_data.get(scenario, {})

        vals = {
            "pt_r": r.get("pt"),
            "pt_u": u.get("pt"),
            "hbs": r.get("hbs"),
            "mus": r.get("mus"),
        }

        # Find winner among all 4
        candidates = {k: v for k, v in vals.items() if v is not None}
        winner = min(candidates, key=lambda k: candidates[k]) if candidates else None

        def fmt(key: str, width: int, _vals: dict[str, Any] = vals, _winner: str | None = winner) -> str:
            v = _vals.get(key)
            if v is None:
                return f"{'N/A':>{width}}"
            s = _format_ns_comma(v)
            if key == _winner:
                s = f"**{s}** 🏆"
            return f"{s:>{width}}"

        pad = max(0, 15 - len(scenario) - 4)
        lines.append(
            f"| **{scenario}**{'':<{pad}} "
            f"| {fmt('pt_r', 8)} | {fmt('pt_u', 17)} | {fmt('hbs', 15)} | {fmt('mus', 15)} |"
        )

    return "\n".join(lines)


def build_ts_comparison_roundtrip_table(
    rt_data: dict[str, dict[str, float]],
) -> str:
    """Build TS comparison round-trip table."""
    lines = []
    lines.append(
        "| Scenario   | md-tmpl | Handlebars |        Mustache |"
    )
    lines.append(
        "| ---------- | ---------------: | ---------: | --------------: |"
    )

    for scenario in TS_COMP_SCENARIOS:
        r = rt_data.get(scenario, {})
        pt = r.get("pt")
        hbs = r.get("hbs")
        mus = r.get("mus")

        if pt is None and hbs is None and mus is None:
            continue

        candidates = {k: v for k, v in {"pt": pt, "hbs": hbs, "mus": mus}.items() if v is not None}
        winner = min(candidates, key=lambda k: candidates[k]) if candidates else None

        def fmt(key: str, val: float | None, width: int, _winner: str | None = winner) -> str:
            if val is None:
                return f"{'N/A':>{width}}"
            s = _format_ns_comma(val)
            if key == _winner:
                s = f"**{s}** 🏆"
            return f"{s:>{width}}"

        pad = max(0, 10 - len(scenario) - 4)
        lines.append(
            f"| **{scenario}**{'':<{pad}} "
            f"| {fmt('pt', pt, 16)} | {fmt('hbs', hbs, 10)} | {fmt('mus', mus, 15)} |"
        )

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Table generation — WASM vs TypeScript
# ---------------------------------------------------------------------------


WASM_REPRESENTATIVE_SCENARIOS = [
    "parse simple",
    "render simple (1 param)",
    "render list/for (20 items)",
    "render complex (nested+list+filter)",
    "declarations()",
    "renderJson complex (nested+list+filter)",
]


def build_wasm_table(wasm_data: dict[str, Any]) -> str:
    """Build WASM vs TypeScript comparison table."""
    lines = []
    lines.append(
        "| Scenario                         | WASM (Rust) | TypeScript | speedup |"
    )
    lines.append(
        "| -------------------------------- | ----------: | ---------: | ------: |"
    )

    for scenario in WASM_REPRESENTATIVE_SCENARIOS:
        data = wasm_data.get(scenario)
        if not data:
            continue
        wasm_ns = data.get("wasm", {}).get("median_ns")
        ts_ns = data.get("ts", {}).get("median_ns")
        speedup = data.get("speedup", 0)

        if wasm_ns is None or ts_ns is None:
            continue

        w_str = format_ns(wasm_ns)
        t_str = format_ns(ts_ns)

        if speedup >= 1.05:
            w_str = f"**{w_str}** 🏆"
            sp_str = f"{speedup:.1f}×"
        elif speedup <= 0.95:
            t_str = f"**{t_str}** 🏆"
            sp_str = f"{1/speedup:.1f}× TS"
        else:
            sp_str = "~1.0×"

        lines.append(
            f"| {scenario:<32} | {w_str:>11} | {t_str:>10} | {sp_str:>7} |"
        )

    return "\n".join(lines)


def update_ts_readme(readme_path: Path, data: dict[str, Any]) -> bool:
    """Update benchmark tables in the TypeScript/WASM README.md."""
    content = readme_path.read_text()
    changes_made = False

    # 1. Internal benchmarks table (from bench.ts)
    if "ts" in data:
        ts = data["ts"]
        render = ts.get("render", {})
        unchecked = ts.get("unchecked", {})
        if render:
            new_table = build_ts_internal_table(render, unchecked)
            new_content = replace_table(content, "### Internal Benchmarks", new_table)
            if new_content != content:
                content = new_content
                changes_made = True
                log.info("Updated TS internal benchmarks table")

    # 2. Comparison render table (from comparison.ts)
    if "ts_comparison" in data:
        tc = data["ts_comparison"]
        render = tc.get("render", {})
        unchecked = tc.get("unchecked", {})
        if render:
            new_table = build_ts_comparison_render_table(render, unchecked)
            new_content = replace_table(
                content,
                "**Render only** (pre-parsed template + data",
                new_table,
            )
            if new_content != content:
                content = new_content
                changes_made = True
                log.info("Updated TS comparison render table")

        # 3. Comparison round-trip table
        rt = tc.get("roundtrip", {})
        if rt:
            new_table = build_ts_comparison_roundtrip_table(rt)
            new_content = replace_table(
                content,
                "**Round-trip** (parse + render",
                new_table,
            )
            if new_content != content:
                content = new_content
                changes_made = True
                log.info("Updated TS comparison round-trip table")

    if changes_made:
        readme_path.write_text(content)
    return changes_made


def update_wasm_readme(readme_path: Path, data: dict[str, Any]) -> bool:
    """Update benchmark tables in the WASM README.md."""
    if "wasm" not in data:
        return False

    content = readme_path.read_text()
    changes_made = False

    wasm_table = build_wasm_table(data["wasm"])
    new_content = replace_table(
        content,
        "### WASM vs Pure-TypeScript",
        wasm_table,
    )
    if new_content != content:
        content = new_content
        changes_made = True
        log.info("Updated WASM vs TS table")

    if changes_made:
        readme_path.write_text(content)
    return changes_made


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(
        description="Update benchmark tables in README.md."
    )
    parser.add_argument(
        "--json",
        type=Path,
        help="Path to unified JSON results file (from parse_benchmarks.py). "
        "If not specified, reads from stdin.",
    )
    parser.add_argument(
        "--readme",
        type=Path,
        default=None,
        help="Path to root README.md to update (Rust tables only)",
    )
    parser.add_argument(
        "--rust-crate-readme",
        type=Path,
        default=None,
        help="Path to crates/md-tmpl/README.md to update",
    )
    parser.add_argument(
        "--python-readme",
        type=Path,
        default=None,
        help="Path to Python crate README.md to update",
    )
    parser.add_argument(
        "--go-readme",
        type=Path,
        default=None,
        help="Path to Go README.md to update",
    )
    parser.add_argument(
        "--ts-readme",
        type=Path,
        default=None,
        help="Path to TypeScript/WASM README.md to update",
    )
    parser.add_argument(
        "--wasm-readme",
        type=Path,
        default=None,
        help="Path to WASM README.md to update",
    )

    args = parser.parse_args()

    # Read JSON data
    if args.json:
        data = json.loads(args.json.read_text())
    else:
        data = json.load(sys.stdin)

    if not data:
        log.error("No benchmark data found")
        sys.exit(1)

    updated_any = False
    has_targets = args.readme or args.rust_crate_readme or args.python_readme or args.go_readme or args.ts_readme or args.wasm_readme

    if args.readme:
        if update_readme(args.readme, data):
            log.info("README.md updated successfully")
            updated_any = True
        else:
            log.info("README.md — no changes needed")

    if args.rust_crate_readme:
        if update_readme(args.rust_crate_readme, data):
            log.info("crates/md-tmpl/README.md updated successfully")
            updated_any = True
        else:
            log.info("crates/md-tmpl/README.md — no changes needed")

    if args.python_readme:
        if update_python_readme(args.python_readme, data):
            log.info("Python README.md updated successfully")
            updated_any = True
        else:
            log.info("Python README.md — no changes needed")

    if args.go_readme:
        if update_go_readme(args.go_readme, data):
            log.info("Go README.md updated successfully")
            updated_any = True
        else:
            log.info("Go README.md — no changes needed")

    if args.ts_readme:
        if update_ts_readme(args.ts_readme, data):
            log.info("TypeScript README.md updated successfully")
            updated_any = True
        else:
            log.info("TypeScript README.md — no changes needed")

    if args.wasm_readme:
        if update_wasm_readme(args.wasm_readme, data):
            log.info("WASM README.md updated successfully")
            updated_any = True
        else:
            log.info("WASM README.md — no changes needed")

    if not has_targets:
        # Just dump the formatted tables to stdout for inspection
        if "rust" in data:
            print("=== Rust ===")
            print(build_rust_table(data["rust"]))
            print()
        if "python" in data:
            for phase in ("render", "compile", "end_to_end"):
                if phase in data["python"]:
                    print(f"=== Python ({phase}) ===")
                    print(
                        build_python_table(
                            data["python"][phase], PYTHON_ENGINES_README_RENDER
                        )
                    )
                    print()
        if "go" in data:
            pt = data["go"].get("pt", {})
            go = data["go"].get("go", {})
            if pt and go:
                print("=== Go (Render) ===")
                print(build_go_table(pt, go, "Render"))
                print()
                print("=== Go (RoundTrip) ===")
                print(build_go_table(pt, go, "RoundTrip"))
                print()
        if "ts" in data:
            ts = data["ts"]
            if "render" in ts:
                print("=== TypeScript (internal) ===")
                print(build_ts_internal_table(ts.get("render", {}), ts.get("unchecked", {})))
                print()
        if "ts_comparison" in data:
            tc = data["ts_comparison"]
            if "render" in tc:
                print("=== TypeScript (render comparison) ===")
                print(build_ts_comparison_render_table(tc.get("render", {}), tc.get("unchecked", {})))
                print()
            if "roundtrip" in tc:
                print("=== TypeScript (round-trip comparison) ===")
                print(build_ts_comparison_roundtrip_table(tc["roundtrip"]))
                print()
        if "wasm" in data:
            print("=== WASM vs TypeScript ===")
            print(build_wasm_table(data["wasm"]))
            print()

    if updated_any:
        print("✓ Benchmark tables updated.")
    elif has_targets:
        print("No changes were needed.")


if __name__ == "__main__":
    main()
