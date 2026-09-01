use super::*;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;

#[test]
fn cursor_row_lookup_includes_the_first_empty_eob_row() {
    let mut grid = OutputWindowRowGrid::new(6, 80);

    let mut content = GlyphRow::new(GlyphRowRole::Text);
    content.enabled = true;
    content.displays_text = true;
    content.start_charpos = 10;
    content.end_charpos = 11;
    grid.replace_row(3, content);

    let mut eob = GlyphRow::new(GlyphRowRole::Text);
    eob.enabled = true;
    eob.displays_text = false;
    eob.ends_at_zv = true;
    eob.start_charpos = 12;
    eob.end_charpos = 12;
    grid.replace_row(4, eob.clone());

    // `indicate-empty-lines` may add more rows at the same ZV.  The first
    // empty row is the one GNU assigns the hardware cursor to.
    grid.replace_row(5, eob);

    assert_eq!(grid.find_cursor_row_for_charpos(12), Some(4));
}
