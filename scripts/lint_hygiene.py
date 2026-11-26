#!/usr/bin/env python3
"""Hygiene linter — flags suppression patterns and structural issues.

Run via `just lint-hygiene` or directly: `python3 scripts/lint_hygiene.py`

Exit code 0 = clean, 1 = violations found.

Escape hatch: add `// NOLINT: <reason>` (Rust/TS) on the line ABOVE a flagged
pattern to suppress it.  The reason is mandatory.
Bare `// NOLINT` without a reason is itself flagged as a violation.
Stale NOLINTs (where the next line doesn't trigger any check) are also flagged.
"""

import os
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

sys.dont_write_bytecode = True

# ── Configuration ────────────────────────────────────────────────────────────

MAX_FILE_LINES = 1200

RUST_DIRS = ["crates/", "benchmarks/"]
TS_DIRS = ["crates/md-tmpl-typescript/src/", "crates/md-tmpl-wasm/"]
RUST_EXTS = {".rs"}
TS_EXTS = {".ts", ".tsx"}
GO_DIRS = ["go/"]
GO_EXTS = {".go"}
PY_DIRS = ["crates/md-tmpl-python/python/"]
PY_EXTS = {".py"}
# Shared cross-backend test fixtures — covered by the long-file check only.
FIXTURE_DIRS = ["tests/shared/"]
FIXTURE_EXTS = {".toml"}
ALL_DIRS = RUST_DIRS + TS_DIRS + GO_DIRS + PY_DIRS
ALL_EXTS = RUST_EXTS | TS_EXTS | GO_EXTS | PY_EXTS

# Suppression markers.
NOLINT_WITH_REASON = re.compile(r"//\s*NOLINT:\s*\S")
NOLINT_ANY = re.compile(r"//\s*NOLINT")
NOLINT_BARE = re.compile(r"//\s*NOLINT\s*$")


# ── Check definitions ────────────────────────────────────────────────────────


@dataclass
class Check:
    """A single hygiene check definition."""

    name: str
    pattern: re.Pattern[str]
    dirs: list[str]
    exts: set[str]
    message: str
    exclude: re.Pattern[str] | None = None  # lines matching this are skipped
    exclude_path: re.Pattern[str] | None = None  # file paths matching this are skipped
    # When True, NOLINT comments are ignored — the check always fires.
    # Use for structural issues where suppression is never acceptable.
    no_nolint: bool = False
    # When True, skip lines inside #[cfg(test)] items/modules and line comments.
    prod_rust_only: bool = False
    hits: list[str] = field(default_factory=list)
    suppressed: list[str] = field(default_factory=list)


CHECKS: list[Check] = [
    # ── Rust: lint suppression ────────────────────────────────────────────
    Check(
        name="Rust: #[allow(...)]",
        pattern=re.compile(r"#\s*!?\s*\[\s*allow\s*\("),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Fix the underlying issue instead of suppressing the lint.",
        no_nolint=True,
    ),
    Check(
        name="Rust: #[expect(...)]",
        pattern=re.compile(r"#\s*!?\s*\[\s*expect\s*\("),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Fix the underlying issue instead of suppressing the lint.",
        # PyO3/FFI bindings legitimately need these.
        exclude_path=re.compile(r"md-tmpl-python/|md-tmpl-wasm/|md-tmpl-ffi/"),
    ),
    Check(
        name="Rust: too_many_lines suppression",
        pattern=re.compile(r"#\s*!?\s*\[\s*(allow|expect)\s*\(.*too_many_lines"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="NEVER suppress too_many_lines — split the function instead.",
        no_nolint=True,
    ),
    Check(
        name="Rust: type_complexity suppression",
        pattern=re.compile(r"#\s*!?\s*\[\s*(allow|expect)\s*\(.*type_complexity"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="NEVER suppress type_complexity — introduce a type alias instead.",
        no_nolint=True,
    ),
    Check(
        name="Rust: cognitive_complexity suppression",
        pattern=re.compile(r"#\s*!?\s*\[\s*(allow|expect)\s*\(.*cognitive_complexity"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="NEVER suppress cognitive_complexity — extract sub-functions.",
        no_nolint=True,
    ),
    Check(
        name="Rust: #[path = ...] module override",
        pattern=re.compile(r"#!?\s*\[\s*path\s*="),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Always use standard Rust <module>/mod.rs directory hierarchy instead of #[path = ...] overrides.",
        no_nolint=True,
    ),
    Check(
        name="Rust: json!() in production crate source",
        pattern=re.compile(r"\b(?:serde_json::)?json!\s*[\(\[\{]"),
        dirs=["crates/"],
        exts=RUST_EXTS,
        message="Define #[derive(Serialize, Deserialize)] structs instead of untyped json!() in production crates/*/src/.",
        exclude_path=re.compile(
            r"(?:^|/)crates/[^/]+/(?!src/)|/tests?/|(?:/|^)tests\.rs$|_tests?\.rs$"
        ),
        no_nolint=True,
        prod_rust_only=True,
    ),
    # ── Rust: silently discarded values ───────────────────────────────────
    Check(
        name="Rust: let _ = (ignored Result/value)",
        pattern=re.compile(r"\blet _\s*="),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Handle the Result/value properly — don't silently discard it.",
        # Legitimate in test code to prove types exist.
        exclude_path=re.compile(r"tests/"),
    ),
    Check(
        name="Rust: if let Ok(...) (silent Err drop)",
        pattern=re.compile(r"\bif let Ok\("),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="The Err branch is silently ignored. Use match, map_err, or ? to handle errors.",
        # Parsing attempts (parse::<type>) are idiomatic try-or-fallback.
        exclude=re.compile(r"\.parse::|try_from"),
        # Test files and FFI bindings use this legitimately.
        exclude_path=re.compile(r"tests/|_tests?\.rs$|md-tmpl-ffi/"),
    ),
    Check(
        name="Rust: .unwrap_or_default() (hidden errors)",
        pattern=re.compile(r"\.unwrap_or_default\(\)"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Silently replaces errors/None with defaults. Log or propagate instead.",
    ),
    Check(
        name="Rust: .ok() (Result→Option, error discarded)",
        pattern=re.compile(r"\.ok\(\)"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Converts Result to Option, silently discarding the error.",
        # .parse().ok() is idiomatic for optional parsing.
        exclude=re.compile(r"\.parse.*\.ok\(\)"),
    ),
    Check(
        name="Rust: .is_ok() / .is_err() (value discarded)",
        pattern=re.compile(r"\.(is_ok|is_err)\(\)"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Checks the Result but discards the inner value. Use match or ? instead.",
        # Legitimate in assertions, conditions, and boolean expressions.
        exclude=re.compile(r"assert|if |while |\|\||&&|return .*\.is_"),
        # Very common in test assertions; only flag production code.
        exclude_path=re.compile(r"tests/|_tests?\.rs$|validation\.rs$|include_core\.rs$"),
    ),
    Check(
        name="Rust: discarded closure arg |_|",
        pattern=re.compile(r"\|_\|"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Closure discards its argument. Name it and use it (log, propagate, etc.).",
        # Skip doc-comments, string literals, map_err(|_| ...) which rewrites errors.
        exclude=re.compile(r"^\s*///|^\s*//[^/].*\|_\||" + r'".*\|_\||map_err\(\|_\||unwrap_or_else\(\|_\|'),
        # FFI bindings legitimately use this for CString conversion.
        exclude_path=re.compile(r"md-tmpl-ffi/"),
    ),
    Check(
        name="Rust: Err(_) / Err(_err) (error value discarded in match)",
        pattern=re.compile(r"\bErr\(_\w*\)"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Error value is discarded in pattern match. Capture and log/propagate it.",
    ),
    Check(
        name="Rust: .unwrap_or(()) / .unwrap_or(0) (silent swallow)",
        pattern=re.compile(r"\.unwrap_or\(\s*(\(\)|\b0\b|false|true)\s*\)"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Silently swallows errors with a trivial default. Handle the error explicitly.",
        # size_hint().unwrap_or(0) and capacity hints are idiomatic.
        # .position().unwrap_or(0) / find().unwrap_or(0) is common.
        exclude=re.compile(r"size_hint|capacity|with_capacity|position|find"),
    ),
    # ── TypeScript: lint suppression ──────────────────────────────────────
    Check(
        name="TypeScript: @ts-ignore / @ts-expect-error",
        pattern=re.compile(r"@ts-ignore|@ts-expect-error"),
        dirs=TS_DIRS,
        exts=TS_EXTS,
        message="Fix the type error instead of suppressing it.",
    ),
    Check(
        name="TypeScript: eslint-disable",
        pattern=re.compile(r"eslint-disable"),
        dirs=TS_DIRS,
        exts=TS_EXTS,
        message="Fix the lint error instead of disabling the rule.",
    ),
    Check(
        name="TypeScript: @ts-nocheck",
        pattern=re.compile(r"@ts-nocheck"),
        dirs=TS_DIRS,
        exts=TS_EXTS,
        message="Whole-file type suppression — fix the type errors instead.",
    ),
    # ── TypeScript: type safety ───────────────────────────────────────────
    Check(
        name="TypeScript: as any (type escape)",
        pattern=re.compile(r"\bas\s+any\b"),
        dirs=TS_DIRS,
        exts=TS_EXTS,
        message="'as any' defeats TypeScript's type system. Use a proper type or generic.",
        # Exclude comments that happen to say "as any".
        exclude=re.compile(r"^\s*//|^\s*\*"),
    ),
    # ── TypeScript: swallowed errors ──────────────────────────────────────
    Check(
        name="TypeScript: .catch(() => null/undefined) (swallowed error)",
        pattern=re.compile(r"\.catch\(\s*\(\)\s*=>\s*(null|undefined|\{\s*\})"),
        dirs=TS_DIRS,
        exts=TS_EXTS,
        message="Promise error is silently swallowed. Log or propagate the error.",
    ),
    Check(
        name="TypeScript: non-null assertion (!) (unsafe)",
        # Match foo!. or foo![ — the TS non-null assertion operator.
        pattern=re.compile(r"\w![\.\[]"),
        dirs=TS_DIRS,
        exts=TS_EXTS,
        message="Non-null assertion will crash at runtime if value is null. Use ?? or handle the null case.",
        # Exclude comments.
        exclude=re.compile(r"^\s*//|^\s*\*"),
        # Exclude test and benchmark files.
        exclude_path=re.compile(r"\.test\.|\.spec\.|benchmarks/|correctness\.ts"),
    ),
    Check(
        name="TypeScript: console.log in production code",
        pattern=re.compile(r"\bconsole\.log\b"),
        dirs=TS_DIRS,
        exts=TS_EXTS,
        message="Use a proper logger or console.error/warn for intentional output.",
        # Exclude test, benchmark, test-runner files, and doc comments.
        exclude_path=re.compile(r"\.test\.|\.spec\.|benchmarks/|correctness\.ts"),
        exclude=re.compile(r"^\s*\*|^\s*//"),
    ),
    # ── Rust: panic-at-runtime markers ────────────────────────────────────
    Check(
        name="Rust: todo!() / unimplemented!()",
        pattern=re.compile(r"\b(todo|unimplemented)!\("),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Will panic at runtime. Implement or return an error.",
    ),
    # ── Test integrity: ignored/skipped tests are never allowed ───────────
    Check(
        name="Rust: #[ignore] (skipped test)",
        pattern=re.compile(r"#\[ignore\b"),
        dirs=RUST_DIRS,
        exts=RUST_EXTS,
        message="Tests must never be ignored — fix or delete the test.",
    ),
    Check(
        name="Go: t.Skip (skipped test)",
        pattern=re.compile(r"\b[tb]\.Skip(Now|f)?\("),
        dirs=GO_DIRS,
        exts=GO_EXTS,
        message="Tests must never be skipped — fix or delete the test.",
    ),
    Check(
        name="Python: pytest skip/xfail (skipped test)",
        pattern=re.compile(
            r"pytest\.mark\.(skip|skipif|xfail)|pytest\.(skip|xfail)\(|unittest\.skip"
        ),
        dirs=PY_DIRS,
        exts=PY_EXTS,
        message="Tests must never be skipped or xfail'd — fix or delete the test.",
    ),
    Check(
        name="TypeScript: skipped/focused test (.skip/.only/.todo)",
        pattern=re.compile(r"\b(describe|it|test|suite)\.(skip|only|todo)\b"),
        dirs=TS_DIRS,
        exts=TS_EXTS,
        message="Tests must never be skipped or focused — remove .skip/.only/.todo.",
    ),
    Check(
        name="Template source: single-line \\n-escaped frontmatter",
        pattern=re.compile(r"""['"`]---\\n"""),
        dirs=ALL_DIRS,
        exts=ALL_EXTS,
        message="NEVER use single-line \\n-escaped frontmatter strings; always use proper multiline strings.",
        no_nolint=True,
    ),
]


# ── Helpers ──────────────────────────────────────────────────────────────────


EXCLUDED_DIRS = {"node_modules", "target", "dist", "pkg", ".venv", "__pycache__"}


def source_files(dirs: list[str], exts: set[str]) -> list[Path]:
    """Collect all source files under `dirs` matching `exts`."""
    files: list[Path] = []
    for d in dirs:
        root = Path(d)
        if not root.is_dir():
            continue
        for path in root.rglob("*"):
            if path.is_file() and path.suffix in exts:
                # Skip files inside excluded directories.
                if any(part in EXCLUDED_DIRS for part in path.parts):
                    continue
                files.append(path)
    return sorted(files)


RUST_CFG_TEST_RE = re.compile(
    r"#\s*\[\s*cfg\s*\(\s*(?:test\b|all\s*\([^)]*\btest\b)"
)


def compute_rust_cfg_test_mask(lines: list[str]) -> list[bool]:
    """Return a 0-indexed mask marking lines inside `#[cfg(test)]` items/modules."""
    code_lines = [ln.split("//", 1)[0] for ln in lines]
    total = len(code_lines)
    in_test = [False] * total
    idx = 0
    while idx < total:
        if not RUST_CFG_TEST_RE.search(code_lines[idx]):
            idx += 1
            continue
        start = idx
        cursor = idx
        while cursor < total:
            stripped = code_lines[cursor].strip()
            if not stripped or stripped.startswith("#"):
                in_test[cursor] = True
                cursor += 1
                continue
            break
        depth = 0
        opened = False
        while cursor < total:
            in_test[cursor] = True
            line_code = code_lines[cursor]
            if not opened and ";" in line_code and "{" not in line_code:
                cursor += 1
                break
            for ch in line_code:
                if ch == "{":
                    depth += 1
                    opened = True
                elif ch == "}":
                    depth -= 1
            cursor += 1
            if opened and depth <= 0:
                break
        idx = max(cursor, start + 1)
    return in_test


def is_nolinted(lines: list[str], lineno: int) -> bool:
    """Check if the line above `lineno` (1-indexed) has a valid NOLINT: reason."""
    if lineno < 2:
        return False
    prev_line = lines[lineno - 2]  # lineno 1-indexed, list 0-indexed
    return bool(NOLINT_WITH_REASON.search(prev_line))


def line_matches_any_check(line: str, checks: list[Check]) -> bool:
    """Return True if `line` would trigger any check pattern (ignoring excludes)."""
    for check in checks:
        if check.pattern.search(line):
            return True
    return False


# ── Check runners ────────────────────────────────────────────────────────────


def run_pattern_checks() -> bool:
    """Run all pattern checks. Returns True if any failed.

    Tracks which NOLINT comments are consumed (suppress a real hit) so we can
    detect stale ones afterwards.
    """
    failed = False
    # Collect all NOLINT locations and whether they were consumed.
    # Key: (path, lineno of NOLINT line), value: consumed?
    nolint_locations: dict[tuple[Path, int], bool] = {}

    # First pass: find all NOLINT lines.
    all_dirs_exts: set[tuple[str, str]] = set()
    for check in CHECKS:
        for d in check.dirs:
            for ext in check.exts:
                all_dirs_exts.add((d, ext))
    all_dirs_set = {d for d, _ in all_dirs_exts}
    all_exts_set = {e for _, e in all_dirs_exts}
    for path in source_files(list(all_dirs_set), all_exts_set):
        try:
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        for lineno, line in enumerate(lines, start=1):
            if NOLINT_WITH_REASON.search(line):
                nolint_locations[(path, lineno)] = False  # not yet consumed

    # Second pass: run checks, marking consumed NOLINTs.
    for check in CHECKS:
        print(f"=== {check.name} ===")
        files = source_files(check.dirs, check.exts)
        for path in files:
            posix_path = path.as_posix()
            if check.exclude_path and check.exclude_path.search(posix_path):
                continue
            try:
                lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
            except OSError:
                continue
            cfg_test_mask = (
                compute_rust_cfg_test_mask(lines) if check.prod_rust_only else None
            )
            for lineno, line in enumerate(lines, start=1):
                if cfg_test_mask is not None and cfg_test_mask[lineno - 1]:
                    continue
                target_line = line.split("//", 1)[0] if check.prod_rust_only else line
                if not check.pattern.search(target_line):
                    continue
                if check.exclude and check.exclude.search(line):
                    continue
                if not check.no_nolint:
                    if is_nolinted(lines, lineno):
                        # Mark the NOLINT as consumed.
                        nolint_key = (path, lineno - 1)
                        if nolint_key in nolint_locations:
                            nolint_locations[nolint_key] = True
                        check.suppressed.append(
                            f"  {posix_path}:{lineno}: {line.strip()}"
                        )
                        continue
                check.hits.append(f"  {posix_path}:{lineno}: {line.strip()}")
        if check.hits:
            for hit in check.hits:
                print(hit)
            print(f"^^^ {check.message}")
            failed = True
        else:
            print("  ✓ clean")
        if check.suppressed:
            for sup in check.suppressed:
                print(f"  ℹ  (suppressed) {sup.strip()}")

    # Check for stale NOLINTs (not consumed by any check).
    print("=== Stale NOLINT comments ===")
    stale: list[str] = []
    for (path, lineno), consumed in sorted(nolint_locations.items()):
        if not consumed:
            try:
                lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
                line_text = lines[lineno - 1].strip() if lineno <= len(lines) else "???"
            except OSError:
                line_text = "???"
            stale.append(f"  {path.as_posix()}:{lineno}: {line_text}")
    if stale:
        for s in stale:
            print(s)
        print("^^^ NOLINT comment doesn't suppress anything — remove it.")
        failed = True
    else:
        print("  ✓ clean")

    return failed


def run_bare_nolint_check() -> bool:
    """Flag NOLINT comments that don't include a reason."""
    print("=== Bare NOLINT (missing reason) ===")
    hits: list[str] = []
    for path in source_files(ALL_DIRS, ALL_EXTS):
        try:
            content = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for lineno, line in enumerate(content.splitlines(), start=1):
            if NOLINT_BARE.search(line):
                hits.append(f"  {path.as_posix()}:{lineno}: {line.strip()}")
    if hits:
        for hit in hits:
            print(hit)
        print("^^^ NOLINT must include a reason: // NOLINT: <why this is acceptable>")
        return True
    print("  ✓ clean")
    return False


def run_empty_catch_check() -> bool:
    """Flag empty catch blocks in TypeScript.

    Detects patterns like:
        catch (err) {
        }
    where the body contains nothing meaningful.
    """
    print("=== TypeScript: empty catch blocks ===")
    hits: list[str] = []
    for path in source_files(TS_DIRS, TS_EXTS):
        try:
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        for i, line in enumerate(lines):
            stripped = line.strip()
            if not re.search(r"\bcatch\b.*\{", stripped):
                continue
            if is_nolinted(lines, i + 1):
                continue
            # Scan forward for closing brace, check if body is empty.
            body_has_content = False
            found_close = False
            for j in range(i + 1, min(i + 10, len(lines))):
                body = lines[j].strip()
                if body == "}":
                    found_close = True
                    break
                # Any non-blank line (including comments) means someone
                # thought about this catch block — it's not truly empty.
                if body:
                    body_has_content = True
                    break
            if found_close and not body_has_content:
                hits.append(f"  {path.as_posix()}:{i + 1}: {stripped}")
    if hits:
        for hit in hits:
            print(hit)
        print("^^^ Empty catch block — log or handle the error, don't swallow it.")
        return True
    print("  ✓ clean")
    return False


def run_long_file_check() -> bool:
    """Flag every source, test, and fixture file at or exceeding MAX_FILE_LINES."""
    print(f"=== Long files (>={MAX_FILE_LINES} lines) ===")
    long_files: list[tuple[Path, int]] = []

    seen: set[Path] = set()
    scanned = source_files(ALL_DIRS, ALL_EXTS) + source_files(FIXTURE_DIRS, FIXTURE_EXTS)
    for path in scanned:
        resolved = path.resolve()
        if resolved in seen:
            continue
        seen.add(resolved)
        try:
            with path.open(encoding="utf-8", errors="replace") as handle:
                line_count = sum(1 for _ in handle)
        except OSError:
            continue
        if line_count >= MAX_FILE_LINES:
            long_files.append((path, line_count))

    if long_files:
        long_files.sort(key=lambda x: -x[1])
        for path, count in long_files:
            print(f"  ⚠  {path.as_posix()} ({count} lines)")
        print(
            "^^^ Long file detected. Should be refactored to be modular "
            "and testable (with good testing) and a good folder structure."
        )
        return True

    print("  ✓ clean")
    return False


def run_packaging_invariant_check() -> bool:
    """Ensure the published `md-tmpl` crate ships no build script / build-deps.

    The compile-time `template!` test generator lives in the unpublished
    `md-tmpl-compile-tests` crate precisely so downstream consumers don't
    compile the toml/toml_edit/winnow tree (a build-dependency) or run a build
    script that produces nothing outside this workspace. This check fails if
    either is reintroduced into `crates/md-tmpl`.
    """
    print("=== Packaging: md-tmpl has no build script / build-dependencies ===")
    crate_dir = Path("crates/md-tmpl")
    hits: list[str] = []

    build_rs = crate_dir / "build.rs"
    if build_rs.is_file():
        hits.append(
            f"  {build_rs}: build script present — move it to "
            "crates/md-tmpl-compile-tests instead."
        )

    # Match `[build-dependencies]` and target-scoped variants such as
    # `[target.'cfg(...)'.build-dependencies]`, whether inline or dotted.
    manifest = crate_dir / "Cargo.toml"
    build_dep_header = re.compile(r"^\s*\[[^\]]*build-dependencies\]")
    try:
        for lineno, line in enumerate(manifest.read_text(encoding="utf-8").splitlines(), start=1):
            if build_dep_header.search(line):
                hits.append(f"  {manifest.as_posix()}:{lineno}: {line.strip()}")
    except OSError as err:
        # Don't silently ignore a read failure — surface it as a violation.
        hits.append(f"  {manifest}: could not read manifest ({err})")

    if hits:
        for hit in hits:
            print(hit)
        print(
            "^^^ The published md-tmpl crate must stay free of a build script "
            "and build-dependencies (adds compile cost to every downstream)."
        )
        return True
    print("  ✓ clean")
    return False


def run_synced_files_check() -> bool:
    """Verify non-symlink copies stay identical to their canonical sources.

    Those copies replace git symlinks so the repo builds cleanly on Windows
    when `core.symlinks` is disabled (the default on Git for Windows).
    """
    print("=== Synced crate copies (Windows-safe replacement for symlinks) ===")
    pairs = [
        (Path("AGENTS.md"), Path("AGENT.md")),
        (Path("SPEC.md"), Path("crates/md-tmpl/SPEC.md")),
    ]
    macros_prompts = Path("crates/md-tmpl-macros/prompts")
    if macros_prompts.is_dir():
        for copy_path in sorted(macros_prompts.glob("*.tmpl.md")):
            pairs.append((Path("crates/md-tmpl/prompts") / copy_path.name, copy_path))

    hits: list[str] = []
    for src, dst in pairs:
        if not src.is_file():
            hits.append(f"  missing canonical source: {src.as_posix()}")
            continue
        if not dst.is_file():
            hits.append(f"  missing copy: {dst.as_posix()}")
            continue
        src_norm = src.read_bytes().replace(b"\r\n", b"\n")
        dst_norm = dst.read_bytes().replace(b"\r\n", b"\n")
        if src_norm != dst_norm:
            hits.append(
                f"  {dst.as_posix()} differs from {src.as_posix()} — sync them"
            )

    if hits:
        for hit in hits:
            print(hit)
        print("^^^ Copy drifted from its canonical source.")
        return True
    print("  ✓ clean")
    return False


TS_IMPORT_FROM_RE = re.compile(
    r"(?m)^\s*import\s+([^;]+?)\s+from\s+[\"'](\.[^\"']+)[\"']\s*;?",
    re.S,
)
TS_EXPORT_FROM_RE = re.compile(
    r"(?m)^\s*export\s+((?:type\s+)?(?:\*(?:\s+as\s+[A-Za-z_$][\w$]*)?|\{[^;{}]*\}))\s+from\s+[\"'](\.[^\"']+)[\"']\s*;?",
    re.S,
)
TS_BARE_IMPORT_RE = re.compile(r"(?m)^\s*import\s+[\"'](\.[^\"']+)[\"']\s*;?")


def _strip_ts_comments(source: str) -> str:
    """Strip `//` and `/* ... */` comments while preserving string literals."""
    out: list[str] = []
    idx = 0
    length = len(source)
    while idx < length:
        ch = source[idx]
        if ch in ("'", '"', "`"):
            quote = ch
            out.append(ch)
            idx += 1
            while idx < length:
                cur = source[idx]
                out.append(cur)
                idx += 1
                if cur == "\\" and idx < length:
                    out.append(source[idx])
                    idx += 1
                elif cur == quote:
                    break
            continue
        if ch == "/" and idx + 1 < length:
            nxt = source[idx + 1]
            if nxt == "/":
                idx += 2
                while idx < length and source[idx] != "\n":
                    idx += 1
                continue
            if nxt == "*":
                idx += 2
                while idx + 1 < length and not (
                    source[idx] == "*" and source[idx + 1] == "/"
                ):
                    if source[idx] == "\n":
                        out.append("\n")
                    idx += 1
                idx += 2
                continue
        out.append(ch)
        idx += 1
    return "".join(out)


def _is_ts_type_only_clause(clause: str) -> bool:
    """Return True if an import/export clause imports only TypeScript types."""
    stripped = clause.strip()
    if (
        stripped.startswith("type ")
        or stripped.startswith("type{")
        or stripped.startswith("type*")
    ):
        return True
    brace_match = re.fullmatch(r"\{(.*)\}", stripped, re.S)
    if brace_match:
        items = [x.strip() for x in brace_match.group(1).split(",") if x.strip()]
        if items and all(re.match(r"^type\s+", item) for item in items):
            return True
    return False


def _resolve_ts_relative(
    src: Path, spec: str, known: set[Path]
) -> Path | None:
    """Resolve a relative TypeScript ESM specifier (`.js` -> `.ts`) within `known`."""
    target_spec = spec[:-3] + ".ts" if spec.endswith(".js") else spec
    candidate = (src.parent / target_spec).resolve()
    if candidate in known:
        return candidate
    if candidate.with_suffix(".ts") in known:
        return candidate.with_suffix(".ts")
    index_ts = candidate / "index.ts"
    if index_ts in known:
        return index_ts
    return None


def run_ts_runtime_cycle_check() -> bool:
    """Detect runtime circular imports in `crates/md-tmpl-typescript/src/`.

    Type-only imports (`import type ...` / `import { type X }`) are erased at
    compile time and allowed; runtime cycles break Closure/tsickle and ESM
    topological sorting and must never be introduced.
    """
    print("=== TypeScript: runtime import cycles ===")
    ts_src = Path("crates/md-tmpl-typescript/src")
    if not ts_src.is_dir():
        print("  ✓ clean")
        return False

    files = sorted(
        p.resolve()
        for p in ts_src.rglob("*.ts")
        if p.is_file()
        and "tests" not in p.parts
        and not any(part in EXCLUDED_DIRS for part in p.parts)
    )
    known = set(files)
    adj: dict[Path, set[Path]] = {p: set() for p in files}

    for path in files:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        no_comments = _strip_ts_comments(text)
        for pattern in (TS_IMPORT_FROM_RE, TS_EXPORT_FROM_RE):
            for match in pattern.finditer(no_comments):
                if _is_ts_type_only_clause(match.group(1)):
                    continue
                target = _resolve_ts_relative(path, match.group(2), known)
                if target is not None and target != path:
                    adj[path].add(target)
        for match in TS_BARE_IMPORT_RE.finditer(no_comments):
            target = _resolve_ts_relative(path, match.group(1), known)
            if target is not None and target != path:
                adj[path].add(target)

    # Tarjan's strongly connected components algorithm.
    counter = 0
    indices: dict[Path, int] = {}
    lowlink: dict[Path, int] = {}
    stack: list[Path] = []
    on_stack: set[Path] = set()
    sccs: list[list[Path]] = []

    def strongconnect(node: Path) -> None:
        nonlocal counter
        indices[node] = counter
        lowlink[node] = counter
        counter += 1
        stack.append(node)
        on_stack.add(node)
        for neighbor in sorted(adj[node]):
            if neighbor not in indices:
                strongconnect(neighbor)
                lowlink[node] = min(lowlink[node], lowlink[neighbor])
            elif neighbor in on_stack:
                lowlink[node] = min(lowlink[node], indices[neighbor])
        if lowlink[node] == indices[node]:
            component: list[Path] = []
            while True:
                member = stack.pop()
                on_stack.remove(member)
                component.append(member)
                if member == node:
                    break
            if len(component) > 1:
                sccs.append(sorted(component))

    for node in files:
        if node not in indices:
            strongconnect(node)

    if sccs:
        ts_src_resolved = ts_src.resolve()
        for idx, comp in enumerate(sccs, start=1):
            comp_set = set(comp)
            print(f"  Cycle #{idx} ({len(comp)} modules):")
            for member in comp:
                rel = member.relative_to(ts_src_resolved).as_posix()
                outs = sorted(
                    n.relative_to(ts_src_resolved).as_posix()
                    for n in adj[member]
                    if n in comp_set
                )
                print(f"    {rel} -> {outs}")
        print(
            "^^^ Runtime circular import detected in TypeScript source — "
            "extract shared helpers into a leaf module or inject callbacks."
        )
        return True

    print("  ✓ clean")
    return False


# ── Main ─────────────────────────────────────────────────────────────────────


def check_workspace(
    root_dir: Path, *, check_packaging: bool = True
) -> bool:
    """Run hygiene checks rooted at `root_dir`. Returns True if any check failed."""
    prev_cwd = Path.cwd()
    os.chdir(root_dir)
    try:
        for check in CHECKS:
            check.hits.clear()
            check.suppressed.clear()
        failed = run_pattern_checks()
        failed = run_bare_nolint_check() or failed
        failed = run_empty_catch_check() or failed
        failed = run_ts_runtime_cycle_check() or failed
        if check_packaging:
            failed = run_packaging_invariant_check() or failed
            failed = run_synced_files_check() or failed
        failed = run_long_file_check() or failed
        return failed
    finally:
        os.chdir(prev_cwd)


def main() -> int:
    # Ensure Unicode status markers don't raise UnicodeEncodeError on legacy
    # Windows consoles (e.g. cp1252).
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")

    repo_root = Path(__file__).resolve().parent.parent
    failed = check_workspace(repo_root, check_packaging=True)

    print()
    if failed:
        print("❌ Hygiene check failed — see above")
        return 1
    print("✅ All hygiene checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())

