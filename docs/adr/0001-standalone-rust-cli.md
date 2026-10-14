# ADR-0001: Standalone Rust CLI, no hosted service

- **Status:** Accepted
- **Date:** 2026-10-14

## Context

PyPerfGuard must be usable in CI without a hosted account or external SaaS
service, and must be independently installable. It also must not depend on or
modify sibling repositories.

## Decision

Implement the core as a single, standalone Rust crate producing a native CLI
binary. Persistence is local-first (JSON artifacts plus a simple index). There
are no mandatory services, submodules, cross-repository orchestration, or
sibling-repository scripts.

## Consequences

- `cargo build --release` yields a self-contained binary with no Python
  requirement.
- CI integrations are provider-neutral; GitHub Actions is supported through a
  documented workflow rather than a privileged integration.
- Optional features (Python bindings, profilers) are additive and never
  prerequisites for building, testing, installing, or releasing the core.
