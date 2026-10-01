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

struct TestFaces;

impl super::LineEndFaceResolver for TestFaces {
    fn trailing_whitespace_face_id(&mut self) -> FaceId {
        unreachable!("no trailing-whitespace step in this test")
    }

    fn fill_column_indicator_face_id(
        &mut self,
        _extend_bg: Option<neomacs_display_protocol::types::Color>,
    ) -> FaceId {
        FCI_FACE
    }
}

/// GNU locates the fill-column indicator column with the DEFAULT face's font
/// width (`fill_column_indicator_column` is called with
/// `default_face->font->average_width`, src/xdisp.c:24540-24546). A line that
/// ends in another font -- a fixed-pitch `org-block` row, a `:height` run --
/// must still put the indicator at the same column, so the position may not
/// follow the face active at the line end.
#[test]
fn indicator_column_uses_the_default_face_advance_not_the_line_end_face() {
    let ctx = super::LineEndContext {
        newline_face_id: TEXT_FACE,
        measurement_mode: crate::display_row::face_state::DisplayRowMeasurementMode::ConcreteFont,
        pen_x: 8.0,
        pen_col: 1,
        right_edge_x: 400.0,
        // The face active at the line end: 11px per cell.
        char_width: 11.0,
        indicator: Some(super::LineEndIndicator { col: 10, ch: '│' }),
        extend: None,
        frame_background: neomacs_display_protocol::types::Color::from_pixel(0x00FFFFFF),
        trailing_whitespace_enabled: false,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges::Neither,
        box_run_membership: neomacs_display_protocol::face::BoxRunMembership::Unboxed,
    };
    let geometry = super::LineEndFillGeometry {
        content_x: 8.0,
        height_px: 16.0,
        ascent_px: 12.0,
        fill_char_width: 8.0,
        // The default face: 8px per cell.
        indicator_char_width: 8.0,
    };

    let fill = super::resolve_indicator_fill(&ctx, geometry, 8.0, &mut TestFaces);

    // 8 (content_x) + 10 * 8 (default advance) - 8 (pen) = 80. Following the
    // line-end face's 11px advance would give 110.
    assert_eq!(
        fill.gap_px, 80.0,
        "the indicator column must be measured with the default face's advance"
    );
    assert_eq!(fill.char_width, 8.0);
}
