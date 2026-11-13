//! Explicit regeneration of the H3/H3b upper-layer registration snapshots.
//!
//! The generator is source-only and lives in neovm-eval's `build_support` so
//! that eval's cfg(test) snapshot-consistency test includes the exact same
//! parser as this command. `cargo xtask upper-snapshots` writes the two
//! checked-in TSVs; `--check` fails when they are stale instead.

use std::ffi::OsString;
use std::path::Path;

#[path = "../../neovm-eval/build_support/upper_snapshots.rs"]
mod generator;

pub(super) fn run(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> super::Result<()> {
    let mut check = false;
    for arg in args {
        match arg.to_str() {
            Some("--check") if !check => check = true,
            other => {
                return Err(format!(
                    "upper-snapshots accepts only --check; unexpected argument `{}`",
                    other.unwrap_or("<non-utf8>")
                )
                .into());
            }
        }
    }
    generator::generate(repo_root, check).map_err(|error| format!("upper-snapshots: {error}"))?;
    println!(
        "{} {}, {} and {}",
        if check { "verified" } else { "wrote" },
        generator::SUBRS_SNAPSHOT,
        generator::BOOTSTRAP_SNAPSHOT,
        generator::BOOTSTRAP_SOURCE
    );
    Ok(())
}
