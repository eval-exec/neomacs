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
//! A boundary substitution was tested and refuted on 2026-10-31: a local
//! `mod std` in the *including* crate does not capture `std::` paths inside a
//! `#[path]`-included module (they resolve to the extern prelude; compile
//! test E0425). The capture works only when the paste target and the shim
//! share the module, which is why the model uses `include!` of this stripped
//! copy with a sibling `mod std` shim (tests/ledger.rs).
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let source = Path::new("../../src/tagged/gc/symbol_table/publication.rs");
    println!("cargo:rerun-if-changed={}", source.display());
    let original = fs::read_to_string(source).expect("read the real publication.rs");
    let stripped = strip_leading_inner_docs(&original);
    let stripped = strip_trailing_test_mod(&stripped);
    if !stripped.starts_with("use super::{") {
        panic!(
            "the stripped copy must begin with the super import block; \
             publication.rs changed its header - re-anchor this seam"
        );
    }
    // Seam contract: the ledger imports its synchronization directly from
    // std, so the loom shim (tests/ledger.rs `mod std`) is the seam. When
    // P9.1b lands the in-file `super::sync` seam, delete this substitution
    // and bind the model to the file's own imports.
    assert!(
        !stripped.contains("use super::sync::"),
        "publication.rs now carries the in-file sync seam: drop this \
         build-time seam and include the file's own imports"
    );
    for expected in [
        "use std::sync::atomic::{AtomicPtr, Ordering};",
        "use std::sync::{Mutex, MutexGuard, TryLockError};",
    ] {
        assert!(
            stripped.contains(expected),
            "publication.rs no longer imports `{expected}`: the loom seam \
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
