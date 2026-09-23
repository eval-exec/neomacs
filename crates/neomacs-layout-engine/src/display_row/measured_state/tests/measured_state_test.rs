use super::*;
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea};
use neomacs_display_protocol::types::FaceId;

#[test]
fn content_height_uses_the_font_metrics_stamped_on_the_glyph() {
    let face_id = FaceId::new(31);
    let mut row = GlyphRow::new(GlyphRowRole::TabLine);
    row.enabled = true;
    row.height_px = 12.0;
    row.ascent_px = 9.0;
    row.glyphs[GlyphArea::Text.index()].push(
        Glyph::char('\u{e632}', face_id, 0)
            .with_pixel_geometry(10.0, 12.0, 9.0)
            .with_vertical_offset(-1.0),
    );

    // The face's primary font is taller than the concrete fontset fallback
    // that contains this Nerd Font character.  GNU sizes the row from the
    // font selected for the glyph, not from the face recipe.
    let mut face = Face::new(face_id);
    face.font_ascent = 11;
    face.font_descent = 3;
    let rendered = RenderedDisplayRow::new(
        row,
        DisplayRowOutputProgress::new(10.0, 1, 0.0, 12.0),
        Vec::new(),
        vec![
            crate::frame_face_arena::FrameFaceArena::default()
                .begin_attempt()
                .import_face(face)
                .unwrap(),
        ],
    );

    assert_eq!(rendered_display_row_content_height(&rendered), 12.0);
}
