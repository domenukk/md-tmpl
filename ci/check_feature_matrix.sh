#!/usr/bin/env bash
# ci/check_feature_matrix.sh — Verify all feature combinations compile cleanly.
#
# This script checks every meaningful combination of features for the
# `md-tmpl` crate.  It is designed to catch regressions in the
# no_std / alloc / serde / typed-builder feature gating.
#
# Usage:  ./ci/check_feature_matrix.sh
# Exit:   0 on success, 1 on first failure.

set -euo pipefail

CRATE="md-tmpl"
PASS=0
FAIL=0

check() {
    local desc="$1"
    shift
    printf "  %-45s " "$desc"
    local tmpout
    tmpout=$(mktemp)
    if cargo "$@" >"$tmpout" 2>&1 && grep -qE 'Finished|test result: ok' "$tmpout"; then
        printf "✅\n"
        PASS=$((PASS + 1))
    else
        printf "❌\n"
        echo "    FAILED: cargo $*"
        grep '^error' "$tmpout" | head -5
        FAIL=$((FAIL + 1))
    fi
    rm -f "$tmpout"
}

echo "╔══════════════════════════════════════════════════╗"
echo "║  Feature matrix check: $CRATE            ║"
echo "╚══════════════════════════════════════════════════╝"
echo ""

echo "── Compile checks (host) ─────────────────────────"
check "no_std (bare)"                     check -p "$CRATE" --no-default-features
check "no_std + serde"                    check -p "$CRATE" --no-default-features --features serde
check "no_std + typed-builder"            check -p "$CRATE" --no-default-features --features typed-builder
check "no_std + serde + typed-builder"    check -p "$CRATE" --no-default-features --features serde,typed-builder
check "std (default features)"            check -p "$CRATE"
check "std + serde only"                  check -p "$CRATE" --no-default-features --features std,serde
check "std + typed-builder only"          check -p "$CRATE" --no-default-features --features std,typed-builder
check "all features"                      check -p "$CRATE" --all-features

echo ""
echo "── True no_std target (thumbv7em-none-eabihf) ───"
NO_STD_TARGET="thumbv7em-none-eabihf"
check "no_std target (bare)"              build -p "$CRATE" --no-default-features --target "$NO_STD_TARGET"
check "no_std target + serde"             build -p "$CRATE" --no-default-features --features serde --target "$NO_STD_TARGET"
check "no_std target + typed-builder"     build -p "$CRATE" --no-default-features --features typed-builder --target "$NO_STD_TARGET"
check "no_std target + serde + tb"        build -p "$CRATE" --no-default-features --features serde,typed-builder --target "$NO_STD_TARGET"

echo ""
echo "── Test checks ─────────────────────────────────"
check "lib tests (default features)"      test  -p "$CRATE" --lib
check "no_std integration tests"          test  -p "$CRATE" --no-default-features --test no_std_compat
check "integration tests (all features)"  test  -p "$CRATE" --test no_std_compat

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Results: $PASS passed, $FAIL failed"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
