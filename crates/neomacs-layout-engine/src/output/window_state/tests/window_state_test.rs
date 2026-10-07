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

/// GNU `row_containing_pos` scans top-to-bottom and returns the first real
/// displayed row containing point (`src/xdisp.c:22425-22504`).  Blank matrix
/// rows below visible EOB can carry the same shifted anchor after an insertion,
/// but they must not steal the cursor from an earlier displayed span.
#[test]
fn cursor_row_lookup_prefers_an_earlier_displayed_span_over_synthetic_blank_anchors() {
    let mut grid = OutputWindowRowGrid::new(6, 80);

    let mut content = GlyphRow::new(GlyphRowRole::Text);
    content.enabled = true;
    content.displays_text = true;
    content.start_charpos = 0;
    content.end_charpos = 13;
    grid.replace_row(0, content);

    let mut synthetic_blank = GlyphRow::new(GlyphRowRole::Text);
    synthetic_blank.enabled = true;
    synthetic_blank.displays_text = false;
    synthetic_blank.start_charpos = 1;
    synthetic_blank.end_charpos = 1;
    grid.replace_row(4, synthetic_blank.clone());
    grid.replace_row(5, synthetic_blank);

    assert_eq!(grid.find_cursor_row_for_charpos(1), Some(0));
}

/// GNU assigns a shared non-ZV endpoint to the next display row; only visible
/// ZV lets the earlier row retain it (`row_for_charpos_p`, src/xdisp.c).
#[test]
fn cursor_row_lookup_assigns_a_shared_non_zv_endpoint_to_the_following_row() {
    let mut grid = OutputWindowRowGrid::new(2, 80);

    let mut first = GlyphRow::new(GlyphRowRole::Text);
    first.enabled = true;
    first.displays_text = true;
    first.start_charpos = 0;
    first.end_charpos = 5;
    grid.replace_row(0, first);

    let mut second = GlyphRow::new(GlyphRowRole::Text);
    second.enabled = true;
    second.displays_text = true;
    second.start_charpos = 5;
    second.end_charpos = 10;
    grid.replace_row(1, second);

    assert_eq!(grid.find_cursor_row_for_charpos(5), Some(1));
}
