//! Metric identifiers shared between configuration, measurement, analysis,
//! and reporting.
//!
//! Every metric has an explicit unit and a stable machine name. Measurement
//! availability is tracked separately from the metric identity so that a
//! missing measurement is never confused with a zero value.

use serde::{Deserialize, Serialize};

/// A process resource or timing metric that PyPerfGuard can measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Elapsed wall-clock time.
    WallTime,
    /// CPU time (user + system) consumed by the process.
    CpuTime,
    /// Peak resident set size of the process.
    PeakRss,
    /// CPU time divided by wall time, in percent (may exceed 100 with threads).
    CpuUtilization,
}

impl Metric {
    /// Stable machine-readable name (used in JSON and config).
    pub fn name(self) -> &'static str {
        match self {
            Metric::WallTime => "wall_time",
            Metric::CpuTime => "cpu_time",
            Metric::PeakRss => "peak_rss",
            Metric::CpuUtilization => "cpu_utilization",
        }
    }

    /// Human-readable unit string.
    pub fn unit(self) -> &'static str {
        match self {
            Metric::WallTime => "ms",
            Metric::CpuTime => "ms",
            Metric::PeakRss => "bytes",
            Metric::CpuUtilization => "percent",
        }
    }

    /// Parse a metric from its machine name.
    pub fn parse(s: &str) -> Option<Metric> {
        match s {
            "wall_time" | "time" => Some(Metric::WallTime),
            "cpu_time" => Some(Metric::CpuTime),
            "peak_rss" | "rss" | "memory" => Some(Metric::PeakRss),
            "cpu_utilization" | "cpu" => Some(Metric::CpuUtilization),
            _ => None,
        }
    }

    /// All metrics in a stable order.
    pub fn all() -> [Metric; 4] {
        [
            Metric::WallTime,
            Metric::CpuTime,
            Metric::PeakRss,
            Metric::CpuUtilization,
        ]
    }

    /// Whether a lower value is better (true for all current metrics).
    pub fn lower_is_better(self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for m in Metric::all() {
            assert_eq!(Metric::parse(m.name()), Some(m));
        }
    }

    #[test]
    fn aliases_parse() {
        assert_eq!(Metric::parse("time"), Some(Metric::WallTime));
        assert_eq!(Metric::parse("memory"), Some(Metric::PeakRss));
        assert_eq!(Metric::parse("cpu"), Some(Metric::CpuUtilization));
        assert_eq!(Metric::parse("bogus"), None);
    }
}
