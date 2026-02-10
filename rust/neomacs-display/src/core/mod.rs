//! Core types and data structures for the display engine.

pub mod types;
pub mod scene;
pub mod face;
pub mod error;
pub mod animation;
pub mod frame_glyphs;
pub mod cursor_animation;
pub mod buffer_transition;
pub mod animation_config;
pub mod scroll_animation;
pub mod itree;
pub mod regex;
pub mod gap_buffer;

pub use types::*;
pub use scene::*;
pub use face::*;
pub use error::*;
pub use animation::*;
pub use frame_glyphs::*;
pub use cursor_animation::*;
pub use buffer_transition::*;
pub use animation_config::*;
pub use scroll_animation::*;
