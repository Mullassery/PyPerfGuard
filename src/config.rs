//! Configuration schema, loading, and validation.
//!
//! The canonical first-release configuration format is **TOML**. It is
//! human-readable, supports comments, and is already native to the Rust
//! ecosystem. See `docs/adr/0002-configuration-format.md`.
//!
//! Commands are expressed as explicit argument arrays rather than shell
//! strings. This avoids implicit shell expansion and makes execution
//! reproducible and safe.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::metric::Metric;

/// Controls how a benchmark's standard output/error streams are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Redirect to the null device (default; keeps benchmark noise off the
    /// tool's own output streams).
    #[default]
    Null,
    /// Inherit the parent's streams (useful for debugging).
    Inherit,
    /// Capture into the result artifact (bounded by `capture_limit_bytes`).
    Capture,
}

/// Severity of a policy threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Recorded but never affects the exit code.
    Info,
    /// Reported as a warning; does not fail CI.
    Warn,
    /// Reported as a regression; fails CI (exit code 1).
    #[default]
    Block,
}

/// A single regression threshold rule for one metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Threshold {
    /// Metric this rule applies to.
    pub metric: Metric,
    /// Maximum allowed relative increase, in percent (e.g. `5.0` for 5%).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_increase_pct: Option<f64>,
    /// Maximum allowed absolute increase in the metric's native unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_increase_abs: Option<f64>,
    /// Severity when the threshold is breached.
    #[serde(default)]
    pub severity: Severity,
}

/// Regression policy for a benchmark.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    /// Ordered rules evaluated per metric.
    #[serde(default)]
    pub thresholds: Vec<Threshold>,
    /// Minimum number of successful measured repetitions required to make a
    /// comparison at all. Below this, outcomes are inconclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_successful_repetitions: Option<usize>,
    /// Extra tolerance (percent) added to every percentage threshold to
    /// absorb known noise on this workload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_tolerance_pct: Option<f64>,
    /// Treat an inconclusive comparison as blocking (exit code 4).
    #[serde(default)]
    pub fail_on_inconclusive: bool,
    /// Require a statistically significant difference before downgrading a
    /// threshold breach from warning to regression. When `false` (default),
    /// a threshold breach with insufficient evidence is reported as a
    /// regression at the configured severity.
    #[serde(default)]
    pub require_statistical_significance: bool,
}

/// Measurement selection for a benchmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasureConfig {
    /// Metrics to collect. Defaults to wall time and peak RSS.
    #[serde(default = "default_metrics")]
    pub metrics: Vec<Metric>,
}

impl Default for MeasureConfig {
    fn default() -> Self {
        MeasureConfig {
            metrics: default_metrics(),
        }
    }
}

fn default_metrics() -> Vec<Metric> {
    vec![Metric::WallTime, Metric::PeakRss, Metric::CpuTime]
}

/// A single benchmark definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkConfig {
    /// Stable identifier used to correlate history and baselines.
    pub id: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Executable followed by its arguments. `command[0]` is the program.
    pub command: Vec<String>,
    /// Working directory (relative to the config file unless absolute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Number of unmeasured warm-up iterations.
    #[serde(default = "default_warmups")]
    pub warmups: u32,
    /// Number of measured repetitions.
    #[serde(default = "default_repetitions")]
    pub repetitions: u32,
    /// Per-run timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<f64>,
    /// Environment variable overrides for the child process.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Environment variable names whose values must be redacted from logs and
    /// artifacts.
    #[serde(default)]
    pub redact_env: Vec<String>,
    /// Standard output handling.
    #[serde(default)]
    pub stdout: OutputMode,
    /// Standard error handling.
    #[serde(default)]
    pub stderr: OutputMode,
    /// Maximum captured bytes per stream when using `capture`.
    #[serde(default = "default_capture_limit")]
    pub capture_limit_bytes: usize,
    /// Measurement selection.
    #[serde(default)]
    pub measure: MeasureConfig,
    /// Regression policy.
    #[serde(default)]
    pub policy: PolicyConfig,
}

fn default_warmups() -> u32 {
    1
}
fn default_repetitions() -> u32 {
    5
}
fn default_capture_limit() -> usize {
    64 * 1024
}

/// Project-level configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    /// Project name.
    #[serde(default = "default_project_name")]
    pub name: String,
    /// Default baseline selector: `latest`, a result file path, or a commit
    /// SHA prefix. Overridable per run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            name: default_project_name(),
            baseline: None,
        }
    }
}

fn default_project_name() -> String {
    "pyperfguard-project".to_string()
}

/// Output and retention configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    /// Directory for local history and results.
    #[serde(default = "default_output_dir")]
    pub dir: String,
    /// Maximum number of results retained per benchmark (0 = unlimited).
    #[serde(default = "default_retain")]
    pub retain: usize,
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig {
            dir: default_output_dir(),
            retain: default_retain(),
        }
    }
}

fn default_output_dir() -> String {
    ".pyperfguard".to_string()
}
fn default_retain() -> usize {
    100
}

/// Root configuration document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Project settings.
    #[serde(default)]
    pub project: ProjectConfig,
    /// Output settings.
    #[serde(default)]
    pub output: OutputConfig,
    /// Benchmark definitions.
    #[serde(default)]
    pub benchmarks: Vec<BenchmarkConfig>,
}

/// A loaded configuration plus the directory it came from, used to resolve
/// relative paths.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    /// Parsed configuration.
    pub config: Config,
    /// Absolute path to the config file.
    pub path: PathBuf,
    /// Directory containing the config file.
    pub base_dir: PathBuf,
}

impl LoadedConfig {
    /// Load and validate a configuration file.
    pub fn load(path: &Path) -> Result<LoadedConfig> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::io(format!("reading config {}", path.display()), e))?;
        let config: Config = toml::from_str(&text)
            .map_err(|e| Error::Config(format!("failed to parse {}: {}", path.display(), e)))?;
        let base_dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let base_dir = if base_dir.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            base_dir
        };
        let loaded = LoadedConfig {
            config,
            path: path.to_path_buf(),
            base_dir,
        };
        loaded.validate()?;
        Ok(loaded)
    }

    /// Parse a configuration from a string (used in tests).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Result<LoadedConfig> {
        let config: Config = toml::from_str(text)
            .map_err(|e| Error::Config(format!("failed to parse config: {e}")))?;
        let loaded = LoadedConfig {
            config,
            path: PathBuf::from("pyperfguard.toml"),
            base_dir: PathBuf::from("."),
        };
        loaded.validate()?;
        Ok(loaded)
    }

    /// Find the configuration by walking up from the current directory.
    pub fn discover(start: &Path) -> Result<LoadedConfig> {
        let mut dir = start.to_path_buf();
        loop {
            for name in ["pyperfguard.toml", ".pyperfguard.toml"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return LoadedConfig::load(&candidate);
                }
            }
            if !dir.pop() {
                return Err(Error::NotFound(
                    "no pyperfguard.toml found in this directory or any parent".into(),
                ));
            }
        }
    }

    /// Look up a benchmark by id.
    pub fn benchmark(&self, id: &str) -> Result<&BenchmarkConfig> {
        self.config
            .benchmarks
            .iter()
            .find(|b| b.id == id)
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "benchmark '{id}' is not defined in the configuration"
                ))
            })
    }

    /// Resolve the output directory against the config's base directory.
    pub fn output_dir(&self) -> PathBuf {
        let p = Path::new(&self.config.output.dir);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.base_dir.join(p)
        }
    }

    /// Resolve a benchmark's working directory.
    pub fn resolve_cwd(&self, bench: &BenchmarkConfig) -> PathBuf {
        match &bench.cwd {
            Some(c) => {
                let p = Path::new(c);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    self.base_dir.join(p)
                }
            }
            None => self.base_dir.clone(),
        }
    }

    /// Validate the configuration, returning an actionable error on the first
    /// problem found.
    pub fn validate(&self) -> Result<()> {
        if self.config.benchmarks.is_empty() {
            // An empty config is allowed (e.g. for `doctor`/`init` flows), but
            // duplicate and malformed entries are not.
        }
        let mut seen = std::collections::BTreeSet::new();
        for (i, b) in self.config.benchmarks.iter().enumerate() {
            if b.id.trim().is_empty() {
                return Err(Error::Config(format!(
                    "benchmark at index {i} has an empty id"
                )));
            }
            if !b
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            {
                return Err(Error::Config(format!(
                    "benchmark id '{}' may only contain letters, digits, '-', '_', '.'",
                    b.id
                )));
            }
            if !seen.insert(b.id.clone()) {
                return Err(Error::Config(format!("duplicate benchmark id '{}'", b.id)));
            }
            if b.command.is_empty() {
                return Err(Error::Config(format!(
                    "benchmark '{}' has an empty command",
                    b.id
                )));
            }
            if b.command[0].trim().is_empty() {
                return Err(Error::Config(format!(
                    "benchmark '{}' has an empty executable",
                    b.id
                )));
            }
            if b.repetitions == 0 {
                return Err(Error::Config(format!(
                    "benchmark '{}' must have at least 1 repetition",
                    b.id
                )));
            }
            if b.measure.metrics.is_empty() {
                return Err(Error::Config(format!(
                    "benchmark '{}' requests no metrics",
                    b.id
                )));
            }
            for t in &b.policy.thresholds {
                if t.max_increase_pct.is_none() && t.max_increase_abs.is_none() {
                    return Err(Error::Config(format!(
                        "benchmark '{}' has a threshold for metric '{}' with neither \
                         max_increase_pct nor max_increase_abs",
                        b.id,
                        t.metric.name()
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Render a starter configuration file.
pub fn starter_config() -> String {
    r#"# PyPerfGuard configuration
# See https://github.com/example/pyperfguard for documentation.

[project]
name = "my-project"
# Default baseline selector. Override per run with --baseline.
#   "latest"            -> most recent stored result for the benchmark
#   "path/to/file.json" -> an explicit result artifact
#   "<commit-sha>"      -> a stored result whose revision matches
# baseline = "latest"

[output]
dir = ".pyperfguard"
retain = 100

[[benchmarks]]
id = "example-sleep"
description = "Example workload; replace with your own command."
command = ["python3", "-c", "import time; time.sleep(0.02)"]
warmups = 1
repetitions = 5
timeout_seconds = 30

  [benchmarks.measure]
  metrics = ["wall_time", "peak_rss", "cpu_time"]

  [[benchmarks.policy.thresholds]]
  metric = "wall_time"
  max_increase_pct = 5.0
  severity = "block"

  [[benchmarks.policy.thresholds]]
  metric = "peak_rss"
  max_increase_pct = 10.0
  severity = "warn"
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let cfg = LoadedConfig::from_str(
            r#"
[[benchmarks]]
id = "b1"
command = ["echo", "hi"]
"#,
        )
        .unwrap();
        assert_eq!(cfg.config.benchmarks.len(), 1);
        assert_eq!(cfg.config.benchmarks[0].warmups, 1);
        assert_eq!(cfg.config.benchmarks[0].repetitions, 5);
    }

    #[test]
    fn rejects_duplicate_ids() {
        let err = LoadedConfig::from_str(
            r#"
[[benchmarks]]
id = "b1"
command = ["echo"]

[[benchmarks]]
id = "b1"
command = ["echo"]
"#,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("duplicate"));
    }

    #[test]
    fn rejects_empty_command() {
        let err = LoadedConfig::from_str(
            r#"
[[benchmarks]]
id = "b1"
command = []
"#,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("empty command"));
    }

    #[test]
    fn rejects_zero_repetitions() {
        let err = LoadedConfig::from_str(
            r#"
[[benchmarks]]
id = "b1"
command = ["echo"]
repetitions = 0
"#,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("repetition"));
    }

    #[test]
    fn rejects_threshold_without_bounds() {
        let err = LoadedConfig::from_str(
            r#"
[[benchmarks]]
id = "b1"
command = ["echo"]
  [[benchmarks.policy.thresholds]]
  metric = "wall_time"
"#,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("neither"));
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = LoadedConfig::from_str(
            r#"
[[benchmarks]]
id = "b1"
command = ["echo"]
nope = 1
"#,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("nope") || format!("{err}").contains("unknown"));
    }

    #[test]
    fn starter_config_is_valid() {
        let cfg = LoadedConfig::from_str(&starter_config()).unwrap();
        assert_eq!(cfg.config.benchmarks.len(), 1);
        assert_eq!(cfg.config.benchmarks[0].id, "example-sleep");
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Info < Severity::Warn);
        assert!(Severity::Warn < Severity::Block);
    }
}
