# ADR-0005: Baseline persistence and result schema

- **Status:** Accepted
- **Date:** 2026-10-14

## Context

Results must survive across runs, be importable from CI artifacts, and be
readable years later. Corrupted local state must not silently lose data.
Baselines must be inspectable so the user knows what they are comparing against.

## Decision

- Store results as **versioned JSON artifacts** with a top-level
  `schema_version`. Current version: `1`.
- Layout: `.pyperfguard/results/<benchmark-id>/<timestamp>.json` plus
  `.pyperfguard/index.json` and `.pyperfguard/baselines.json`.
- Writes are **atomic** (temp file + `rename`), so an interrupted write cannot
  produce a half-written artifact.
- The index is a **cache**: if missing or corrupt it is rebuilt by scanning the
  result files. Rebuilding never deletes data.
- Baselines are **references** to stored results, not copies. A registry maps a
  benchmark id to a baseline file, timestamp, and revision.
- We do **not** check out Git revisions on the user's behalf. A "revision"
  baseline means a stored result whose recorded commit matches. This cannot
  clobber a working tree.

## Forward-compatibility rules

- Adding optional fields is backward compatible.
- Removing or renaming fields requires a schema-version bump.
- Artifacts with a **newer** schema version than the tool supports are rejected
  with an actionable error, and are never executed.
- Unknown fields are tolerated on read (results) while config parsing is strict.

## Consequences

- An embedded database is unnecessary; portable JSON plus an index is
  reviewable, diffable, and CI-artifact friendly.
- Users can retain artifacts in CI, publish them as build artifacts, and import
  them into local history.
