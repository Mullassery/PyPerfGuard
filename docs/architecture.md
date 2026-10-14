# Architecture

PyPerfGuard is a single Rust crate with internal boundaries that map to
independently testable engines. Domain logic never depends on the CLI or on
GitHub, so it can be unit tested in isolation.

## Engines and modules

| Engine | Module | Responsibility |
| --- | --- | --- |
| A. CLI and configuration | `cli`, `config` | Argument parsing, validation, defaults, config loading, exit codes |
| B. Benchmark execution | `process`, `runner` | Workload execution, warm-ups, repetitions, timeouts, exit status, process lifecycle |
| C. Measurement | `process`, `results`, `metric` | Timing and process metrics, provenance, units, availability |
| D. Statistical analysis | `stats`, `compare` | Summary statistics, variability, significance, sample validation |
| E. Baseline manager | `baseline`, `store`, `git` | Baseline identification, selection, persistence, revision metadata |
| F. Regression policy | `policy` | Thresholds, tolerances, severities, outcome classification |
| G. Results store | `store`, `results` | Versioned schema, atomic persistence, history index, integrity |
| H. Reporting | `report` | Terminal, JSON, Markdown, CSV, SVG |
| I. Git and CI | `git`, `commands` | Revision metadata, job summaries, exit-code behavior |
| J. Optional adapters | *(future)* | Profilers and instrumentation behind a documented interface |

## Dependency direction

```text
            cli ──► commands ──► runner ──► process
                               ├─► baseline ──► store ──► results
                               ├─► policy   ──► compare ──► stats
                               ├─► report
                               └─► git
config ◄────────────────────── everything
error, metric, time_util ────── leaf utilities (no internal deps)
```

There are no cycles. `stats`, `metric`, and `time_util` are leaves. `config`
depends only on `metric`. `policy` depends on `compare` and `config`. Nothing
below `commands` depends on `cli` or on GitHub.

## Data flow for `check`

```text
config ──► runner ──► BenchmarkResult ──┐
                                        ├──► compare ──► policy ──► PolicyReport ──► exit code
baseline (store/git/artifact) ──────────┘                                    └──► Markdown summary
```

1. The baseline is selected **before** the current run so a run can never be
   compared against itself.
2. `runner` produces a versioned `BenchmarkResult` (raw observations plus
   summaries).
3. `compare` validates compatibility and computes per-metric deltas, caveats,
   and a Mann-Whitney U test.
4. `policy` classifies each rule as pass / info / inconclusive / warning /
   regression / error and combines them by severity.

## Measurement provenance

Every metric in a result records:
- machine name and unit;
- measurement method (for example, `getrusage(ru_maxrss) via wait4; bytes on macOS`);
- run status and success flag;
- timestamp, benchmark id, revision, tool version, and schema version.

Availability is represented as an absent value (`null`), never as zero.

## Repository isolation

PyPerfGuard is self-contained. It does not import from, depend on, or modify
sibling repositories and creates no shared workspace. Optional external tools
are integrated only through documented adapters; the core remains independently
useful. `tests/cli.rs::does_not_access_sibling_repositories` and the absence of
path traversal outside configured directories enforce this boundary in practice.
