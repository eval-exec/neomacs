//! Window-marker data facade (stage-4 W2/E5 K2).
//!
//! The marker integration moved to emacs_core::eval::data::window_markers;
//! this module re-exports it at the old paths and keeps the tests.

pub use neovm_eval::emacs_core::eval::data::window_markers::*;
