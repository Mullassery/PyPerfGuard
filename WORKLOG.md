# Engineering Worklog

Continuous log of decisions, progress, blockers, and verification. Newest
entries at the top.

## 2026-10-14 — Phase 9: release hardening (0.1.0)

- Resolved all `cargo clippy --all-targets -- -D warnings` findings:
  - `config.rs`: derived `Default` with `#[default]` for `OutputMode`/`Severity`;
    documented `from_str` kept with a targeted `#[allow]`.
  - `stats.rs`, `results.rs`, `runner.rs`, `report.rs`, `git.rs`: idiomatic
    rewrites (iterator-based loops, `push('\n')`, `Option::filter`).
- `cargo fmt --all` applied; `cargo fmt --all -- --check` is clean.
- Full suite green: 71 library unit tests, 20 CLI integration tests, 11
  property-based tests (102 total).
- Wrote `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`,
  `IMPLEMENTATION_PLAN.md`, `docs/architecture.md`, `docs/configuration.md`,
  `docs/exit-codes.md`, `docs/troubleshooting.md`, ADR-0001..0006.
- Added `examples/pyperfguard.toml`, CI workflows, and `scripts/local_ci.sh`.

## 2026-10-14 — Phases 5–8

- Reporting: terminal/markdown/csv/svg/sparkline renderers; deterministic except
  timestamps.
- History index, retention pruning, artifact import/export.
- GitHub Actions example and local CI simulation plan.
- ADR-0006 documents the optional instrumentation adapter; no profiler shipped
  (Phase 7 partial). Python interface deferred (Phase 8 not started).

## 2026-10-14 — Phase 4: baselines and policy

- Added `BaselineManager` with `latest`, artifact path, and commit SHA
  selectors; selection happens before the current run in `check` to prevent
  self-comparison.
- `policy` classifies per-rule outcomes with documented precedence.
- Verified exit-code behavior end to end: pass → 0, regression → 1, usage → 2,
  execution → 3, inconclusive (blocking) → 4.

## 2026-10-14 — Phase 3: statistics

- Implemented Mann-Whitney U (tie- and continuity-corrected, normal
  approximation), type-7 quantiles, sample stddev, IQR, CV.
- Established the rule that significance is only claimed with adequate sample
  sizes; otherwise inconclusive.

## 2026-10-14 — Phase 2: measurement

- `wait4(2)`-based executor with per-child rusage and process-group isolation.
- Peak RSS normalized to bytes (macOS bytes, Linux KiB); CPU utilization
  derived from CPU time / wall time.
- Missing metrics represented as `null`; `doctor` reports capability.

## 2026-10-14 — Phase 1: minimal CLI

- End-to-end `init → run → store` with strict TOML validation and versioned
  JSON artifacts written atomically.

## 2026-10-14 — Phase 0: setup

- Confirmed environment: macOS 25.x / aarch64, Rust/Cargo 1.97.1, Python 3.9.6,
  no `/proc`. Bootstrapped the standalone crate and ADRs.

## Blockers

- None. Phase 7 (profilers) and Phase 8 (Python interface) are intentionally
  deferred per ADR-0001/0006 and are not release blockers for 0.1.0.
