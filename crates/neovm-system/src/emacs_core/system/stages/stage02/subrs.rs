//! W-REG R1 stage 02: core manifest lines 322-327 (insert-file-contents .. write-region).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "insert-file-contents",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_insert_file_contents),
            SubrArity::new(1, Some(5)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "write-region",
                NativeFn::ContextVec(crate::emacs_core::fileio::builtin_write_region),
                SubrArity::new(3, Some(7)),
            ))
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                    "r\nFWrite region to file: \ni\ni\ni\np",
                ),
            ),
}
