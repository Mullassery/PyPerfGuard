# PyPerfGuard

[![PyPI](https://img.shields.io/pypi/v/pyperfguard.svg)](https://pypi.org/project/pyperfguard/)

**Make performance regressions measurable, explainable, reproducible, and enforceable.**

PyPerfGuard is a native benchmark runner and performance-regression guard written in
Rust. It runs commands, scripts, and binaries; measures wall-clock time and process
resource usage; compares results against baselines (including Git revisions); evaluates
configurable regression policies; and stores versioned result artifacts for historical
trend analysis. It needs **no hosted service** and integrates with any CI system,
including GitHub Actions.

## What makes it different from a basic benchmark runner

- **Honest measurement semantics.** Missing measurements are reported as `n/a`, never as
  zero. Failed executions are never silently treated as valid observations.
- **Statistics, not just averages.** Median, mean, min/max, standard deviation, quantiles,
  coefficients of variation, and a non-parametric Mann-Whitney U test for significance.
- **A clear separation between "over budget" and "statistically real".** A crossed
  threshold can be reported as a *regression*, a *warning*, or *inconclusive* depending on
  the evidence and the configured policy.
- **Git-aware baselines.** Record the commit, branch, environment, and tool version with
  every result, and compare across revisions or stored artifacts.
- **Machine-readable everything.** Versioned JSON result artifacts, JSON check reports,
  Markdown job summaries, CSV exports, and SVG trend charts.
- **Native, no Python required.** The core is a standalone Rust CLI. (The `Py` prefix
  reflects the project's Python-benchmarking origins; an optional Python interface may be
  added later, but is never required.)

## Supported platforms and measurement capabilities

| Capability | macOS | Linux | Other |
| --- | --- | --- | --- |
| Wall-clock time | yes | yes | yes |
| CPU time (user + system) | yes | yes | unavailable |
| Peak RSS | yes (bytes) | yes (KiB) | unavailable |
| CPU utilization (derived) | yes | yes | unavailable |
| Timeout + process-group kill | yes | yes | limited |

Measurement uses `wait4(2)` per child process. On macOS `ru_maxrss` is reported in
**bytes**; on Linux it is reported in **kilobytes**. PyPerfGuard converts to bytes and
records the method in every result. See [`docs/adr/0003-measurement-semantics.md`](docs/adr/0003-measurement-semantics.md).

> **Reproducibility limits.** Benchmark results depend on external state: other running
> processes, thermal throttling, CPU frequency scaling, network calls, filesystem cache,
> and nondeterministic program input. PyPerfGuard records environment metadata and reports
> caveats, but it cannot make a noisy machine quiet.

## Installation

### From PyPI

```sh
pip install pyperfguard
pyperfguard --version
```

The PyPI package ships the native binary as a platform wheel, so there is no
Rust toolchain or separate build step. Wheels are published per platform; if no
wheel exists for yours yet, use one of the source installs below.

### From source (requires a Rust toolchain)

```sh
git clone https://github.com/Mullassery/PyPerfGuard.git
cd pyperfguard
cargo build --release
./target/release/pyperfguard --version
```

### Install into `~/.cargo/bin`

```sh
cargo install --path .
```

## A first benchmark

```sh
pyperfguard init            # writes pyperfguard.toml
pyperfguard run example-sleep
```

Sample output:

```text
benchmark: example-sleep
  Example workload; replace with your own command.
  command:   python3 -c import time; time.sleep(0.02)
  revision:  (no git)
  status:    Completed
  samples:   5 successful / 0 failed (measured)

  metric            n   median        mean          min           max
  wall_time          5   42.63ms      42.72ms      41.07ms      44.64ms
  peak_rss           5   8.31 MiB     8.32 MiB     8.30 MiB     8.33 MiB
  cpu_time           5   15.20ms      15.15ms      14.84ms      15.43ms
```

You can also run an ad-hoc command without a configuration file:

```sh
pyperfguard run --repetitions 10 -- ./my-binary --input fixture.txt
```

## A baseline comparison

```sh
pyperfguard baseline set --benchmark example-sleep --latest
pyperfguard compare example-sleep 'example-sleep@latest' --format markdown
```

`compare` accepts `latest`, `benchmark-id@<commit-sha>`, or a path to a result artifact.

Share a baseline registry through CI artifacts:

```sh
pyperfguard baseline export --output baselines.json
pyperfguard baseline import baselines.json
```

## A regression policy

Policies live in `pyperfguard.toml`:

```toml
[[benchmarks.policy.thresholds]]
metric = "wall_time"
max_increase_pct = 5.0     # any example threshold is configurable
severity = "block"          # block = fail CI (exit 1)

[[benchmarks.policy.thresholds]]
metric = "peak_rss"
max_increase_pct = 10.0
severity = "warn"           # warn = report only (exit 0)
```

Then gate your build:

```sh
pyperfguard check example-sleep   # exit 1 on a blocking regression
```

## Local historical reporting

```sh
pyperfguard history --benchmark example-sleep
pyperfguard report --benchmark example-sleep --format csv  --metric wall_time
pyperfguard report --benchmark example-sleep --format svg  --metric wall_time --output trend.svg
pyperfguard report --benchmark example-sleep --format json
```

## GitHub Actions integration

A complete example lives in
[`.github/workflows/benchmark-example.yml`](.github/workflows/benchmark-example.yml).
It checks out the code, fetches the baseline artifact, builds PyPerfGuard, runs the
benchmark, saves a machine-readable artifact, compares against the baseline, evaluates
policies, writes a Markdown job summary, and exits according to policy.

The same flow can be simulated locally without GitHub:

```sh
scripts/local_ci.sh
```

## Queue status

Shipped, blocked/failed, and rejected work for this repository is documented in
[`QUEUE_STATUS.md`](QUEUE_STATUS.md).

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — internal boundaries and data flow.
- [`docs/configuration.md`](docs/configuration.md) — full configuration reference.
- [`docs/exit-codes.md`](docs/exit-codes.md) — exit-code contract.
- [`docs/troubleshooting.md`](docs/troubleshooting.md) — common problems.
- [`docs/adr/`](docs/adr/) — architecture decision records.
- [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) — phased plan and status.
- [`WORKLOG.md`](WORKLOG.md) — continuously maintained engineering log.

## Known limitations

- Peak RSS and CPU time require a Unix platform (`wait4`).
- PyPerfGuard is **not** a security sandbox. It executes arbitrary user-supplied
  programs with the permissions of the invoking user. Timeouts and process-group
  termination are best-effort and are not a security boundary.
- Statistical significance is not claimed when the sample size is too small; such
  comparisons are reported as *inconclusive*.
- Profiling/flamegraph support is an optional, later capability and is not universal
  across platforms.

## Security considerations

- Commands are explicit argument arrays; there is **no** implicit shell expansion.
- Sensitive environment variables can be redacted from logs via `redact_env`.
- Imported result artifacts are validated and are never executed.
- Local writes are atomic (temp file + rename), and a corrupt history index is rebuilt
  from the result files.

See [`SECURITY.md`](SECURITY.md).

## Development

```sh
cargo fmt --all -- --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

Apache-2.0. See [`LICENSE`](LICENSE).
