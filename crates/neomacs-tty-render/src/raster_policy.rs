//! The evaluator policies the terminal rasterizer is constructed with.
//!
//! Two rasterizer decisions belong to the evaluator, not to the presentation:
//! GNU's `CHARACTER_WIDTH` (`src/buffer.h:1707-1715`), which decides whether
//! an ordinary character occupies a terminal cell at all, and where the face
//! of a text row's end-of-line fill stops. The display host supplies both
//! from the evaluator; this crate never links it (DECISIONS D47).

use neomacs_display_protocol::glyph_matrix::TerminalCharWidths;

/// Where the terminal branch of GNU's `extend_face_to_end_of_line`
/// (`src/xdisp.c:24698-24827`) stops filling a TEXT_AREA row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFillEdge {
    /// The text area's structural right edge, or the start of a non-empty
    /// right margin.
    Structural,
    /// The right edge of the posn object extent the row's area layout
    /// carries (`NEOMACS_POSN_OBJECT_EXTENT`).
    PosnObjectExtent,
}

/// What one [`TtyRif`](crate::rif::TtyRif) rasterizes with, fixed for its
/// lifetime.
#[derive(Clone, Copy, Debug)]
pub struct TtyRasterPolicy {
    /// GNU `CHARACTER_WIDTH`, in terminal cells.
    pub char_widths: TerminalCharWidths,
    /// Where a text row's end-of-line face fill stops.
    pub text_fill: TextFillEdge,
}
