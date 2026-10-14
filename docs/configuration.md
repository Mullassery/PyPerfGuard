# Configuration reference

PyPerfGuard uses **TOML** as its canonical configuration format
([ADR-0002](adr/0002-configuration-format.md)). The file is discovered by
walking up from the current directory looking for `pyperfguard.toml` (or
`.pyperfguard.toml`). Override with `--config <path>`.

Unknown fields are rejected to catch typos early.

## Top-level tables

```toml
[project]
name = "my-project"
baseline = "latest"   # optional default baseline selector

[output]
dir = ".pyperfguard"  # local history directory
retain = 100          # max results per benchmark (0 = unlimited)

[[benchmarks]]
# one entry per benchmark
```

### `[project]`

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `name` | string | `pyperfguard-project` | Project name |
| `baseline` | string | *(none)* | Default baseline selector: `latest`, a `.json` path, or a commit SHA |

### `[output]`

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `dir` | string | `.pyperfguard` | Directory for results, index, and baselines |
| `retain` | integer | `100` | Maximum stored results per benchmark; oldest are pruned |

## `[[benchmarks]]`

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `id` | string | *(required)* | Stable identifier (`[A-Za-z0-9._-]`) |
| `description` | string | `""` | Human-readable description |
| `command` | array of strings | *(required)* | Executable followed by arguments |
| `cwd` | string | config dir | Working directory |
| `warmups` | integer | `1` | Unmeasured iterations |
| `repetitions` | integer | `5` | Measured iterations |
| `timeout_seconds` | float | *(none)* | Per-run timeout |
| `env` | table | `{}` | Environment overrides for the child |
| `redact_env` | array of strings | `[]` | Env var names to redact from logs |
| `stdout` | `null` \| `inherit` \| `capture` | `null` | Stdout handling |
| `stderr` | `null` \| `inherit` \| `capture` | `null` | Stderr handling |
| `capture_limit_bytes` | integer | `65536` | Max captured bytes per stream |

### `[benchmarks.measure]`

```toml
[benchmarks.measure]
metrics = ["wall_time", "peak_rss", "cpu_time"]
```

Recognized metrics: `wall_time`, `cpu_time`, `peak_rss`, `cpu_utilization`.

### `[benchmarks.policy]`

```toml
[benchmarks.policy]
min_successful_repetitions = 5   # below this, comparisons are inconclusive
noise_tolerance_pct = 1.0        # extra tolerance added to percentage thresholds
fail_on_inconclusive = false     # inconclusive becomes exit code 4
require_statistical_significance = false  # downgrade unsubstantiated breaches to warnings

[[benchmarks.policy.thresholds]]
metric = "wall_time"
max_increase_pct = 5.0           # relative budget
severity = "block"               # info | warn | block

[[benchmarks.policy.thresholds]]
metric = "peak_rss"
max_increase_abs = 1048576       # absolute budget, in the metric's unit (bytes)
severity = "warn"
```

A threshold must set at least one of `max_increase_pct` or `max_increase_abs`.

## Command execution and security

`command` is an array. The first element is the program; the rest are
arguments. There is **no shell**: metacharacters such as `|`, `>`, `*`, and
`$VAR` are not interpreted. If you need shell behavior, invoke it explicitly:

```toml
command = ["/bin/sh", "-c", "cat input.txt | ./processor"]
```

Doing so opts into the shell's security implications; PyPerfGuard does not
sanitize the resulting command.
