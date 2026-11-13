//! `cargo xtask oracle-preflight`: refuse to run an oracle suite against a
//! binary whose Lisp tree the same fresh build did not produce.
//!
//! The check is [`neomacs_test_env::fresh_build_stamp::verify`], always with a
//! full content verification (no nextest-run cache: the recipe calls this once
//! before the suite).  It refuses when
//!
//! * the stamp beside the binary is missing or unreadable,
//! * the binary's SHA-256 differs from the stamp's,
//! * the generated-Lisp manifest differs (a sync, deletion or regeneration
//!   changed the tree after the build), or
//! * a preloaded `.el` named by `loadup.el` has no `.elc`.
//!
//! `NEOVM_SKIP_FRESH_BUILD_STAMP=<non-empty>` overrides the check; its value is
//! echoed so the run log records that the runtime is unattested.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::Result;

const USAGE: &str = "usage: cargo xtask oracle-preflight --binary <path> [--lisp-root <path>]";

pub fn run(repo_root: &Path, args: impl Iterator<Item = OsString>) -> Result<()> {
    let mut binary: Option<PathBuf> = None;
    let mut lisp_root: Option<PathBuf> = None;
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--binary") => {
                binary = Some(PathBuf::from(args.next().ok_or_else(|| {
                    format!("oracle-preflight: --binary needs a path\n{USAGE}")
                })?));
            }
            Some("--lisp-root") => {
                lisp_root = Some(PathBuf::from(args.next().ok_or_else(|| {
                    format!("oracle-preflight: --lisp-root needs a path\n{USAGE}")
                })?));
            }
            Some("--help" | "-h") => {
                println!("{USAGE}");
                return Ok(());
            }
            other => {
                return Err(
                    format!("oracle-preflight: unexpected argument {other:?}\n{USAGE}").into(),
                );
            }
        }
    }
    let binary =
        binary.ok_or_else(|| format!("oracle-preflight: --binary <path> is required\n{USAGE}"))?;
    let lisp_root = lisp_root.unwrap_or_else(|| repo_root.join("lisp"));

    match neomacs_test_env::fresh_build_stamp::verify(&binary, &lisp_root) {
        Ok(verdict) => {
            use neomacs_test_env::fresh_build_stamp::StampVerdict;
            match verdict {
                StampVerdict::Verified {
                    binary_sha256,
                    lisp_files,
                } => println!(
                    "oracle-preflight: OK: {} matches its fresh-build stamp (lisp manifest: {} files, binary sha256 {})",
                    binary.display(),
                    lisp_files,
                    binary_sha256
                ),
                StampVerdict::Overridden { variable, value } => println!(
                    "oracle-preflight: OVERRIDDEN by {variable}={value}: the runtime is NOT attested"
                ),
            }
            Ok(())
        }
        Err(message) => Err(format!("oracle-preflight: REFUSED: {message}").into()),
    }
}
