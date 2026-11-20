//! W-REG R1 stage 10: core manifest lines 1466-1466 (gnutls-get-initstage .. gnutls-get-initstage).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "gnutls-get-initstage",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_get_initstage),
            SubrArity::new(1, Some(1)),
        )),
}
