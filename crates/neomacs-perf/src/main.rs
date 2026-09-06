//! `cargo run -p neomacs-perf -- …` --- the performance harness CLI.
//!
//! The harness is a library with a CLI in front of it, and xtask launches this
//! binary rather than linking the library. xtask is a command launcher: linking
//! the harness put its dependency tree into every `cargo xtask fresh-build`,
//! including the release jobs, for a command they never run.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Same root xtask resolves: the workspace directory, overridden under
    // nextest so a test run addresses the tree it was built from.
    let workspace_root = std::env::var_os("NEXTEST_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_WORKSPACE_DIR")));

    match neomacs_perf::run_cli(workspace_root, std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
