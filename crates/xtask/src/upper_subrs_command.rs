//! Explicit regeneration of the H3 unit-runtime upper-subr source.
//!
//! The generator is source-only and lives in neovm-eval's `build_support` so
//! that eval's cfg(test) drift test can render the exact same checked-in
//! artifact. `cargo xtask upper-subrs` writes
//! `crates/neovm-eval/src/upper_subrs.generated.rs`; `--check` fails when the
//! tracked file is stale instead.

use std::ffi::OsString;
use std::path::Path;

#[path = "../../neovm-eval/build_support/upper_subrs_generated.rs"]
mod generator;

pub(super) fn run(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> super::Result<()> {
    let mut check = false;
    for arg in args {
        match arg.to_str() {
            Some("--check") if !check => check = true,
            other => {
                return Err(format!(
                    "upper-subrs accepts only --check; unexpected argument `{}`",
                    other.unwrap_or("<non-utf8>")
                )
                .into());
            }
        }
    }
    generator::generate(repo_root, check).map_err(|error| format!("upper-subrs: {error}"))?;
    println!(
        "{} {}",
        if check { "verified" } else { "wrote" },
        generator::GENERATED
    );
    Ok(())
}
