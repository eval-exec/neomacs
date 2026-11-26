use super::*;

/// GNU 31.1's automatic composition fills each gstring glyph with unshaped
/// `text_extents` before any rule runs (`font_fill_lglyph_metrics`,
/// src/font.c).  Under the pinned FreeSerif at 14 px the oracle dumps
/// U+0301 as `[0 0 769 708 0 -4 0 9 -7 [4 0 4]]`: width 0, lbearing -4,
/// rbearing 0, ascent 9, descent -7, and compose-gstring-for-graphic then
/// stores the spacing adjustment [4 0 4].  These four fields are the ones
/// the orphan-combining-mark advance is derived from, so pin them directly.
#[test]
fn orphan_combining_mark_text_extents_match_the_gnu_dump() {
    let file = neomacs_test_fonts::free_serif();
    let catalog =
        test_catalog::TestFontCatalog::from_files(&[file]).expect("pinned FreeSerif face");
    let mut service = catalog.metrics_service();
    let selection = RealizedFaceFontSelection::new("FreeSerif", 400, false, 14.0);
    let extents = service
        .glyph_text_extents_for_realized_face_char('\u{0301}', selection)
        .expect("U+0301 text extents");
    assert_eq!(extents.advance_px, 0.0, "GNU width = lround(x_advance)");
    assert_eq!(extents.lbearing_px, -4.0, "GNU floor(x_bearing)");
    assert_eq!(extents.rbearing_px, 0.0, "GNU ceil(x_bearing + ink width)");
    assert_eq!(extents.ascent_px, 9.0, "GNU ceil(-y_bearing - 1/256)");
    assert_eq!(extents.descent_px, -7.0, "GNU ceil(ink height + y_bearing)");
    // compose-gstring-for-graphic `nchars = 1`: with width 0 and
    // rbearing == width, wadjust = rbearing - lbearing = 4.
    assert_eq!(extents.rbearing_px - extents.lbearing_px, 4.0);
}
