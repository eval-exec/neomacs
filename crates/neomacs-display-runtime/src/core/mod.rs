//! Core types and data structures for the display engine.

pub mod error;
#[cfg(test)]
mod grid;
pub mod profiler;

pub use neomacs_display_protocol::{face, frame_glyphs, scene, types};

pub use error::*;
pub use face::*;
pub use frame_glyphs::*;
pub use scene::*;
pub use types::*;
