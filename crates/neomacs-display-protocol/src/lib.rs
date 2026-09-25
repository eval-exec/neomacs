//! Shared protocol types between layout, renderer, and runtime crates.

// The wide scene-/glyph-builder and FFI-index constructors in this crate
// (frame_glyphs, scroll_animation, transition_policy) mirror fixed wire/data
// layouts; folding their parameters into structs is a separate refactor, so
// `too_many_arguments` is allowed crate-wide rather than at each of the ~15 sites.
#![allow(clippy::too_many_arguments)]

pub mod clipboard;
pub mod cursor;
pub mod display_identity;
pub mod display_scale;
pub mod dma_buf;
pub mod effect_command;
pub mod effect_config;
pub mod face;
pub mod font;
pub mod frame_chrome;
pub mod frame_glyphs;
pub mod frame_time;
pub mod geometry;
pub mod glyph_matrix;
pub mod gradient;
pub mod image;
pub mod interaction_projection;
pub mod menu;
pub mod modifier_policy;
pub mod motion_spec;
pub mod popup_placement;
pub mod present_mapping;
pub mod presentation_origin;
pub mod presented_frame;
pub mod presented_pointer;
pub mod scene;
pub mod scroll_animation;
pub mod sealed_frame_presentation;
pub mod snapshot_text;
pub mod terminal_color;
pub mod toolbar_icon;
pub mod tooltip;
pub mod transition_policy;
pub mod tty_palette;
pub mod types;
pub mod ui_types;
pub mod visual_config;
pub mod window_animation;
pub mod window_chrome;
pub mod xterm_palette;
pub mod xwidget_extent;
pub use glyph_matrix::*;
pub mod tty_capabilities;

pub use clipboard::*;
pub use display_identity::*;
pub use display_scale::*;
pub use dma_buf::*;
pub use effect_command::*;
pub use effect_config::*;
pub use face::*;
pub use frame_chrome::*;
pub use frame_glyphs::*;
pub use geometry::*;
pub use gradient::*;
pub use image::*;
pub use interaction_projection::*;
pub use modifier_policy::*;
pub use popup_placement::*;
pub use present_mapping::*;
pub use window_chrome::*;

pub use presented_frame::*;
pub use presented_pointer::*;
pub use scene::*;
pub use scroll_animation::*;
pub use sealed_frame_presentation::*;
pub use terminal_color::TerminalColor;
pub use toolbar_icon::*;
pub use transition_policy::*;
pub use tty_palette::{TtyPalette, TtyPaletteEntry};
pub use types::*;
pub use ui_types::*;
pub use visual_config::*;
pub use xterm_palette::xterm_256_rgb;
pub use xwidget_extent::*;

#[cfg(test)]
mod tests;
