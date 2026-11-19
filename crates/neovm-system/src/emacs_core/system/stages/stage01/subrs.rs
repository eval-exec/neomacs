//! W-REG R1 stage 01: core manifest lines 148-148 (getenv-internal .. getenv-internal).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "getenv-internal",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_getenv_internal),
            SubrArity::new(1, Some(2)),
        )),
}
