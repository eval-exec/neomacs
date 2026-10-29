use super::*;

#[test]
fn ordinary_unicode_and_ascii_use_the_same_selected_device_grid() {
    let file = neomacs_test_fonts::jetbrains_mono_nerd_regular_3_4_0();
    let catalog = test_catalog::TestFontCatalog::from_files(&[file]).expect("pinned Nerd face");
    let mut service = catalog.metrics_service();
    for scale in [1.0, 1.75] {
        service
            .set_device_scale(neomacs_display_protocol::geometry::DeviceScale::new(scale).unwrap());
        for size in [14.0, 10.5] {
            for ch in ['\u{25b6}', '\u{f48a}'] {
                let selection =
                    RealizedFaceFontSelection::new("JetBrainsMono Nerd Font", 400, false, size);
                let materialized = service
                    .materialized_font_for_realized_face_char(ch, selection)
                    .expect("exact covering font");
                assert_eq!(
                    materialized.font.identity.file_path.as_deref(),
                    file.to_str(),
                    "device metrics must belong to the selected file"
                );
                let width = service.char_width_for_realized_face(ch, selection);
                let space = service.char_width_for_realized_face(' ', selection);
                assert_eq!(width, space, "{ch:?}, size {size}, scale {scale}");
                let entries = service.char_cache.len();
                assert_eq!(service.char_width_for_realized_face(ch, selection), width);
                assert_eq!(
                    service.char_cache.len(),
                    entries,
                    "existing cache owns reuse"
                );
                let (_, emitted_width) = service
                    .simple_copy_glyph_for_char(&materialized, ch)
                    .expect("selected ordinary glyph");
                assert_eq!(emitted_width, width, "simple-copy output and layout agree");
            }
        }
    }
}

#[test]
fn device_glyph_character_cache_is_partitioned_by_scale_and_cleared_on_font_refresh() {
    let file = neomacs_test_fonts::jetbrains_mono_nerd_regular_3_4_0();
    let catalog = test_catalog::TestFontCatalog::from_files(&[file]).expect("pinned Nerd face");
    let mut service = catalog.metrics_service();
    let selection = RealizedFaceFontSelection::new("JetBrainsMono Nerd Font", 400, false, 14.0);
    let first = service.char_width_for_realized_face('\u{f48a}', selection);
    assert_eq!(first, 8.0, "GNU pinned 14px font: native glyph advance");
    let original_key = (service.realized_face_font_cache_key(selection), '\u{f48a}');
    assert_eq!(service.char_cache.get(&original_key), Some(&first));
    service.set_device_scale(neomacs_display_protocol::geometry::DeviceScale::new(1.75).unwrap());
    let scaled_key = (service.realized_face_font_cache_key(selection), '\u{f48a}');
    assert_ne!(original_key, scaled_key);
    assert!(!service.char_cache.contains_key(&scaled_key));
    let scaled = service.char_width_for_realized_face('\u{f48a}', selection);
    assert_eq!(scaled, service.char_width_for_realized_face(' ', selection));
    assert_eq!(service.char_cache.get(&original_key), Some(&first));
    assert_eq!(service.char_cache.get(&scaled_key), Some(&scaled));
    service.clear_caches();
    assert!(service.char_cache.is_empty());
    assert_eq!(
        service.char_width_for_realized_face('\u{f48a}', selection),
        scaled
    );
}
