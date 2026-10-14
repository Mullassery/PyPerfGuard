#!/usr/bin/env bash
#
# Local CI simulation for PyPerfGuard.
#
# Verifies the same exit-code and reporting behavior that
# .github/workflows/benchmark-example.yml relies on, without requiring a GitHub
# repository, network access, or the GitHub API.
#
# Usage:
#   scripts/local_ci.sh
#
# Environment:
#   PYPERFGUARD_BIN   Path to an existing pyperfguard binary (skips building).
#   PYG_SKIP_BUILD    Set to 1 to reuse target/release/pyperfguard as-is.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -n "${PYPERFGUARD_BIN:-}" ]]; then
  PYG="$PYPERFGUARD_BIN"
elif [[ "${PYG_SKIP_BUILD:-0}" == "1" ]]; then
  PYG="$ROOT/target/release/pyperfguard"
else
  echo "==> Building pyperfguard (release)"
  cargo build --release --manifest-path "$ROOT/Cargo.toml"
  PYG="$ROOT/target/release/pyperfguard"
fi

if [[ ! -x "$PYG" ]]; then
  echo "error: pyperfguard binary not found or not executable: $PYG" >&2
  exit 2
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/pyperfguard-ci.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

pass=0
fail=0

# expect_exit <expected> <description> <command...>
expect_exit() {
  local expected="$1" desc="$2"; shift 2
  local actual=0
  set +e
  "$@" >/dev/null 2>&1
  actual=$?
  set -e
  if [[ "$actual" == "$expected" ]]; then
    printf 'ok   - %s (exit %s)\n' "$desc" "$actual"
    pass=$((pass + 1))
  else
    printf 'FAIL - %s (expected exit %s, got %s)\n' "$desc" "$expected" "$actual" >&2
    fail=$((fail + 1))
  fi
}

echo "==> Producing baseline (fast) result"
"$PYG" run --repetitions 7 --warmups 1 --no-store --output fast.json -- \
  /bin/sh -c 'sleep 0.01' >/dev/null

echo "==> Producing regressed (slow) result"
"$PYG" run --repetitions 7 --warmups 1 --no-store --output slow.json -- \
  /bin/sh -c 'sleep 0.06' >/dev/null

echo "==> Checking policy outcomes"

# A comparison against itself must pass.
expect_exit 0 "pass: unchanged vs baseline" \
  "$PYG" check --result fast.json --baseline fast.json \
  --max-time-increase-pct 5

# A large wall-time increase must block (exit 1).
expect_exit 1 "regression: 6x wall-time increase blocks" \
  "$PYG" check --result slow.json --baseline fast.json \
  --max-time-increase-pct 5

# A generous budget must not block.
expect_exit 0 "pass: large budget tolerates the change" \
  "$PYG" check --result slow.json --baseline fast.json \
  --max-time-increase-pct 1000

echo "==> Rendering a report (as the workflow job summary does)"
"$PYG" compare fast.json slow.json --format markdown > compare.md
if [[ -s compare.md ]]; then
  printf 'ok   - markdown report produced (%s bytes)\n' "$(wc -c < compare.md | tr -d ' ')"
  pass=$((pass + 1))
else
  printf 'FAIL - markdown report was empty\n' >&2
  fail=$((fail + 1))
fi

echo
if [[ "$fail" -eq 0 ]]; then
  echo "local CI: all ${pass} checks passed"
  exit 0
else
  echo "local CI: ${fail} of $((pass + fail)) checks failed" >&2
  exit 1
fi
