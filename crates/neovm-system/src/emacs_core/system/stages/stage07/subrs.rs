//! W-REG R1 stage 07: core manifest lines 1356-1356 (do-auto-save .. do-auto-save).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "do-auto-save",
                NativeFn::ContextVec(crate::emacs_core::fileio::builtin_do_auto_save),
                SubrArity::new(0, Some(2)),
            ))
            .interactive(neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("")),
}
