use super::trailing_box_run_terminal_for_row;
use neomacs_display_protocol::face::BoxVerticalEdges;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphRow};
use neomacs_display_protocol::types::FaceId;

#[test]
fn visual_wrap_fill_inherits_only_the_last_source_glyphs_right_terminal() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut glyph = Glyph::char('a', FaceId::new(1), 0);
    glyph.box_vertical_edges = BoxVerticalEdges::Both;
    row.glyphs[GlyphArea::Text.index()].push(glyph);
    assert_eq!(
        trailing_box_run_terminal_for_row(&row),
        BoxVerticalEdges::Right,
        "a source run ending at the wrap must close on the filler"
    );

    row.glyphs[GlyphArea::Text.index()][0].box_vertical_edges = BoxVerticalEdges::Left;
    assert_eq!(
        trailing_box_run_terminal_for_row(&row),
        BoxVerticalEdges::Neither,
        "a source run continuing past the wrap keeps the filler open"
    );
}
