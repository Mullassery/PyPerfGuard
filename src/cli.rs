//! Command-line interface definitions.
//!
//! Parsing, help text, and argument validation live here. Command *behavior*
//! lives in [`crate::commands`]. Machine-readable output always goes to
//! stdout; diagnostics and progress go to stderr.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Output format selection for reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Human-readable text.
    Terminal,
    /// Machine-readable JSON.
    Json,
    /// Markdown tables.
    Markdown,
    /// Comma-separated values.
    Csv,
    /// SVG trend chart.
    Svg,
}

/// Top-level CLI.
#[derive(Debug, Parser)]
#[command(
    name = "pyperfguard",
    version,
    about = "Make performance regressions measurable, explainable, reproducible, and enforceable.",
    long_about = "PyPerfGuard is a native benchmark runner and performance regression \
                  guard. It runs commands, measures wall time and process resource usage, \
                  compares against baselines, evaluates configurable regression policies, \
                  and stores versioned results for trend analysis. It requires no hosted \
                  service and integrates with any CI system.",
    disable_help_subcommand = true,
    propagate_version = true
)]
pub struct Cli {
    /// Path to the configuration file (defaults to discovery from the current directory).
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Suppress progress and informational output.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Increase diagnostic verbosity.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a starter configuration file.
    Init(InitArgs),
    /// Execute a configured or ad-hoc benchmark and store the result.
    Run(RunArgs),
    /// Compare two result sets.
    Compare(CompareArgs),
    /// Inspect, register, or clear baselines.
    Baseline(BaselineArgs),
    /// Run benchmarks and evaluate regression policies (CI gate).
    Check(CheckArgs),
    /// Generate reports from stored results.
    Report(ReportArgs),
    /// Query historical benchmark measurements.
    History(HistoryArgs),
    /// Validate configuration or result files.
    Validate(ValidateArgs),
    /// Inspect supported capabilities and environment readiness.
    Doctor,
    /// Display version and build information.
    Version,
}

/// Arguments for `init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Overwrite an existing configuration file.
    #[arg(long)]
    pub force: bool,
}

/// Shared benchmark-selection arguments.
#[derive(Debug, Args)]
pub struct BenchSelectArgs {
    /// Benchmark id defined in the configuration.
    #[arg(value_name = "BENCHMARK")]
    pub benchmark: Option<String>,

    /// Ad-hoc command to benchmark, placed after `--`.
    #[arg(last = true, value_name = "COMMAND")]
    pub command: Vec<String>,
}

/// Arguments for `run`.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Benchmark selection.
    #[command(flatten)]
    pub select: BenchSelectArgs,

    /// Override the number of warm-up iterations.
    #[arg(long)]
    pub warmups: Option<u32>,

    /// Override the number of measured repetitions.
    #[arg(long)]
    pub repetitions: Option<u32>,

    /// Override the per-run timeout in seconds.
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<f64>,

    /// Metrics to measure (repeatable). Defaults to the configured metrics.
    #[arg(long, value_name = "METRIC")]
    pub metric: Vec<String>,

    /// Do not persist the result to local history.
    #[arg(long)]
    pub no_store: bool,

    /// Write the machine-readable result JSON to this file.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Print the machine-readable result JSON to stdout.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Benchmark selection.
    #[command(flatten)]
    pub select: BenchSelectArgs,

    /// Baseline selector: `latest`, `none`, a result file, or a commit SHA.
    #[arg(long, value_name = "SELECTOR")]
    pub baseline: Option<String>,

    /// Override the number of warm-up iterations.
    #[arg(long)]
    pub warmups: Option<u32>,

    /// Override the number of measured repetitions.
    #[arg(long)]
    pub repetitions: Option<u32>,

    /// Override the per-run timeout in seconds.
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<f64>,

    /// Compare against this existing result instead of running a benchmark.
    #[arg(long, value_name = "PATH", conflicts_with = "command")]
    pub result: Option<PathBuf>,

    /// Add a blocking wall-time regression rule (percent increase).
    #[arg(long, value_name = "PERCENT")]
    pub max_time_increase_pct: Option<f64>,

    /// Add a blocking peak-RSS regression rule (percent increase).
    #[arg(long, value_name = "PERCENT")]
    pub max_rss_increase_pct: Option<f64>,

    /// Treat an inconclusive comparison as blocking.
    #[arg(long)]
    pub fail_on_inconclusive: bool,

    /// Do not persist the result to local history.
    #[arg(long)]
    pub no_store: bool,

    /// Write a Markdown summary to this file.
    #[arg(long, value_name = "PATH")]
    pub summary_file: Option<PathBuf>,

    /// Emit the machine-readable check JSON to stdout.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `compare`.
#[derive(Debug, Args)]
pub struct CompareArgs {
    /// First (baseline) result: a file path, `latest`, or a benchmark id.
    #[arg(value_name = "BASELINE")]
    pub baseline: String,

    /// Second (current) result: a file path, `latest`, or a benchmark id.
    #[arg(value_name = "CURRENT")]
    pub current: String,

    /// Restrict output to one metric.
    #[arg(long, value_name = "METRIC")]
    pub metric: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Terminal)]
    pub format: Format,

    /// Write output to a file instead of stdout.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,
}

/// Baseline management subcommands.
#[derive(Debug, Args)]
pub struct BaselineArgs {
    /// Baseline subcommand.
    #[command(subcommand)]
    pub action: BaselineAction,
}

/// Baseline subcommands.
#[derive(Debug, Subcommand)]
pub enum BaselineAction {
    /// List registered baselines.
    List,
    /// Register a baseline.
    Set {
        /// Benchmark id.
        #[arg(long, value_name = "ID")]
        benchmark: String,
        /// Use the most recent stored result.
        #[arg(long, conflicts_with = "result")]
        latest: bool,
        /// Use an explicit result file.
        #[arg(long, value_name = "PATH", conflicts_with = "latest")]
        result: Option<PathBuf>,
    },
    /// Show the registered baseline for a benchmark.
    Show {
        /// Benchmark id.
        #[arg(long, value_name = "ID")]
        benchmark: String,
    },
    /// Remove the registered baseline for a benchmark.
    Clear {
        /// Benchmark id.
        #[arg(long, value_name = "ID")]
        benchmark: String,
    },
    /// Export the baseline registry as JSON for a CI artifact.
    Export {
        /// Output JSON path.
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
    },
    /// Import a previously exported baseline registry.
    Import {
        /// Input JSON path.
        #[arg(value_name = "PATH")]
        input: PathBuf,
    },
}

/// Arguments for `report`.
#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Benchmark id (required unless an explicit result file is given).
    #[arg(long, value_name = "ID")]
    pub benchmark: Option<String>,

    /// Generate the report from this explicit result file.
    #[arg(long, value_name = "PATH", conflicts_with = "benchmark")]
    pub result: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Terminal)]
    pub format: Format,

    /// Metric for trend/CSV/SVG outputs.
    #[arg(long, value_name = "METRIC")]
    pub metric: Option<String>,

    /// Maximum number of history points to include.
    #[arg(long, default_value_t = 30)]
    pub limit: usize,

    /// Write output to a file instead of stdout.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,
}

/// Arguments for `history`.
#[derive(Debug, Args)]
pub struct HistoryArgs {
    /// Benchmark id (optional; all benchmarks when omitted).
    #[arg(long, value_name = "ID")]
    pub benchmark: Option<String>,

    /// Maximum number of entries to list.
    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Terminal)]
    pub format: Format,
}

/// Arguments for `validate`.
#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Files to validate (`.toml` config or `.json` result). When omitted, the
    /// discovered configuration is validated.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Emit machine-readable JSON describing the validation outcome.
    #[arg(long)]
    pub json: bool,
}
