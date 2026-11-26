use super::*;

#[test]
fn successful_ink_classification_uses_the_source_coverage_channels() {
    for (content, pixels, expected) in [
        (RasterContent::Mask, vec![0], false),
        (RasterContent::Mask, vec![1], true),
        (RasterContent::Color, vec![255, 80, 40, 0], false),
        (RasterContent::Color, vec![0, 0, 0, 1], true),
        (RasterContent::SubpixelMask, vec![1, 0, 0], true),
        // Zeno legitimately leaves A=0 while RGB holds visible coverage.
        (RasterContent::SubpixelMask, vec![1, 0, 0, 0], true),
        (RasterContent::SubpixelMask, vec![0, 0, 0, 255], false),
    ] {
        assert_eq!(raster_has_ink(1, 1, &pixels, content), Ok(expected));
    }
    assert!(
        matches!(
            raster_has_ink(1, 1, &[], RasterContent::Mask),
            Err(GlyphAtlasError::PixelDataLength { .. })
        ),
        "malformed data is not a successful blank"
    );
}

#[test]
fn transparent_nonzero_raster_retains_advance_without_allocating_a_texture() {
    let result = RasterizeResult {
        width: 2,
        height: 3,
        pixel_data: vec![0; 6],
        bearing_x: 0.0,
        bearing_y: 0.0,
        pixel_kind: GlyphPixelKind::AlphaMask,
        advance_width: 12.0,
        sampling: neomacs_display_protocol::font::GlyphSampling::Linear,
    };
    assert!(matches!(Rasterized::from_result(result),
        Ok(Rasterized::Empty { advance_width }) if advance_width == 12.0));
}
