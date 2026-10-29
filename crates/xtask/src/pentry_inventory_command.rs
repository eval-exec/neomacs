//! Explicit regeneration of the cold parallel-entry inventory.
//!
//! Include the source-only generator rather than depending on neovm-core: this
//! command neither links the Lisp runtime nor adds scanning to its build script.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

#[path = "../../neovm-core/build_support/pentry_inventory.rs"]
mod generator;

pub(super) fn run(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> super::Result<()> {
    let mut args = args.into_iter();
    let raw_output = args
        .next()
        .ok_or("usage: cargo xtask pentry-inventory <OUTPUT-DIR>")?;
    if let Some(unexpected) = args.next() {
        return Err(format!(
            "pentry-inventory accepts exactly one output directory; unexpected argument `{}`",
            unexpected.to_string_lossy()
        )
        .into());
    }
    let output = super::resolve_cli_path(repo_root, raw_output);
    fs::create_dir_all(&output).map_err(|error| format!("create {}: {error}", output.display()))?;
    let manifest = repo_root.join("crates/neovm-core");
    generator::generate(&manifest, repo_root, &output)
        .map_err(|error| format!("pentry-inventory: {error}"))?;

    let generated_rust = output.join("pentry_inventory.rs");
    let generated_json = output.join("pentry-inventory.json");
    let checked_in = manifest.join("src/emacs_core/lisp/native/subr/effect_inventory.inc");
    println!("generated {}", generated_rust.display());
    println!("generated {}", generated_json.display());
    println!(
        "After reviewing the inventory, copy {} to {} and run the catalog architecture test.",
        generated_rust.display(),
        checked_in.display()
    );
    Ok(())
}
