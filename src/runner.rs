//! Benchmark execution engine.
//!
//! Owns warm-up runs, measured repetitions, per-run and overall timeouts, the
//! distinction between successful measurements and failed executions, and the
//! construction of a [`BenchmarkResult`]. It never silently treats a failed
//! execution as a valid performance observation.

use std::time::Duration;

use crate::config::{BenchmarkConfig, LoadedConfig, OutputMode};
use crate::error::Result;
use crate::metric::Metric;
use crate::process::{self, CommandSpec};
use crate::results::{
    BenchmarkResult, EnvironmentInfo, Observation, ObservationStatus, Phase, RevisionInfo,
};
use crate::time_util;

/// Per-invocation overrides supplied on the command line.
#[derive(Debug, Clone, Default)]
pub struct RunnerOptions {
    /// Override the configured warm-up count.
    pub warmups: Option<u32>,
    /// Override the configured measured repetition count.
    pub repetitions: Option<u32>,
    /// Override the configured per-run timeout.
    pub timeout: Option<Duration>,
}

impl RunnerOptions {
    fn warmups(&self, bench: &BenchmarkConfig) -> u32 {
        self.warmups.unwrap_or(bench.warmups)
    }
    fn repetitions(&self, bench: &BenchmarkConfig) -> u32 {
        self.repetitions.unwrap_or(bench.repetitions)
    }
    fn timeout(&self, bench: &BenchmarkConfig) -> Option<Duration> {
        match self.timeout {
            Some(t) => Some(t),
            None => bench.timeout_seconds.map(Duration::from_secs_f64),
        }
    }
}

fn build_spec(loaded: &LoadedConfig, bench: &BenchmarkConfig, opts: &RunnerOptions) -> CommandSpec {
    CommandSpec {
        program_args: bench.command.clone(),
        cwd: Some(loaded.resolve_cwd(bench)),
        env: bench.env.clone(),
        timeout: opts.timeout(bench),
        stdout: bench.stdout,
        stderr: bench.stderr,
        capture_limit: bench.capture_limit_bytes,
    }
}

fn observation_from(index: u32, phase: Phase, raw: &process::RawRun) -> Observation {
    let status = if raw.timed_out {
        ObservationStatus::TimedOut
    } else if raw.exit_code == Some(0) {
        ObservationStatus::Ok
    } else {
        ObservationStatus::Failed
    };
    let cpu_ms = raw.cpu_ms();
    let cpu_utilization_pct = match (cpu_ms, raw.wall_ms) {
        (Some(cpu), wall) if wall > 0.0 => Some(cpu / wall * 100.0),
        _ => None,
    };
    Observation {
        index,
        phase,
        status,
        success: raw.success(),
        exit_code: raw.exit_code,
        signal: raw.signal,
        wall_time_ms: Some(raw.wall_ms),
        cpu_time_ms: cpu_ms,
        peak_rss_bytes: raw.peak_rss_bytes,
        cpu_utilization_pct,
        stdout: raw
            .stdout
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned()),
        stderr: raw
            .stderr
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned()),
        error: None,
    }
}

fn failure_observation(index: u32, phase: Phase, message: String) -> Observation {
    Observation {
        index,
        phase,
        status: ObservationStatus::SpawnError,
        success: false,
        exit_code: None,
        signal: None,
        wall_time_ms: None,
        cpu_time_ms: None,
        peak_rss_bytes: None,
        cpu_utilization_pct: None,
        stdout: None,
        stderr: None,
        error: Some(message),
    }
}

/// Execute a benchmark according to its configuration, returning a fully
/// populated result artifact.
///
/// Warm-up iterations are executed first and are excluded from the summary.
/// If the process cannot be spawned at all, execution stops early, because
/// repeating an identical failure wastes time and yields no measurement.
pub fn run_benchmark(
    loaded: &LoadedConfig,
    bench: &BenchmarkConfig,
    opts: &RunnerOptions,
    revision: RevisionInfo,
    environment: EnvironmentInfo,
) -> Result<BenchmarkResult> {
    let warmups = opts.warmups(bench);
    let repetitions = opts.repetitions(bench);
    let spec = build_spec(loaded, bench, opts);

    let started_at = time_util::now_rfc3339();
    let mut observations: Vec<Observation> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    let total = warmups + repetitions;
    'outer: for (index, i) in (0u32..).zip(0..total) {
        let phase = if i < warmups {
            Phase::Warmup
        } else {
            Phase::Measured
        };
        match process::execute(&spec) {
            Ok(raw) => {
                observations.push(observation_from(index, phase, &raw));
            }
            Err(e) => {
                let msg = format!("{e}");
                errors.push(msg.clone());
                observations.push(failure_observation(index, phase, msg));
                // Cannot spawn: stop early.
                break 'outer;
            }
        }
    }
    let finished_at = time_util::now_rfc3339();

    let command = spec.program_args.clone();
    let cwd = spec.cwd.as_ref().map(|p| p.display().to_string());
    let mut requested_metrics = bench.measure.metrics.clone();
    if requested_metrics.is_empty() {
        requested_metrics = vec![Metric::WallTime];
    }

    Ok(BenchmarkResult::build(
        bench.id.clone(),
        bench.description.clone(),
        command,
        cwd,
        requested_metrics,
        revision,
        environment,
        started_at,
        finished_at,
        warmups,
        repetitions,
        observations,
        errors,
    ))
}

/// Build an ad-hoc benchmark definition from an explicit command. Used by
/// `run`/`check` when no configuration or benchmark id is supplied.
pub fn ad_hoc_benchmark(id: &str, command: Vec<String>) -> BenchmarkConfig {
    BenchmarkConfig {
        id: id.to_string(),
        description: "ad-hoc benchmark".to_string(),
        command,
        cwd: None,
        warmups: 1,
        repetitions: 5,
        timeout_seconds: Some(30.0),
        env: Default::default(),
        redact_env: Vec::new(),
        stdout: OutputMode::Null,
        stderr: OutputMode::Null,
        capture_limit_bytes: 64 * 1024,
        measure: Default::default(),
        policy: Default::default(),
    }
}

/// Run an ad-hoc command without a configuration file.
pub fn run_ad_hoc(
    id: &str,
    command: Vec<String>,
    opts: &RunnerOptions,
    base_dir: &std::path::Path,
) -> Result<BenchmarkResult> {
    let bench = {
        let mut b = ad_hoc_benchmark(id, command);
        if let Some(w) = opts.warmups {
            b.warmups = w;
        }
        if let Some(r) = opts.repetitions {
            b.repetitions = r;
        }
        if let Some(t) = opts.timeout {
            b.timeout_seconds = Some(t.as_secs_f64());
        }
        b
    };
    let loaded = LoadedConfig {
        config: crate::config::Config::default(),
        path: base_dir.join("pyperfguard.toml"),
        base_dir: base_dir.to_path_buf(),
    };
    let revision = crate::git::current_revision(base_dir);
    let environment = EnvironmentInfo::current();
    run_benchmark(&loaded, &bench, opts, revision, environment)
}
