//! Model-side seam for the real N3 ledger (N5 design rev 2 section 2.2).
//!
//! The real `publication.rs` is consumed byte-for-byte, read-only. Two of
//! its properties are include-hostile: the leading `//!` block (inner
//! attributes cannot appear in an `include!` expansion, E0753) and the
//! trailing `#[cfg(test)] mod tests;` — a consumer test crate compiles with
//! `--test`, which sets `cfg(test)`, so that declaration would activate and
//! point at a module the consumer does not have (E0583). This script strips
//! exactly those two regions (nothing else) into `OUT_DIR` and asserts the
//! seam contract on the imports and the stripped tail, so an edit that
//! changes how `publication.rs` obtains its synchronization or organizes
//! its unit tests fails the build with a re-anchor message instead of
//! silently modeling something else. With `cfg(test)` enabled by the
//! harness, the ledger's own test-gated items (the limit override) remain
//! compiled and the override defaults to production limits, unchanged.
//!
//! Synchronization is the parent `sync` module (`use super::sync::`), not a
//! `std` path. The Loom target and the native target each define that module
//! above the include. `include!` remains because a `#[path]` module cannot
//! carry the stripped `//!` header (E0753) or the consumer's `cfg(test)`
//! test-module declaration (E0583).
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let source = Path::new("../../../neovm-value/src/tagged/gc/symbol_table/publication.rs");
    println!("cargo:rerun-if-changed={}", source.display());
    let original = fs::read_to_string(source).expect("read the real publication.rs");
    let stripped = strip_leading_inner_docs(&original);
    let stripped = strip_trailing_test_mod(&stripped);
    // rustfmt places `use super::sync::` ahead of the vocabulary `use super::{`.
    if !stripped.starts_with("use super::sync::") {
        panic!(
            "the stripped copy must begin with the super::sync import; \
             publication.rs changed its header - re-anchor this seam"
        );
    }
    assert!(
        stripped.contains("use super::{"),
        "publication.rs dropped its vocabulary import from super; \
         re-anchor this seam"
    );
    // Seam contract: synchronization comes from the parent `sync` module.
    // The Loom target binds that module to Loom; the native target binds it
    // to the production re-export. A direct `std::sync` import would skip
    // the seam and model real atomics.
    assert!(
        !stripped.contains("use std::sync::"),
        "publication.rs imports `std::sync` directly: the grace model would \
         miss those operations. Keep synchronization on `super::sync`"
    );
    // Names the included file reaches outside the sync import.
    // The model supplies them from the stand-in and from real std.
    // Removing one of them is a re-anchor, not a silent drop.
    for expected in [
        "std::iter::once",
        "std::rc::Rc",
        "super::SymbolChunkIndex",
        "crate::intern::NameId",
        "crate::symbol::LispSymbol",
        ".as_slice()",
        "generation.0",
    ] {
        assert!(
            stripped.contains(expected),
            "publication.rs no longer contains `{expected}`: the loom seam \
             must be re-anchored (N5 design rev 2 section 2.2)"
        );
    }
    let out =
        Path::new(&env::var("OUT_DIR").expect("OUT_DIR set by cargo")).join("publication.inc");
    fs::write(&out, stripped).expect("write the stripped ledger copy");
}

/// Remove only the leading `//!` block; every other byte is preserved.
fn strip_leading_inner_docs(original: &str) -> String {
    let mut stripped = String::with_capacity(original.len());
    let mut past_header = false;
    for line in original.lines() {
        if !past_header && line.starts_with("//!") {
            continue;
        }
        past_header = true;
        stripped.push_str(line);
        stripped.push('\n');
    }
    stripped
}

/// Remove exactly the trailing unit-test submodule declaration; every other
/// byte is preserved. The declaration's shape is asserted so an edit to the
/// ledger's test organization re-anchors this seam loudly.
fn strip_trailing_test_mod(stripped: &str) -> String {
    const DECLARATION: &str = "#[cfg(test)]\nmod tests;";
    let trimmed = stripped.trim_end_matches('\n');
    let cut = trimmed.strip_suffix(DECLARATION).unwrap_or_else(|| {
        panic!(
            "publication.rs must end with its `#[cfg(test)] mod tests;` \
             declaration; it moved or grew - re-anchor this seam"
        )
    });
    format!("{cut}\n")
}
