//! W-REG R1 stage 08: core manifest lines 1421-1426 (gnutls-asynchronous-parameters .. gnutls-bye).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "gnutls-asynchronous-parameters",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_asynchronous_parameters),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "gnutls-bye",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_bye),
            SubrArity::new(2, Some(2)),
        )),
}
