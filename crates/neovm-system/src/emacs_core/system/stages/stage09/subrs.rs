//! W-REG R1 stage 09: core manifest lines 1436-1436 (gnutls-deinit .. gnutls-deinit).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "gnutls-deinit",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_deinit),
            SubrArity::new(1, Some(1)),
        )),
}
