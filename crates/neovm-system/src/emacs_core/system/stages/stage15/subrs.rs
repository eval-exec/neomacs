//! W-REG R1 stage 15: core manifest lines 1840-1840 (find-file-name-handler .. find-file-name-handler).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "find-file-name-handler",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_find_file_name_handler),
            SubrArity::new(2, Some(2)),
        )),
}
