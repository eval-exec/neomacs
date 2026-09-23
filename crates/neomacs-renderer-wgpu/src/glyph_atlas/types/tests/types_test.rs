use super::*;

#[test]
fn pixel_size_rejects_zero_width() {
    assert!(PixelSize::new(0, 10).is_none());
}

#[test]
fn pixel_size_rejects_zero_height() {
    assert!(PixelSize::new(10, 0).is_none());
}

#[test]
fn pixel_size_accepts_valid() {
    let s = PixelSize::new(10, 20).unwrap();
    assert_eq!(s.width(), 10);
    assert_eq!(s.height(), 20);
}

#[test]
fn page_id_requires_nonzero() {
    let id = PageId::<AlphaMask>::new(NonZeroU32::new(1).unwrap());
    assert_eq!(id.get(), 1);
}

#[test]
fn atlas_content_rect_preserves_values() {
    let r = AtlasContentRect::new(
        10,
        20,
        NonZeroU32::new(30).unwrap(),
        NonZeroU32::new(40).unwrap(),
    );
    assert_eq!(r.x(), 10);
    assert_eq!(r.y(), 20);
    assert_eq!(r.width(), 30);
    assert_eq!(r.height(), 40);
}

#[test]
fn uv_rect_from_content_rect() {
    let content = AtlasContentRect::new(
        0,
        0,
        NonZeroU32::new(512).unwrap(),
        NonZeroU32::new(512).unwrap(),
    );
    let uv = UvRect::from_content_rect(content, 2048);
    assert_eq!(uv.min(), [0.0, 0.0]);
    let eps = 0.001;
    assert!((uv.max()[0] - 0.25).abs() < eps);
    assert!((uv.max()[1] - 0.25).abs() < eps);
}

#[test]
fn rasterized_glyph_pixels_alpha_validation_ok() {
    let pixels = RasterizedGlyphPixels::Alpha {
        size: PixelSize::new(2, 2).unwrap(),
        bytes: vec![0u8, 128, 200, 255],
    };
    assert!(pixels.validated().is_ok());
}

#[test]
fn rasterized_glyph_pixels_alpha_validation_wrong_length() {
    let pixels = RasterizedGlyphPixels::Alpha {
        size: PixelSize::new(2, 2).unwrap(),
        bytes: vec![0u8, 128],
    };
    let err = pixels.validated().unwrap_err();
    match err {
        GlyphAtlasError::PixelDataLength {
            material,
            expected,
            actual,
        } => {
            assert_eq!(material, GlyphMaterialKind::AlphaMask);
            assert_eq!(expected, 4);
            assert_eq!(actual, 2);
        }
        other => panic!("expected PixelDataLength, got {:?}", other),
    }
}

#[test]
fn rasterized_glyph_pixels_subpixel_validation_ok() {
    let pixels = RasterizedGlyphPixels::Subpixel {
        size: PixelSize::new(2, 2).unwrap(),
        rgba: vec![0u8; 16],
    };
    assert!(pixels.validated().is_ok());
}

#[test]
fn rasterized_glyph_pixels_color_validation_ok() {
    let pixels = RasterizedGlyphPixels::Color {
        size: PixelSize::new(3, 3).unwrap(),
        rgba_srgb: vec![0u8; 36],
    };
    assert!(pixels.validated().is_ok());
}

#[test]
fn material_kind_texture_format_matches() {
    assert_eq!(
        GlyphMaterialKind::AlphaMask.texture_format(),
        wgpu::TextureFormat::R8Unorm
    );
    assert_eq!(
        GlyphMaterialKind::SubpixelMask.texture_format(),
        wgpu::TextureFormat::Rgba8Unorm
    );
    assert_eq!(
        GlyphMaterialKind::ColorRgba.texture_format(),
        wgpu::TextureFormat::Rgba8UnormSrgb
    );
}

#[test]
fn material_kind_bytes_per_pixel() {
    assert_eq!(GlyphMaterialKind::AlphaMask.bytes_per_pixel(), 1);
    assert_eq!(GlyphMaterialKind::SubpixelMask.bytes_per_pixel(), 4);
    assert_eq!(GlyphMaterialKind::ColorRgba.bytes_per_pixel(), 4);
}

#[test]
fn any_atlas_entry_material_kind_roundtrip() {
    let page = PageId::<AlphaMask>::new(NonZeroU32::new(1).unwrap());
    let rect = AtlasContentRect::new(
        0,
        0,
        NonZeroU32::new(10).unwrap(),
        NonZeroU32::new(10).unwrap(),
    );
    let uv = UvRect::new([0.0, 0.0], [1.0, 1.0]);
    let metrics = GlyphMetrics {
        bearing_x: 0.0,
        bearing_y: 10.0,
        advance_width: 8.0,
    };
    let entry = AtlasEntry::new(page, 0, rect, uv, metrics);
    let any = AnyAtlasEntry::Alpha(entry);
    assert_eq!(any.material_kind(), GlyphMaterialKind::AlphaMask);
}

#[test]
fn atlas_entry_validates_page_generation() {
    let page = PageId::<AlphaMask>::new(NonZeroU32::new(1).unwrap());
    let rect = AtlasContentRect::new(
        0,
        0,
        NonZeroU32::new(10).unwrap(),
        NonZeroU32::new(10).unwrap(),
    );
    let uv = UvRect::new([0.0, 0.0], [1.0, 1.0]);
    let metrics = GlyphMetrics {
        bearing_x: 0.0,
        bearing_y: 10.0,
        advance_width: 8.0,
    };
    let entry = AtlasEntry::new(page, 7, rect, uv, metrics);

    assert!(entry.matches_generation(7));
    assert!(!entry.matches_generation(8));
}

#[test]
fn glyph_atlas_config_default() {
    let config = GlyphAtlasConfig::default();
    assert_eq!(config.page_size, 2048);
    assert_eq!(config.padding, 1);
    assert_eq!(config.max_pages_per_material, 8);
}

#[test]
fn glyph_atlas_config_rejects_content_larger_than_padded_page() {
    let config = GlyphAtlasConfig {
        page_size: 64,
        padding: 1,
        max_pages_per_material: 1,
    };

    assert!(config.can_fit(PixelSize::new(62, 62).unwrap()));
    assert!(!config.can_fit(PixelSize::new(63, 62).unwrap()));
    assert!(!config.can_fit(PixelSize::new(62, 63).unwrap()));
}

#[test]
fn page_allocation_result_need_new_page() {
    let result: PageAllocationResult<AlphaMask> = PageAllocationResult::NeedNewPage;
    assert_eq!(result, PageAllocationResult::NeedNewPage);
}
