//! Model-side seam for the L6 rendezvous control/ring.
//!
//! `rendezvous/control.rs` is consumed read-only. Two regions are
//! include-hostile: a leading `//!` block (inner attributes cannot appear in an
//! `include!` expansion, E0753) and a trailing `#[cfg(test)]` module (the model
//! compiles with `--test`, so `cfg(test)` is set and the module would point at
//! tests the model does not have). The copy also re-anchors crate visibility
//! and the product `super::super::MutatorKey` import; the model supplies
//! `MutatorKey` and `RendezvousError` at its crate root.
//!
//! Seam contract: the copied control/ring must not name `std::sync`, `crate::`
//! or `super::super`. The ring control uses no synchronization; the model
//! drives the product state under the test's own Loom Mutex/Condvar. The
//! product park/wake (`gate.rs`/`mod.rs`) is copied in L6.7, not here.
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let source =
        Path::new("../../../neovm-value/src/tagged/gc/mutator_registration/rendezvous/control.rs");
    println!("cargo:rerun-if-changed={}", source.display());
    let original = fs::read_to_string(source).expect("read the real control.rs");
    let stripped = strip_leading_inner_docs(&original);
    let stripped = strip_trailing_test_mod(&stripped);
    let stripped = stripped.replace("pub(in crate::tagged::gc)", "pub(crate)");
    let stripped = stripped.replace("use super::super::MutatorKey;", "use super::MutatorKey;");

    assert!(
        !stripped.contains("std::sync"),
        "control.rs names std::sync directly: keep synchronization on the rendezvous sync seam"
    );
    assert!(
        !stripped.contains("crate::"),
        "control.rs names crate::: the model copy must stand alone"
    );
    assert!(
        !stripped.contains("super::super"),
        "control.rs kept a super::super hop: re-anchor this seam"
    );
    for expected in [
        "pub(crate) struct State",
        "fn enqueue_serial",
        "fn try_grant_head",
        "fn retire_serial",
    ] {
        assert!(
            stripped.contains(expected),
            "control.rs lost `{expected}`: re-anchor the L6 ring seam"
        );
    }

    let out_dir = Path::new(&env::var("OUT_DIR").expect("OUT_DIR set by cargo")).to_path_buf();
    fs::write(out_dir.join("control.rs"), stripped).expect("write the model copy");
}

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

fn strip_trailing_test_mod(stripped: &str) -> String {
    const DECLARATION: &str = "#[cfg(test)]\n#[path = \"control_tests.rs\"]\nmod tests;";
    let trimmed = stripped.trim_end_matches('\n');
    let cut = trimmed.strip_suffix(DECLARATION).unwrap_or_else(|| {
        panic!(
            "control.rs must end with its cfg(test) control_tests module; \
             it moved or grew - re-anchor this seam"
        )
    });
    format!("{cut}\n")
}
