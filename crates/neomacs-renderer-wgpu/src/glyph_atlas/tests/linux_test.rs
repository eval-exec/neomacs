use super::*;

#[test]
fn native_bitmap_rows_follow_signed_pitch_without_copying_padding() {
    // FreeType's FT_Bitmap.buffer always points to the lowest address of the
    // `height * |pitch|` allocation. Keep a guard prefix so a broken negative
    // offset before `buffer` is observable without UB.
    let guarded = [0xEE, 0xEE, 0xEE, 1, 2, 99, 3, 4, 99];
    let buffer = unsafe { guarded.as_ptr().add(3) };
    // SAFETY: `buffer` points to 2 rows * 3 bytes = 6 readable bytes.
    unsafe {
        assert_eq!(copy_rows(buffer, 3, 2, 2), Some(vec![1, 2, 3, 4]));
        assert_eq!(copy_rows(buffer, -3, 2, 2), Some(vec![3, 4, 1, 2]));
        assert_eq!(copy_rows(buffer, 1, 2, 2), None);
        assert_eq!(copy_rows(std::ptr::null(), 3, 2, 2), None);
    }
}

#[test]
fn native_mono_and_lcd_masks_keep_their_coverage_channels() {
    assert_eq!(
        unpack_mono(&[0b10100000, 0b01000000], 3, 2),
        Some(vec![255, 0, 255, 0, 255, 0])
    );
    assert_eq!(
        normalize_gray(vec![0, 1, 2, 3], 4),
        Some(vec![0, 85, 170, 255])
    );
    assert_eq!(
        pack_lcd(&[10, 80, 30, 255, 0, 7]),
        Some(vec![10, 80, 30, 80, 255, 0, 7, 255])
    );
    assert_eq!(pack_lcd(&[10, 80]), None);
}

#[test]
fn changed_surface_policy_invalidates_native_caches_and_survives_clear() {
    use neomacs_font_materializer::fontconfig::XftResources;
    let mut rasterizer = NativeRasterizer::default();
    let key = (fontdb::ID::dummy(), 16.0f32.to_bits());
    rasterizer.options.insert(key, None);
    rasterizer.faces.insert(key.0, None);
    assert!(!rasterizer.set_surface_options(SurfaceFontOptions::default()));
    assert_eq!(rasterizer.options.len(), 1);
    assert_eq!(rasterizer.faces.len(), 1);

    let screen = SurfaceFontOptions::from_xlib_resources(
        XftResources::default(),
        neomacs_font_materializer::fontconfig::SurfaceSubpixelOrder::Unknown,
    );
    assert!(rasterizer.set_surface_options(screen));
    assert!(rasterizer.options.is_empty());
    assert!(rasterizer.faces.is_empty());
    rasterizer.options.insert(key, None);
    assert!(!rasterizer.set_surface_options(screen));
    assert_eq!(rasterizer.options.len(), 1);
    rasterizer.clear();
    assert_eq!(rasterizer.surface_options(), screen);
    assert!(rasterizer.options.is_empty());
}
