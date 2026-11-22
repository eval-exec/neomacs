use super::super::test_support::rasterize_at_result;
use super::*;

#[test]
fn valid_empty_svg_glyph_is_successful_no_ink() {
    let base = glyph('A');
    let doc = document(&format!("<g id=\"glyph{base}\"/>"));
    let font = fixture_with_tables(&[(b"SVG ", svg_table(&[(base, base, doc, false)]))]);
    assert_eq!(layered_color_glyph_claim(&font, 0, base), Some(true));
    let raster = rasterize_at_result(&font, "empty-svg", base, [0, 0, 0, 255], 48.0)
        .unwrap()
        .expect("a valid empty glyph is a successful paint");
    assert_eq!(raster.source, ColorGlyphSource::SvgDocuments);
    assert_eq!(
        raster.rgba.len(),
        raster.width as usize * raster.height as usize * 4
    );
    assert!(raster.rgba.chunks_exact(4).all(|pixel| pixel[3] == 0));
}

#[test]
fn claimed_malformed_svg_document_is_not_a_successful_empty_glyph() {
    let base = glyph('A');
    let font = fixture_with_tables(&[(
        b"SVG ",
        svg_table(&[(base, base, b"<svg><broken".to_vec(), false)]),
    )]);
    assert_eq!(
        rasterize_at_result(&font, "bad-svg", base, [0, 0, 0, 255], 48.0),
        Err(ColorGlyphError::Paint {
            glyph_id: base,
            paint_source: ColorGlyphSource::SvgDocuments,
            reason: ColorPaintFailure::Malformed
        }),
    );
}
