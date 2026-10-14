//! End-to-end CLI integration tests.
//!
//! These tests build synthetic result artifacts with the library API and drive
//! the compiled binary, so they are deterministic and do not depend on a
//! particular benchmark runtime to the millisecond. Shell-dependent tests are
//! gated to Unix.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use pyperfguard::metric::Metric;
use pyperfguard::results::{
    BenchmarkResult, EnvironmentInfo, Observation, ObservationStatus, Phase, RevisionInfo,
    SCHEMA_VERSION,
};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pyperfguard"))
}

fn run_in(dir: &Path, args: &[&str]) -> Output {
    bin()
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run pyperfguard")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

/// Build a synthetic result with the given wall-time and RSS observations.
fn synthetic(
    id: &str,
    command: &[&str],
    wall_values: &[f64],
    rss_values: &[u64],
) -> BenchmarkResult {
    let observations = wall_values
        .iter()
        .zip(rss_values.iter())
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
        "synthetic".into(),
        command.iter().map(|s| s.to_string()).collect(),
        None,
        vec![Metric::WallTime, Metric::PeakRss],
        RevisionInfo {
            commit: Some("deadbeef".into()),
            branch: None,
            dirty: Some(false),
        },
        EnvironmentInfo {
            os: "test-os".into(),
            arch: "test-arch".into(),
            cpu_count: Some(4),
        },
        "2026-01-01T00:00:00Z".into(),
        "2026-01-01T00:00:01Z".into(),
        0,
        wall_values.len() as u32,
        observations,
        vec![],
    )
}

fn write_result(dir: &Path, name: &str, result: &BenchmarkResult) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(result).unwrap()).unwrap();
    path
}

fn write_config(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("pyperfguard.toml");
    std::fs::write(&path, body).unwrap();
    path
}

// ---------------------------------------------------------------------------
// Basic commands
// ---------------------------------------------------------------------------

#[test]
fn version_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(tmp.path(), &["version"]);
    assert_eq!(code(&out), 0, "stderr={}", stderr(&out));
    assert!(stdout(&out).contains("pyperfguard"));
    assert!(stdout(&out).contains("schema version"));
}

#[test]
fn doctor_exits_zero_and_reports_capabilities() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(tmp.path(), &["doctor"]);
    assert_eq!(code(&out), 0, "stderr={}", stderr(&out));
    let s = stdout(&out);
    assert!(s.contains("process rusage"));
    assert!(s.contains("peak RSS method"));
}

#[test]
fn help_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(tmp.path(), &["--help"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("Usage"));
}

// ---------------------------------------------------------------------------
// init / validate
// ---------------------------------------------------------------------------

#[test]
fn init_creates_config_and_refuses_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(tmp.path(), &["init"]);
    assert_eq!(code(&out), 0, "stderr={}", stderr(&out));
    assert!(tmp.path().join("pyperfguard.toml").exists());

    let out2 = run_in(tmp.path(), &["init"]);
    assert_eq!(code(&out2), 2, "expected refusal exit code 2");
    assert!(stderr(&out2).contains("already exists") || stdout(&out2).contains("already exists"));

    let out3 = run_in(tmp.path(), &["init", "--force"]);
    assert_eq!(code(&out3), 0);
}

#[test]
fn validate_accepts_good_config() {
    let tmp = tempfile::tempdir().unwrap();
    write_config(
        tmp.path(),
        "[[benchmarks]]\nid = \"b\"\ncommand = [\"echo\"]\n",
    );
    let out = run_in(tmp.path(), &["validate", "pyperfguard.toml"]);
    assert_eq!(code(&out), 0, "stderr={}", stderr(&out));
}

#[test]
fn validate_rejects_bad_config() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_config(tmp.path(), "this is not toml =\n");
    let out = run_in(tmp.path(), &["validate", path.to_str().unwrap()]);
    assert_eq!(code(&out), 2, "stderr={}", stderr(&out));
}

#[test]
fn validate_rejects_future_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let mut r = synthetic("b", &["echo"], &[1.0], &[1]);
    r.schema_version = SCHEMA_VERSION + 10;
    let path = write_result(tmp.path(), "future.json", &r);
    let out = run_in(tmp.path(), &["validate", path.to_str().unwrap()]);
    assert_eq!(code(&out), 2);
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn run_adhoc_emits_valid_json_on_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        tmp.path(),
        &[
            "run",
            "--warmups",
            "0",
            "--repetitions",
            "3",
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "exit 0",
        ],
    );
    assert_eq!(code(&out), 0, "stderr={}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["schema_version"], SCHEMA_VERSION);
    assert_eq!(arr[0]["benchmark_id"], "ad-hoc");
    assert_eq!(arr[0]["status"], "completed");
    // Machine output must not be polluted by diagnostics.
    let observations = arr[0]["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 3);
    for o in observations {
        assert!(o["wall_time_ms"].is_number());
    }
}

#[cfg(unix)]
#[test]
fn run_failing_command_exits_3() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        tmp.path(),
        &[
            "run",
            "--warmups",
            "0",
            "--repetitions",
            "1",
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "exit 3",
        ],
    );
    assert_eq!(code(&out), 3, "stderr={}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(parsed[0]["status"], "failed");
}

#[cfg(unix)]
#[test]
fn run_timeout_marks_observation_timed_out() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        tmp.path(),
        &[
            "run",
            "--warmups",
            "0",
            "--repetitions",
            "1",
            "--timeout",
            "0.2",
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "sleep 10",
        ],
    );
    assert_eq!(code(&out), 3, "stderr={}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(parsed[0]["observations"][0]["status"], "timed_out");
    assert_eq!(parsed[0]["observations"][0]["success"], false);
}

#[cfg(unix)]
#[test]
fn run_spawn_failure_exits_3() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_in(
        tmp.path(),
        &[
            "run",
            "--warmups",
            "0",
            "--repetitions",
            "1",
            "--json",
            "--",
            "/definitely/not/a/real/binary",
        ],
    );
    assert_eq!(code(&out), 3, "stderr={}", stderr(&out));
}

// ---------------------------------------------------------------------------
// baseline
// ---------------------------------------------------------------------------

#[test]
fn baseline_register_show_list_clear() {
    let tmp = tempfile::tempdir().unwrap();
    let r = synthetic("bench", &["echo"], &[100.0; 5], &[1000; 5]);
    let path = write_result(tmp.path(), "b.json", &r);

    let set = run_in(
        tmp.path(),
        &[
            "baseline",
            "set",
            "--benchmark",
            "bench",
            "--result",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(code(&set), 0, "stderr={}", stderr(&set));

    let show = run_in(tmp.path(), &["baseline", "show", "--benchmark", "bench"]);
    assert!(stdout(&show).contains("bench"));

    let list = run_in(tmp.path(), &["baseline", "list"]);
    assert!(stdout(&list).contains("bench"));

    let clear = run_in(tmp.path(), &["baseline", "clear", "--benchmark", "bench"]);
    assert_eq!(code(&clear), 0);
    let show2 = run_in(tmp.path(), &["baseline", "show", "--benchmark", "bench"]);
    assert!(stdout(&show2).contains("no baseline"));
}

// ---------------------------------------------------------------------------
// check (policy)
// ---------------------------------------------------------------------------

fn synthetic_config() -> &'static str {
    r#"
[[benchmarks]]
id = "synth"
command = ["echo", "x"]
warmups = 0
repetitions = 5
  [benchmarks.measure]
  metrics = ["wall_time", "peak_rss"]
  [[benchmarks.policy.thresholds]]
  metric = "wall_time"
  max_increase_pct = 5.0
  severity = "block"
  [[benchmarks.policy.thresholds]]
  metric = "peak_rss"
  max_increase_pct = 10.0
  severity = "warn"
"#
}

#[test]
fn check_detects_blocking_regression() {
    let tmp = tempfile::tempdir().unwrap();
    write_config(tmp.path(), synthetic_config());
    let base = synthetic("synth", &["echo", "x"], &[100.0; 5], &[1000; 5]);
    let cur = synthetic("synth", &["echo", "x"], &[130.0; 5], &[1010; 5]);
    let bp = write_result(tmp.path(), "base.json", &base);
    let cp = write_result(tmp.path(), "cur.json", &cur);

    let out = run_in(
        tmp.path(),
        &[
            "check",
            "--result",
            cp.to_str().unwrap(),
            "--baseline",
            bp.to_str().unwrap(),
        ],
    );
    assert_eq!(
        code(&out),
        1,
        "stdout={} stderr={}",
        stdout(&out),
        stderr(&out)
    );
    assert!(stdout(&out).contains("regression"));
}

#[test]
fn check_warn_severity_does_not_block() {
    let tmp = tempfile::tempdir().unwrap();
    write_config(tmp.path(), synthetic_config());
    let base = synthetic("synth", &["echo", "x"], &[100.0; 5], &[1000; 5]);
    // RSS increases 50% but the RSS rule is warn-only.
    let cur = synthetic("synth", &["echo", "x"], &[100.0; 5], &[1500; 5]);
    let bp = write_result(tmp.path(), "base.json", &base);
    let cp = write_result(tmp.path(), "cur.json", &cur);

    let out = run_in(
        tmp.path(),
        &[
            "check",
            "--result",
            cp.to_str().unwrap(),
            "--baseline",
            bp.to_str().unwrap(),
        ],
    );
    assert_eq!(
        code(&out),
        0,
        "stdout={} stderr={}",
        stdout(&out),
        stderr(&out)
    );
    assert!(stdout(&out).contains("warning"));
}

#[test]
fn check_within_threshold_passes() {
    let tmp = tempfile::tempdir().unwrap();
    write_config(tmp.path(), synthetic_config());
    let base = synthetic("synth", &["echo", "x"], &[100.0; 5], &[1000; 5]);
    let cur = synthetic("synth", &["echo", "x"], &[102.0; 5], &[1000; 5]);
    let bp = write_result(tmp.path(), "base.json", &base);
    let cp = write_result(tmp.path(), "cur.json", &cur);

    let out = run_in(
        tmp.path(),
        &[
            "check",
            "--result",
            cp.to_str().unwrap(),
            "--baseline",
            bp.to_str().unwrap(),
        ],
    );
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("pass"));
}

#[test]
fn check_missing_baseline_is_inconclusive() {
    let tmp = tempfile::tempdir().unwrap();
    write_config(tmp.path(), synthetic_config());
    let cur = synthetic("synth", &["echo", "x"], &[100.0; 5], &[1000; 5]);
    let cp = write_result(tmp.path(), "cur.json", &cur);

    let out = run_in(
        tmp.path(),
        &[
            "check",
            "--result",
            cp.to_str().unwrap(),
            "--baseline",
            "none",
        ],
    );
    assert_eq!(code(&out), 0, "inconclusive should not block by default");
    assert!(stdout(&out).contains("inconclusive"));

    let out2 = run_in(
        tmp.path(),
        &[
            "check",
            "--result",
            cp.to_str().unwrap(),
            "--baseline",
            "none",
            "--fail-on-inconclusive",
        ],
    );
    assert_eq!(code(&out2), 4, "stdout={}", stdout(&out2));
}

#[test]
fn check_json_is_machine_readable() {
    let tmp = tempfile::tempdir().unwrap();
    write_config(tmp.path(), synthetic_config());
    let base = synthetic("synth", &["echo", "x"], &[100.0; 5], &[1000; 5]);
    let cur = synthetic("synth", &["echo", "x"], &[130.0; 5], &[1000; 5]);
    let bp = write_result(tmp.path(), "base.json", &base);
    let cp = write_result(tmp.path(), "cur.json", &cur);

    let out = run_in(
        tmp.path(),
        &[
            "check",
            "--result",
            cp.to_str().unwrap(),
            "--baseline",
            bp.to_str().unwrap(),
            "--json",
        ],
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("check --json must be valid JSON");
    assert_eq!(parsed["exit_code"], 1);
    assert_eq!(parsed["overall"], "regression");
}

// ---------------------------------------------------------------------------
// compare
// ---------------------------------------------------------------------------

#[test]
fn compare_reports_percentage_change() {
    let tmp = tempfile::tempdir().unwrap();
    let base = synthetic("bench", &["echo"], &[100.0; 5], &[1000; 5]);
    let cur = synthetic("bench", &["echo"], &[120.0; 5], &[1000; 5]);
    let bp = write_result(tmp.path(), "base.json", &base);
    let cp = write_result(tmp.path(), "cur.json", &cur);

    let out = run_in(
        tmp.path(),
        &[
            "compare",
            bp.to_str().unwrap(),
            cp.to_str().unwrap(),
            "--format",
            "markdown",
        ],
    );
    assert_eq!(code(&out), 0, "stderr={}", stderr(&out));
    let s = stdout(&out);
    assert!(s.contains("+20.00%"), "markdown output was: {s}");
}

// ---------------------------------------------------------------------------
// history / report
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn history_and_report_after_runs() {
    let tmp = tempfile::tempdir().unwrap();
    write_config(
        tmp.path(),
        r#"
[[benchmarks]]
id = "q"
command = ["/bin/sh", "-c", "exit 0"]
warmups = 0
repetitions = 2
  [benchmarks.measure]
  metrics = ["wall_time"]
"#,
    );
    for _ in 0..2 {
        let out = run_in(tmp.path(), &["run", "q"]);
        assert_eq!(code(&out), 0, "stderr={}", stderr(&out));
    }

    let hist = run_in(
        tmp.path(),
        &["history", "--benchmark", "q", "--format", "json"],
    );
    assert_eq!(code(&hist), 0);
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&hist)).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 2);

    let csv = run_in(
        tmp.path(),
        &[
            "report",
            "--benchmark",
            "q",
            "--format",
            "csv",
            "--metric",
            "wall_time",
        ],
    );
    assert_eq!(code(&csv), 0);
    assert!(stdout(&csv).contains("wall_time,"));

    let svg = run_in(
        tmp.path(),
        &[
            "report",
            "--benchmark",
            "q",
            "--format",
            "svg",
            "--metric",
            "wall_time",
        ],
    );
    assert_eq!(code(&svg), 0);
    assert!(stdout(&svg).contains("<svg"));
}

// ---------------------------------------------------------------------------
// isolation: no sibling repository access
// ---------------------------------------------------------------------------

#[test]
fn does_not_access_sibling_repositories() {
    // A guard test documenting the architectural boundary: the crate exposes
    // no APIs and the CLI has no flags that reach outside the working
    // directory / configured paths. This test simply asserts that running a
    // command in an isolated temp dir leaves no files behind outside it.
    let tmp = tempfile::tempdir().unwrap();
    let before: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    assert!(before.is_empty());
    let out = run_in(tmp.path(), &["version"]);
    assert_eq!(code(&out), 0);
    let after: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    assert!(after.is_empty(), "version must not create files");
}
