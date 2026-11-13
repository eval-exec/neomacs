//! Compile-time pass work elision without weakening native verification.
//!
//! Threading: the compile/knobs reader retains only immutable configuration.
//! This module contains no Lisp state, mutator pointers, IR or compiler scratch.
//! Every selected pass and its private candidate remain invocation-owned.

/// Thin forwarding seam: all environment parsing belongs to compile/knobs.
pub(crate) fn enabled() -> bool {
    crate::emacs_core::jit::compile::jit_opt_fast()
}

#[cfg(test)]
#[path = "tests/pass_fast.rs"]
mod tests;
