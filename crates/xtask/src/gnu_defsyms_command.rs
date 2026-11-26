//! Regeneration of the GNU DEFSYM boot-membership table.
//!
//! The generator lives in the plain `neovm-gnu-ledgers` library so that the
//! checked-in ledger can be re-extracted, rendered and drift-checked with one
//! implementation shared by xtask and the core architecture guard.
//! `cargo xtask gnu-defsyms --source <GNU checkout>`
//! rewrites `gnu_defsyms.snapshot.tsv` and `gnu_defsyms.generated.rs`;
//! `--check` fails when the tracked generated file no longer matches the
//! ledger, and (with `--source`, or `GNU_EMACS_SOURCE` set) when the ledger no
//! longer matches the pinned GNU source.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use neovm_gnu_ledgers as generator;

pub(super) fn run(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> super::Result<()> {
    let mut check = false;
    let mut source: Option<PathBuf> = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--check") if !check => check = true,
            Some("--source") => {
                let path = args.next().ok_or("gnu-defsyms: --source needs a path")?;
                source = Some(PathBuf::from(path));
            }
            Some(text) if text.starts_with("--source=") => {
                source = Some(PathBuf::from(&text["--source=".len()..]));
            }
            other => {
                return Err(format!(
                    "gnu-defsyms accepts only --check and --source <path>; unexpected argument `{}`",
                    other.unwrap_or("<non-utf8>")
                )
                .into());
            }
        }
    }
    let source = source.or_else(|| std::env::var_os("GNU_EMACS_SOURCE").map(PathBuf::from));
    generator::generate(repo_root, check, source.as_deref())
        .map_err(|error| format!("gnu-defsyms: {error}"))?;
    println!(
        "{} {} and {}",
        if check { "verified" } else { "wrote" },
        generator::LEDGER,
        generator::GENERATED
    );
    Ok(())
}
