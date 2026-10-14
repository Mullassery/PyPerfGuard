# Troubleshooting

## "no pyperfguard.toml found in this directory or any parent"

Run `pyperfguard init`, or pass `--config path/to/pyperfguard.toml`. Ad-hoc runs
(`pyperfguard run -- ./cmd`) do not require a configuration file.

## "benchmark 'x' is not defined in the configuration"

The id you passed does not match any `[[benchmarks]]` entry. Check
`pyperfguard doctor` for the resolved config path and benchmark count.

## `check` says "no baseline available"

There is no stored result to compare against. Either run the benchmark once and
register it:

```sh
pyperfguard run my-bench
pyperfguard baseline set --benchmark my-bench --latest
```

or pass an explicit `--baseline latest|none|path.json|<sha>`.

## `check` is inconclusive

Common causes:
- Too few successful repetitions relative to `min_successful_repetitions`.
- A zero baseline (percentage change is undefined).
- A threshold that is only a percentage, with a baseline of zero.

Inconclusive does **not** block unless `fail_on_inconclusive` is set.

## Peak RSS or CPU time shows `n/a`

Those metrics require a Unix platform (`wait4`). On other platforms only wall
time is available. Run `pyperfguard doctor` to confirm.

## Results are noisy and thresholds flap

- Increase `repetitions` and add `warmups`.
- Raise `noise_tolerance_pct`.
- Set `require_statistical_significance = true` to downgrade unsubstantiated
  breaches to warnings.
- Reduce external interference (other builds, browsers, thermal load).

## "result schema version N is newer than supported version 1"

The artifact was produced by a newer PyPerfGuard. Upgrade PyPerfGuard, or
regenerate the artifact.

## The history index looks wrong

The index is a cache. Delete `.pyperfguard/index.json`; it is rebuilt from the
result files on the next command. Result files are never destroyed by a rebuild.

## Git metadata is empty

PyPerfGuard reports `(no git)` when the working directory is not a Git
repository or git is unavailable. This is not an error. In a shallow clone,
revision lookups may be limited; `pyperfguard doctor` reports whether the clone
is shallow.
