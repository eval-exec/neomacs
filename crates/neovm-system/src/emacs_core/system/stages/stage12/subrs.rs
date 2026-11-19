//! W-REG R1 stage 12: core manifest lines 1718-1733 (lock-buffer .. unlock-file).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "lock-buffer",
            NativeFn::ContextVec(crate::emacs_core::filelock::builtin_lock_buffer),
            SubrArity::new(0, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "lock-file",
            NativeFn::ContextVec(crate::emacs_core::filelock::builtin_lock_file),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "unlock-buffer",
            NativeFn::ContextVec(crate::emacs_core::filelock::builtin_unlock_buffer),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "unlock-file",
            NativeFn::ContextVec(crate::emacs_core::filelock::builtin_unlock_file),
            SubrArity::new(1, Some(1)),
        )),
}
