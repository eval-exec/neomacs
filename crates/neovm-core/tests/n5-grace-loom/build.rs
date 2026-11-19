//! Model-side seam for the N3 adapter and the generic publication owner.
//!
//! The adapter (`symbol_table/publication.rs`) and the generic owner
//! (`gc/publication/`) are consumed read-only. Two properties are
//! include-hostile: a leading `//!` block (inner attributes cannot appear
//! in an `include!` expansion, E0753) and a trailing `#[cfg(test)]` test
//! module — a consumer test crate compiles with `--test`, which sets
//! `cfg(test)`, so that module would activate and point at tests the
//! consumer does not have (E0583). This script strips exactly those regions
//! into `OUT_DIR`.
//!
//! The adapter's `super::sync` import is the parent seam. The owner's
//! `super::sync` import is the `sync` module nested under
//! `crate::tagged::gc::publication`, which each target defines beside the
//! included core. The owner keeps the atomic root and the metadata mutex.
//! The adapter's `PublishedSymbolTables` body must name that owner and must
//! not carry its own `AtomicPtr`, `Mutex`, or `Vec`.
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let adapter = Path::new("../../../neovm-value/src/tagged/gc/symbol_table/publication.rs");
    let types = Path::new("../../../neovm-value/src/tagged/gc/publication/mod.rs");
    let owner = Path::new("../../../neovm-value/src/tagged/gc/publication/ledger.rs");
    for source in [adapter, types, owner] {
        println!("cargo:rerun-if-changed={}", source.display());
    }
    let out_dir = Path::new(&env::var("OUT_DIR").expect("OUT_DIR set by cargo")).to_path_buf();

    let original = fs::read_to_string(adapter).expect("read the real publication.rs");
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
    assert!(
        !stripped.contains("Mutex"),
        "publication.rs names Mutex: the metadata lock belongs to the generic \
         owner, not the N3 adapter"
    );
    assert!(
        !stripped.contains("Vec<"),
        "publication.rs owns a Vec: predecessors belong to the generic owner"
    );
    let body = owner_body(&stripped);
    assert!(
        body.contains("PublicationLedger"),
        "PublishedSymbolTables must wrap PublicationLedger; re-anchor this seam"
    );
    for forbidden in ["AtomicPtr", "Mutex", "Vec"] {
        assert!(
            !body.contains(forbidden),
            "PublishedSymbolTables owns `{forbidden}`: the adapter must not \
             keep its own root, lock, or predecessor vector"
        );
    }
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
    fs::write(out_dir.join("publication.inc"), &stripped).expect("write the stripped adapter");

    let types = fs::read_to_string(types).expect("read publication/mod.rs");
    let types = strip_type_module(&types);
    fs::write(out_dir.join("publication_types.inc"), types).expect("write the stripped types");

    let owner = fs::read_to_string(owner).expect("read publication/ledger.rs");
    let owner = strip_owner_tests(&owner);
    assert!(
        owner.contains("fn commit_replacement"),
        "the generic owner no longer has commit_replacement; re-anchor this seam"
    );
    fs::write(out_dir.join("publication_ledger.inc"), owner).expect("write the stripped owner");
}

/// `PublishedSymbolTables`'s brace body, not the borrowed view beside it.
fn owner_body(source: &str) -> &str {
    let start = source
        .find("pub(crate) struct PublishedSymbolTables {")
        .unwrap_or_else(|| {
            panic!("PublishedSymbolTables moved; re-anchor the owner-body check")
        });
    let rest = &source[start..];
    let open = rest.find('{').unwrap_or_else(|| {
        panic!("PublishedSymbolTables has no body; re-anchor the owner-body check")
    });
    let close = rest[open..].find('}').unwrap_or_else(|| {
        panic!("PublishedSymbolTables body is unclosed; re-anchor the owner-body check")
    });
    &rest[open..=open + close]
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

/// Types module for `include!` inside `crate::tagged::gc::publication`.
/// Drop the inner docs, the child-module declarations, and the re-export:
/// the model defines `sync` and `ledger` itself and re-exports the owner.
fn strip_type_module(original: &str) -> String {
    let stripped = strip_leading_inner_docs(original);
    let mut out = String::new();
    let mut skipping_reexport = false;
    for line in stripped.lines() {
        let trimmed = line.trim();
        if trimmed == "mod ledger;" || trimmed == "mod sync;" {
            continue;
        }
        if trimmed == "#[allow(unused_imports)]" {
            skipping_reexport = true;
            continue;
        }
        if skipping_reexport {
            if trimmed.ends_with(';') {
                skipping_reexport = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if skipping_reexport {
        panic!("publication/mod.rs re-export did not end; re-anchor this seam");
    }
    out
}

/// Owner body for `include!` inside `mod ledger`. Keep the `cfg(test)`
/// release witness. Drop only the unit-test module, which names helpers
/// the model crate does not have.
fn strip_owner_tests(original: &str) -> String {
    let stripped = strip_leading_inner_docs(original);
    const MARKER: &str = "\n#[cfg(test)]\nmod tests {";
    let index = stripped.rfind(MARKER).unwrap_or_else(|| {
        panic!(
            "publication/ledger.rs must keep its `#[cfg(test)] mod tests` \
             block; re-anchor this seam"
        )
    });
    format!("{}\n", stripped[..index].trim_end())
}
