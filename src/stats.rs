//! Statistical analysis engine.
//!
//! The functions here are pure and operate on `&[f64]` so they can be tested
//! without running the CLI or spawning processes. They intentionally avoid
//! claiming more precision than the sample size supports: summary values that
//! require a minimum number of samples return `None` rather than guessing.
//!
//! Outliers are **never** removed automatically. Callers can inspect the raw
//! observations and apply their own policy explicitly.

use serde::{Deserialize, Serialize};

/// A robust summary of a set of observations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    /// Number of samples summarized.
    pub n: usize,
    /// Arithmetic mean.
    pub mean: f64,
    /// Median (50th percentile).
    pub median: f64,
    /// Minimum value.
    pub min: f64,
    /// Maximum value.
    pub max: f64,
    /// Sample standard deviation (n-1 denominator). `None` when n < 2.
    pub std_dev: Option<f64>,
    /// 90th percentile (type-7 interpolation). `None` when n < 2.
    pub p90: Option<f64>,
    /// Interquartile range (p75 - p25). `None` when n < 4.
    pub iqr: Option<f64>,
    /// Coefficient of variation (std_dev / mean). `None` when undefined.
    pub cv: Option<f64>,
}

/// Arithmetic mean. Returns `None` for an empty slice.
pub fn mean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let sum: f64 = xs.iter().sum();
    Some(sum / xs.len() as f64)
}

/// Median of the values (type-7 interpolation). Sorts a copy internally.
pub fn median(xs: &[f64]) -> Option<f64> {
    quantile(xs, 0.5)
}

/// Minimum value.
pub fn min(xs: &[f64]) -> Option<f64> {
    xs.iter().copied().reduce(f64::min)
}

/// Maximum value.
pub fn max(xs: &[f64]) -> Option<f64> {
    xs.iter().copied().reduce(f64::max)
}

/// Sample standard deviation (n-1 denominator). `None` when n < 2.
pub fn std_dev(xs: &[f64]) -> Option<f64> {
    if xs.len() < 2 {
        return None;
    }
    let m = mean(xs)?;
    let ss: f64 = xs.iter().map(|x| (x - m) * (x - m)).sum();
    Some((ss / (xs.len() as f64 - 1.0)).sqrt())
}

/// Type-7 (linear interpolation) quantile of `xs` for `q` in `[0, 1]`.
///
/// This matches the default used by NumPy and R, so reported summary values
/// are comparable with common tooling.
pub fn quantile(xs: &[f64], q: f64) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let q = q.clamp(0.0, 1.0);
    let mut sorted: Vec<f64> = xs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(quantile_sorted(&sorted, q))
}

fn quantile_sorted(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let rank = q * (n as f64 - 1.0);
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = rank - lo as f64;
        sorted[lo] + (sorted[hi] - sorted[lo]) * frac
    }
}

/// Interquartile range (p75 - p25). `None` when n < 4.
pub fn iqr(xs: &[f64]) -> Option<f64> {
    if xs.len() < 4 {
        return None;
    }
    Some(quantile(xs, 0.75)? - quantile(xs, 0.25)?)
}

/// Build a [`Summary`] from observations. Returns `None` for an empty slice.
pub fn summarize(xs: &[f64]) -> Option<Summary> {
    if xs.is_empty() {
        return None;
    }
    let m = mean(xs)?;
    let sd = std_dev(xs);
    let cv = match sd {
        Some(sd) if m.abs() > f64::EPSILON => Some(sd / m),
        _ => None,
    };
    Some(Summary {
        n: xs.len(),
        mean: m,
        median: median(xs)?,
        min: min(xs)?,
        max: max(xs)?,
        std_dev: sd,
        p90: if xs.len() >= 2 {
            quantile(xs, 0.9)
        } else {
            None
        },
        iqr: iqr(xs),
        cv,
    })
}

/// Relative change from `baseline` to `current`, in percent.
///
/// Returns `None` when the baseline is zero (or within epsilon of zero),
/// because a percentage change from zero is undefined. Callers must fall back
/// to an absolute comparison and report that the percentage is unavailable.
pub fn pct_change(current: f64, baseline: f64) -> Option<f64> {
    if baseline.abs() < f64::EPSILON {
        None
    } else {
        Some((current - baseline) / baseline * 100.0)
    }
}

/// Result of a Mann-Whitney U (Wilcoxon rank-sum) test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MannWhitney {
    /// U statistic for the first sample.
    pub u: f64,
    /// Standardized z score (continuity corrected).
    pub z: f64,
    /// Two-sided p-value from the normal approximation.
    pub p_value: f64,
    /// Number of samples in each group.
    pub n_a: usize,
    pub n_b: usize,
    /// Whether the normal approximation is considered reliable
    /// (both groups have at least 5 samples).
    pub reliable: bool,
}

/// Two-sided Mann-Whitney U test with tie correction and continuity
/// correction, using the normal approximation.
///
/// This is a non-parametric test: it does not assume normally distributed
/// observations. It returns `None` when either sample is empty. When the
/// samples are small the `reliable` flag is `false`; callers must not present
/// the p-value as conclusive in that case.
pub fn mann_whitney(a: &[f64], b: &[f64]) -> Option<MannWhitney> {
    let n_a = a.len();
    let n_b = b.len();
    if n_a == 0 || n_b == 0 {
        return None;
    }
    if n_a == 1 && n_b == 1 {
        // Degenerate: report no significant difference.
        return Some(MannWhitney {
            u: 0.0,
            z: 0.0,
            p_value: 1.0,
            n_a,
            n_b,
            reliable: false,
        });
    }

    // Combine and rank with average ranks for ties.
    let mut combined: Vec<(f64, usize)> = Vec::with_capacity(n_a + n_b);
    for &x in a {
        combined.push((x, 0));
    }
    for &x in b {
        combined.push((x, 1));
    }
    combined.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));

    let n = combined.len() as f64;
    let mut ranks = vec![0.0f64; combined.len()];
    let mut tie_sum = 0.0f64;
    let mut i = 0;
    while i < combined.len() {
        let mut j = i;
        while j + 1 < combined.len() && combined[j + 1].0 == combined[i].0 {
            j += 1;
        }
        let avg_rank = ((i + 1) + (j + 1)) as f64 / 2.0;
        for rank in ranks.iter_mut().take(j + 1).skip(i) {
            *rank = avg_rank;
        }
        let t = (j - i + 1) as f64;
        if t > 1.0 {
            tie_sum += t * t * t - t;
        }
        i = j + 1;
    }

    let rank_sum_a: f64 = combined
        .iter()
        .zip(ranks.iter())
        .filter(|(c, _)| c.1 == 0)
        .map(|(_, r)| *r)
        .sum();

    let u_a = rank_sum_a - (n_a as f64) * (n_a as f64 + 1.0) / 2.0;
    let u_b = (n_a * n_b) as f64 - u_a;
    let u = u_a.min(u_b);

    let mu = (n_a * n_b) as f64 / 2.0;
    let tie_correction = tie_sum / (n * (n - 1.0));
    let sigma_sq = ((n_a * n_b) as f64 / 12.0) * ((n + 1.0) - tie_correction);
    if sigma_sq <= 0.0 {
        return Some(MannWhitney {
            u,
            z: 0.0,
            p_value: 1.0,
            n_a,
            n_b,
            reliable: false,
        });
    }
    let sigma = sigma_sq.sqrt();

    // Continuity correction toward the mean.
    let diff = u_a - mu;
    let corrected = if diff.abs() > 0.5 {
        diff - 0.5 * diff.signum()
    } else {
        0.0
    };
    let z = corrected / sigma;
    let p_value = 2.0 * (1.0 - normal_cdf(z.abs()));

    Some(MannWhitney {
        u,
        z,
        p_value,
        n_a,
        n_b,
        reliable: n_a >= 5 && n_b >= 5,
    })
}

/// Standard normal CDF using the error function.
fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

/// Abramowitz & Stegun 7.1.26 error function approximation.
///
/// Maximum absolute error is ~1.5e-7, which is far below the precision at
/// which benchmark p-values are interpreted.
fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {a} ~= {b}");
    }

    #[test]
    fn empty_slices_return_none() {
        assert_eq!(mean(&[]), None);
        assert_eq!(median(&[]), None);
        assert_eq!(std_dev(&[]), None);
        assert_eq!(summarize(&[]), None);
        assert_eq!(quantile(&[], 0.5), None);
    }

    #[test]
    fn mean_and_median() {
        approx(mean(&[1.0, 2.0, 3.0, 4.0]).unwrap(), 2.5);
        approx(median(&[1.0, 2.0, 3.0, 4.0]).unwrap(), 2.5);
        approx(median(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap(), 3.0);
    }

    #[test]
    fn quantile_type7_matches_known_values() {
        let xs = [1.0, 2.0, 3.0, 4.0];
        approx(quantile(&xs, 0.0).unwrap(), 1.0);
        approx(quantile(&xs, 0.25).unwrap(), 1.75);
        approx(quantile(&xs, 0.5).unwrap(), 2.5);
        approx(quantile(&xs, 1.0).unwrap(), 4.0);
    }

    #[test]
    fn std_dev_requires_two_samples() {
        assert_eq!(std_dev(&[5.0]), None);
        // Sample stddev of [2,4,4,4,5,5,7,9] is ~2.138.
        approx(
            std_dev(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap(),
            2.13808993529939,
        );
    }

    #[test]
    fn iqr_requires_four_samples() {
        assert_eq!(iqr(&[1.0, 2.0, 3.0]), None);
        approx(iqr(&[1.0, 2.0, 3.0, 4.0]).unwrap(), 1.5);
    }

    #[test]
    fn pct_change_zero_baseline_is_none() {
        assert_eq!(pct_change(10.0, 0.0), None);
        approx(pct_change(110.0, 100.0).unwrap(), 10.0);
        approx(pct_change(90.0, 100.0).unwrap(), -10.0);
    }

    #[test]
    fn mann_whitney_detects_separation() {
        let a = [10.0, 11.0, 12.0, 13.0, 14.0];
        let b = [20.0, 21.0, 22.0, 23.0, 24.0];
        let mw = mann_whitney(&a, &b).unwrap();
        assert!(mw.reliable);
        assert!(mw.p_value < 0.05, "p={}", mw.p_value);
    }

    #[test]
    fn mann_whitney_identical_samples_not_significant() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mw = mann_whitney(&a, &a).unwrap();
        assert!(mw.p_value > 0.9, "p={}", mw.p_value);
    }

    #[test]
    fn mann_whitney_handles_ties() {
        let a = [1.0, 1.0, 2.0, 2.0, 3.0];
        let b = [1.0, 2.0, 2.0, 3.0, 3.0];
        let mw = mann_whitney(&a, &b).unwrap();
        assert!(mw.p_value >= 0.0 && mw.p_value <= 1.0);
    }

    #[test]
    fn erf_is_accurate() {
        approx(erf(0.0), 0.0);
        assert!((erf(1.0) - 0.8427007929).abs() < 1e-6);
        assert!((erf(-1.0) + 0.8427007929).abs() < 1e-6);
    }
}
