//! The opt-in global-SSA mid-end. The initial tier shares the baseline emitters;
//! optimization passes are separately gated and do not run during construction.
//!
//! Threading: each compiler owns its IR exclusively. Finished IR contains only
//! immutable Rust data and opaque value bits, so it can move to a worker. The
//! mutator retains and roots the source constants; workers never dereference
//! Lisp pointers or read a mutator's thread-local state.

#![allow(dead_code)] // The independently gated mid-end is being brought up.

pub(crate) mod build;
pub(crate) mod ir;
pub(crate) mod mem;
pub(crate) mod types;
pub(crate) mod verify;

#[cfg(test)]
pub(crate) mod eval;
#[cfg(test)]
#[path = "tests/eval.rs"]
mod eval_tests;
#[cfg(test)]
#[path = "tests/ir_verify.rs"]
mod ir_verify_tests;
#[cfg(test)]
#[path = "tests/types.rs"]
mod type_tests;
