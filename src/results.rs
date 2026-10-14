//! Versioned result schema.
//!
//! Result artifacts are JSON documents with an explicit `schema_version`.
//! Raw per-run observations are stored separately from aggregated summaries so
//! that history can be re-analyzed and so that a failed run can never be
//! mistaken for a valid observation. Missing measurements are represented as
//! JSON `null`, never as zero.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::metric::Metric;
use crate::stats::{self, Summary};

/// Current result schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Identity of the tool that produced a result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Tool name.
    pub name: String,
    /// Tool version.
    pub version: String,
}

impl ToolInfo {
    /// Identity of the running build.
    pub fn current() -> ToolInfo {
        ToolInfo {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Git revision metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionInfo {
    /// Full commit SHA, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Branch name, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Whether the working tree had uncommitted changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
}

/// Environment metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    /// Operating system (`std::env::consts::OS`).
    pub os: String,
    /// CPU architecture (`std::env::consts::ARCH`).
    pub arch: String,
    /// Available parallelism, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_count: Option<usize>,
}

impl EnvironmentInfo {
    /// Capture the current environment.
    pub fn current() -> EnvironmentInfo {
        EnvironmentInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_count: std::thread::available_parallelism().ok().map(|n| n.get()),
        }
    }
}

/// Status of a single process execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStatus {
    /// Exited with status 0 within the timeout.
    Ok,
    /// Exited with a non-zero status.
    Failed,
    /// Terminated because it exceeded the timeout.
    TimedOut,
    /// Could not be executed at all (spawn failure).
    SpawnError,
}

/// Phase of an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Warm-up iteration; not included in summaries.
    Warmup,
    /// Measured iteration.
    Measured,
}

/// One process execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// Zero-based index within the whole run.
    pub index: u32,
    /// Warm-up or measured.
    pub phase: Phase,
    /// Execution status.
    pub status: ObservationStatus,
    /// Whether this observation is a valid measurement (status `Ok`).
    pub success: bool,
    /// Exit code when the process exited normally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Terminating signal, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    /// Wall-clock time in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_time_ms: Option<f64>,
    /// CPU time (user + system) in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_time_ms: Option<f64>,
    /// Peak resident set size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_rss_bytes: Option<u64>,
    /// Derived CPU utilization, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_utilization_pct: Option<f64>,
    /// Captured stdout (only when capture was requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// Captured stderr (only when capture was requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    /// Error message when the run failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Observation {
    /// The value of `metric` for this observation, if measured.
    pub fn metric(&self, metric: Metric) -> Option<f64> {
        match metric {
            Metric::WallTime => self.wall_time_ms,
            Metric::CpuTime => self.cpu_time_ms,
            Metric::PeakRss => self.peak_rss_bytes.map(|b| b as f64),
            Metric::CpuUtilization => self.cpu_utilization_pct,
        }
    }
}

/// Overall status of a benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkStatus {
    /// All measured repetitions succeeded.
    Completed,
    /// Some measured repetitions failed but at least one succeeded.
    PartialFailure,
    /// No measured repetition succeeded.
    Failed,
}

/// A measured metric summary plus its provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSummary {
    /// Unit of the metric.
    pub unit: String,
    /// Measurement method and platform notes.
    pub method: String,
    /// Summary statistics of the successful observations.
    pub summary: Summary,
}

/// Aggregated summary of a benchmark run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    /// Number of successful measured repetitions.
    pub successful: usize,
    /// Number of failed measured repetitions.
    pub failed: usize,
    /// Per-metric summaries, keyed by metric machine name.
    pub metrics: BTreeMap<String, MetricSummary>,
}

/// A complete benchmark result artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Schema version of this document.
    pub schema_version: u32,
    /// Producing tool identity.
    pub tool: ToolInfo,
    /// Benchmark identifier.
    pub benchmark_id: String,
    /// Benchmark description.
    #[serde(default)]
    pub description: String,
    /// Command that was executed.
    pub command: Vec<String>,
    /// Working directory used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Requested metrics.
    pub requested_metrics: Vec<Metric>,
    /// Git revision metadata.
    pub revision: RevisionInfo,
    /// Environment metadata.
    pub environment: EnvironmentInfo,
    /// Run start timestamp (RFC 3339, UTC).
    pub started_at: String,
    /// Run finish timestamp (RFC 3339, UTC).
    pub finished_at: String,
    /// Overall run status.
    pub status: BenchmarkStatus,
    /// Number of warm-up iterations requested.
    pub warmups_requested: u32,
    /// Number of measured repetitions requested.
    pub repetitions_requested: u32,
    /// Raw observations (warm-up and measured).
    pub observations: Vec<Observation>,
    /// Run-level errors.
    #[serde(default)]
    pub errors: Vec<String>,
    /// Aggregated summary (absent when no measurement succeeded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<BenchmarkSummary>,
}

/// Human description of the measurement method for a metric.
pub fn metric_method(metric: Metric) -> &'static str {
    match metric {
        Metric::WallTime => "monotonic clock around child wait (Instant)",
        Metric::CpuTime => "getrusage(ru_utime + ru_stime) via wait4",
        Metric::PeakRss => {
            if cfg!(target_os = "macos") {
                "getrusage(ru_maxrss) via wait4; bytes on macOS"
            } else if cfg!(unix) {
                "getrusage(ru_maxrss) via wait4; KiB on Linux"
            } else {
                "unsupported on this platform"
            }
        }
        Metric::CpuUtilization => "derived: cpu_time / wall_time * 100",
    }
}

impl BenchmarkResult {
    /// Construct a result after executing observations.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        benchmark_id: String,
        description: String,
        command: Vec<String>,
        cwd: Option<String>,
        requested_metrics: Vec<Metric>,
        revision: RevisionInfo,
        environment: EnvironmentInfo,
        started_at: String,
        finished_at: String,
        warmups_requested: u32,
        repetitions_requested: u32,
        observations: Vec<Observation>,
        errors: Vec<String>,
    ) -> BenchmarkResult {
        let measured: Vec<&Observation> = observations
            .iter()
            .filter(|o| o.phase == Phase::Measured)
            .collect();
        let successful = measured.iter().filter(|o| o.success).count();
        let failed = measured.len() - successful;
        let status = if successful == 0 {
            BenchmarkStatus::Failed
        } else if failed == 0 {
            BenchmarkStatus::Completed
        } else {
            BenchmarkStatus::PartialFailure
        };

        let summary = if successful == 0 {
            None
        } else {
            let mut metrics = BTreeMap::new();
            for m in &requested_metrics {
                let values: Vec<f64> = measured
                    .iter()
                    .filter(|o| o.success)
                    .filter_map(|o| o.metric(*m))
                    .collect();
                if let Some(s) = stats::summarize(&values) {
                    metrics.insert(
                        m.name().to_string(),
                        MetricSummary {
                            unit: m.unit().to_string(),
                            method: metric_method(*m).to_string(),
                            summary: s,
                        },
                    );
                }
            }
            Some(BenchmarkSummary {
                successful,
                failed,
                metrics,
            })
        };

        BenchmarkResult {
            schema_version: SCHEMA_VERSION,
            tool: ToolInfo::current(),
            benchmark_id,
            description,
            command,
            cwd,
            requested_metrics,
            revision,
            environment,
            started_at,
            finished_at,
            status,
            warmups_requested,
            repetitions_requested,
            observations,
            errors,
            summary,
        }
    }

    /// Successful measured observations.
    pub fn successful_observations(&self) -> impl Iterator<Item = &Observation> {
        self.observations
            .iter()
            .filter(|o| o.phase == Phase::Measured && o.success)
    }

    /// Extract the raw values of a metric from successful measured
    /// observations.
    pub fn metric_values(&self, metric: Metric) -> Vec<f64> {
        self.successful_observations()
            .filter_map(|o| o.metric(metric))
            .collect()
    }

    /// The summary for a metric, if present.
    pub fn metric_summary(&self, metric: Metric) -> Option<&MetricSummary> {
        self.summary
            .as_ref()
            .and_then(|s| s.metrics.get(metric.name()))
    }

    /// Validate structural invariants of an imported artifact.
    ///
    /// This must be called on any artifact before use. It rejects incompatible
    /// schema versions and internally inconsistent documents. It never
    /// executes anything contained in the artifact.
    pub fn validate(&self) -> crate::error::Result<()> {
        use crate::error::Error;
        if self.schema_version > SCHEMA_VERSION {
            return Err(Error::Config(format!(
                "result schema version {} is newer than supported version {}",
                self.schema_version, SCHEMA_VERSION
            )));
        }
        if self.schema_version == 0 {
            return Err(Error::Config("result schema version 0 is invalid".into()));
        }
        if self.benchmark_id.trim().is_empty() {
            return Err(Error::Config("result has an empty benchmark id".into()));
        }
        if self.command.is_empty() {
            return Err(Error::Config(format!(
                "result for '{}' has no command",
                self.benchmark_id
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(index: u32, phase: Phase, wall: Option<f64>, rss: Option<u64>) -> Observation {
        Observation {
            index,
            phase,
            status: if wall.is_some() {
                ObservationStatus::Ok
            } else {
                ObservationStatus::Failed
            },
            success: wall.is_some(),
            exit_code: if wall.is_some() { Some(0) } else { Some(1) },
            signal: None,
            wall_time_ms: wall,
            cpu_time_ms: wall.map(|w| w * 0.5),
            peak_rss_bytes: rss,
            cpu_utilization_pct: wall.map(|_| 50.0),
            stdout: None,
            stderr: None,
            error: None,
        }
    }

    fn sample_result() -> BenchmarkResult {
        let observations = vec![
            obs(0, Phase::Warmup, Some(100.0), Some(1000)),
            obs(1, Phase::Measured, Some(100.0), Some(1000)),
            obs(2, Phase::Measured, Some(110.0), Some(1100)),
            obs(3, Phase::Measured, Some(90.0), Some(900)),
        ];
        BenchmarkResult::build(
            "b".into(),
            "d".into(),
            vec!["echo".into()],
            None,
            vec![Metric::WallTime, Metric::PeakRss, Metric::CpuTime],
            RevisionInfo::default(),
            EnvironmentInfo::current(),
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:01Z".into(),
            1,
            3,
            observations,
            vec![],
        )
    }

    #[test]
    fn summary_excludes_warmups_and_uses_measured_only() {
        let r = sample_result();
        let s = r.summary.as_ref().unwrap();
        assert_eq!(s.successful, 3);
        let wall = &s.metrics["wall_time"].summary;
        assert_eq!(wall.n, 3);
        assert!((wall.median - 100.0).abs() < 1e-9);
        assert!((wall.min - 90.0).abs() < 1e-9);
        assert!((wall.max - 110.0).abs() < 1e-9);
    }

    #[test]
    fn peak_rss_recorded_in_bytes() {
        let r = sample_result();
        let v = r.metric_values(Metric::PeakRss);
        assert_eq!(v, vec![1000.0, 1100.0, 900.0]);
    }

    #[test]
    fn failed_runs_produce_no_summary() {
        let observations = vec![obs(0, Phase::Measured, None, None)];
        let r = BenchmarkResult::build(
            "b".into(),
            String::new(),
            vec!["x".into()],
            None,
            vec![Metric::WallTime],
            RevisionInfo::default(),
            EnvironmentInfo::current(),
            "t0".into(),
            "t1".into(),
            0,
            1,
            observations,
            vec!["boom".into()],
        );
        assert_eq!(r.status, BenchmarkStatus::Failed);
        assert!(r.summary.is_none());
    }

    #[test]
    fn partial_failure_status() {
        let observations = vec![
            obs(0, Phase::Measured, Some(100.0), None),
            obs(1, Phase::Measured, None, None),
        ];
        let r = BenchmarkResult::build(
            "b".into(),
            String::new(),
            vec!["x".into()],
            None,
            vec![Metric::WallTime],
            RevisionInfo::default(),
            EnvironmentInfo::current(),
            "t0".into(),
            "t1".into(),
            0,
            2,
            observations,
            vec![],
        );
        assert_eq!(r.status, BenchmarkStatus::PartialFailure);
        assert_eq!(r.summary.as_ref().unwrap().failed, 1);
    }

    #[test]
    fn validation_rejects_future_schema() {
        let mut r = sample_result();
        r.schema_version = SCHEMA_VERSION + 1;
        assert!(r.validate().is_err());
    }

    #[test]
    fn json_round_trip_preserves_data() {
        let r = sample_result();
        let json = serde_json::to_string(&r).unwrap();
        let back: BenchmarkResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }
}
