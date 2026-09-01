#![cfg(any(unix, windows))]

use neomacs_display_protocol::geometry::DeviceScale;
use neomacs_font_materializer::RasterPixels;
use neomacs_font_materializer::{
    FixedFontSpacing, FontDbSourceError, FontFileAsset, FontFileCache, FontMaterializer,
    FontOpenRequest, FontReplay, LegacyBitmapFormat,
};

fn bitmap_fixture() -> String {
    neomacs_test_fonts::spleen_2_2_0()
        .bdf()
        .to_string_lossy()
        .into_owned()
}

fn fixture(extension: &str) -> String {
    let fixtures = neomacs_test_fonts::spleen_2_2_0();
    let path = match extension {
        "pcf" => fixtures.pcf(),
        "pcf.gz" => fixtures.pcf_gz(),
        "otb" => fixtures.otb(),
        "woff" => fixtures.woff(),
        "woff2" => fixtures.woff2(),
        other => panic!("unknown downloaded font fixture extension: {other}"),
    };
    path.to_string_lossy().into_owned()
}

#[test]
fn rasterizes_bitmap_glyphs_to_a_normalized_eight_bit_mask() {
    let asset = FontFileAsset::new(bitmap_fixture(), 0).expect("fixture asset");
    let materializer = FontMaterializer::new().expect("FreeType materializer");
    let opened = materializer
        .open(FontOpenRequest {
            asset: &asset,
            requested_layout_px: 16.0,
            device_scale: DeviceScale::new(1.0).unwrap(),
            selected_device_ppem_26_6: None,
            line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
            spacing: FixedFontSpacing::MonospaceOrCharacterCell,
        })
        .expect("exact BDF face");

    let glyph = opened.glyph_for_char('A').expect("fixture A glyph");
    let raster = opened.rasterize(glyph).expect("normalized raster");

    assert_eq!((raster.width_px, raster.height_px), (8, 16));
    assert_eq!(raster.left_px, 0);
    assert_eq!(raster.advance_px, 8.0);
    let RasterPixels::Mask8(mask) = raster.pixels else {
        panic!("monochrome BDF must become an alpha mask");
    };
    assert_eq!(mask.len(), 128);
    assert!(mask.contains(&0));
    assert!(mask.contains(&255));
}

#[test]
fn replays_the_exact_selected_strike_for_pcf_gzip_and_otb() {
    let materializer = FontMaterializer::new().expect("FreeType materializer");
    for extension in ["pcf", "pcf.gz", "otb"] {
        let asset = FontFileAsset::new(fixture(extension), 0).expect("fixture asset");
        let request = FontOpenRequest {
            asset: &asset,
            requested_layout_px: 16.0,
            device_scale: DeviceScale::new(1.0).unwrap(),
            selected_device_ppem_26_6: None,
            line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
            spacing: FixedFontSpacing::MonospaceOrCharacterCell,
        };
        let selected = materializer.open(request).expect("layout opens exact face");
        let reopened = materializer
            .reopen(request, selected.replay())
            .expect("renderer reopens exact strike");

        assert_eq!(reopened.replay(), selected.replay(), "{extension}");
        assert_eq!(reopened.metrics(), selected.metrics(), "{extension}");
        let glyph = reopened.glyph_for_char('A').expect("fixture A glyph");
        assert_eq!(reopened.rasterize(glyph).unwrap().advance_px, 8.0);
    }
}

#[test]
fn bitmap_replay_rejects_a_different_file_asset() {
    let materializer = FontMaterializer::new().expect("FreeType materializer");
    let selected_asset = FontFileAsset::new(fixture("otb"), 0).expect("selected asset");
    let selected_request = FontOpenRequest {
        asset: &selected_asset,
        requested_layout_px: 16.0,
        device_scale: DeviceScale::new(1.0).unwrap(),
        selected_device_ppem_26_6: None,
        line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
        spacing: FixedFontSpacing::MonospaceOrCharacterCell,
    };
    let selected = materializer
        .open(selected_request)
        .expect("select exact bitmap asset");
    let different_asset = FontFileAsset::new(fixture("pcf"), 0).expect("different asset");
    let different_request = FontOpenRequest {
        asset: &different_asset,
        ..selected_request
    };

    assert_eq!(
        materializer
            .reopen(different_request, selected.replay())
            .expect_err("replay must not substitute a different source file"),
        neomacs_font_materializer::FontMaterializationError::ReplayAssetMismatch
    );
}

#[test]
fn selects_the_nearest_fixed_strike_when_otb_rejects_the_requested_size() {
    let materializer = FontMaterializer::new().expect("FreeType materializer");
    let asset = FontFileAsset::new(fixture("otb"), 0).expect("fixture asset");
    let request = FontOpenRequest {
        asset: &asset,
        requested_layout_px: 11.0,
        device_scale: DeviceScale::new(1.0).unwrap(),
        selected_device_ppem_26_6: None,
        line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
        spacing: FixedFontSpacing::MonospaceOrCharacterCell,
    };

    let selected = materializer
        .open(request)
        .expect("fixed-strike selection must use the closest exact strike");
    let FontReplay::FreeTypeBitmap { strike, .. } = selected.replay() else {
        panic!("OTB must use the bitmap replay protocol");
    };

    assert_eq!(strike.y_ppem_26_6, 16 * 64);
    assert_eq!(selected.metrics().effective_layout_px, 16.0);
    assert_eq!(selected.metrics().height_px, 16.0);
    assert_eq!(
        materializer
            .reopen(request, selected.replay())
            .unwrap()
            .replay(),
        selected.replay()
    );
}

#[test]
fn replays_the_native_selectors_exact_fixed_size_instead_of_reselecting() {
    let materializer = FontMaterializer::new().expect("FreeType materializer");
    let asset = FontFileAsset::new(fixture("otb"), 0).expect("fixture asset");
    let selected = materializer
        .open(FontOpenRequest {
            asset: &asset,
            // Deliberately disagree with the native entity. GNU selects the
            // concrete entity first; opening must honor that selected size.
            requested_layout_px: 30.0,
            device_scale: DeviceScale::new(1.0).unwrap(),
            selected_device_ppem_26_6: Some(16 * 64),
            line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
            spacing: FixedFontSpacing::MonospaceOrCharacterCell,
        })
        .expect("native-selected 16px strike");

    let FontReplay::FreeTypeBitmap { strike, .. } = selected.replay() else {
        panic!("OTB must use bitmap replay");
    };
    assert_eq!(strike.y_ppem_26_6, 16 * 64);
    assert_eq!(selected.metrics().effective_layout_px, 16.0);
}

#[test]
fn selects_a_device_scale_strike_and_reports_logical_metrics() {
    let materializer = FontMaterializer::new().expect("FreeType materializer");
    let asset = FontFileAsset::new(fixture("otb"), 0).expect("fixture asset");
    let layout_request = FontOpenRequest {
        asset: &asset,
        requested_layout_px: 8.0,
        device_scale: DeviceScale::new(2.0).unwrap(),
        selected_device_ppem_26_6: None,
        line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
        spacing: FixedFontSpacing::MonospaceOrCharacterCell,
    };

    let selected = materializer
        .open(layout_request)
        .expect("16px device strike");
    let FontReplay::FreeTypeBitmap { strike, .. } = selected.replay() else {
        panic!("OTB must use bitmap replay");
    };
    assert_eq!(strike.y_ppem_26_6, 16 * 64);
    assert_eq!(selected.metrics().effective_layout_px, 8.0);
    assert_eq!(selected.metrics().height_px, 8.0);
    let selected_raster = selected
        .rasterize(selected.glyph_for_char('A').expect("layout fixture glyph"))
        .expect("layout bitmap");

    let render_request = FontOpenRequest {
        asset: &asset,
        requested_layout_px: selected.metrics().effective_layout_px,
        device_scale: DeviceScale::new(1.0).unwrap(),
        selected_device_ppem_26_6: None,
        line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
        spacing: FixedFontSpacing::MonospaceOrCharacterCell,
    };
    let renderer = materializer
        .reopen(render_request, selected.replay())
        .expect("reopen physical strike");
    let glyph = renderer.glyph_for_char('A').expect("fixture glyph");
    let raster = renderer.rasterize(glyph).expect("physical bitmap");
    assert_eq!(
        (raster.width_px, raster.height_px),
        (selected_raster.width_px, selected_raster.height_px),
        "renderer must replay the selected physical strike without 2x stretching"
    );
    assert_eq!(raster.advance_px, 8.0);
}

#[test]
fn opens_an_exact_freetype_bitmap_face_without_semantic_fallback() {
    let asset = FontFileAsset::new(bitmap_fixture(), 0).expect("fixture asset");
    let materializer = FontMaterializer::new().expect("FreeType materializer");

    let opened = materializer
        .open(FontOpenRequest {
            asset: &asset,
            requested_layout_px: 16.0,
            device_scale: DeviceScale::new(1.0).unwrap(),
            selected_device_ppem_26_6: None,
            line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
            spacing: FixedFontSpacing::MonospaceOrCharacterCell,
        })
        .expect("the exact BDF face must open");

    assert!(matches!(opened.replay(), FontReplay::FreeTypeBitmap { .. }));
    assert_eq!(opened.metrics().requested_layout_px, 16.0);
    assert_eq!(opened.metrics().ascent_px, 12.0);
    assert_eq!(opened.metrics().descent_px, 4.0);
    assert_eq!(opened.metrics().height_px, 16.0);
    assert_eq!(opened.metrics().max_advance_px, 8.0);
}

#[test]
fn fixed_font_spacing_policy_controls_gnu_space_and_average_metrics() {
    let source = neomacs_test_fonts::spleen_2_2_0().bdf();
    let original = std::fs::read_to_string(&source).expect("downloaded BDF fixture text");
    let altered = original.replacen(
        "STARTCHAR SPACE\nENCODING 32\nSWIDTH 500 0\nDWIDTH 8 0",
        "STARTCHAR SPACE\nENCODING 32\nSWIDTH 250 0\nDWIDTH 4 0",
        1,
    );
    assert_ne!(
        altered, original,
        "fixture must expose one editable SPACE glyph"
    );
    let altered_path =
        source.with_file_name(format!("spleen-narrow-space-{}.bdf", std::process::id()));
    std::fs::write(&altered_path, altered).expect("derived text fixture under ignored ./tmp");

    let asset = FontFileAsset::new(altered_path.to_string_lossy(), 0).expect("fixture asset");
    let materializer = FontMaterializer::new().expect("FreeType materializer");
    let open = |spacing| {
        materializer
            .open(FontOpenRequest {
                asset: &asset,
                requested_layout_px: 16.0,
                device_scale: DeviceScale::new(1.0).unwrap(),
                selected_device_ppem_26_6: None,
                line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
                spacing,
            })
            .expect("derived BDF face")
    };

    let proportional = open(FixedFontSpacing::ProportionalOrDual);
    let mono = open(FixedFontSpacing::MonospaceOrCharacterCell);
    assert_eq!(proportional.metrics().space_advance_px, 4.0);
    assert!(proportional.metrics().average_advance_px < 8.0);
    assert_eq!(mono.metrics().space_advance_px, 8.0);
    assert_eq!(mono.metrics().average_advance_px, 8.0);
}

#[test]
fn selected_fixed_entity_size_must_exist_in_the_exact_face() {
    let asset = FontFileAsset::new(fixture("otb"), 0).expect("fixture asset");
    let error = FontMaterializer::new()
        .expect("FreeType materializer")
        .open(FontOpenRequest {
            asset: &asset,
            requested_layout_px: 16.0,
            device_scale: DeviceScale::new(1.0).unwrap(),
            selected_device_ppem_26_6: Some(99 * 64),
            line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
            spacing: FixedFontSpacing::MonospaceOrCharacterCell,
        })
        .expect_err("a selected entity strike is replay identity, not a nearest-size hint");
    assert_eq!(
        error,
        neomacs_font_materializer::FontMaterializationError::MissingSelectedStrike
    );
}

#[test]
fn fontdb_source_boundary_distinguishes_bitmap_capability_from_io_failure() {
    let mut db = fontdb::Database::new();
    assert_eq!(
        FontFileCache::open_file(&mut db, &bitmap_fixture(), 0),
        Err(FontDbSourceError::Unsupported {
            format: LegacyBitmapFormat::Bdf,
        })
    );
    assert!(matches!(
        FontFileCache::open_file(&mut db, "/neomacs/missing/font.ttf", 0),
        Err(FontDbSourceError::Read { .. })
    ));
}

#[test]
fn shared_fontdb_source_opens_a_deterministic_woff_fixture() {
    let path = fixture("woff");
    let mut db = fontdb::Database::new();
    let ids = FontFileCache::open_file(&mut db, &path, 0).expect("decoded WOFF source");
    assert!(!ids.is_empty());
    assert!(ids.iter().any(|id| {
        db.face(*id).is_some_and(|face| {
            matches!(
                &face.source,
                fontdb::Source::SharedFile(source, _) if source == std::path::Path::new(&path)
            ) && face
                .families
                .iter()
                .any(|(family, _)| family.contains("Spleen"))
        })
    }));
}

#[test]
fn decoded_woff2_collection_preserves_a_nonzero_source_selector() {
    let path = neomacs_test_fonts::woff2_collection().to_string_lossy();
    let mut font_system = cosmic_text::FontSystem::new();
    let mut cache = FontFileCache::new();

    let selected = cache
        .pin_exact_face(&mut font_system, &path, 1)
        .expect("decode and pin source face 1 as standalone SFNT face 0");
    assert!(
        font_system
            .db()
            .face(selected.fontdb_id())
            .is_some_and(|face| {
                face.index == 0
                    && face
                        .families
                        .iter()
                        .any(|(candidate, _)| candidate == selected.family())
            })
    );
}

#[test]
fn failed_exact_face_pins_retry_only_after_an_explicit_generation_boundary() {
    let source = neomacs_test_fonts::spleen_2_2_0().woff();
    let unique = format!(
        "neomacs-retry-exact-face-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("test clock after Unix epoch")
            .as_nanos()
    );
    let initially_missing = source.with_file_name(unique);
    let path = initially_missing.to_string_lossy();
    let mut font_system = cosmic_text::FontSystem::new();
    let mut cache = FontFileCache::new();

    assert!(matches!(
        cache.pin_exact_face(&mut font_system, &path, 0),
        Err(FontDbSourceError::Read { .. })
    ));
    std::fs::copy(&source, &initially_missing).expect("make the exact face available");
    assert!(
        cache.pin_exact_face(&mut font_system, &path, 0).is_err(),
        "the same generation must keep its observed failure"
    );

    cache.retry_failed_exact_faces();
    assert!(
        cache.pin_exact_face(&mut font_system, &path, 0).is_ok(),
        "the next generation must retry a transient source failure"
    );
    std::fs::remove_file(initially_missing).expect("remove the derived cached fixture");
}

#[test]
fn fontdb_source_detects_webfonts_by_magic_without_an_extension() {
    let source = neomacs_test_fonts::spleen_2_2_0().woff();
    let extensionless = source.with_file_name("spleen-8x16-extensionless-webfont");
    std::fs::copy(&source, &extensionless).expect("derive extensionless cached fixture");
    let path = extensionless.to_string_lossy();
    let mut db = fontdb::Database::new();

    assert!(
        !FontFileCache::open_file(&mut db, &path, 0)
            .expect("WOFF magic is authoritative")
            .is_empty()
    );
}

#[test]
fn fontdb_source_magic_overrides_a_misleading_bitmap_suffix() {
    let source = neomacs_test_fonts::spleen_2_2_0().woff();
    let mislabeled = source.with_file_name("spleen-8x16-webfont.pcf");
    std::fs::copy(&source, &mislabeled).expect("derive mislabeled cached fixture");
    let path = mislabeled.to_string_lossy();
    let mut db = fontdb::Database::new();

    assert!(
        !FontFileCache::open_file(&mut db, &path, 0)
            .expect("container magic must win over the path hint")
            .is_empty()
    );
}

#[test]
fn fontdb_source_detects_bitmap_only_sfnt_without_an_otb_suffix() {
    let source = neomacs_test_fonts::spleen_2_2_0().otb();
    let extensionless = source.with_file_name("spleen-8x16-extensionless-bitmap-sfnt");
    std::fs::copy(&source, &extensionless).expect("derive extensionless cached fixture");
    let path = extensionless.to_string_lossy();
    let mut db = fontdb::Database::new();

    assert_eq!(
        FontFileCache::open_file(&mut db, &path, 0),
        Err(FontDbSourceError::Unsupported {
            format: LegacyBitmapFormat::OpenTypeMonochromeBitmap,
        })
    );
}

#[test]
fn fontdb_source_accepts_scalable_color_bitmap_sfnt() {
    let path = neomacs_test_fonts::noto_color_emoji_2_051().to_string_lossy();
    let mut db = fontdb::Database::new();

    let ids = FontFileCache::open_file(&mut db, &path, 0)
        .expect("Swash owns scalable CBDT/CBLC color bitmap faces");

    assert_eq!(ids.len(), 1);
    assert_eq!(
        db.face(ids[0]).map(|face| face.post_script_name.as_str()),
        Some("NotoColorEmoji")
    );
}
