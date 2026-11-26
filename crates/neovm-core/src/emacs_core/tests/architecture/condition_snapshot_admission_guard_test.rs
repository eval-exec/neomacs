//! Context constructors stay dormant.
//!
//! `admit_condition_snapshots` is the only flip. Assembly and pdump
//! reconstruction must not call it, or a startup context would publish a
//! condition-snapshot id before any caller asked.

use super::architecture_production_source_roots::production_file_in;

#[test]
fn architecture_context_constructors_do_not_admit_condition_snapshots() {
    for relative in [
        "emacs_core/runtime/eval/construct.rs",
        "emacs_core/runtime/eval/pdump_reconstruct.rs",
    ] {
        let path = production_file_in("neovm-eval", relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            !source.contains("admit_condition_snapshots"),
            "{} names admit_condition_snapshots",
            path.display()
        );
    }
}
