//! Error type and documented process exit codes for PyPerfGuard.
//!
//! Exit codes are part of the CLI contract and are asserted by integration
//! tests. They are intentionally stable so CI systems can rely on them.

use std::fmt;

/// Documented process exit codes.
///
/// These values are the public contract used by CI workflows.
pub mod exit {
    /// All requested checks passed.
    pub const SUCCESS: i32 = 0;
    /// A regression was detected by a blocking policy.
    pub const REGRESSION: i32 = 1;
    /// Invalid configuration, malformed input, or usage error.
    pub const CONFIG: i32 = 2;
    /// Benchmark execution failed (non-zero exit, timeout, spawn failure).
    pub const EXECUTION: i32 = 3;
    /// A comparison was inconclusive and the policy treats that as blocking.
    pub const INCONCLUSIVE: i32 = 4;
    /// Unexpected internal error.
    pub const INTERNAL: i32 = 5;
}

/// The unified error type for the library and CLI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Configuration could not be parsed, loaded, or validated.
    #[error("configuration error: {0}")]
    Config(String),

    /// The caller used an invalid combination of arguments.
    #[error("usage error: {0}")]
    Usage(String),

    /// An I/O operation failed.
    #[error("io error: {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// Serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// A benchmark process failed to run or produced an invalid result.
    #[error("benchmark execution error: {0}")]
    Execution(String),

    /// A requested resource (file, benchmark, baseline) does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// A bug or unexpected condition.
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// The exit code this error maps to.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Config(_) | Error::Usage(_) => exit::CONFIG,
            Error::Execution(_) | Error::NotFound(_) => exit::EXECUTION,
            Error::Io { .. } => exit::INTERNAL,
            Error::Serialization(_) => exit::CONFIG,
            Error::Internal(_) => exit::INTERNAL,
        }
    }

    /// Build an [`Error::Io`] with context.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Error::Io {
            context: context.into(),
            source,
        }
    }
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Marker used by the CLI to carry an intended exit code without an error
/// message (for example, a policy regression which is a normal outcome).
#[derive(Debug, Clone)]
pub struct ExitCode(pub i32);

impl fmt::Display for ExitCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit code {}", self.0)
    }
}

impl std::error::Error for ExitCode {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_map_to_documented_exit_codes() {
        assert_eq!(Error::Config("x".into()).exit_code(), exit::CONFIG);
        assert_eq!(Error::Usage("x".into()).exit_code(), exit::CONFIG);
        assert_eq!(Error::Execution("x".into()).exit_code(), exit::EXECUTION);
        assert_eq!(Error::NotFound("x".into()).exit_code(), exit::EXECUTION);
        assert_eq!(Error::Internal("x".into()).exit_code(), exit::INTERNAL);
        assert_eq!(Error::Serialization("x".into()).exit_code(), exit::CONFIG);
    }
}
