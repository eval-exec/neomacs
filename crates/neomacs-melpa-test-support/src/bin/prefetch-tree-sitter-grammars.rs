//! Explicit Tree-sitter grammar preparation for `cargo xtask treesit-grammars`.
//!
//! This is the only place a grammar fixture touches the network: it fetches
//! each pinned revision, verifies the `parser.c` SHA-256 against the pin,
//! and builds the shared library through GNU Emacs's native installer. Tests
//! read the result through the cache-only reader and fail loudly when this
//! step has not run.

use neomacs_melpa_test_support::{
    EXTRA_TREE_SITTER_GRAMMAR_PINS, EmacsRuntime, GNU31_TREE_SITTER_GRAMMARS,
};
use std::process::ExitCode;

fn main() -> ExitCode {
    let gnu = EmacsRuntime::gnu_emacs();
    let mut failures = 0;
    for case in GNU31_TREE_SITTER_GRAMMARS {
        let language = case.language;
        match case.prefetch(&gnu) {
            Ok(directory) => println!(
                "verified {language} revision {} sha256={} at {}",
                case.revision,
                case.source_sha256,
                directory.display()
            ),
            Err(error) => {
                failures += 1;
                eprintln!("GRAMMAR-PREFETCH: {language} error={error}");
            }
        }
    }
    for pin in EXTRA_TREE_SITTER_GRAMMAR_PINS {
        let language = pin.language;
        match pin.prefetch(&gnu) {
            Ok(directory) => println!(
                "verified {language} revision {} sha256={} at {}",
                pin.revision,
                pin.source_sha256,
                directory.display()
            ),
            Err(error) => {
                failures += 1;
                eprintln!("GRAMMAR-PREFETCH: {language} error={error}");
            }
        }
    }
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        eprintln!("{failures} grammar prefetches failed");
        ExitCode::FAILURE
    }
}
