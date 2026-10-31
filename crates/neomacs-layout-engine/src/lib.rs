//! Rust Display Layout Engine.
//!
//! Replaces the C display engine (xdisp.c) for computing glyph layout.
//! Reads window/buffer state from neovm-core and publishes immutable
//! `FrameDisplayState` snapshots that renderers materialize downstream.

// FFI-heavy layout code; migrate to explicit `unsafe {}` blocks incrementally.
#![allow(unsafe_op_in_unsafe_fn)]

pub(crate) mod attempt_freshness;
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
pub mod display_when;
pub mod engine;
pub(crate) mod fixed_symbols;
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
pub mod incremental_layout;
pub(crate) mod layout_effect;
pub mod mock_frame;
pub mod neovm_bridge;
pub(crate) mod output;
pub(crate) mod presentation;
pub(crate) mod redisplay_fontification;
pub(crate) mod row_layout;
pub(crate) mod scroll_policy;
pub mod text_shaper;
pub mod tty_menu_bar;
pub mod types;
pub mod unicode;
pub(crate) mod viewport_resolution;
pub(crate) mod window_layout;
pub mod window_output;

pub use engine::*;
pub use types::*;

#[cfg(test)]
#[path = "tests/test_composition.rs"]
mod test_composition;

/// Carrier park (Green S-track): reset this crate's classified thread-local
/// state. Registered with the neovm-core carrier pool by the editor binary.
pub fn reset_carrier_tls() {
    display_status_line::reset_carrier_tls();
    gui_chrome::reset_carrier_tls();
    tty_menu_bar::reset_carrier_tls();
    incremental_layout::chrome_memo::reset_carrier_tls();
}

/// Debug-only fingerprint of this crate's carrier TLS.
#[cfg(debug_assertions)]
pub fn carrier_tls_fingerprint() -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    display_status_line::carrier_tls_fingerprint(&mut h);
    gui_chrome::carrier_tls_fingerprint(&mut h);
    tty_menu_bar::carrier_tls_fingerprint(&mut h);
    incremental_layout::chrome_memo::carrier_tls_fingerprint(&mut h);
    h
}
