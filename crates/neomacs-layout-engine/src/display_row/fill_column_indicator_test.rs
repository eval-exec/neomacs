//! Unit tests for the `display-fill-column-indicator` glyph mutation (GNU
//! `extend_face_to_end_of_line` indicator substitution, xdisp.c).
//!
//! `FillColumnIndicatorMutation` pads the current row's trailing region with a
//! gap stretch (when the text ends before the indicator column) and then
//! appends the indicator character carrying the `fill-column-indicator` face.

use super::{FillColumnIndicatorFill, FillColumnIndicatorMutation};
use crate::output::row_request::DisplayCurrentRowMutation;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphRow, GlyphType};
use neomacs_display_protocol::types::FaceId;

const TEXT_FACE: FaceId = FaceId::new(1);
const FCI_FACE: FaceId = FaceId::new(7);
const EXTEND_FACE: FaceId = FaceId::new(9);

fn text_glyphs(row: &GlyphRow) -> &[Glyph] {
    &row.glyphs[GlyphArea::Text.index()]
}

fn fill(gap_px: f32, gap_cols: u16) -> FillColumnIndicatorFill {
    FillColumnIndicatorFill {
        gap_px,
        gap_cols,
        gap_face_id: FCI_FACE,
        indicator_char: '│',
        indicator_face_id: FCI_FACE,
        tail_px: 0.0,
        tail_cols: 0,
        tail_face_id: FCI_FACE,
        char_width: 8.0,
        height_px: 16.0,
        ascent_px: 12.0,
    }
}

/// Extend-highlighted variant: a `tail` stretch continues past the indicator.
fn fill_extend(
    gap_px: f32,
    gap_cols: u16,
    tail_px: f32,
    tail_cols: u16,
) -> FillColumnIndicatorFill {
    FillColumnIndicatorFill {
        gap_px,
        gap_cols,
        gap_face_id: EXTEND_FACE,
        indicator_char: '│',
        indicator_face_id: FCI_FACE,
        tail_px,
        tail_cols,
        tail_face_id: EXTEND_FACE,
        char_width: 8.0,
        height_px: 16.0,
        ascent_px: 12.0,
    }
}

#[test]
fn short_line_gets_gap_stretch_then_indicator_char() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('a', TEXT_FACE, 0).with_pixel_width(8.0));

    // Text ends before the indicator column: pad 3 columns, then the `│`.
    FillColumnIndicatorMutation {
        fill: fill(24.0, 3),
    }
    .apply(&mut row);

    let glyphs = text_glyphs(&row);
    assert_eq!(glyphs.len(), 3, "text + gap stretch + indicator char");
    assert!(
        matches!(glyphs[1].glyph_type, GlyphType::Stretch { .. }),
        "the gap is a stretch"
    );
    assert_eq!(glyphs[1].face_id, FCI_FACE);
    assert_eq!(glyphs[1].pixel_width, 24.0);
    assert!(
        matches!(glyphs[2].glyph_type, GlyphType::Char { ch: '│' }),
        "the indicator char terminates the run"
    );
    assert_eq!(glyphs[2].face_id, FCI_FACE);
    assert!(row.displays_text);
}

#[test]
fn indicator_at_exact_column_has_no_gap_stretch() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('a', TEXT_FACE, 0).with_pixel_width(8.0));

    // Text ends exactly at the indicator column: no gap, just the `│`.
    FillColumnIndicatorMutation { fill: fill(0.0, 0) }.apply(&mut row);

    let glyphs = text_glyphs(&row);
    assert_eq!(glyphs.len(), 2, "text + indicator char, no gap stretch");
    assert!(matches!(glyphs[1].glyph_type, GlyphType::Char { ch: '│' }));
    assert_eq!(glyphs[1].face_id, FCI_FACE);
}

#[test]
fn empty_row_gets_indicator() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    assert!(text_glyphs(&row).is_empty());

    // A blank buffer line: gap from the content origin, then the `│`.
    FillColumnIndicatorMutation {
        fill: fill(160.0, 20),
    }
    .apply(&mut row);

    let glyphs = text_glyphs(&row);
    assert_eq!(glyphs.len(), 2, "gap stretch + indicator on a blank line");
    assert!(matches!(glyphs[0].glyph_type, GlyphType::Stretch { .. }));
    assert!(matches!(glyphs[1].glyph_type, GlyphType::Char { ch: '│' }));
}

#[test]
fn extend_row_wraps_indicator_in_extend_stretches() {
    // A region/hl-line row (:extend): gap + tail carry the EXTEND face so the
    // highlight stays continuous, and the indicator char keeps its own face.
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('a', TEXT_FACE, 0).with_pixel_width(8.0));

    FillColumnIndicatorMutation {
        fill: fill_extend(24.0, 3, 40.0, 5),
    }
    .apply(&mut row);

    let glyphs = text_glyphs(&row);
    assert_eq!(glyphs.len(), 4, "text + gap + indicator + tail");
    assert!(matches!(glyphs[1].glyph_type, GlyphType::Stretch { .. }));
    assert_eq!(glyphs[1].face_id, EXTEND_FACE, "gap keeps the highlight");
    assert!(matches!(glyphs[2].glyph_type, GlyphType::Char { ch: '│' }));
    assert_eq!(glyphs[2].face_id, FCI_FACE, "indicator char face");
    assert!(matches!(glyphs[3].glyph_type, GlyphType::Stretch { .. }));
    assert_eq!(glyphs[3].face_id, EXTEND_FACE, "tail keeps the highlight");
    assert_eq!(glyphs[3].pixel_width, 40.0);
}
