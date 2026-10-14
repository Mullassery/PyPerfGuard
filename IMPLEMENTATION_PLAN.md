# Implementation Plan

This plan is executed sequentially. Status is updated only after a phase's
capabilities are implemented **and** verified by tests. See `WORKLOG.md` for the
running engineering log.

Status legend: **Done** · **Partial** · **Not started**

## Phase 0 — Repository reconnaissance and design — **Done**

- Inspected the environment: macOS 25.x / aarch64, Rust 1.97.1, Cargo 1.97.1,
  Python 3.9.6, no `/proc` (macOS), `wait4`/`getrusage` available.
- Initialized a standalone Cargo project; established README, LICENSE
  (Apache-2.0), `.gitignore`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`,
  `Cargo.toml`, source/test directories, docs and ADRs.
- Defined scope, architectural boundaries, and supported-platform assumptions.

**Acceptance:** repository scaffold and ADRs exist; `cargo build` succeeds.

## Phase 1 — Minimal functional CLI — **Done**

- CLI with `init`, `run`, `baseline`, `check`, `compare`, `report`, `history`,
  `validate`, `doctor`, `version`.
- TOML configuration loader with strict validation.
- Benchmark runner with warm-ups, repetitions, timeouts, exit-status capture,
  and process-group termination.
- Versioned JSON result schema with atomic persistence and a rebuildable index.
- Tests: config validation, execution, timeout, store integrity.

**Acceptance:** `run` executes a command and stores a versioned result.

## Phase 2 — Resource measurement — **Done**

- Peak RSS and CPU time via `wait4(2)`; per-platform unit handling; derived CPU
  utilization.
- Unsupported capabilities reported as `null`; `doctor` reports availability.
- Documented in ADR-0003.

**Acceptance:** results contain CPU time and peak RSS on Unix; unavailable on
other platforms.

## Phase 3 — Statistical analysis — **Done**

- Robust summaries, type-7 quantiles, IQR, CV, Mann-Whitney U with tie
  correction, minimum-sample handling, inconclusive outcomes.
- Property-based tests for invariants; documented in ADR-0004.

**Acceptance:** comparisons report significance only when the sample size
supports it; otherwise inconclusive.

## Phase 4 — Baselines and regression policies — **Done**

- Baseline registry and selection (`latest`, artifact path, commit SHA).
- Git revision metadata.
- Configurable thresholds with severities, noise tolerance, and precedence.
- Exit codes for pass / regression / config / execution / inconclusive.

**Acceptance:** a synthetic regression yields exit code 1; a warn threshold does
not block.

## Phase 5 — Reporting and historical results — **Done**

- Terminal, JSON, Markdown, CSV, sparkline, and SVG outputs.
- Local history index, retention pruning, artifact import/export.
- Deterministic output except timestamps.

**Acceptance:** CSV/SVG/Markdown generated from stored history.

## Phase 6 — CI integration — **Done**

- Working GitHub Actions example with artifact-based baseline retrieval and a
  job summary.
- Local CI simulation (`scripts/local_ci.sh`) verifying identical exit-code and
  reporting behavior without GitHub.

**Acceptance:** the example workflow and local simulation both gate on the
policy exit code.

## Phase 7 — Advanced instrumentation — **Partial**

- Documented adapter interface for external profilers (ADR-0006). No profiler
  implementation is shipped yet; the interface and artifact association are
  specified but not yet exposed as a stable API.

**Acceptance (pending):** an optional adapter produces an artifact associated
with a benchmark run, with overhead clearly distinguished.

## Phase 8 — Python interface — **Not started**

- Deferred until the Rust CLI, configuration, and schemas are stable. The core
  remains Rust-only and independently usable.

**Acceptance (pending):** an optional binding exposes a small API and is tested
independently; the core builds and tests without Python.

## Phase 9 — Release hardening — **Done (for 0.1.0)**

- Full test, fmt, check, and clippy (deny warnings) passes on macOS.
- Dependency review; documentation consistency pass; installation validated
  (`cargo build --release`, `cargo install --path .`).
- Release checklist in `WORKLOG.md`.

**Acceptance:** all quality gates pass locally; limitations documented.
