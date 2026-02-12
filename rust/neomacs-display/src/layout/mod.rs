//! Rust Display Layout Engine
//!
//! Replaces the C display engine (xdisp.c) for computing glyph layout.
//! Reads buffer data via FFI and produces FrameGlyphBuffer for the renderer.
//!
//! # Architecture
//!
//! ```text
//! Emacs buffer data ──► FFI helpers ──► RustLayoutEngine ──► FrameGlyphBuffer ──► Renderer
//! ```
//!
//! The layout engine runs on the Emacs thread (not the render thread) because:
//! 1. Lisp functions need synchronous layout results (pos-visible-in-window-p, etc.)
//! 2. Fontification (jit-lock) runs DURING layout via Lisp callbacks
//! 3. Layout results must be written back to Emacs window structs

pub mod types;
pub mod engine;
pub mod emacs_ffi;
pub mod emacs_types;
pub mod unicode;
pub mod hit_test;
pub mod status_line;
pub mod bidi_layout;

pub use types::*;
pub use engine::*;
pub use hit_test::{hit_test_charpos_at_pixel, hit_test_window_charpos};
