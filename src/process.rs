//! Benchmark process execution.
//!
//! This module owns spawning the child process, enforcing timeouts, capturing
//! exit status and (on Unix) per-child resource usage. It deliberately avoids
//! any shell interpretation: the command is an explicit program and argument
//! vector.
//!
//! # Measurement method
//!
//! On Unix we spawn via `std::process::Command` and then reap the child with
//! `wait4(2)`, which returns a `rusage` struct for that specific child. This
//! yields:
//!
//! * `ru_utime` + `ru_stime` -> CPU time (user + system)
//! * `ru_maxrss` -> peak resident set size
//!
//! `ru_maxrss` is expressed in **bytes on macOS** and in **kilobytes on
//! Linux**. The conversion is applied per platform and documented in the
//! result metadata.
//!
//! # Timeout handling
//!
//! A timed-out run first receives `SIGTERM`. After a short grace period a
//! still-running run receives `SIGKILL`. If the child was placed in its own
//! process group (which we request via `setpgid`), the whole group is
//! signalled so orphaned grandchildren are cleaned up. This is best-effort:
//! processes that detach from the group or double-fork can survive. PyPerfGuard
//! is not a sandbox.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::config::OutputMode;
use crate::error::{Error, Result};

/// Everything needed to execute one benchmark process.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// Program and arguments. `program_args[0]` is the executable.
    pub program_args: Vec<String>,
    /// Working directory.
    pub cwd: Option<PathBuf>,
    /// Environment overrides applied on top of the inherited environment.
    pub env: BTreeMap<String, String>,
    /// Per-run timeout.
    pub timeout: Option<Duration>,
    /// Standard output handling.
    pub stdout: OutputMode,
    /// Standard error handling.
    pub stderr: OutputMode,
    /// Maximum captured bytes per stream.
    pub capture_limit: usize,
}

/// The raw outcome of a single process execution.
#[derive(Debug, Clone)]
pub struct RawRun {
    /// Wall-clock elapsed time in milliseconds.
    pub wall_ms: f64,
    /// User CPU time in milliseconds (`None` when unsupported).
    pub user_ms: Option<f64>,
    /// System CPU time in milliseconds (`None` when unsupported).
    pub sys_ms: Option<f64>,
    /// Peak RSS in bytes (`None` when unsupported).
    pub peak_rss_bytes: Option<u64>,
    /// Exit code when the process exited normally.
    pub exit_code: Option<i32>,
    /// Terminating signal when the process was killed by a signal.
    pub signal: Option<i32>,
    /// Whether the run was terminated because it exceeded the timeout.
    pub timed_out: bool,
    /// Captured stdout, when `OutputMode::Capture` was requested.
    pub stdout: Option<Vec<u8>>,
    /// Captured stderr, when `OutputMode::Capture` was requested.
    pub stderr: Option<Vec<u8>>,
}

impl RawRun {
    /// A run counts as successful only when it exited with status 0 and was
    /// not terminated by the timeout. Failed runs are never treated as valid
    /// observations.
    pub fn success(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }

    /// Total CPU time in milliseconds, when both components are known.
    pub fn cpu_ms(&self) -> Option<f64> {
        match (self.user_ms, self.sys_ms) {
            (Some(u), Some(s)) => Some(u + s),
            (Some(u), None) => Some(u),
            (None, Some(s)) => Some(s),
            (None, None) => None,
        }
    }
}

/// Decode a `wait`-style status word into `(exit_code, signal)`.
///
/// Implemented without libc macros so behavior is identical across platforms
/// and does not trip `unused_unsafe` lints.
fn decode_status(status: i32) -> (Option<i32>, Option<i32>) {
    let low = status & 0x7f;
    if low == 0x7f {
        // Stopped/continued; not a termination. Treat as no result.
        (None, None)
    } else if low == 0 {
        (Some((status >> 8) & 0xff), None)
    } else {
        (None, Some(low))
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    fn stdio_for(mode: OutputMode) -> Stdio {
        match mode {
            OutputMode::Null => Stdio::null(),
            OutputMode::Inherit => Stdio::inherit(),
            OutputMode::Capture => Stdio::piped(),
        }
    }

    fn read_limited<R: Read + Send + 'static>(
        mut r: R,
        cap: usize,
    ) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                match r.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        // Keep reading past the cap to avoid blocking the child
                        // on a full pipe, but only retain up to `cap` bytes.
                        if out.len() < cap {
                            let remaining = cap - out.len();
                            let take = remaining.min(n);
                            out.extend_from_slice(&buf[..take]);
                        }
                    }
                    Err(_) => break,
                }
            }
            out
        })
    }

    fn timeval_ms(tv: libc::timeval) -> f64 {
        (tv.tv_sec as f64) * 1000.0 + (tv.tv_usec as f64) / 1000.0
    }

    #[cfg(target_os = "macos")]
    fn maxrss_to_bytes(maxrss: libc::c_long) -> Option<u64> {
        // macOS reports ru_maxrss in bytes.
        if maxrss > 0 {
            Some(maxrss as u64)
        } else {
            None
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn maxrss_to_bytes(maxrss: libc::c_long) -> Option<u64> {
        // Linux and most other Unixes report ru_maxrss in kilobytes.
        if maxrss > 0 {
            Some(maxrss as u64 * 1024)
        } else {
            None
        }
    }

    /// Signal a child (and its process group, if we created one).
    fn signal_child(pid: libc::pid_t, sig: libc::c_int) {
        unsafe {
            // Negative pid signals the process group led by `pid`.
            if libc::kill(-pid, sig) != 0 {
                libc::kill(pid, sig);
            }
        }
    }

    pub fn execute(spec: &CommandSpec) -> Result<RawRun> {
        if spec.program_args.is_empty() {
            return Err(Error::Usage("command is empty".into()));
        }
        let program = &spec.program_args[0];
        let mut cmd = Command::new(program);
        cmd.args(&spec.program_args[1..]);
        cmd.stdin(Stdio::null());
        cmd.stdout(stdio_for(spec.stdout));
        cmd.stderr(stdio_for(spec.stderr));
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        // Place the child in its own process group so we can terminate any
        // descendants it spawns. `setpgid` is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let start = Instant::now();
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Execution(format!("failed to spawn '{program}': {e}")))?;
        let pid = child.id() as libc::pid_t;

        let stdout_handle = match spec.stdout {
            OutputMode::Capture => child
                .stdout
                .take()
                .map(|h| read_limited(h, spec.capture_limit)),
            _ => None,
        };
        let stderr_handle = match spec.stderr {
            OutputMode::Capture => child
                .stderr
                .take()
                .map(|h| read_limited(h, spec.capture_limit)),
            _ => None,
        };

        // Drop the Child handle: `Child` does not reap on drop, and we reap
        // explicitly below. The null/inherited stdio handles are closed here,
        // which is fine because we are not using them. Captured pipes were
        // moved into reader threads above.
        drop(child);

        let grace = Duration::from_millis(500);
        let poll = Duration::from_millis(1);

        let mut status: libc::c_int = 0;
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        let mut timed_out = false;
        let mut reaped = false;

        loop {
            let ret = unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut ru) };
            if ret == pid {
                reaped = true;
                break;
            } else if ret == 0 {
                if let Some(t) = spec.timeout {
                    if start.elapsed() >= t {
                        // Graceful, then forceful.
                        signal_child(pid, libc::SIGTERM);
                        let grace_start = Instant::now();
                        loop {
                            let r2 =
                                unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut ru) };
                            if r2 == pid {
                                reaped = true;
                                break;
                            }
                            if grace_start.elapsed() >= grace {
                                signal_child(pid, libc::SIGKILL);
                                // Block until it is reaped.
                                loop {
                                    let r3 = unsafe { libc::wait4(pid, &mut status, 0, &mut ru) };
                                    if r3 == pid {
                                        reaped = true;
                                        break;
                                    }
                                    if r3 == -1 {
                                        break;
                                    }
                                }
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        timed_out = true;
                        break;
                    }
                }
                std::thread::sleep(poll);
            } else {
                // ret == -1: the child could not be waited for.
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::ECHILD) {
                    // Already reaped elsewhere; treat as failure.
                    return Err(Error::Execution(format!(
                        "child process {pid} could not be reaped (ECHILD)"
                    )));
                }
                return Err(Error::io("wait4", err));
            }
        }

        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;

        let stdout = stdout_handle.map(|h| h.join().unwrap_or_default());
        let stderr = stderr_handle.map(|h| h.join().unwrap_or_default());

        if !reaped {
            return Err(Error::Execution(
                "child process finished without a wait status".into(),
            ));
        }

        let (exit_code, signal) = decode_status(status);
        Ok(RawRun {
            wall_ms,
            user_ms: Some(timeval_ms(ru.ru_utime)),
            sys_ms: Some(timeval_ms(ru.ru_stime)),
            peak_rss_bytes: maxrss_to_bytes(ru.ru_maxrss),
            exit_code,
            signal,
            timed_out,
            stdout,
            stderr,
        })
    }
}

#[cfg(not(unix))]
mod imp {
    use super::*;
    use std::process::{Command, Stdio};

    fn stdio_for(mode: OutputMode) -> Stdio {
        match mode {
            OutputMode::Null => Stdio::null(),
            OutputMode::Inherit => Stdio::inherit(),
            OutputMode::Capture => Stdio::piped(),
        }
    }

    /// Portable fallback: measures wall time and exit status only. Peak RSS
    /// and CPU time are reported as unavailable on non-Unix platforms.
    pub fn execute(spec: &CommandSpec) -> Result<RawRun> {
        if spec.program_args.is_empty() {
            return Err(Error::Usage("command is empty".into()));
        }
        let program = &spec.program_args[0];
        let mut cmd = Command::new(program);
        cmd.args(&spec.program_args[1..]);
        cmd.stdin(Stdio::null());
        cmd.stdout(stdio_for(spec.stdout));
        cmd.stderr(stdio_for(spec.stderr));
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        let start = Instant::now();
        let output = cmd
            .output()
            .map_err(|e| Error::Execution(format!("failed to spawn '{program}': {e}")))?;
        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
        Ok(RawRun {
            wall_ms,
            user_ms: None,
            sys_ms: None,
            peak_rss_bytes: None,
            exit_code: output.status.code(),
            signal: None,
            timed_out: false,
            stdout: if spec.stdout == OutputMode::Capture {
                Some(output.stdout)
            } else {
                None
            },
            stderr: if spec.stderr == OutputMode::Capture {
                Some(output.stderr)
            } else {
                None
            },
        })
    }
}

pub use imp::execute;

/// Whether this build can collect per-process CPU time and peak RSS.
pub fn supports_process_rusage() -> bool {
    cfg!(unix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(program_args: &[&str]) -> CommandSpec {
        CommandSpec {
            program_args: program_args.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            env: BTreeMap::new(),
            timeout: None,
            stdout: OutputMode::Null,
            stderr: OutputMode::Null,
            capture_limit: 4096,
        }
    }

    #[test]
    fn decode_status_encodes_exit_and_signal() {
        assert_eq!(decode_status(0), (Some(0), None));
        assert_eq!(decode_status(1 << 8), (Some(1), None));
        assert_eq!(decode_status(9), (None, Some(9)));
    }

    #[cfg(unix)]
    #[test]
    fn runs_true_like_command() {
        let run = execute(&spec(&["/bin/sh", "-c", "exit 0"])).unwrap();
        assert!(run.success());
        assert_eq!(run.exit_code, Some(0));
        assert!(run.wall_ms >= 0.0);
        assert!(run.cpu_ms().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn captures_nonzero_exit() {
        let run = execute(&spec(&["/bin/sh", "-c", "exit 7"])).unwrap();
        assert!(!run.success());
        assert_eq!(run.exit_code, Some(7));
    }

    #[cfg(unix)]
    #[test]
    fn spawn_failure_is_execution_error() {
        let err = execute(&spec(&["/nonexistent/definitely-not-here"])).unwrap_err();
        assert!(matches!(err, Error::Execution(_)));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_long_run() {
        let mut s = spec(&["/bin/sh", "-c", "sleep 30"]);
        s.timeout = Some(Duration::from_millis(200));
        let run = execute(&s).unwrap();
        assert!(run.timed_out);
        assert!(!run.success());
        assert!(run.wall_ms < 5000.0, "wall_ms={}", run.wall_ms);
    }

    #[cfg(unix)]
    #[test]
    fn captures_stdout() {
        let mut s = spec(&["/bin/sh", "-c", "printf hello"]);
        s.stdout = OutputMode::Capture;
        let run = execute(&s).unwrap();
        assert_eq!(run.stdout.as_deref(), Some(b"hello".as_slice()));
    }

    #[cfg(unix)]
    #[test]
    fn capture_limit_bounds_output() {
        let mut s = spec(&["/bin/sh", "-c", "yes | head -n 100000"]);
        s.stdout = OutputMode::Capture;
        s.capture_limit = 128;
        let run = execute(&s).unwrap();
        let out = run.stdout.unwrap();
        assert!(out.len() <= 128, "len={}", out.len());
    }
}
