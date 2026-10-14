//! Property-based tests for critical calculations.
//!
//! These complement the example-based unit tests by checking invariants that
//! must hold for arbitrary inputs.

use proptest::prelude::*;

use pyperfguard::metric::Metric;
use pyperfguard::report::sparkline;
use pyperfguard::stats::{mann_whitney, mean, median, pct_change, quantile, std_dev, summarize};

fn finite_vec(max_len: usize) -> impl Strategy<Value = Vec<f64>> {
    prop::collection::vec(0.0f64..1_000_000.0f64, 0..max_len)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn median_lies_within_range(values in finite_vec(64), extra in 0.0f64..1_000_000.0f64) {
        prop_assume!(!values.is_empty());
        let mut v = values.clone();
        v.push(extra);
        let m = median(&v).unwrap();
        let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        prop_assert!(m >= lo - 1e-9 && m <= hi + 1e-9);
    }

    #[test]
    fn mean_lies_within_range(values in finite_vec(64)) {
        prop_assume!(!values.is_empty());
        let m = mean(&values).unwrap();
        let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        prop_assert!(m >= lo - 1e-9 && m <= hi + 1e-9);
    }

    #[test]
    fn quantile_endpoints_match_min_and_max(values in finite_vec(64)) {
        prop_assume!(!values.is_empty());
        let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        prop_assert!((quantile(&values, 0.0).unwrap() - lo).abs() < 1e-9);
        prop_assert!((quantile(&values, 1.0).unwrap() - hi).abs() < 1e-9);
    }

    #[test]
    fn quantile_is_monotonic(values in finite_vec(64), a in 0.0f64..1.0f64, b in 0.0f64..1.0f64) {
        prop_assume!(!values.is_empty());
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let qlo = quantile(&values, lo).unwrap();
        let qhi = quantile(&values, hi).unwrap();
        prop_assert!(qlo <= qhi + 1e-9);
    }

    #[test]
    fn std_dev_is_non_negative(values in finite_vec(64)) {
        if let Some(sd) = std_dev(&values) {
            prop_assert!(sd >= 0.0);
        }
    }

    #[test]
    fn summary_counts_match(values in finite_vec(64)) {
        prop_assume!(!values.is_empty());
        let s = summarize(&values).unwrap();
        prop_assert_eq!(s.n, values.len());
        prop_assert!(s.std_dev.is_none() || s.n >= 2);
        prop_assert!(s.iqr.is_none() || s.n >= 4);
    }

    #[test]
    fn pct_change_recovers_ratio(base in 1.0f64..1_000_000.0f64, delta in -0.9f64..10.0f64) {
        let current = base * (1.0 + delta);
        let pct = pct_change(current, base).unwrap();
        prop_assert!((pct - delta * 100.0).abs() < 1e-6);
    }

    #[test]
    fn pct_change_zero_baseline_is_none(current in proptest::num::f64::ANY) {
        prop_assert!(pct_change(current, 0.0).is_none());
    }

    #[test]
    fn mann_whitney_p_value_in_unit_interval(
        a in prop::collection::vec(-1000.0f64..1000.0f64, 1..40),
        b in prop::collection::vec(-1000.0f64..1000.0f64, 1..40),
    ) {
        let mw = mann_whitney(&a, &b).unwrap();
        prop_assert!(mw.p_value >= 0.0 && mw.p_value <= 1.0);
        prop_assert!(mw.reliable == (a.len() >= 5 && b.len() >= 5));
    }

    #[test]
    fn sparkline_length_matches(values in finite_vec(200)) {
        prop_assert_eq!(sparkline(&values).chars().count(), values.len());
    }

    #[test]
    fn metric_units_are_stable(values in finite_vec(4)) {
        // Units never depend on values.
        for m in Metric::all() {
            prop_assert!(!m.unit().is_empty());
            prop_assert!(!m.name().is_empty());
        }
        let _ = values;
    }
}
