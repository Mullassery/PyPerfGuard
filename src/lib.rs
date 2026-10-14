//! PyPerfGuard: measurable, explainable, reproducible, enforceable
//! performance regression testing.
//!
//! The crate is split into small, independently testable engines described in
//! `docs/architecture.md`. Domain logic does not depend on the CLI or on
//! GitHub, so it can be unit tested in isolation.

pub mod baseline;
pub mod cli;
pub mod commands;
pub mod compare;
pub mod config;
pub mod error;
pub mod git;
pub mod metric;
pub mod policy;
pub mod process;
pub mod report;
pub mod results;
pub mod runner;
pub mod stats;
pub mod store;
pub mod time_util;

pub use error::{Error, Result};
