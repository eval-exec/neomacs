//! W-REG R1 stage 14: core manifest lines 1825-1830 (delete-directory-internal .. make-directory-internal).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "delete-directory-internal",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_delete_directory_internal),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-directory-internal",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_make_directory_internal),
            SubrArity::new(1, Some(1)),
        )),
}
