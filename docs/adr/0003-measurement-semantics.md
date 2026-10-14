# ADR-0003: Measurement semantics

- **Status:** Accepted
- **Date:** 2026-10-14

## Context

Timing and resource metrics have subtle platform semantics. Reporting a value
that looks precise but is measured incorrectly is worse than reporting that a
metric is unavailable.

## Decision

- **Wall-clock time** is measured with a monotonic clock (`std::time::Instant`)
  around the child wait. It includes process spawn-to-exit latency but not
  PyPerfGuard's own overhead beyond the wait.
- **CPU time** is `ru_utime + ru_stime` from `wait4(2)` (`getrusage`).
- **Peak RSS** is `ru_maxrss` from `wait4(2)`. Units differ by platform:
  **bytes on macOS**, **kilobytes on Linux**. PyPerfGuard normalizes to bytes
  and records the method string in every result.
- **CPU utilization** is derived as `cpu_time / wall_time * 100`. It may exceed
  100% for multi-threaded workloads.
- **Missing measurements are `null`, never zero.**
- We do **not** claim to measure CPU cycles: having CPU *time* does not imply
  cycle counts, which require hardware performance counters and special
  permissions.

## Consequences

- Non-Unix platforms support wall time only; CPU time and peak RSS are reported
  as unavailable.
- Every result carries a measurement-method string and the platform/arch so a
  reader can interpret values correctly.
- Hardware performance counters and heap-allocation metrics are explicitly
  optional, platform-specific capabilities (ADR-0006).
