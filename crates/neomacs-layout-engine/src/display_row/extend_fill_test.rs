//! Unit tests for the trailing `:extend` fill mutation (GNU
//! `extend_face_to_end_of_line`).
//!
//! These exercise `RowExtendFill` directly against a freshly built
//! `GlyphRow` (deterministic, no window fixture needed): a non-empty row gains
//! just the trailing stretch; an empty row first gains a leading face-anchor
//! space glyph then the stretch; an R2L row moves the logical trailing fill
//! to the physical left and reverses its source-side edge ownership.

use super::RowExtendFill;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphRow, GlyphType};
use neomacs_display_protocol::types::Color;
use neomacs_display_protocol::types::FaceId;

const EXTEND_FACE_ID: FaceId = FaceId::new(17);

fn extend_bg() -> Color {
    Color::from_pixel(0x00112233)
}

fn fill() -> RowExtendFill {
    // bg, face_id, width_px, height_px, ascent_px, char_width
    RowExtendFill::new(extend_bg(), EXTEND_FACE_ID, 40.0, 16.0, 12.0, 8.0)
}

fn text_glyphs(row: &GlyphRow) -> &[Glyph] {
    &row.glyphs[GlyphArea::Text.index()]
}

#[test]
fn non_empty_row_gets_only_trailing_stretch() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    // Pre-existing text "ab" so the row is non-empty.
    row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('a', FaceId::new(1), 0).with_pixel_width(8.0));
    row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('b', FaceId::new(1), 1).with_pixel_width(8.0));

    let applied = fill().apply_to(&mut row);
    assert!(applied, "fill should apply to a non-empty LTR row");

    let glyphs = text_glyphs(&row);
    assert_eq!(glyphs.len(), 3, "only one trailing stretch is appended");
    let last = glyphs.last().expect("trailing glyph");
    assert!(
        matches!(last.glyph_type, GlyphType::Stretch { .. }),
        "trailing glyph must be a stretch"
    );
    assert_eq!(last.face_id, EXTEND_FACE_ID);
    assert_eq!(last.pixel_width, 40.0);
    assert_eq!(last.pixel_height, 16.0);
    assert_eq!(last.pixel_ascent, 12.0);
    assert!(row.displays_text);
}

#[test]
fn empty_row_gets_leading_space_then_stretch() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    assert!(text_glyphs(&row).is_empty());

    let applied = fill().apply_to(&mut row);
    assert!(applied, "fill should apply to an empty highlighted row");

    let glyphs = text_glyphs(&row);
    assert_eq!(
        glyphs.len(),
        2,
        "empty row gets a leading space anchor + the fill stretch"
    );
    // Leading face-anchor space (GNU xdisp.c:24420).
    assert!(matches!(glyphs[0].glyph_type, GlyphType::Char { ch: ' ' }));
    assert_eq!(glyphs[0].face_id, EXTEND_FACE_ID);
    // Trailing stretch.
    assert!(matches!(glyphs[1].glyph_type, GlyphType::Stretch { .. }));
    assert_eq!(glyphs[1].face_id, EXTEND_FACE_ID);
    assert!(row.displays_text, "empty highlighted row now displays text");
}

#[test]
fn rtl_row_moves_fill_to_physical_left_and_swaps_terminal_edge() {
    use neomacs_display_protocol::face::BoxVerticalEdges;

    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut source = Glyph::char('\u{05d0}', EXTEND_FACE_ID, 0).with_pixel_width(8.0);
    source.box_vertical_edges = BoxVerticalEdges::Left;
    row.glyphs[GlyphArea::Text.index()].push(source);

    assert!(
        fill()
            .with_box_vertical_edges(BoxVerticalEdges::Right)
            .apply_to(&mut row)
    );
    crate::glyph_row_writer::reorder_row_bidi(&mut row, None);

    assert!(row.reversed_p);
    let glyphs = text_glyphs(&row);
    assert!(matches!(glyphs[0].glyph_type, GlyphType::Stretch { .. }));
    assert_eq!(glyphs[0].box_vertical_edges, BoxVerticalEdges::Left);
}

#[test]
fn width_cols_covers_fill_width_at_face_advance() {
    // 40px / 8px advance => 5 columns.
    assert_eq!(fill().width_cols(), 5);
    // Sub-column widths round up to at least one column.
    let narrow = RowExtendFill::new(extend_bg(), EXTEND_FACE_ID, 3.0, 16.0, 12.0, 8.0);
    assert_eq!(narrow.width_cols(), 1);
}

#[test]
fn applying_the_same_fill_twice_is_idempotent() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text.index()]
        .push(Glyph::char('a', FaceId::new(1), 0).with_pixel_width(8.0));

    assert!(fill().apply_to(&mut row));
    assert!(fill().apply_to(&mut row));

    assert_eq!(
        text_glyphs(&row).len(),
        2,
        "the legacy lifecycle may repeat the shared finalizer's exact fill"
    );
}

#[test]
fn a_different_synthetic_stretch_with_the_same_face_does_not_suppress_fill() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut existing = Glyph::stretch(1, EXTEND_FACE_ID).with_pixel_geometry(3.0, 2.0, 1.0);
    existing.provenance = neomacs_display_protocol::glyph_matrix::GlyphProvenance::line_end();
    row.glyphs[GlyphArea::Text.index()].push(existing);

    assert!(fill().apply_to(&mut row));

    let glyphs = text_glyphs(&row);
    assert_eq!(
        glyphs.len(),
        2,
        "a display-space stretch is content, not proof that EOL was filled"
    );
    assert_eq!(glyphs.last().expect("fill stretch").pixel_width, 40.0);
}
