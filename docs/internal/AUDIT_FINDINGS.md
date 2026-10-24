# Audit findings — 2026-10-19

This file records the repository-specific findings from the portfolio audit
conducted on 2026-10-19. It is documentation only; no code or release artifacts
were changed in this pass unless explicitly noted under "Implementation status".

Sources: the repository's own README/source/workflows, GitHub metadata and
Actions runs, PyPI metadata, and official competitor documentation. Claims are
marked as verified, inferred, or open questions. "Absent" means not found in the
inspected repository/docs, not proof that it cannot exist.

## What it is

Rust benchmark runner and performance-regression guard: wall/CPU/RSS, statistics, Git-aware baselines, policies, and CI-friendly artifacts.

## Verified strengths

- Rust CLI; median/quantiles/CV/Mann-Whitney; regression/warning/inconclusive policy; Git-aware baselines; JSON/MD/CSV/SVG; redaction; property tests; example GitHub workflow.
- Vendor-neutral, policy-driven regression guarding is a real wedge.

## Verified gaps and user-facing risks

- No crates.io/PyPI/prebuilt binary; install is `cargo install --path .`.
- No shared baseline storage; no profiling; CPU/RSS only on Unix; no external users.

## Direct competitors

- Bencher (bare-metal continuous benchmarking), CodSpeed (CI performance), hyperfine (stats/outliers), pytest-benchmark/criterion.

## Ranked next actions

- P1: publish crate + prebuilt binaries + official GitHub Action (M).
- P2: shared baseline storage (S3/GH artifacts) (M).

## Do not build

- Flamegraph/profiling now; later and not core.

## Evidence gaps / validation needed

- No external benchmark; no noise study across CI runners.

## Recommended disposition

Invest; distribution is the only blocker.

## Implementation status

Added `baseline export`/`import` for CI artifact sharing, with a round-trip
test. Version `0.2.0`. Publishing to crates.io/prebuilt binaries remains open.
