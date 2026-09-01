//! Mock-display frame content types — the handoff from mock-display to the
//! layout engine's `layout_mock_frame()`.
//!
//! These types are only used by `crates/neomacs/src/bin/mock-display.rs` and
//! `crates/neomacs-layout-engine/src/engine.rs: layout_mock_frame()`.  The real
//! neomacs GUI pipeline goes through `layout_frame_rust(evaluator)` instead.

use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, Rect};

/// A single glyph with its face assignment and display property.
#[derive(Debug, Clone)]
pub struct MockStyledGlyph {
    pub ch: char,
    pub face_id: FaceId,
    pub display: Option<MockDisplayProperty>,
}

/// Display properties resolved by the evaluator into Rust enums.
#[derive(Debug, Clone)]
pub enum MockDisplayProperty {
    /// Character is invisible (invisible text property).
    Invisible,
    /// Replace with a different string and face.
    Replace(String, FaceId),
    /// A composed sequence (combining marks, ZWJ sequences, etc.).
    Composition(Vec<MockStyledGlyph>),
}

/// One row of buffer text: a sequence of styled glyphs.
#[derive(Debug, Clone)]
pub struct MockStyledLine {
    pub glyphs: Vec<MockStyledGlyph>,
}

impl MockStyledLine {
    pub fn from_str(text: &str, face_id: FaceId) -> Self {
        Self {
            glyphs: text
                .chars()
                .map(|ch| MockStyledGlyph {
                    ch,
                    face_id,
                    display: None,
                })
                .collect(),
        }
    }
}

/// Content for one window in a frame.
#[derive(Debug, Clone)]
pub struct MockWindowContent {
    pub window_id: u64,
    pub lines: Vec<MockStyledLine>,
    /// Pre-formatted mode-line.  Each glyph carries its own face_id,
    /// matching GNU's propertized mode-line-format output.
    pub mode_line: MockStyledLine,
    /// Pixel bounds relative to frame, computed by the evaluator from
    /// frame parameters and split configuration.
    pub pixel_bounds: Rect,
    pub selected: bool,
    /// Whether the buffer has been scrolled horizontally.
    pub truncated_lines: bool,
}

/// Content for a floating child frame (posframe, completion popup, etc.).
#[derive(Debug, Clone)]
pub struct MockChildFrameContent {
    pub frame_id: u64,
    pub window: MockWindowContent,
    /// Position within the parent frame's pixel area.
    pub parent_x: f32,
    pub parent_y: f32,
    /// Stacking order relative to other child frames.
    pub z_order: i32,
}

/// Mock-display frame content: everything needed to lay out and render a
/// frame without a live Lisp evaluator.  Only used by mock-display and
/// `layout_mock_frame()`.
#[derive(Debug, Clone)]
pub struct MockFrameContent {
    pub frame_id: u64,
    /// Faces keyed by numeric ID.  Face 0 is the default face.
    pub faces: Vec<Face>,
    pub windows: Vec<MockWindowContent>,
    pub child_frames: Vec<MockChildFrameContent>,
    /// Full frame pixel dimensions (from frame parameters).
    pub frame_pixel_width: f32,
    pub frame_pixel_height: f32,
    pub background: Color,
    /// Per-level menu bar items, if any.  Pre-formatted strings keyed by
    /// level.  Level 0 is the top-level menu bar.
    pub menu_bar: Option<Vec<String>>,
    /// Minibuffer / echo-area window.  Always present at frame bottom.
    pub minibuffer: Option<MockWindowContent>,
}
