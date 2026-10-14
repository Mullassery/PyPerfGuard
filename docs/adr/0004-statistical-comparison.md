# ADR-0004: Statistical comparison and inconclusive outcomes

- **Status:** Accepted
- **Date:** 2026-10-14

## Context

Benchmarking is an experiment. Averages hide variability, and small samples
cannot support strong claims. We must distinguish "crossed a budget" from "the
difference is real", and avoid false pass/regression verdicts.

## Decision

- Retain raw per-run observations in every artifact.
- Report robust summaries: median (primary), mean, min/max, sample standard
  deviation, type-7 quantiles (matching NumPy/R), IQR, and coefficient of
  variation.
- Compare medians for relative/absolute change, with percentage change
  undefined when the baseline is zero.
- Use a **Mann-Whitney U test** (tie-corrected, continuity-corrected, normal
  approximation) on raw observations. It is non-parametric and does not assume
  normality.
- Mark the test `reliable` only when both groups have at least five samples.
- Distinguish outcomes: `pass`, `info`, `inconclusive`, `warning`, `regression`,
  `error`. When evidence is insufficient, return `inconclusive` rather than
  guessing.
- Never automatically discard outliers. Raw observations remain available.

## Consequences

- Thresholds can be enforced independently of significance. Setting
  `require_statistical_significance = true` downgrades an unsubstantiated
  breach to a warning.
- The normal approximation is documented; the `erf` approximation has a maximum
  absolute error around 1.5e-7, far below the precision at which p-values are
  interpreted.
- Paired comparisons are only used when observations are genuinely paired;
  otherwise independent-sample comparison is used.
