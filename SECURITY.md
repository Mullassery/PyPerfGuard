# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately to the maintainers rather
than opening a public issue. Include a description, reproduction steps, and the
affected version. We will acknowledge receipt and coordinate a fix and
disclosure.

## Threat model

PyPerfGuard **executes user-supplied programs**. This is inherent to a
benchmark runner. The relevant protections and their limits:

### What PyPerfGuard does

- Executes commands as **explicit argument arrays**. There is **no implicit
  shell expansion**; metacharacters are passed literally to the target program.
- Allows redacting named environment variables from logs and excludes
  environment values from result artifacts by default (only the overrides the
  user configures are passed to the child).
- **Validates imported result artifacts** and never executes code contained in
  them. Unknown or newer schema versions are rejected.
- Uses **atomic writes** (temp file + rename) and rebuilds a corrupt history
  index from result files.
- Applies per-run **timeouts** and terminates the child's **process group**
  (best-effort) after a grace period.
- Keeps all automation confined to the current repository and configured paths.

### What PyPerfGuard does not do

- It is **not a sandbox**. Benchmarked programs run with the permissions of the
  invoking user, and can access the filesystem and network. Do not benchmark
  untrusted code without an external sandbox (container, VM, seccomp, or
  similar).
- Timeouts and process-group termination are best-effort. Detached or
  double-forked processes may survive.
- It cannot prevent a benchmarked program from reading secrets that are already
  present in the process environment. Configure `redact_env` and minimal `env`
  overrides for sensitive workloads.

## CI integration

- Example workflows request **read-only** permissions by default and never
  embed personal access tokens in configuration or logs.
- Baseline artifacts are retrieved from the workflow's own artifact storage, not
  from a hosted PyPerfGuard service.

## Dependency policy

- Prefer a small dependency set with active maintenance.
- Review dependencies during upgrades; run `cargo audit` where available.
- Pin GitHub Actions to reviewed versions; pinning to immutable commit SHAs is
  recommended and tracked as hardening work.
