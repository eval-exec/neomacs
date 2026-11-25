//! The evaluator's policies for a terminal rasterizer (DECISIONS D47).

use neomacs_display_protocol::glyph_matrix::TerminalCharWidths;
use neomacs_tty_render::{TextFillEdge, TtyRasterPolicy};
use neovm_core::window::{PosnObjectExtentMode, posn_object_extent_mode};

/// The policy a production `TtyRif` rasterizes with: GNU `CHARACTER_WIDTH`
/// from the evaluator's width table, and the text fill of the process's
/// posn-object-extent mode.
///
/// The mode is an immutable process `OnceLock` over
/// `NEOMACS_POSN_OBJECT_EXTENT`, so reading it once, when the rasterizer is
/// constructed, answers what reading it on every frame did.
pub fn evaluator_tty_raster_policy() -> TtyRasterPolicy {
    TtyRasterPolicy {
        char_widths: TerminalCharWidths::new(neovm_core::encoding::char_width),
        text_fill: text_fill_edge(posn_object_extent_mode()),
    }
}

/// Where a terminal text row's end-of-line fill stops under `mode`.
pub fn text_fill_edge(mode: PosnObjectExtentMode) -> TextFillEdge {
    match mode {
        PosnObjectExtentMode::Off => TextFillEdge::Structural,
        PosnObjectExtentMode::On => TextFillEdge::PosnObjectExtent,
    }
}
