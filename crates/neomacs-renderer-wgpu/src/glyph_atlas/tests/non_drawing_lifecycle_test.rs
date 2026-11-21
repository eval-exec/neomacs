use super::*;
use neomacs_display_protocol::font::ResolvedGlyphId;
use neomacs_display_protocol::frame_chrome::ResolvedMenuLabel;

fn glyph(glyph_id: u32) -> ResolvedGlyph {
    ResolvedGlyph {
        resolved_font_id: ResolvedFontId(817),
        glyph_id: ResolvedGlyphId::new(glyph_id),
        x: 0.0,
        y: 0.0,
        x_advance: 12.0,
        cluster_start: 0,
        cluster_end: 3,
    }
}

#[test]
fn composed_non_drawing_retention_keeps_active_menu_failures_and_evicts_cold_text() {
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    atlas.font_system = FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new());
    let menu = ResolvedMenuLabel::new(vec![glyph(1)], Vec::new(), 16.0);
    assert_eq!(
        atlas.menu_label_atlas(&menu, &device, &queue).err(),
        Some(GlyphAtlasError::RasterizeFailed)
    );
    for index in 0..1024 {
        assert_eq!(
            atlas
                .get_or_create_composed_atlas_result(
                    &device,
                    &queue,
                    &format!("unavailable-{index}"),
                    FaceId::new(0),
                    16.0f32.to_bits(),
                    None,
                    SubpixelBin::Zero,
                    SubpixelBin::Zero,
                    SubpixelRequest::Disabled
                )
                .err(),
            Some(GlyphAtlasError::RasterizeFailed)
        );
    }
    assert_eq!(atlas.non_drawing_composed_cache.len(), 1025);
    for _ in 0..60 {
        atlas.advance_generation();
        assert_eq!(
            atlas.menu_label_atlas(&menu, &device, &queue).err(),
            Some(GlyphAtlasError::RasterizeFailed)
        );
    }
    atlas.advance_generation();
    assert_eq!(atlas.non_drawing_composed_cache.len(), 1);
    let (key, retained) = atlas.non_drawing_composed_cache.iter().next().unwrap();
    assert!(key.glyph.text.is_empty(), "the active menu key survives");
    assert_eq!(retained.last_accessed, 60);
    assert!(matches!(
        retained.outcome,
        NonDrawingOutcome::Failed(GlyphAtlasError::RasterizeFailed)
    ));
    assert_eq!(
        atlas.resolved_rasterization_attempts_total(),
        1,
        "sixty repeated menu failures never rerasterize"
    );
    assert_eq!(
        atlas.fallback_shape_attempts_total(),
        0,
        "an empty catalog never enters cosmic shaping"
    );
}

#[test]
fn menu_failures_are_memoized_and_font_publication_and_resets_heal_them() {
    let (device, queue, mut atlas) = super::tests::try_test_device_and_atlas()
        .expect("#817 regression requires a real GPU adapter");
    let font = super::raster_outcome_tests::blank_font();
    // Load the exact immutable bytes without publishing the font table yet.
    let local = atlas
        .font_file_cache
        .pin_exact_asset(&mut atlas.font_system, font.replay.outline_asset().unwrap())
        .unwrap()
        .fontdb_id();
    let opened = atlas
        .font_system
        .get_font(local, fontdb::Weight::NORMAL)
        .unwrap();
    let glyph_id = opened.as_swash().charmap().map('\u{2800}');
    assert_ne!(glyph_id, 0);
    let missing = ResolvedMenuLabel::new(vec![glyph(u32::from(glyph_id))], Vec::new(), 16.0);
    let before = atlas.resolved_rasterization_attempts_total();
    for _ in 0..20 {
        assert_eq!(
            atlas.menu_label_atlas(&missing, &device, &queue).err(),
            Some(GlyphAtlasError::RasterizeFailed)
        );
    }
    assert_eq!(atlas.resolved_rasterization_attempts_total() - before, 1);
    let published = ResolvedMenuLabel::new(missing.glyphs().to_vec(), vec![font], 16.0);
    for _ in 0..20 {
        assert!(
            atlas
                .menu_label_atlas(&published, &device, &queue)
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(atlas.resolved_rasterization_attempts_total() - before, 2);
    assert!(
        atlas.atlas_composed_cache.is_empty(),
        "blank menus allocate no texture"
    );
    atlas.set_scale_factor(2.0);
    assert!(
        atlas
            .menu_label_atlas(&published, &device, &queue)
            .unwrap()
            .is_empty()
    );
    assert_eq!(atlas.resolved_rasterization_attempts_total() - before, 3);
    atlas.clear();
    assert!(
        atlas
            .menu_label_atlas(&published, &device, &queue)
            .unwrap()
            .is_empty()
    );
    assert_eq!(atlas.resolved_rasterization_attempts_total() - before, 4);
    atlas.synchronize_font_catalog_generation(FontCatalogGeneration::from_raw(817));
    assert!(
        atlas
            .menu_label_atlas(&published, &device, &queue)
            .unwrap()
            .is_empty()
    );
    assert_eq!(atlas.resolved_rasterization_attempts_total() - before, 5);
}
