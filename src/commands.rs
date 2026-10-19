//! Command implementations.
//!
//! Each command returns a process exit code. Machine-readable output is
//! written to stdout; diagnostics go to stderr through `tracing`. No command
//! prints progress to stdout when `--json` is requested.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::baseline::BaselineManager;
use crate::cli::*;
use crate::compare;
use crate::config::{Config, LoadedConfig, PolicyConfig, Severity, Threshold};
use crate::error::{exit, Error, Result};
use crate::metric::Metric;
use crate::policy::{self, Outcome, PolicyReport};
use crate::report::{self, TrendPoint};
use crate::results::{BenchmarkResult, BenchmarkStatus, EnvironmentInfo};
use crate::runner::{self, RunnerOptions};
use crate::store::{atomic_write, ResultStore};

/// Dispatch a parsed subcommand.
pub fn dispatch(command: Command, config_path: Option<PathBuf>) -> Result<i32> {
    match command {
        Command::Init(a) => init(a, config_path),
        Command::Run(a) => run(a, config_path),
        Command::Check(a) => check(a, config_path),
        Command::Compare(a) => compare_cmd(a, config_path),
        Command::Baseline(a) => baseline_cmd(a, config_path),
        Command::Report(a) => report_cmd(a, config_path),
        Command::History(a) => history_cmd(a, config_path),
        Command::Validate(a) => validate_cmd(a, config_path),
        Command::Doctor => doctor(config_path),
        Command::Version => version(),
    }
}

fn cwd() -> Result<PathBuf> {
    std::env::current_dir().map_err(|e| Error::io("getting current directory", e))
}

fn load_config(explicit: Option<&Path>) -> Result<Option<LoadedConfig>> {
    match explicit {
        Some(p) => Ok(Some(LoadedConfig::load(p)?)),
        None => match LoadedConfig::discover(&cwd()?) {
            Ok(c) => Ok(Some(c)),
            Err(Error::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        },
    }
}

fn store_for(cfg: Option<&LoadedConfig>) -> Result<ResultStore> {
    match cfg {
        Some(c) => ResultStore::open(c.output_dir()),
        None => ResultStore::open(cwd()?.join(".pyperfguard")),
    }
}

fn base_dir(cfg: Option<&LoadedConfig>) -> Result<PathBuf> {
    match cfg {
        Some(c) => Ok(c.base_dir.clone()),
        None => cwd(),
    }
}

fn minimal_loaded(base: PathBuf) -> LoadedConfig {
    LoadedConfig {
        config: Config::default(),
        path: base.join("pyperfguard.toml"),
        base_dir: base,
    }
}

fn parse_metrics(list: &[String]) -> Result<Vec<Metric>> {
    list.iter()
        .map(|s| {
            Metric::parse(s).ok_or_else(|| {
                Error::Usage(format!(
                    "unknown metric '{s}' (expected one of wall_time, cpu_time, peak_rss, cpu_utilization)"
                ))
            })
        })
        .collect()
}

fn emit(text: &str, output: Option<&Path>) -> Result<()> {
    match output {
        Some(p) => {
            atomic_write(p, text.as_bytes())?;
            tracing::info!("wrote {}", p.display());
        }
        None => {
            print!("{text}");
            if !text.ends_with('\n') {
                println!();
            }
        }
    }
    Ok(())
}

fn runner_options(warmups: Option<u32>, reps: Option<u32>, timeout: Option<f64>) -> RunnerOptions {
    RunnerOptions {
        warmups,
        repetitions: reps,
        timeout: timeout.map(Duration::from_secs_f64),
    }
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

fn init(args: InitArgs, config_path: Option<PathBuf>) -> Result<i32> {
    let path = config_path.unwrap_or_else(|| PathBuf::from("pyperfguard.toml"));
    if path.exists() && !args.force {
        return Err(Error::Usage(format!(
            "{} already exists; use --force to overwrite",
            path.display()
        )));
    }
    atomic_write(&path, crate::config::starter_config().as_bytes())?;
    println!("wrote {}", path.display());
    Ok(exit::SUCCESS)
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn run(args: RunArgs, config_path: Option<PathBuf>) -> Result<i32> {
    if !args.select.command.is_empty() && args.select.benchmark.is_some() {
        return Err(Error::Usage(
            "specify either a benchmark id or an ad-hoc command after `--`, not both".into(),
        ));
    }
    let cfg = load_config(config_path.as_deref())?;
    let store = store_for(cfg.as_ref())?;
    let metrics_override = parse_metrics(&args.metric)?;
    let opts = runner_options(args.warmups, args.repetitions, args.timeout);
    let environment = EnvironmentInfo::current();

    let mut results: Vec<BenchmarkResult> = Vec::new();

    if !args.select.command.is_empty() {
        let id = args
            .select
            .benchmark
            .clone()
            .unwrap_or_else(|| "ad-hoc".to_string());
        let base = base_dir(cfg.as_ref())?;
        let loaded = minimal_loaded(base.clone());
        let mut bench = runner::ad_hoc_benchmark(&id, args.select.command.clone());
        if !metrics_override.is_empty() {
            bench.measure.metrics = metrics_override.clone();
        }
        let revision = crate::git::current_revision(&base);
        results.push(runner::run_benchmark(
            &loaded,
            &bench,
            &opts,
            revision,
            environment,
        )?);
    } else {
        let cfg = cfg.as_ref().ok_or_else(|| {
            Error::Usage(
                "no benchmark specified and no configuration found; pass a benchmark id, an \
                 ad-hoc command after `--`, or run `pyperfguard init`"
                    .into(),
            )
        })?;
        let benches: Vec<_> = match &args.select.benchmark {
            Some(id) => vec![cfg.benchmark(id)?.clone()],
            None => cfg.config.benchmarks.clone(),
        };
        if benches.is_empty() {
            return Err(Error::Usage(
                "the configuration defines no benchmarks".into(),
            ));
        }
        for mut b in benches {
            if !metrics_override.is_empty() {
                b.measure.metrics = metrics_override.clone();
            }
            let revision = crate::git::current_revision(&cfg.base_dir);
            results.push(runner::run_benchmark(
                cfg,
                &b,
                &opts,
                revision,
                environment.clone(),
            )?);
        }
    }

    if args.json {
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        println!("{json}");
    } else {
        for r in &results {
            print!("{}", report::terminal_run(r));
            println!();
        }
    }

    if !args.no_store {
        for r in &results {
            let path = store.save(r)?;
            tracing::debug!("stored result at {}", path.display());
        }
        let retain = cfg.as_ref().map(|c| c.config.output.retain).unwrap_or(100);
        store.enforce_retention(retain)?;
    }

    if let Some(out) = &args.output {
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        atomic_write(out, json.as_bytes())?;
    }

    let any_failed = results.iter().any(|r| r.status == BenchmarkStatus::Failed);
    Ok(if any_failed {
        exit::EXECUTION
    } else {
        exit::SUCCESS
    })
}

// ---------------------------------------------------------------------------
// check
// ---------------------------------------------------------------------------

fn policy_with_flags(mut p: PolicyConfig, args: &CheckArgs) -> PolicyConfig {
    if let Some(t) = args.max_time_increase_pct {
        p.thresholds.push(Threshold {
            metric: Metric::WallTime,
            max_increase_pct: Some(t),
            max_increase_abs: None,
            severity: Severity::Block,
        });
    }
    if let Some(t) = args.max_rss_increase_pct {
        p.thresholds.push(Threshold {
            metric: Metric::PeakRss,
            max_increase_pct: Some(t),
            max_increase_abs: None,
            severity: Severity::Block,
        });
    }
    if args.fail_on_inconclusive {
        p.fail_on_inconclusive = true;
    }
    p
}

/// Choose a baseline with a documented precedence:
///
/// 1. the `--baseline` CLI selector;
/// 2. the `project.baseline` configuration value;
/// 3. the registered baseline for the benchmark;
/// 4. the most recent stored result.
fn choose_baseline(
    mgr: &BaselineManager,
    bench_id: &str,
    cli: Option<&str>,
    cfg_default: Option<&str>,
) -> Result<Option<crate::baseline::BaselineSelection>> {
    if let Some(sel) = cli.or(cfg_default) {
        return mgr.select(bench_id, Some(sel), None);
    }
    if let Some(s) = mgr.select(bench_id, None, None)? {
        return Ok(Some(s));
    }
    mgr.select(bench_id, Some("latest"), None)
}

#[derive(Serialize)]
struct CheckOutput {
    reports: Vec<PolicyReport>,
    overall: Outcome,
    exit_code: i32,
}

fn combine_exit(reports: &[PolicyReport]) -> i32 {
    let overall = reports
        .iter()
        .map(|r| r.overall)
        .max_by_key(|o| o.rank())
        .unwrap_or(Outcome::Pass);
    match overall {
        Outcome::Pass | Outcome::Info | Outcome::Warning => exit::SUCCESS,
        Outcome::Regression => exit::REGRESSION,
        Outcome::Inconclusive => {
            if reports.iter().any(|r| r.fail_on_inconclusive) {
                exit::INCONCLUSIVE
            } else {
                exit::SUCCESS
            }
        }
        Outcome::Error => exit::EXECUTION,
    }
}

fn check(args: CheckArgs, config_path: Option<PathBuf>) -> Result<i32> {
    if !args.select.command.is_empty() && args.select.benchmark.is_some() {
        return Err(Error::Usage(
            "specify either a benchmark id or an ad-hoc command after `--`, not both".into(),
        ));
    }
    let cfg = load_config(config_path.as_deref())?;
    let store = store_for(cfg.as_ref())?;
    let mgr = BaselineManager::new(store.clone());
    let environment = EnvironmentInfo::current();
    let opts = runner_options(args.warmups, args.repetitions, args.timeout);

    let mut reports: Vec<PolicyReport> = Vec::new();

    if let Some(result_path) = &args.result {
        // Compare an existing result; do not run anything.
        let current = store.load_path(result_path)?;
        let policy = match cfg.as_ref().and_then(|c| {
            c.config
                .benchmarks
                .iter()
                .find(|b| b.id == current.benchmark_id)
        }) {
            Some(b) => policy_with_flags(b.policy.clone(), &args),
            None => policy_with_flags(PolicyConfig::default(), &args),
        };
        let selector = args
            .baseline
            .clone()
            .or_else(|| cfg.as_ref().and_then(|c| c.config.project.baseline.clone()));
        let baseline = choose_baseline(&mgr, &current.benchmark_id, selector.as_deref(), None)?;
        reports.push(policy::evaluate(
            &policy,
            baseline.as_ref().map(|s| &s.result),
            baseline.as_ref().map(|s| s.source.clone()),
            &current,
        ));
    } else {
        let jobs: Vec<(String, PolicyConfig)> = if !args.select.command.is_empty() {
            let id = args
                .select
                .benchmark
                .clone()
                .unwrap_or_else(|| "ad-hoc".to_string());
            let bench = runner::ad_hoc_benchmark(&id, args.select.command.clone());
            vec![(
                bench.id.clone(),
                policy_with_flags(bench.policy.clone(), &args),
            )]
        } else {
            let cfg = cfg.as_ref().ok_or_else(|| {
                Error::Usage("no benchmark specified and no configuration found".into())
            })?;
            let benches: Vec<_> = match &args.select.benchmark {
                Some(id) => vec![cfg.benchmark(id)?.clone()],
                None => cfg.config.benchmarks.clone(),
            };
            benches
                .into_iter()
                .map(|b| {
                    let p = policy_with_flags(b.policy.clone(), &args);
                    (b.id.clone(), p)
                })
                .collect()
        };

        for (bench_id, policy) in jobs {
            // Select the baseline *before* running so we never compare a run
            // against itself.
            let selector = args
                .baseline
                .clone()
                .or_else(|| cfg.as_ref().and_then(|c| c.config.project.baseline.clone()));
            let baseline = choose_baseline(&mgr, &bench_id, selector.as_deref(), None)?;

            let current = if !args.select.command.is_empty() {
                let base = base_dir(cfg.as_ref())?;
                let loaded = minimal_loaded(base.clone());
                let bench = runner::ad_hoc_benchmark(&bench_id, args.select.command.clone());
                let revision = crate::git::current_revision(&base);
                runner::run_benchmark(&loaded, &bench, &opts, revision, environment.clone())?
            } else {
                let cfgref = cfg.as_ref().unwrap();
                let bench = cfgref.benchmark(&bench_id)?.clone();
                let revision = crate::git::current_revision(&cfgref.base_dir);
                runner::run_benchmark(cfgref, &bench, &opts, revision, environment.clone())?
            };

            if !args.no_store {
                store.save(&current)?;
                let retain = cfg.as_ref().map(|c| c.config.output.retain).unwrap_or(100);
                store.enforce_retention(retain)?;
            }

            reports.push(policy::evaluate(
                &policy,
                baseline.as_ref().map(|s| &s.result),
                baseline.as_ref().map(|s| s.source.clone()),
                &current,
            ));
        }
    }

    let exit_code = combine_exit(&reports);
    let overall = reports
        .iter()
        .map(|r| r.overall)
        .max_by_key(|o| o.rank())
        .unwrap_or(Outcome::Pass);

    if args.json {
        let out = CheckOutput {
            reports: reports.clone(),
            overall,
            exit_code,
        };
        let json =
            serde_json::to_string_pretty(&out).map_err(|e| Error::Serialization(e.to_string()))?;
        println!("{json}");
    } else {
        for r in &reports {
            print!("{}", report::terminal_policy(r));
            println!();
        }
    }

    // Markdown summary for CI.
    let markdown = report::markdown_summary(&reports);
    if let Some(path) = &args.summary_file {
        atomic_write(path, markdown.as_bytes())?;
    }
    if let Ok(step_summary) = std::env::var("GITHUB_STEP_SUMMARY") {
        append_to_file(Path::new(&step_summary), &markdown);
    }

    Ok(exit_code)
}

fn append_to_file(path: &Path, text: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(text.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// compare
// ---------------------------------------------------------------------------

fn resolve_result(
    store: &ResultStore,
    mgr: &BaselineManager,
    value: &str,
) -> Result<BenchmarkResult> {
    let as_path = Path::new(value);
    if as_path.extension().and_then(|e| e.to_str()) == Some("json") || as_path.exists() {
        if !as_path.exists() {
            return Err(Error::NotFound(format!(
                "result file '{value}' does not exist"
            )));
        }
        return store.load_path(as_path);
    }
    if let Some((id, selector)) = value.split_once('@') {
        let sel = mgr.select(id, Some(selector), None)?.ok_or_else(|| {
            Error::NotFound(format!("no result for '{id}' matching '{selector}'"))
        })?;
        return Ok(sel.result);
    }
    // A bare benchmark id: use its most recent stored result.
    let entry = store
        .latest(value, None)?
        .ok_or_else(|| Error::NotFound(format!("no stored results for benchmark '{value}'")))?;
    store.load_entry(&entry)
}

fn compare_cmd(args: CompareArgs, config_path: Option<PathBuf>) -> Result<i32> {
    let cfg = load_config(config_path.as_deref())?;
    let store = store_for(cfg.as_ref())?;
    let mgr = BaselineManager::new(store.clone());
    let baseline = resolve_result(&store, &mgr, &args.baseline)?;
    let current = resolve_result(&store, &mgr, &args.current)?;
    let mut cmp = compare::compare(&baseline, &current);
    if let Some(metric) = &args.metric {
        let m = Metric::parse(metric)
            .ok_or_else(|| Error::Usage(format!("unknown metric '{metric}'")))?;
        cmp.metrics.retain(|mc| mc.metric == m);
    }
    let text = match args.format {
        Format::Terminal => report::terminal_comparison(&cmp),
        Format::Json => {
            serde_json::to_string_pretty(&cmp).map_err(|e| Error::Serialization(e.to_string()))?
        }
        Format::Markdown => report::markdown_comparison(&cmp),
        Format::Csv => report::comparison_csv(&cmp),
        Format::Svg => {
            return Err(Error::Usage(
                "SVG output is only available from `report` (it needs history)".into(),
            ))
        }
    };
    emit(&text, args.output.as_deref())?;
    Ok(exit::SUCCESS)
}

// ---------------------------------------------------------------------------
// baseline
// ---------------------------------------------------------------------------

fn baseline_cmd(args: BaselineArgs, config_path: Option<PathBuf>) -> Result<i32> {
    let cfg = load_config(config_path.as_deref())?;
    let store = store_for(cfg.as_ref())?;
    let mgr = BaselineManager::new(store.clone());
    match args.action {
        BaselineAction::List => {
            let entries = mgr.list()?;
            if entries.is_empty() {
                println!("no baselines registered");
            } else {
                println!(
                    "{:<24} {:<22} {:<10} file",
                    "benchmark", "started_at", "commit"
                );
                for e in entries {
                    println!(
                        "{:<24} {:<22} {:<10} {}",
                        e.benchmark_id,
                        e.started_at,
                        e.revision
                            .commit
                            .as_deref()
                            .map(crate::git::short_sha)
                            .unwrap_or_else(|| "-".into()),
                        e.file
                    );
                }
            }
        }
        BaselineAction::Set {
            benchmark,
            latest,
            result,
        } => {
            let entry = if let Some(path) = result {
                mgr.register_file(&path)?
            } else if latest {
                mgr.register_latest(&benchmark)?
            } else {
                return Err(Error::Usage(
                    "specify --latest or --result <path> when registering a baseline".into(),
                ));
            };
            println!(
                "baseline for '{}' set to {} ({})",
                entry.benchmark_id, entry.file, entry.started_at
            );
        }
        BaselineAction::Show { benchmark } => match mgr.get(&benchmark)? {
            Some(e) => {
                println!("benchmark: {}", e.benchmark_id);
                println!("file:      {}", e.file);
                println!("started:   {}", e.started_at);
                println!(
                    "commit:    {}",
                    e.revision.commit.unwrap_or_else(|| "(none)".into())
                );
            }
            None => println!("no baseline registered for '{benchmark}'"),
        },
        BaselineAction::Clear { benchmark } => {
            if mgr.clear(&benchmark)? {
                println!("cleared baseline for '{benchmark}'");
            } else {
                println!("no baseline registered for '{benchmark}'");
            }
        }
        BaselineAction::Export { output } => {
            mgr.export_to(&output)?;
            println!("exported baselines to {}", output.display());
        }
        BaselineAction::Import { input } => {
            let count = mgr.import_from(&input)?;
            println!("imported {} baseline(s) from {}", count, input.display());
        }
    }
    Ok(exit::SUCCESS)
}

// ---------------------------------------------------------------------------
// report
// ---------------------------------------------------------------------------

fn trend_points(
    store: &ResultStore,
    benchmark: &str,
    metric: Metric,
    limit: usize,
) -> Result<Vec<TrendPoint>> {
    let entries = store.list(Some(benchmark))?;
    let mut points = Vec::new();
    for entry in entries.into_iter().take(limit) {
        let result = store.load_entry(&entry)?;
        if let Some(ms) = result.metric_summary(metric) {
            points.push(TrendPoint {
                started_at: result.started_at.clone(),
                commit: result.revision.commit.clone(),
                n: ms.summary.n,
                summary: ms.summary.clone(),
            });
        }
    }
    // Oldest first for charts and CSV.
    points.reverse();
    Ok(points)
}

fn report_cmd(args: ReportArgs, config_path: Option<PathBuf>) -> Result<i32> {
    let cfg = load_config(config_path.as_deref())?;
    let store = store_for(cfg.as_ref())?;
    let metric = match &args.metric {
        Some(m) => Metric::parse(m).ok_or_else(|| Error::Usage(format!("unknown metric '{m}'")))?,
        None => Metric::WallTime,
    };

    let text = if let Some(path) = &args.result {
        let result = store.load_path(path)?;
        match args.format {
            Format::Terminal => report::terminal_run(&result),
            Format::Json => serde_json::to_string_pretty(&result)
                .map_err(|e| Error::Serialization(e.to_string()))?,
            Format::Markdown => report::markdown_run(&result),
            Format::Csv => {
                let points = vec![TrendPoint {
                    started_at: result.started_at.clone(),
                    commit: result.revision.commit.clone(),
                    n: result
                        .metric_summary(metric)
                        .map(|m| m.summary.n)
                        .unwrap_or(0),
                    summary: result
                        .metric_summary(metric)
                        .map(|m| m.summary.clone())
                        .ok_or_else(|| {
                            Error::NotFound(format!("metric '{}' not measured", metric.name()))
                        })?,
                }];
                report::csv_trend(metric, &points)
            }
            Format::Svg => {
                return Err(Error::Usage(
                    "SVG needs at least two history points; use --benchmark instead".into(),
                ))
            }
        }
    } else {
        let benchmark = args
            .benchmark
            .as_ref()
            .ok_or_else(|| Error::Usage("specify --benchmark <id> or --result <path>".into()))?;
        let entries = store.list(Some(benchmark))?;
        if entries.is_empty() {
            return Err(Error::NotFound(format!(
                "no stored results for benchmark '{benchmark}'"
            )));
        }
        match args.format {
            Format::Terminal => {
                let latest = store.load_entry(&entries[0])?;
                let points = trend_points(&store, benchmark, metric, args.limit)?;
                let mut out = report::terminal_run(&latest);
                if points.len() >= 2 {
                    let spark: String = report::sparkline(
                        &points.iter().map(|p| p.summary.median).collect::<Vec<_>>(),
                    );
                    out.push_str(&format!(
                        "\n  {} trend (median {}): {}\n",
                        benchmark,
                        metric.name(),
                        spark
                    ));
                }
                out
            }
            Format::Json => serde_json::to_string_pretty(&entries)
                .map_err(|e| Error::Serialization(e.to_string()))?,
            Format::Markdown => {
                let latest = store.load_entry(&entries[0])?;
                report::markdown_run(&latest)
            }
            Format::Csv => {
                let points = trend_points(&store, benchmark, metric, args.limit)?;
                report::csv_trend(metric, &points)
            }
            Format::Svg => {
                let points = trend_points(&store, benchmark, metric, args.limit)?;
                report::svg_trend(metric, &points, benchmark)
            }
        }
    };
    emit(&text, args.output.as_deref())?;
    Ok(exit::SUCCESS)
}

// ---------------------------------------------------------------------------
// history
// ---------------------------------------------------------------------------

fn history_cmd(args: HistoryArgs, config_path: Option<PathBuf>) -> Result<i32> {
    let cfg = load_config(config_path.as_deref())?;
    let store = store_for(cfg.as_ref())?;
    let entries: Vec<_> = store
        .list(args.benchmark.as_deref())?
        .into_iter()
        .take(args.limit)
        .collect();

    match args.format {
        Format::Json => {
            let json = serde_json::to_string_pretty(&entries)
                .map_err(|e| Error::Serialization(e.to_string()))?;
            println!("{json}");
        }
        Format::Csv => {
            let benchmark = args
                .benchmark
                .as_deref()
                .ok_or_else(|| Error::Usage("CSV history output requires --benchmark".into()))?;
            let points = trend_points(&store, benchmark, Metric::WallTime, args.limit)?;
            print!("{}", report::csv_trend(Metric::WallTime, &points));
        }
        _ => {
            if entries.is_empty() {
                println!("no history");
            } else {
                println!(
                    "{:<24} {:<22} {:<10} {:<14} status",
                    "benchmark", "started_at", "commit", "median wall"
                );
                for e in entries {
                    let median = store
                        .load_entry(&e)
                        .ok()
                        .and_then(|r| r.metric_summary(Metric::WallTime).map(|m| m.summary.median))
                        .map(|v| report::fmt_metric(Metric::WallTime, v))
                        .unwrap_or_else(|| "n/a".into());
                    println!(
                        "{:<24} {:<22} {:<10} {:<14} {:?}",
                        e.benchmark_id,
                        e.started_at,
                        e.revision
                            .commit
                            .as_deref()
                            .map(crate::git::short_sha)
                            .unwrap_or_else(|| "-".into()),
                        median,
                        e.status
                    );
                }
            }
        }
    }
    Ok(exit::SUCCESS)
}

// ---------------------------------------------------------------------------
// validate
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ValidationItem {
    path: String,
    kind: String,
    valid: bool,
    error: Option<String>,
}

fn validate_cmd(args: ValidateArgs, config_path: Option<PathBuf>) -> Result<i32> {
    let mut items: Vec<ValidationItem> = Vec::new();

    let paths: Vec<PathBuf> = if args.paths.is_empty() {
        match load_config(config_path.as_deref()) {
            Ok(Some(c)) => vec![c.path],
            Ok(None) => {
                return Err(Error::NotFound(
                    "no configuration file found to validate".into(),
                ))
            }
            Err(e) => return Err(e),
        }
    } else {
        args.paths.clone()
    };

    let mut all_ok = true;
    for path in &paths {
        let kind = match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => "config",
            Some("json") => "result",
            _ => "unknown",
        };
        let outcome = match kind {
            "config" => LoadedConfig::load(path).map(|_| ()),
            "result" => ResultStore::open(cwd()?.join(".pyperfguard"))
                .and_then(|s| s.load_path(path).map(|_| ())),
            _ => Err(Error::Usage(format!(
                "cannot determine the type of {} (expected .toml or .json)",
                path.display()
            ))),
        };
        let (valid, error) = match outcome {
            Ok(()) => (true, None),
            Err(e) => {
                all_ok = false;
                (false, Some(format!("{e}")))
            }
        };
        items.push(ValidationItem {
            path: path.display().to_string(),
            kind: kind.to_string(),
            valid,
            error,
        });
    }

    if args.json {
        let json = serde_json::to_string_pretty(&items)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        println!("{json}");
    } else {
        for it in &items {
            if it.valid {
                println!("ok    {} ({})", it.path, it.kind);
            } else {
                println!(
                    "FAIL  {} ({}): {}",
                    it.path,
                    it.kind,
                    it.error.as_deref().unwrap_or("unknown error")
                );
            }
        }
    }

    Ok(if all_ok { exit::SUCCESS } else { exit::CONFIG })
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

fn doctor(config_path: Option<PathBuf>) -> Result<i32> {
    println!("pyperfguard {}", env!("CARGO_PKG_VERSION"));
    println!("  os:                 {}", std::env::consts::OS);
    println!("  arch:               {}", std::env::consts::ARCH);
    println!(
        "  cpu count:          {}",
        std::thread::available_parallelism()
            .map(|n| n.get().to_string())
            .unwrap_or_else(|_| "unknown".into())
    );
    println!(
        "  process rusage:     {}",
        if crate::process::supports_process_rusage() {
            "available (cpu_time, peak_rss)"
        } else {
            "unavailable (wall time only)"
        }
    );
    println!(
        "  peak RSS method:    {}",
        crate::results::metric_method(Metric::PeakRss)
    );
    let base = base_dir(None)?;
    println!("  git repository:     {}", crate::git::is_repo(&base));
    if crate::git::is_repo(&base) {
        println!("  shallow clone:      {}", crate::git::is_shallow(&base));
        let rev = crate::git::current_revision(&base);
        println!(
            "  current revision:   {}",
            rev.commit.as_deref().unwrap_or("(none)")
        );
    }
    match load_config(config_path.as_deref()) {
        Ok(Some(c)) => {
            println!("  config:             {}", c.path.display());
            println!("  benchmarks:         {}", c.config.benchmarks.len());
            println!("  output dir:         {}", c.output_dir().display());
        }
        Ok(None) => println!("  config:             (none found)"),
        Err(e) => println!("  config:             invalid: {e}"),
    }
    Ok(exit::SUCCESS)
}

// ---------------------------------------------------------------------------
// version
// ---------------------------------------------------------------------------

fn version() -> Result<i32> {
    println!("pyperfguard {}", env!("CARGO_PKG_VERSION"));
    println!("  schema version: {}", crate::results::SCHEMA_VERSION);
    println!(
        "  target: {} / {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    Ok(exit::SUCCESS)
}
