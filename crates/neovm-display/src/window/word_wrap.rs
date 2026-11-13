//! Facade for the word-wrap policy projection, which lives below the
//! evaluator (`eval::data::word_wrap`) so `EditHost::word_wrap_policy` does
//! not reach up into the window layer.

pub use neovm_eval::emacs_core::eval::data::word_wrap::word_wrap_policy_for_buffer;
