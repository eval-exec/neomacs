//! Shared protocol types between layout, renderer, and runtime crates.

// The wide scene-/glyph-builder and FFI-index constructors in this crate
// (frame_glyphs, scroll_animation, transition_policy) mirror fixed wire/data
// layouts; folding their parameters into structs is a separate refactor, so
// `too_many_arguments` is allowed crate-wide rather than at each of the ~15 sites.
#![allow(clippy::too_many_arguments)]

pub mod clipboard;
pub mod cursor;
pub mod display_scale;
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
pub mod transition_policy;
pub mod tty_palette;
pub mod types;
pub mod ui_types;
pub mod visual_config;
pub mod xterm_palette;
pub mod xwidget_extent;
pub use glyph_matrix::*;
pub mod tty_capabilities;

pub use clipboard::*;
pub use display_scale::*;
pub use effect_command::*;
pub use effect_config::*;
pub use face::*;
pub use frame_chrome::*;
pub use frame_glyphs::*;
pub use geometry::*;
pub use gradient::*;
pub use image::*;
pub use popup_placement::*;
pub use present_mapping::*;
pub use presented_frame::*;
pub use presented_pointer::*;
pub use scene::*;
pub use scroll_animation::*;
pub use sealed_frame_presentation::*;
pub use terminal_color::TerminalColor;
pub use transition_policy::*;
pub use tty_palette::{TtyPalette, TtyPaletteEntry};
pub use types::*;
pub use ui_types::*;
pub use visual_config::*;
pub use xterm_palette::xterm_256_rgb;
pub use xwidget_extent::*;

#[cfg(test)]
#[path = "frame_chrome_test.rs"]
mod frame_chrome_test;

#[cfg(test)]
#[path = "image_test.rs"]
mod image_test;

#[cfg(test)]
#[path = "font_catalog_test.rs"]
mod font_catalog_test;

#[cfg(test)]
#[path = "presented_pointer_test.rs"]
mod presented_pointer_test;

#[cfg(test)]
#[path = "popup_placement_test.rs"]
mod popup_placement_test;

#[cfg(test)]
#[path = "geometry_test.rs"]
mod geometry_test;

#[cfg(test)]
#[path = "present_mapping_test.rs"]
mod present_mapping_test;

#[cfg(test)]
#[path = "sealed_frame_presentation_test.rs"]
mod sealed_frame_presentation_test;

#[cfg(test)]
#[path = "terminal_color_test.rs"]
mod terminal_color_test;

#[cfg(test)]
#[path = "xterm_palette_test.rs"]
mod xterm_palette_test;

#[cfg(test)]
#[path = "tty_palette_test.rs"]
mod tty_palette_test;
