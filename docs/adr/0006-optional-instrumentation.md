# ADR-0006: Optional instrumentation adapters

- **Status:** Accepted
- **Date:** 2026-10-14

## Context

Profiling and flamegraphs are valuable but platform-specific, may require
elevated permissions or licensing, and impose overhead that changes performance.
Claiming universal support would be dishonest.

## Decision

- Keep the core free of profiling dependencies. Users who only need timing and
  regression testing never pull them in.
- Define a documented **adapter interface** for external profilers before
  implementing any specific one. Adapters associate profiling artifacts with a
  benchmark run and are clearly distinguished from ordinary measurements.
- Treat differential flamegraphs as a later, optional capability.

## Consequences

- Profiling artifacts are compared only when they are compatible.
- No promise of universal flamegraph support, zero external dependencies, or
  identical capabilities across macOS and Linux.
- Instrumented runs are labeled distinctly, because profiling overhead changes
  the measured values.
