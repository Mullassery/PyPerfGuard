//! Comparison engine: compute per-metric differences between two results.
//!
//! Comparisons validate that the two results are compatible enough to compare
//! and record *caveats* when differences in environment, command, tool
//! version, or measurement method weaken the comparison. The engine never
//! claims causation merely because one revision is slower.

use serde::{Deserialize, Serialize};

use crate::metric::Metric;
use crate::results::{BenchmarkResult, RevisionInfo};
use crate::stats::{self, MannWhitney, Summary};

/// A per-metric comparison between baseline and current.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricComparison {
    /// Metric compared.
    pub metric: Metric,
    /// Unit.
    pub unit: String,
    /// Baseline summary, if the metric was measured in the baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<Summary>,
    /// Current summary, if the metric was measured now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<Summary>,
    /// Absolute change of medians (current - baseline), native unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abs_change: Option<f64>,
    /// Relative change of medians, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pct_change: Option<f64>,
    /// Result of a Mann-Whitney U test on the raw observations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub significance: Option<MannWhitney>,
}

impl MetricComparison {
    /// Whether the two sets of raw observations differ significantly at the
    /// given alpha, requiring a reliable sample size.
    pub fn significantly_different(&self, alpha: f64) -> Option<bool> {
        self.significance
            .as_ref()
            .map(|s| s.reliable && s.p_value < alpha)
    }
}

/// A full comparison of two results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// Benchmark id.
    pub benchmark_id: String,
    /// Baseline revision.
    pub baseline_revision: RevisionInfo,
    /// Current revision.
    pub current_revision: RevisionInfo,
    /// Baseline start timestamp.
    pub baseline_started_at: String,
    /// Current start timestamp.
    pub current_started_at: String,
    /// Per-metric comparisons.
    pub metrics: Vec<MetricComparison>,
    /// Compatibility caveats (empty when the comparison is clean).
    pub caveats: Vec<String>,
    /// Whether the two results are compatible enough to compare at all.
    pub compatible: bool,
}

/// Compare a baseline result against a current result.
pub fn compare(baseline: &BenchmarkResult, current: &BenchmarkResult) -> Comparison {
    let mut caveats = Vec::new();
    let mut compatible = true;

    if baseline.benchmark_id != current.benchmark_id {
        compatible = false;
        caveats.push(format!(
            "benchmark ids differ ('{}' vs '{}'); results are not comparable",
            baseline.benchmark_id, current.benchmark_id
        ));
    }
    if baseline.command != current.command {
        caveats.push(format!(
            "commands differ (baseline: {:?}; current: {:?})",
            baseline.command, current.command
        ));
    }
    if baseline.environment.os != current.environment.os
        || baseline.environment.arch != current.environment.arch
    {
        caveats.push(format!(
            "operating system or architecture differs ({} / {} vs {} / {})",
            baseline.environment.os,
            baseline.environment.arch,
            current.environment.os,
            current.environment.arch
        ));
    }
    if baseline.environment.cpu_count != current.environment.cpu_count {
        caveats.push("CPU count differs between runs".to_string());
    }
    if baseline.tool.version != current.tool.version {
        caveats.push(format!(
            "tool versions differ ({} vs {})",
            baseline.tool.version, current.tool.version
        ));
    }
    if baseline.schema_version != current.schema_version {
        caveats.push(format!(
            "schema versions differ ({} vs {})",
            baseline.schema_version, current.schema_version
        ));
    }

    // Compare the union of metrics that appear in either result.
    let mut metrics: Vec<Metric> = Vec::new();
    for m in Metric::all() {
        if baseline.requested_metrics.contains(&m) || current.requested_metrics.contains(&m) {
            metrics.push(m);
        }
    }

    let mut metric_comparisons = Vec::new();
    for m in metrics {
        let base_summary = baseline.metric_summary(m).map(|s| s.summary.clone());
        let cur_summary = current.metric_summary(m).map(|s| s.summary.clone());
        let base_values = baseline.metric_values(m);
        let cur_values = current.metric_values(m);

        let (abs_change, pct_change) = match (&base_summary, &cur_summary) {
            (Some(b), Some(c)) => (
                Some(c.median - b.median),
                stats::pct_change(c.median, b.median),
            ),
            _ => (None, None),
        };

        let significance = if !base_values.is_empty() && !cur_values.is_empty() {
            stats::mann_whitney(&base_values, &cur_values)
        } else {
            None
        };

        if base_summary.is_none() {
            caveats.push(format!("baseline did not measure '{}'", m.name()));
        }
        if cur_summary.is_none() {
            caveats.push(format!("current run did not measure '{}'", m.name()));
        } else if let Some(c) = &cur_summary {
            if c.n < 5 {
                caveats.push(format!(
                    "'{}' has only {} successful sample(s); comparison is noisy",
                    m.name(),
                    c.n
                ));
            }
        }

        metric_comparisons.push(MetricComparison {
            metric: m,
            unit: m.unit().to_string(),
            baseline: base_summary,
            current: cur_summary,
            abs_change,
            pct_change,
            significance,
        });
    }

    Comparison {
        benchmark_id: current.benchmark_id.clone(),
        baseline_revision: baseline.revision.clone(),
        current_revision: current.revision.clone(),
        baseline_started_at: baseline.started_at.clone(),
        current_started_at: current.started_at.clone(),
        metrics: metric_comparisons,
        caveats,
        compatible,
    }
}

impl Comparison {
    /// Find the comparison for a metric.
    pub fn metric(&self, m: Metric) -> Option<&MetricComparison> {
        self.metrics.iter().find(|c| c.metric == m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{
        BenchmarkResult, EnvironmentInfo, Observation, ObservationStatus, Phase, RevisionInfo,
    };

    fn result(id: &str, commit: &str, wall_values: &[f64], rss: &[u64]) -> BenchmarkResult {
        let observations = wall_values
            .iter()
            .zip(rss.iter())
            .enumerate()
            .map(|(i, (w, r))| Observation {
                index: i as u32,
                phase: Phase::Measured,
                status: ObservationStatus::Ok,
                success: true,
                exit_code: Some(0),
                signal: None,
                wall_time_ms: Some(*w),
                cpu_time_ms: Some(w / 2.0),
                peak_rss_bytes: Some(*r),
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
            RevisionInfo {
                commit: Some(commit.into()),
                branch: None,
                dirty: None,
            },
            EnvironmentInfo {
                os: "macos".into(),
                arch: "aarch64".into(),
                cpu_count: Some(8),
            },
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:01Z".into(),
            0,
            wall_values.len() as u32,
            observations,
            vec![],
        )
    }

    #[test]
    fn detects_pct_increase() {
        let base = result("b", "aaa", &[100.0, 101.0, 99.0, 100.0, 100.0], &[1000; 5]);
        let cur = result("b", "bbb", &[120.0, 121.0, 119.0, 120.0, 120.0], &[1000; 5]);
        let cmp = compare(&base, &cur);
        let m = cmp.metric(Metric::WallTime).unwrap();
        assert!((m.pct_change.unwrap() - 20.0).abs() < 1e-6);
        assert_eq!(m.significantly_different(0.05), Some(true));
    }

    #[test]
    fn incompatible_when_ids_differ() {
        let base = result("a", "aaa", &[100.0], &[1]);
        let cur = result("b", "bbb", &[100.0], &[1]);
        let cmp = compare(&base, &cur);
        assert!(!cmp.compatible);
    }

    #[test]
    fn zero_baseline_yields_no_pct() {
        let base = result("b", "aaa", &[0.0, 0.0, 0.0, 0.0, 0.0], &[1, 1, 1, 1, 1]);
        let cur = result(
            "b",
            "bbb",
            &[10.0, 10.0, 10.0, 10.0, 10.0],
            &[1, 1, 1, 1, 1],
        );
        let cmp = compare(&base, &cur);
        let m = cmp.metric(Metric::WallTime).unwrap();
        assert!(m.pct_change.is_none());
        assert_eq!(m.abs_change, Some(10.0));
    }

    #[test]
    fn records_environment_caveat() {
        let base = result("b", "aaa", &[100.0], &[1]);
        let mut cur = result("b", "bbb", &[100.0], &[1]);
        cur.environment.os = "linux".into();
        let cmp = compare(&base, &cur);
        assert!(cmp.caveats.iter().any(|c| c.contains("operating system")));
    }
}
