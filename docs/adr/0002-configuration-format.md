# ADR-0002: TOML as the canonical configuration format

- **Status:** Accepted
- **Date:** 2026-10-14

## Context

Configuration must be readable, support comments, express nested structures
(benchmark → measure → policy thresholds), and be well supported by Rust
libraries.

## Decision

Use **TOML** for the first release. Reject unknown fields to catch typos.

## Rationale

- Comments and multiline arrays keep benchmark definitions readable.
- Rich, stable Rust support via the `toml` crate.
- Array-of-tables (`[[benchmarks]]`, `[[benchmarks.policy.thresholds]]`) models
  the report structure naturally.

## Consequences

- YAML is not supported in the first release; the choice is documented so a
  future reader understands it was deliberate.
- Schema evolution for the *configuration* is handled by strict validation;
  schema evolution for *results* (ADR-0005) is handled with versioned JSON.
