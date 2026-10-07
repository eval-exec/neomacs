use super::*;
use neomacs_display_protocol::frame_glyphs::{CursorStyle, DisplaySlotId, GlyphRowRole};
use neomacs_display_protocol::glyph_matrix::{GlyphArea, GlyphRow, GlyphType};
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, Rect};

fn push_text(row: &mut neomacs_display_protocol::glyph_matrix::GlyphRow, text: &str) {
    for (idx, ch) in text.chars().enumerate() {
        crate::glyph_row_writer::push_char_to_row(row, ch, FaceId::new(0), idx, 0.0);
    }
}

fn phys_cursor(window_id: i64, row: usize, col: u16) -> PhysCursor {
    PhysCursor {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(window_id),
        charpos: 0,
        row,
        col,
        slot_id: DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(window_id),
            row: row as u32,
            col,
        },
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    }
}

#[test]
fn finalizes_matrix_row_with_bidi_reorder() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_text(&mut row, "אב");

    GlyphRowFinalizationContext::new(1, 0, Rect::new(0.0, 0.0, 80.0, 16.0))
        .finalize_row(&mut row, 10, None);

    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(glyphs[0].glyph_type, GlyphType::Char { ch: 'ב' });
    assert_eq!(glyphs[1].glyph_type, GlyphType::Char { ch: 'א' });
    assert!(row.reversed_p);
}

#[test]
fn bidi_reorder_carries_pointer_metadata_with_its_glyph() {
    use neomacs_display_protocol::glyph_matrix::{
        GlyphPointerAppearance, GlyphPointerOccurrenceIdentity, GlyphPointerSourceIdentity,
        GlyphPointerSourceKind,
    };

    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_text(&mut row, "אב");
    let pointer = GlyphPointerAppearance {
        source: GlyphPointerSourceIdentity {
            kind: GlyphPointerSourceKind::Buffer,
            source_id: 7,
            range_start: 0,
            range_end: 1,
            property_owner: 0,
            occurrence: GlyphPointerOccurrenceIdentity::Source,
        },
        face_id: FaceId::new(9),
    };
    let pointer_token = row
        .intern_pointer_appearance(pointer)
        .expect("pointer appearance token");
    row.glyphs[GlyphArea::Text.index()][0].pointer_appearance = Some(pointer_token);

    GlyphRowFinalizationContext::new(1, 0, Rect::new(0.0, 0.0, 80.0, 16.0))
        .finalize_row(&mut row, 10, None);

    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(glyphs[0].glyph_type, GlyphType::Char { ch: 'ב' });
    assert_eq!(glyphs[0].pointer_appearance, None);
    assert_eq!(glyphs[1].glyph_type, GlyphType::Char { ch: 'א' });
    assert_eq!(glyphs[1].pointer_appearance, Some(pointer_token));
}

/// A logical-order row that has only been normalized (NOT reordered) still
/// reorders correctly at install. Reordering now happens exactly once, at
/// install; there is no pre-pass reorder and no idempotency early-return.
#[test]
fn normalized_but_unreordered_row_reorders_at_install() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_text(&mut row, "אב");

    crate::glyph_row_writer::normalize_external_row(&mut row);
    GlyphRowFinalizationContext::new(1, 0, Rect::new(0.0, 0.0, 80.0, 16.0))
        .finalize_row(&mut row, 10, None);

    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(glyphs[0].glyph_type, GlyphType::Char { ch: 'ב' });
    assert_eq!(glyphs[1].glyph_type, GlyphType::Char { ch: 'א' });
    assert!(row.reversed_p);
}

#[test]
fn remaps_matching_phys_cursor_after_bidi_reorder() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.cursor_col = Some(0);
    push_text(&mut row, "אב");

    let mut cursor = phys_cursor(1, 0, 0);
    GlyphRowFinalizationContext::new(1, 0, Rect::new(4.0, 0.0, 80.0, 16.0)).finalize_row(
        &mut row,
        10,
        Some(&mut cursor),
    );

    assert_eq!(row.cursor_col, Some(1));
    assert_eq!(cursor.col, 1);
    assert_eq!(cursor.slot_id.col, 1);
    assert_eq!(
        cursor.x, 76.0,
        "window origin 4 + RTL origin 64 + glyph advance 8"
    );
}

#[test]
fn leaves_nonmatching_phys_cursor_unchanged() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    push_text(&mut row, "אב");

    let mut cursor = phys_cursor(2, 0, 0);
    GlyphRowFinalizationContext::new(1, 0, Rect::new(4.0, 0.0, 80.0, 16.0)).finalize_row(
        &mut row,
        10,
        Some(&mut cursor),
    );

    assert_eq!(cursor.col, 0);
    assert_eq!(cursor.slot_id.col, 0);
    assert_eq!(cursor.x, 0.0);
}

/// The visual-column remap must measure the caret from the row's own glyph
/// advances. `col * char_w` assumed every glyph was one frame cell wide, so a
/// row whose font differs from the frame's default (or a wide glyph, or a
/// stretch) put the caret off the glyph it decorates.
#[test]
fn remap_measures_cursor_from_row_glyph_advances() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    crate::glyph_row_writer::push_char_to_row(&mut row, 'a', FaceId::new(0), 0, 20.0);
    crate::glyph_row_writer::push_char_to_row(&mut row, 'b', FaceId::new(0), 1, 12.0);
    row.cursor_col = Some(1);

    // window x=4, 10 matrix columns over an 80px window => 8px nominal cell.
    let mut cursor = phys_cursor(1, 0, 1);
    GlyphRowFinalizationContext::new(1, 0, Rect::new(4.0, 0.0, 80.0, 16.0)).finalize_row(
        &mut row,
        10,
        Some(&mut cursor),
    );

    assert_eq!(cursor.col, 1);
    assert_eq!(cursor.slot_id.col, 1);
    assert_eq!(
        cursor.x, 24.0,
        "cursor x must be the row origin plus the preceding glyph's 20px advance, \
         not 4 + 1 * 8px nominal cell"
    );
}

#[test]
fn remapped_cursor_includes_ltr_row_origin() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.pixel_x = 10.0;
    crate::glyph_row_writer::push_char_to_row(&mut row, 'a', FaceId::new(0), 0, 20.0);
    crate::glyph_row_writer::push_char_to_row(&mut row, 'b', FaceId::new(0), 1, 12.0);
    row.cursor_col = Some(1);
    let mut cursor = phys_cursor(1, 0, 1);
    GlyphRowFinalizationContext::new(1, 0, Rect::new(4.0, 0.0, 80.0, 16.0)).finalize_row(
        &mut row,
        10,
        Some(&mut cursor),
    );
    assert_eq!(cursor.x, 34.0, "window 4 + row origin 10 + advance 20");
}
