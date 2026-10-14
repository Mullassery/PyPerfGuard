# Contributing to PyPerfGuard

Thanks for your interest. This project values correctness, honest measurement
semantics, reproducibility, and architectural discipline over feature count.

## Getting started

```sh
git clone https://github.com/example/pyperfguard.git
cd pyperfguard
cargo build
cargo test
```

## Required checks

Every change must pass:

```sh
cargo fmt --all -- --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the same checks on macOS and Linux, plus documentation and
dependency/security checks.

## Guidelines

- **Measurement honesty.** Never report a metric as a value when it is
  unavailable. Use `null`/`Option`, and record the measurement method.
- **Failed runs are not observations.** A non-zero exit or a timeout must never
  be folded into a summary.
- **No implicit shell.** Commands are argument arrays. If you add shell support,
  document its security implications.
- **Keep domain logic testable.** Statistics, comparison, and policy code must
  not depend on the CLI, the filesystem, or GitHub.
- **No sibling-repository coupling.** Do not add dependencies on, or scripts
  that reach into, other repositories. Optional integrations use documented
  adapters.
- **Test the hard cases.** Improvements *and* regressions; zero baselines;
  missing metrics; unit mismatches; insufficient samples; inconclusive results;
  corrupt artifacts; timeouts and failed executions.
- **Avoid fragile tests.** Do not assert a specific runtime to the millisecond.
  Use synthetic fixtures and structural assertions.

## Adding a dependency

Explain in the pull request why the dependency is necessary and what it
replaces. Prefer a small, well-maintained crate over a bespoke framework.
Significant dependencies are recorded in the architecture decision records.

## Commit messages

Write concise, imperative messages. Reference the phase or issue when relevant.

## License

By contributing, you agree that your contributions are licensed under the
Apache-2.0 license (see `LICENSE`).
