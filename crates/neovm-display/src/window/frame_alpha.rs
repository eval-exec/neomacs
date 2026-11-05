//! Frame alpha facade (stage-4 W2/E5 K2).
//!
//! The alpha parsing math moved to emacs_core::eval::data::frame_alpha.

pub use neovm_eval::emacs_core::eval::data::frame_alpha::*;

#[cfg(test)]
#[path = "frame_alpha/tests/frame_alpha_test.rs"]
mod tests;
