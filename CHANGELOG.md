# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial native Rust CLI with commands: `init`, `run`, `compare`, `baseline`,
  `check`, `report`, `history`, `validate`, `doctor`, `version`.
- Benchmark execution engine with warm-ups, measured repetitions, per-run
  timeouts, graceful then forceful (process-group) termination, and output
  capture with a size bound.
- Measurement engine using `wait4(2)`: wall-clock time, CPU time (user +
  system), peak RSS (bytes on macOS, KiB on Linux), and derived CPU utilization.
  Missing metrics are represented as `null`.
- Versioned JSON result schema (schema version 1) with atomic persistence, a
  rebuildable history index, and retention pruning.
- Statistical analysis: median, mean, min/max, sample standard deviation,
  type-7 quantiles, IQR, coefficient of variation, and a non-parametric
  Mann-Whitney U test.
- Baseline manager supporting `latest`, an explicit artifact path, and commit
  SHA selectors, with a persistent registry.
- Regression policy engine with absolute/relative thresholds, per-metric
  severities, noise tolerance, minimum-sample requirements, and pass / info /
  inconclusive / warning / regression / error classification.
- Reporting: terminal summaries, JSON, Markdown (including GitHub job
  summaries), CSV exports, sparklines, and SVG trend charts.
- Git-aware revision metadata and `doctor` capability reporting.
- Documented exit codes and a colour-free machine-readable output path.
- Test suite: unit tests, CLI integration tests, and property-based tests for
  statistical invariants.

[Unreleased]: https://github.com/example/pyperfguard/commits/HEAD
