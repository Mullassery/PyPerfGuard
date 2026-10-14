//! PyPerfGuard command-line entry point.

use clap::Parser;
use tracing_subscriber::EnvFilter;

use pyperfguard::cli::Cli;
use pyperfguard::commands;

fn main() {
    let cli = Cli::parse();
    init_tracing(cli.quiet, cli.verbose);

    let code = match commands::dispatch(cli.command, cli.config) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            e.exit_code()
        }
    };
    std::process::exit(code);
}

/// Initialize diagnostics. All diagnostics go to stderr so that stdout remains
/// clean for machine-readable output.
fn init_tracing(quiet: bool, verbose: bool) {
    let level = if quiet {
        "error"
    } else if verbose {
        "debug"
    } else {
        "info"
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}
