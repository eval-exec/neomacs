//! Rust Display Layout Engine.
//!
//! Replaces the C display engine (xdisp.c) for computing glyph layout.
//! Reads window/buffer state from neovm-core and publishes immutable
//! `FrameDisplayState` snapshots that renderers materialize downstream.

// FFI-heavy layout code; migrate to explicit `unsafe {}` blocks incrementally.
#![allow(unsafe_op_in_unsafe_fn)]
// The display/layout builders and FFI shims here routinely take many positional
// parameters (glyph geometry, face state, window metrics). This crate already
// annotates dozens of such fns individually; allow it crate-wide so the ~20
// remaining sites don't each need a repeat annotation. Folding args into structs
// is a separate refactor, out of scope for the lint gate.
#![allow(clippy::too_many_arguments)]

pub mod bidi;
pub(crate) mod buffer_source;
pub mod composition;
pub(crate) mod coords;
pub(crate) mod display_current_row_output;
pub(crate) mod display_cursor;
pub(crate) mod display_face_layout;
pub(crate) mod display_face_policy;
pub(crate) mod display_face_ref;
pub(crate) mod display_frame_output;
pub(crate) mod display_item;
pub(crate) mod display_mock_frame;
pub(crate) mod display_origin;
mod display_overlay_arrow;
pub mod display_pixel_calc;
pub(crate) mod display_property;
pub(crate) mod display_rendered_row_output_install;
pub(crate) mod display_row;
pub(crate) mod display_source;
pub(crate) mod display_source_append_plan;
pub(crate) mod display_source_item_append;
pub(crate) mod display_source_overflow;
pub(crate) mod display_source_progress;
pub(crate) mod display_source_resolver;
pub(crate) mod display_source_walk;
pub mod display_spec;
pub mod display_status_line;
pub(crate) mod display_text_output_install;
pub(crate) mod display_text_run_measurement;
pub(crate) mod display_text_window_row_lifecycle;
pub mod engine;
pub mod font;
pub mod font_backend;
pub(crate) mod frame_face_arena;
pub(crate) mod frame_layout_transaction;
pub(crate) mod frame_presentation;
pub(crate) mod frame_visual_history;
mod fringe_snapshot;
pub(crate) mod glyph_advance;
pub(crate) mod glyph_row_writer;
pub mod gui_chrome;
pub mod hit_test;
pub mod incremental_layout;
pub(crate) mod layout_effect;
pub mod mock_frame;
pub mod neovm_bridge;
pub(crate) mod output;
pub mod pixel_scroll;
pub(crate) mod presentation;
pub(crate) mod redisplay_fontification;
pub(crate) mod scroll_policy;
pub mod text_shaper;
pub mod tty_menu_bar;
pub mod types;
pub mod unicode;
pub(crate) mod window_layout;
pub mod window_output;

pub use engine::*;
pub use types::*;
