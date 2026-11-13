//! Explicit regeneration of the canonical subr registration plan.
//!
//! The generator is source-only and lives in neovm-eval's `build_support` so
//! that eval's cfg(test) plan-consistency test can parse the exact same
//! checked-in artifacts. `cargo xtask canonical-subr-plan` writes the two
//! TSVs; `--check` fails when they are stale instead.

use std::ffi::OsString;
use std::path::Path;

#[path = "../../neovm-eval/build_support/canonical_subr_plan.rs"]
mod generator;

pub(super) fn run(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> super::Result<()> {
    let mut check = false;
    for arg in args {
        match arg.to_str() {
            Some("--check") if !check => check = true,
            other => {
                return Err(format!(
                    "canonical-subr-plan accepts only --check; unexpected argument `{}`",
                    other.unwrap_or("<non-utf8>")
                )
                .into());
            }
        }
    }
    generator::generate(repo_root, check)
        .map_err(|error| format!("canonical-subr-plan: {error}"))?;
    println!(
        "{} {} and {}",
        if check { "verified" } else { "wrote" },
        generator::NAMES_TSV,
        generator::PLAN_TSV
    );
    Ok(())
}
