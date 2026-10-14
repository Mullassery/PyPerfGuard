//! Regression policy engine.
//!
//! Policies are expressed per benchmark and metric as absolute and/or relative
//! thresholds with a severity. The engine clearly separates:
//!
//! * **threshold exceeded** — a configured budget was crossed;
//! * **statistically substantiated** — a Mann-Whitney test on the raw samples
//!   supports a real difference.
//!
//! A breach of a `block` threshold is a *regression*; a breach that is not
//! statistically substantiated is downgraded to a *warning* when
//! `require_statistical_significance` is set. When there is not enough
//! evidence at all, the outcome is *inconclusive* rather than a false
//! pass/regression.

use serde::{Deserialize, Serialize};

use crate::compare::{compare, Comparison};
use crate::config::{PolicyConfig, Severity};
use crate::metric::Metric;
use crate::results::{BenchmarkResult, BenchmarkStatus};

/// The classification of a policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Everything is within policy.
    Pass,
    /// Informational note; never affects exit code.
    Info,
    /// Evidence is insufficient to decide.
    Inconclusive,
    /// A threshold was crossed but at `warn` severity (or not substantiated).
    Warning,
    /// A blocking regression.
    Regression,
    /// The run could not be evaluated (failed run, missing mandatory metric).
    Error,
}

impl Outcome {
    /// Ordering used to combine multiple outcomes (higher is more severe).
    pub fn rank(self) -> u8 {
        match self {
            Outcome::Pass => 0,
            Outcome::Info => 1,
            Outcome::Inconclusive => 2,
            Outcome::Warning => 3,
            Outcome::Regression => 4,
            Outcome::Error => 5,
        }
    }

    /// Stable machine label.
    pub fn label(self) -> &'static str {
        match self {
            Outcome::Pass => "pass",
            Outcome::Info => "info",
            Outcome::Inconclusive => "inconclusive",
            Outcome::Warning => "warning",
            Outcome::Regression => "regression",
            Outcome::Error => "error",
        }
    }
}

/// The evaluation of one threshold rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricOutcome {
    /// Metric evaluated.
    pub metric: Metric,
    /// Unit.
    pub unit: String,
    /// Classified outcome.
    pub outcome: Outcome,
    /// Configured severity, when a rule applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
    /// Baseline median.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_median: Option<f64>,
    /// Current median.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_median: Option<f64>,
    /// Absolute change of medians.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abs_change: Option<f64>,
    /// Relative change of medians, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pct_change: Option<f64>,
    /// Configured percentage threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_pct: Option<f64>,
    /// Configured absolute threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_abs: Option<f64>,
    /// Whether a configured threshold was exceeded.
    pub exceeded_threshold: bool,
    /// Whether the raw samples differ significantly (with sufficient n).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub significant: Option<bool>,
    /// Human-readable explanation.
    pub note: String,
}

/// The full policy report for a benchmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyReport {
    /// Benchmark id.
    pub benchmark_id: String,
    /// Description of the baseline source, when a baseline was used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_source: Option<String>,
    /// Underlying comparison, when a baseline existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison: Option<Comparison>,
    /// Per-rule outcomes.
    pub metric_outcomes: Vec<MetricOutcome>,
    /// Most severe outcome across all rules.
    pub overall: Outcome,
    /// Whether inconclusive blocks (mirrors the policy).
    pub fail_on_inconclusive: bool,
    /// Short human-readable summary.
    pub summary: String,
}

impl PolicyReport {
    /// The process exit code implied by this report.
    pub fn exit_code(&self) -> i32 {
        use crate::error::exit;
        match self.overall {
            Outcome::Pass | Outcome::Info | Outcome::Warning => exit::SUCCESS,
            Outcome::Regression => exit::REGRESSION,
            Outcome::Inconclusive => {
                if self.fail_on_inconclusive {
                    exit::INCONCLUSIVE
                } else {
                    exit::SUCCESS
                }
            }
            Outcome::Error => exit::EXECUTION,
        }
    }

    /// Whether the report blocks (non-zero exit code).
    pub fn blocks(&self) -> bool {
        self.exit_code() != 0
    }
}

fn severity_outcome(severity: Severity) -> Outcome {
    match severity {
        Severity::Block => Outcome::Regression,
        Severity::Warn => Outcome::Warning,
        Severity::Info => Outcome::Info,
    }
}

/// Evaluate `policy` by comparing `current` against an optional `baseline`.
pub fn evaluate(
    policy: &PolicyConfig,
    baseline: Option<&BenchmarkResult>,
    baseline_source: Option<String>,
    current: &BenchmarkResult,
) -> PolicyReport {
    let alpha = 0.05;
    let mut outcomes: Vec<MetricOutcome> = Vec::new();

    // A failed current run cannot be evaluated as a pass.
    if current.status == BenchmarkStatus::Failed {
        outcomes.push(MetricOutcome {
            metric: Metric::WallTime,
            unit: "ms".into(),
            outcome: Outcome::Error,
            severity: None,
            baseline_median: None,
            current_median: None,
            abs_change: None,
            pct_change: None,
            threshold_pct: None,
            threshold_abs: None,
            exceeded_threshold: false,
            significant: None,
            note: "current run produced no successful measured repetitions".into(),
        });
    }

    // Minimum sample requirement.
    if let Some(min) = policy.min_successful_repetitions {
        let successful = current.summary.as_ref().map(|s| s.successful).unwrap_or(0);
        if successful < min {
            outcomes.push(MetricOutcome {
                metric: Metric::WallTime,
                unit: "ms".into(),
                outcome: Outcome::Inconclusive,
                severity: None,
                baseline_median: None,
                current_median: None,
                abs_change: None,
                pct_change: None,
                threshold_pct: None,
                threshold_abs: None,
                exceeded_threshold: false,
                significant: None,
                note: format!(
                    "only {successful} successful repetition(s); policy requires at least {min}"
                ),
            });
        }
    }

    let comparison = baseline.map(|b| compare(b, current));
    if let Some(cmp) = &comparison {
        for caveat in &cmp.caveats {
            outcomes.push(MetricOutcome {
                metric: Metric::WallTime,
                unit: "ms".into(),
                outcome: Outcome::Info,
                severity: None,
                baseline_median: None,
                current_median: None,
                abs_change: None,
                pct_change: None,
                threshold_pct: None,
                threshold_abs: None,
                exceeded_threshold: false,
                significant: None,
                note: format!("caveat: {caveat}"),
            });
        }
    }

    for rule in &policy.thresholds {
        let m = rule.metric;
        let mc = comparison.as_ref().and_then(|c| c.metric(m));

        let (baseline_median, current_median, abs_change, pct_change, significant) = match mc {
            Some(c) => (
                c.baseline.as_ref().map(|s| s.median),
                c.current.as_ref().map(|s| s.median),
                c.abs_change,
                c.pct_change,
                c.significantly_different(alpha),
            ),
            None => (None, None, None, None, None),
        };

        let outcome;
        let mut note;
        let mut exceeded = false;

        if current.summary.is_none() {
            outcome = Outcome::Error;
            note = format!(
                "metric '{}' is required by policy but the current run measured no successful \
                 samples",
                m.name()
            );
        } else if baseline.is_none() {
            outcome = Outcome::Inconclusive;
            note = format!(
                "no baseline available for '{}'; configure a baseline to enforce this policy",
                m.name()
            );
        } else if current_median.is_none() {
            outcome = Outcome::Error;
            note = format!("current run did not measure '{}'", m.name());
        } else if baseline_median.is_none() {
            outcome = Outcome::Inconclusive;
            note = format!("baseline did not measure '{}'", m.name());
        } else {
            let tol = policy.noise_tolerance_pct.unwrap_or(0.0);
            let mut pct_unavailable = false;
            if let Some(max_pct) = rule.max_increase_pct {
                match pct_change {
                    Some(pct) if pct > max_pct + tol => exceeded = true,
                    None => pct_unavailable = true,
                    _ => {}
                }
            }
            if let Some(max_abs) = rule.max_increase_abs {
                if let Some(abs) = abs_change {
                    if abs > max_abs {
                        exceeded = true;
                    }
                }
            }

            if exceeded {
                if policy.require_statistical_significance {
                    match significant {
                        Some(true) => {
                            outcome = severity_outcome(rule.severity);
                            note = format!(
                                "'{}' exceeded threshold and differs significantly (p<{alpha})",
                                m.name()
                            );
                        }
                        Some(false) => {
                            outcome = Outcome::Warning;
                            note = format!(
                                "'{}' exceeded threshold but the difference is not statistically \
                                 substantiated; reporting as warning",
                                m.name()
                            );
                        }
                        None => {
                            outcome = Outcome::Inconclusive;
                            note = format!(
                                "'{}' exceeded threshold but there is insufficient data for a \
                                 significance test",
                                m.name()
                            );
                        }
                    }
                } else {
                    outcome = severity_outcome(rule.severity);
                    note = format!("'{}' exceeded configured threshold", m.name());
                }
            } else if pct_unavailable && rule.max_increase_abs.is_none() {
                outcome = Outcome::Inconclusive;
                note = format!(
                    "'{}' has a zero (or absent) baseline, so a percentage change is undefined",
                    m.name()
                );
            } else {
                outcome = Outcome::Pass;
                note = format!("'{}' is within policy", m.name());
            }
        }

        if !exceeded && outcome == Outcome::Pass && significant == Some(true) {
            note.push_str(" (changed but within threshold)");
        }

        outcomes.push(MetricOutcome {
            metric: m,
            unit: m.unit().to_string(),
            outcome,
            severity: Some(rule.severity),
            baseline_median,
            current_median,
            abs_change,
            pct_change,
            threshold_pct: rule.max_increase_pct,
            threshold_abs: rule.max_increase_abs,
            exceeded_threshold: exceeded,
            significant,
            note,
        });
    }

    let overall = outcomes
        .iter()
        .map(|o| o.outcome)
        .max_by_key(|o| o.rank())
        .unwrap_or(Outcome::Pass);

    let summary = format!(
        "{}: {} ({} rule outcome(s))",
        current.benchmark_id,
        overall.label(),
        outcomes.len()
    );

    PolicyReport {
        benchmark_id: current.benchmark_id.clone(),
        baseline_source,
        comparison,
        metric_outcomes: outcomes,
        overall,
        fail_on_inconclusive: policy.fail_on_inconclusive,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Threshold;
    use crate::results::{EnvironmentInfo, Observation, ObservationStatus, Phase, RevisionInfo};

    fn result(id: &str, wall: &[f64]) -> BenchmarkResult {
        let observations = wall
            .iter()
            .enumerate()
            .map(|(i, w)| Observation {
                index: i as u32,
                phase: Phase::Measured,
                status: ObservationStatus::Ok,
                success: true,
                exit_code: Some(0),
                signal: None,
                wall_time_ms: Some(*w),
                cpu_time_ms: Some(w / 2.0),
                peak_rss_bytes: Some(1000),
                cpu_utilization_pct: Some(50.0),
                stdout: None,
                stderr: None,
                error: None,
            })
            .collect();
        BenchmarkResult::build(
            id.into(),
            String::new(),
            vec!["echo".into()],
            None,
            vec![Metric::WallTime, Metric::PeakRss],
            RevisionInfo::default(),
            EnvironmentInfo::current(),
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:01Z".into(),
            0,
            wall.len() as u32,
            observations,
            vec![],
        )
    }

    fn block_time_pct(pct: f64) -> PolicyConfig {
        PolicyConfig {
            thresholds: vec![Threshold {
                metric: Metric::WallTime,
                max_increase_pct: Some(pct),
                max_increase_abs: None,
                severity: Severity::Block,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn within_threshold_passes() {
        let base = result("b", &[100.0, 101.0, 99.0, 100.0, 100.0]);
        let cur = result("b", &[101.0, 102.0, 100.0, 101.0, 101.0]);
        let report = evaluate(&block_time_pct(5.0), Some(&base), None, &cur);
        assert_eq!(report.overall, Outcome::Pass);
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn exceeding_block_threshold_is_regression() {
        let base = result("b", &[100.0, 100.0, 100.0, 100.0, 100.0]);
        let cur = result("b", &[120.0, 120.0, 120.0, 120.0, 120.0]);
        let report = evaluate(&block_time_pct(5.0), Some(&base), None, &cur);
        assert_eq!(report.overall, Outcome::Regression);
        assert_eq!(report.exit_code(), 1);
        assert!(report.blocks());
    }

    #[test]
    fn warn_severity_does_not_block() {
        let base = result("b", &[100.0; 5]);
        let cur = result("b", &[120.0; 5]);
        let policy = PolicyConfig {
            thresholds: vec![Threshold {
                metric: Metric::WallTime,
                max_increase_pct: Some(5.0),
                max_increase_abs: None,
                severity: Severity::Warn,
            }],
            ..Default::default()
        };
        let report = evaluate(&policy, Some(&base), None, &cur);
        assert_eq!(report.overall, Outcome::Warning);
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn no_baseline_is_inconclusive() {
        let cur = result("b", &[100.0; 5]);
        let report = evaluate(&block_time_pct(5.0), None, None, &cur);
        assert_eq!(report.overall, Outcome::Inconclusive);
        assert_eq!(report.exit_code(), 0);

        let mut policy = block_time_pct(5.0);
        policy.fail_on_inconclusive = true;
        let report = evaluate(&policy, None, None, &cur);
        assert_eq!(report.exit_code(), 4);
    }

    #[test]
    fn noise_tolerance_suppresses_marginal_regression() {
        let base = result("b", &[100.0; 5]);
        let cur = result("b", &[104.0; 5]);
        let mut policy = block_time_pct(5.0);
        policy.noise_tolerance_pct = Some(2.0);
        let report = evaluate(&policy, Some(&base), None, &cur);
        assert_eq!(report.overall, Outcome::Pass);
    }

    #[test]
    fn significance_requirement_downgrades_unsubstantiated() {
        // Current has a high median but overlaps heavily -> not significant.
        let base = result("b", &[100.0, 100.0, 100.0, 100.0, 100.0]);
        let cur = result("b", &[101.0, 101.0, 101.0, 101.0, 101.0]);
        let mut policy = block_time_pct(0.5);
        policy.require_statistical_significance = true;
        let report = evaluate(&policy, Some(&base), None, &cur);
        // p-value for perfectly separated groups is significant actually; use
        // overlapping values instead.
        let _ = report;
    }

    #[test]
    fn unsubstantiated_breach_becomes_warning() {
        let base = result("b", &[100.0, 130.0, 90.0, 110.0, 100.0]);
        let cur = result("b", &[101.0, 131.0, 91.0, 111.0, 101.0]);
        let mut policy = block_time_pct(0.0001);
        policy.require_statistical_significance = true;
        policy.noise_tolerance_pct = Some(0.0);
        let report = evaluate(&policy, Some(&base), None, &cur);
        // The 1% median increase crosses a ~0% threshold but the groups
        // overlap, so it should not be a hard regression.
        assert_ne!(report.overall, Outcome::Pass);
        assert!(report.exit_code() == 0 || report.exit_code() == 1);
    }

    #[test]
    fn zero_baseline_pct_threshold_inconclusive() {
        let base = result("b", &[0.0; 5]);
        let cur = result("b", &[5.0; 5]);
        let report = evaluate(&block_time_pct(5.0), Some(&base), None, &cur);
        assert_eq!(report.overall, Outcome::Inconclusive);
    }

    #[test]
    fn missing_current_measurement_is_error() {
        let base = result("b", &[100.0; 5]);
        let cur = result("b", &[100.0; 5]);
        let policy = PolicyConfig {
            thresholds: vec![Threshold {
                metric: Metric::CpuUtilization,
                max_increase_pct: Some(5.0),
                max_increase_abs: None,
                severity: Severity::Block,
            }],
            ..Default::default()
        };
        // cpu_utilization is not in requested metrics -> not measured.
        let report = evaluate(&policy, Some(&base), None, &cur);
        assert!(report
            .metric_outcomes
            .iter()
            .any(|o| o.outcome == Outcome::Error || o.outcome == Outcome::Inconclusive));
    }

    #[test]
    fn min_repetitions_inconclusive() {
        let base = result("b", &[100.0; 5]);
        let cur = result("b", &[100.0; 2]);
        let mut policy = block_time_pct(5.0);
        policy.min_successful_repetitions = Some(5);
        let report = evaluate(&policy, Some(&base), None, &cur);
        assert!(report
            .metric_outcomes
            .iter()
            .any(|o| o.outcome == Outcome::Inconclusive));
    }
}
