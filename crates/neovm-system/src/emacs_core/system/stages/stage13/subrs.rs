//! W-REG R1 stage 13: core manifest lines 1762-1769 (default-file-modes .. set-default-file-modes).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "default-file-modes",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::fileio::builtin_default_file_modes(args)
            }),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-default-file-modes",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::fileio::builtin_set_default_file_modes(args)
            }),
            SubrArity::new(1, Some(1)),
        )),
}
