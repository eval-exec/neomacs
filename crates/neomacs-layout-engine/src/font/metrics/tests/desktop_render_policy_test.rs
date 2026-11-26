use super::*;
use freetype::face::LoadFlag;
use neomacs_display_protocol::{
    DesktopFontAntialias, DesktopFontHinting, DesktopFontRenderOptions, GraphicalBackend,
};

fn preferences(hinting: DesktopFontHinting) -> DesktopFontRenderOptions {
    DesktopFontRenderOptions {
        antialias: DesktopFontAntialias::Grayscale,
        hinting,
        ..Default::default()
    }
}

// Independent same-build FreeType reference. Requested targets are invariant;
// exact hinted pixels depend on the native interpreter, so no host literals.
fn reference(
    file: &str,
    size: u32,
    target: LoadFlag,
) -> (crate::font::probe::FontPxMetrics, [i32; 128]) {
    let library = freetype::Library::init().unwrap();
    let face = library.new_face(file, 0).unwrap();
    face.set_pixel_sizes(size, size).unwrap();
    let mut widths = [0; 128];
    for ch in 32_u8..127 {
        face.load_char(usize::from(ch), target).unwrap();
        widths[usize::from(ch)] = ((face.glyph().advance().x + 32) >> 6) as i32;
    }
    let positive: Vec<_> = widths[32..127]
        .iter()
        .copied()
        .filter(|width| *width > 0)
        .collect();
    let metrics = face.size_metrics().unwrap();
    let ascent = ((metrics.ascender + 32) >> 6) as i32;
    let descent = ((-metrics.descender + 32) >> 6) as i32;
    (
        crate::font::probe::FontPxMetrics {
            pixel_size: size,
            height: ascent + descent,
            ascent,
            descent,
            max_width: *positive.iter().max().unwrap(),
            space_width: widths[32],
            average_width: (positive.iter().map(|width| i64::from(*width)).sum::<i64>()
                / positive.len() as i64) as i32,
        },
        widths,
    )
}

#[test]
fn wayland_font_preferences_reopen_metrics_without_reloading_exact_fonts() {
    let file = neomacs_test_fonts::noto_sans_variable();
    let catalog = test_catalog::TestFontCatalog::from_files(&[file])
        .unwrap()
        .to_native_fontconfig();
    let mut service = catalog.metrics_service();
    let display = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::Wayland);
    let full = preferences(DesktopFontHinting::Full);
    assert!(display.observe_font_render_options(full));
    service.set_display_identity(display.clone());
    let initial_generation = service.font_catalog_generation();
    assert!(
        !service.synchronize_font_catalog().changed(),
        "binding startup preferences is not a live edge"
    );
    let db_id = service.font_system.db().faces().next().unwrap().id;
    let retained_font = service
        .font_system
        .get_font(db_id, fontdb::Weight::NORMAL)
        .unwrap();
    let path = file.to_str().unwrap();
    let identity = ResolvedFontIdentity::from_file(path, 0, None);

    for (hinting, target) in [
        (DesktopFontHinting::Full, LoadFlag::TARGET_NORMAL),
        (DesktopFontHinting::Slight, LoadFlag::TARGET_LIGHT),
        (DesktopFontHinting::Full, LoadFlag::TARGET_NORMAL),
    ] {
        let options = preferences(hinting);
        let changed = display.observe_font_render_options(options);
        let previous = service.font_catalog_generation();
        let update = service.synchronize_font_catalog();
        assert_eq!(update.changed(), changed);
        assert_eq!(update.rendering_changed, changed);
        assert!(
            !update.catalog.changed(),
            "settings do not replace font files"
        );
        assert_eq!(update.generation(), previous);
        assert!(
            std::sync::Arc::ptr_eq(
                &retained_font,
                &service
                    .font_system
                    .get_font(db_id, fontdb::Weight::NORMAL)
                    .unwrap()
            ),
            "settings-only changes must retain the opened font database/cache"
        );
        for size in [14, 7] {
            let native_options = service
                .native_render_options(&identity, size as f32)
                .unwrap();
            assert_eq!(
                native_options.metrics_load_flags(),
                target,
                "{hinting:?}/{size}"
            );
            let (expected_metrics, expected_widths) = reference(path, size, target);
            assert_eq!(
                service.probe_font_px_metrics(path, 0, size, None).unwrap(),
                expected_metrics,
                "aggregate/vertical probes must apply the connection options"
            );
            let ascii = service
                .probe_resolved_font_device_ascii_advances(&identity, size as f32)
                .unwrap();
            for ch in 32_u8..127 {
                assert_eq!(
                    ascii.logical_advance(char::from(ch), service.device_scale),
                    (expected_widths[usize::from(ch)] > 0)
                        .then_some(expected_widths[usize::from(ch)] as f32),
                    "{hinting:?}/{size}/{ch}"
                );
            }
        }
        assert!(!display.observe_font_render_options(options));
        assert!(!service.synchronize_font_catalog().changed());
    }
    assert_eq!(service.font_catalog_generation(), initial_generation);
}

#[test]
fn wayland_policy_edges_expire_canonical_records_but_preserve_their_ids() {
    let mut service = test_catalog::metrics_service();
    let display = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::Wayland);
    service.set_display_identity(display.clone());
    let font = service
        .resolved_font_for_face("monospace", 400, false, 14.0)
        .unwrap();
    // A separate canonical key lets the observation count prove invalidation
    // even when this FreeType build produces identical full/slight pixels.
    let observations = std::cell::Cell::new(0);
    let observe = || {
        observations.set(observations.get() + 1);
        FontInstanceProperties {
            family: font.family.clone(),
            full_name: font.full_name.clone(),
            postscript_name: font.postscript_name.clone(),
            weight: font.weight,
            slant: font.slant,
            width: font.width,
            ascent_px: font.ascent_px,
            descent_px: font.descent_px,
            space_advance_px: font.space_advance_px,
            glyph_advance: font.glyph_advance,
            spacing: font.spacing,
        }
    };
    let first =
        service
            .font_instances
            .intern(font.identity.clone(), font.replay.clone(), 13.25, &observe);
    service
        .font_instances
        .intern(font.identity.clone(), font.replay.clone(), 13.25, &observe);
    assert_eq!(observations.get(), 1);
    assert!(display.observe_font_render_options(preferences(DesktopFontHinting::Full)));
    assert!(service.synchronize_font_catalog().changed());
    let reopened =
        service
            .font_instances
            .intern(font.identity.clone(), font.replay.clone(), 13.25, &observe);
    assert_eq!(reopened.id, first.id);
    assert_eq!(observations.get(), 2);
    assert!(!service.synchronize_font_catalog().changed());
    service
        .font_instances
        .intern(font.identity.clone(), font.replay.clone(), 13.25, &observe);
    assert_eq!(observations.get(), 2);
}

#[test]
fn x11_connections_keep_catalog_policy_when_desktop_preferences_change() {
    let mut service = test_catalog::metrics_service();
    let display =
        GraphicalDisplayIdentity::named(GraphicalBackend::X11, ":raster-policy-fixture").unwrap();
    service.set_display_identity(display.clone());
    let file = neomacs_test_fonts::noto_sans_variable().to_str().unwrap();
    let identity = ResolvedFontIdentity::from_file(file, 0, None);
    let before = service.native_render_options(&identity, 14.0).unwrap();
    let metrics = service.probe_font_px_metrics(file, 0, 14, None).unwrap();
    assert!(!display.observe_font_render_options(preferences(DesktopFontHinting::Full)));
    assert!(!service.synchronize_font_catalog().changed());
    let after = service.native_render_options(&identity, 14.0).unwrap();
    assert_eq!(after.metrics_load_flags(), before.metrics_load_flags());
    assert_eq!(after.lcd_filter() as u32, before.lcd_filter() as u32);
    for mode in [
        neomacs_font_materializer::fontconfig::FontRenderMode::Alpha,
        neomacs_font_materializer::fontconfig::FontRenderMode::Subpixel,
    ] {
        assert_eq!(
            after.load_flags(false, mode),
            before.load_flags(false, mode)
        );
        assert_eq!(
            after.render_mode(mode) as u32,
            before.render_mode(mode) as u32
        );
    }
    assert_eq!(
        service.probe_font_px_metrics(file, 0, 14, None),
        Some(metrics)
    );
}

#[test]
fn frame_font_publication_captures_the_applied_policy_at_the_metric_safe_point() {
    use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
    let display = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::Wayland);
    let full = preferences(DesktopFontHinting::Full);
    let slight = preferences(DesktopFontHinting::Slight);
    assert!(display.observe_font_render_options(full));
    let mut metrics = test_catalog::metrics_service();
    metrics.set_display_identity(display.clone());
    let generation = metrics.font_catalog_generation();
    let mut service = Some(metrics);
    let mut initial_frame = FrameDisplayState::new(80, 24, 8.0, 16.0);
    realize_frame_fonts(&mut initial_frame, &mut service);
    assert_eq!(initial_frame.desktop_font_render_options, Some(full));
    assert!(display.observe_font_render_options(slight));
    let mut before_safe_point = FrameDisplayState::new(80, 24, 8.0, 16.0);
    realize_frame_fonts(&mut before_safe_point, &mut service);
    assert_eq!(
        before_safe_point.desktop_font_render_options,
        Some(full),
        "a frame must retain the policy used for its metrics"
    );
    assert!(
        service
            .as_mut()
            .unwrap()
            .synchronize_font_catalog()
            .changed()
    );
    let mut refreshed_frame = FrameDisplayState::new(80, 24, 8.0, 16.0);
    realize_frame_fonts(&mut refreshed_frame, &mut service);
    assert_eq!(refreshed_frame.desktop_font_render_options, Some(slight));
    assert_eq!(refreshed_frame.font_catalog_generation, generation);
    assert_eq!(
        initial_frame.desktop_font_render_options,
        Some(full),
        "old snapshots stay immutable"
    );
}

#[test]
fn wayland_font_preferences_invalidate_open_entity_cache() {
    use crate::font::resolver::FontEntityQuery;
    use crate::font_backend::FontFamilyName;
    use neovm_core::emacs_core::eval::FontSpecSelection;

    let file = neomacs_test_fonts::noto_sans_variable();
    let path = file.to_str().unwrap();
    // Pick a concrete pixel size where FreeType TARGET_NORMAL (Full) and
    // TARGET_LIGHT (Slight) produce genuinely distinct FontPxMetrics so the
    // test fails if desktop_font_render_options is bypassed or stale.
    let (size, expected_full, expected_slight) = (7_u32..=28)
        .find_map(|candidate_size| {
            let (full_m, _) = reference(path, candidate_size, LoadFlag::TARGET_NORMAL);
            let (slight_m, _) = reference(path, candidate_size, LoadFlag::TARGET_LIGHT);
            (full_m != slight_m).then_some((candidate_size, full_m, slight_m))
        })
        .expect("Noto Sans variable must have a pixel size where TARGET_NORMAL != TARGET_LIGHT");
    assert_ne!(expected_full, expected_slight);

    let catalog = test_catalog::TestFontCatalog::from_files(&[file])
        .unwrap()
        .to_native_fontconfig();
    let mut service = catalog.metrics_service();
    let display = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::Wayland);
    assert!(display.observe_font_render_options(preferences(DesktopFontHinting::Full)));
    service.set_display_identity(display.clone());
    assert!(!service.synchronize_font_catalog().changed());

    let query = FontEntityQuery::new(Some(FontFamilyName::new("Noto Sans").unwrap()))
        .with_selection(FontSpecSelection::OpenBySpec);

    crate::font::probe::reset_font_px_metric_probes();
    let first = service
        .open_font_entity(&query, size)
        .expect("initial open under Full hinting");
    assert_eq!(
        first.metrics, expected_full,
        "open_font_entity under Full hinting must apply DesktopFontRenderOptions (TARGET_NORMAL)"
    );
    for _ in 0..10 {
        assert_eq!(service.open_font_entity(&query, size), Some(first.clone()));
    }
    assert_eq!(
        crate::font::probe::font_px_metric_probes(),
        1,
        "repeat open_font_entity must hit opened_entity_cache without re-probing FreeType"
    );

    // Native entity-handle path must reuse opened_identity_cache and return Full metrics.
    let matched_full = service
        .open_platform_entity_metrics(&first.entity.matched, size, None)
        .expect("open_platform_entity_metrics under Full hinting");
    assert_eq!(matched_full, expected_full);
    assert_eq!(
        crate::font::probe::font_px_metric_probes(),
        1,
        "open_platform_entity_metrics should hit opened_identity_cache populated by open_font_entity"
    );

    // Standalone file fallback path must also apply DesktopFontRenderOptions and cache repeats.
    let file_full = service
        .probe_file_entity_metrics(path, size, None)
        .expect("probe_file_entity_metrics under Full hinting");
    assert_eq!(file_full, expected_full);
    let probes_after_file_full = crate::font::probe::font_px_metric_probes();
    assert_eq!(
        service.probe_file_entity_metrics(path, size, None),
        Some(file_full)
    );
    assert_eq!(
        crate::font::probe::font_px_metric_probes(),
        probes_after_file_full,
        "repeat probe_file_entity_metrics must hit opened_identity_cache"
    );

    // Changing desktop render options (Full -> Slight) through
    // `synchronize_font_catalog` MUST invalidate all three caches AND re-probe
    // with TARGET_LIGHT, returning the new Slight metrics.
    assert!(display.observe_font_render_options(preferences(DesktopFontHinting::Slight)));
    assert!(service.synchronize_font_catalog().changed());

    let reopened = service
        .open_font_entity(&query, size)
        .expect("reopen under Slight hinting");
    assert_eq!(
        reopened.metrics, expected_slight,
        "open_font_entity after Full -> Slight must return TARGET_LIGHT metrics"
    );
    assert_ne!(reopened.metrics, first.metrics);

    let matched_slight = service
        .open_platform_entity_metrics(&reopened.entity.matched, size, None)
        .expect("open_platform_entity_metrics under Slight hinting");
    assert_eq!(matched_slight, expected_slight);

    let file_slight = service
        .probe_file_entity_metrics(path, size, None)
        .expect("probe_file_entity_metrics under Slight hinting");
    assert_eq!(file_slight, expected_slight);
    assert_ne!(file_slight, file_full);
    assert_eq!(
        crate::font::probe::font_px_metric_probes(),
        probes_after_file_full + 2,
        "desktop render-options change must invalidate both matched-entity and file-probe caches"
    );
}
