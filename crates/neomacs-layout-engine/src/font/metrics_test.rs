use super::*;
use tracing::debug;

// Helper: create a service (expensive — ~50ms for font scan)
fn make_svc() -> FontMetricsService {
    FontMetricsService::new()
}

fn test_font_path(path: std::path::PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

fn platform_file_candidate(
    identity: ResolvedFontIdentity,
    metadata: crate::font_backend::PlatformFontMetadata,
) -> crate::font_backend::PlatformFontCandidate {
    crate::font_backend::PlatformFontCandidate {
        locator: crate::font_backend::PlatformFontCandidateLocator::File(
            neomacs_display_protocol::font::FontFileAsset::from_identity(&identity)
                .expect("file-backed fixture identity"),
        ),
        identity,
        metadata,
    }
}

fn swash_replay_for(identity: &ResolvedFontIdentity) -> FontReplay {
    FontReplay::Swash {
        asset: neomacs_display_protocol::font::FontOutlineAsset::File(
            neomacs_display_protocol::font::FontFileAsset::from_identity(identity)
                .expect("file-backed fixture identity"),
        ),
    }
}

// ---------------------------------------------------------------
// Construction
// ---------------------------------------------------------------

#[test]
fn service_construction() {
    let svc = make_svc();
    assert!(svc.ascii_cache.is_empty());
    assert!(svc.char_cache.is_empty());
    assert!(svc.metrics_cache.is_empty());
}

#[test]
fn metrics_cache_identity_includes_the_fontset_generation() {
    let scale = neomacs_display_protocol::geometry::DeviceScale::new(1.0).unwrap();
    let before =
        MetricsCacheKey::new_at_fontset_generation("monospace", 400, false, 16.0, scale, 7);
    let after = MetricsCacheKey::new_at_fontset_generation("monospace", 400, false, 16.0, scale, 8);

    assert_ne!(before, after);
}

// ---------------------------------------------------------------
// char_width: basic sanity
// ---------------------------------------------------------------

#[test]
fn char_width_space_is_positive() {
    let mut svc = make_svc();
    let w = svc.char_width(' ', "monospace", 400, false, 14.0);
    assert!(w > 0.0, "space width should be positive, got {w}");
}

#[test]
fn char_width_letter_a_is_positive() {
    let mut svc = make_svc();
    let w = svc.char_width('A', "monospace", 400, false, 14.0);
    assert!(w > 0.0, "'A' width should be positive, got {w}");
}

#[test]
fn char_width_monospace_uniform() {
    // In a monospace font, all printable ASCII should have the same width
    let mut svc = make_svc();
    let w_a = svc.char_width('A', "monospace", 400, false, 14.0);
    let w_m = svc.char_width('M', "monospace", 400, false, 14.0);
    let w_i = svc.char_width('i', "monospace", 400, false, 14.0);
    let w_dot = svc.char_width('.', "monospace", 400, false, 14.0);

    // Allow tiny floating-point differences
    let eps = 0.01;
    assert!(
        (w_a - w_m).abs() < eps,
        "monospace A={w_a} vs M={w_m} differ by more than {eps}"
    );
    assert!(
        (w_a - w_i).abs() < eps,
        "monospace A={w_a} vs i={w_i} differ by more than {eps}"
    );
    assert!(
        (w_a - w_dot).abs() < eps,
        "monospace A={w_a} vs .={w_dot} differ by more than {eps}"
    );
}

#[test]
fn char_width_scales_with_font_size() {
    let mut svc = make_svc();
    let w14 = svc.char_width('A', "monospace", 400, false, 14.0);
    let w28 = svc.char_width('A', "monospace", 400, false, 28.0);
    // Doubling font size should roughly double the width
    let ratio = w28 / w14;
    assert!(
        ratio > 1.5 && ratio < 2.5,
        "width ratio for 2x font size should be ~2.0, got {ratio} (w14={w14}, w28={w28})"
    );
}

// ---------------------------------------------------------------
// char_width: specific fonts
// ---------------------------------------------------------------

#[test]
fn char_width_jetbrains_mono() {
    let mut svc = make_svc();
    let w = svc.char_width('A', "JetBrains Mono", 400, false, 14.0);
    assert!(
        w > 0.0,
        "JetBrains Mono 'A' width should be positive, got {w}"
    );
    // JetBrains Mono is monospace — check uniformity
    let w2 = svc.char_width('W', "JetBrains Mono", 400, false, 14.0);
    assert!(
        (w - w2).abs() < 0.01,
        "JetBrains Mono: A={w} W={w2} should be equal"
    );
}

#[test]
fn char_width_dejavu_sans_mono() {
    let mut svc = make_svc();
    let w = svc.char_width('x', "DejaVu Sans Mono", 400, false, 14.0);
    assert!(
        w > 0.0,
        "DejaVu Sans Mono 'x' width should be positive, got {w}"
    );
}

#[test]
fn char_width_proportional_font_varies() {
    // In a proportional font, 'i' should be narrower than 'W'
    let mut svc = make_svc();
    let w_i = svc.char_width('i', "DejaVu Sans", 400, false, 14.0);
    let w_w = svc.char_width('W', "DejaVu Sans", 400, false, 14.0);
    assert!(
        w_w > w_i,
        "proportional font: W={w_w} should be wider than i={w_i}"
    );
}

// ---------------------------------------------------------------
// char_width: non-ASCII
// ---------------------------------------------------------------

#[test]
fn char_width_cjk() {
    let mut svc = make_svc();
    let w_cjk = svc.char_width('漢', "monospace", 400, false, 14.0);
    let w_a = svc.char_width('A', "monospace", 400, false, 14.0);
    // CJK characters are typically double-width
    assert!(
        w_cjk > 0.0,
        "CJK char width should be positive, got {w_cjk}"
    );
    // Don't assert exact 2x ratio as font fallback varies, but it should
    // be wider than a single-width char
    assert!(
        w_cjk > w_a * 1.2,
        "CJK char ({w_cjk}) should be wider than ASCII ({w_a})"
    );
}

#[test]
fn char_width_accented_latin() {
    let mut svc = make_svc();
    let w = svc.char_width('é', "monospace", 400, false, 14.0);
    assert!(w > 0.0, "accented char width should be positive, got {w}");
}

#[test]
fn realized_face_font_selection_separates_ascii_primary_from_non_ascii_fontset_base() {
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(NoCandidateFontBackend));
    let selection = RealizedFaceFontSelection::new(
        PrimaryFontFamily::new("Symbols Nerd Font Mono"),
        FontsetBaseFamily::new("JetBrainsMono Nerd Font"),
        400,
        false,
        13.0,
    );

    let ascii = svc.font_request_for_char('A', selection);
    let icon = svc.font_request_for_char('\u{f48a}', selection);

    assert_eq!(ascii.family, "Symbols Nerd Font Mono");
    assert_eq!(icon.family, "JetBrainsMono Nerd Font");
}

#[test]
fn symbol_font_policy_tracks_the_live_char_script_table_and_invalidates_char_caches() {
    let mut eval = neovm_core::Context::new();
    eval.eval_str("(set-char-table-range char-script-table '(#x2000 . #x27ff) 'symbol)")
        .expect("classify the Unicode symbol block");
    let table = eval.obarray().symbol_value("char-script-table").copied();

    let mut svc = make_svc();
    assert!(
        svc.synchronize_symbol_font_policy(true, table).changed(),
        "initial GNU symbol policy must be observable as a selection change"
    );
    assert!(svc.symbol_font_policy.uses_primary_font_for('▶'));
    assert!(!svc.symbol_font_policy.uses_primary_font_for('\u{2800}'));

    let selection = RealizedFaceFontSelection::same_fontset("monospace", 400, false, 13.0);
    let cache_key = svc.realized_face_font_cache_key(selection);
    svc.char_cache.insert((cache_key, '▶'), 13.0);

    eval.eval_str(
        "(let ((unrelated (make-char-table 'neomacs-unrelated nil)))
           (set-char-table-range unrelated ?A t))",
    )
    .expect("mutate an unrelated char table");
    assert!(
        !svc.synchronize_symbol_font_policy(true, table).changed(),
        "an unrelated char-table write must not change font selection"
    );
    assert_eq!(
        svc.char_cache.len(),
        1,
        "an unrelated char-table write must retain font caches"
    );

    eval.eval_str("(set-char-table-range char-script-table #x2800 'symbol)")
        .expect("extend the live symbol classification");
    assert!(svc.synchronize_symbol_font_policy(true, table).changed());
    assert!(svc.char_cache.is_empty());
    assert!(svc.symbol_font_policy.uses_primary_font_for('\u{2800}'));

    assert!(svc.synchronize_symbol_font_policy(false, table).changed());
    assert!(!svc.symbol_font_policy.uses_primary_font_for('▶'));
}

#[test]
fn covered_char_publishes_the_same_record_as_the_realized_face() {
    // GNU keeps one font object per realized entity and size; whether the
    // ASCII face or a fontset tier led to it is not a property of the font.
    // The frame font table is keyed by id, so both paths must publish one
    // identical record or the renderer sees an id reused for a new instance.
    let mut svc = make_svc();
    let face = svc
        .resolved_font_for_face("Monospace", 400, false, 14.0)
        .expect("realized primary font");
    let Some(ch) = ['é', 'ü', 'ñ', 'ö'].into_iter().find(|ch| {
        svc.resolved_font_for_char(*ch, "Monospace", 400, false, 14.0)
            .is_some_and(|font| font.id == face.id)
    }) else {
        debug!("skipping: platform monospace font covers none of the probe characters");
        return;
    };

    let via_char = svc
        .resolved_font_for_char(ch, "Monospace", 400, false, 14.0)
        .expect("covered character");
    assert_eq!(
        via_char, face,
        "one resolved font id must name one record on every resolution path"
    );
}

#[test]
fn covered_symbol_uses_and_publishes_the_realized_primary_font() {
    let mut eval = neovm_core::Context::new();
    eval.eval_str("(set-char-table-range char-script-table '(#x2000 . #x27ff) 'symbol)")
        .expect("classify the Unicode symbol block");
    let table = eval.obarray().symbol_value("char-script-table").copied();

    let mut svc = make_svc();
    let _ = svc.synchronize_symbol_font_policy(true, table);
    let selection = RealizedFaceFontSelection::same_fontset("Monospace", 400, false, 14.0);
    let primary = svc
        .materialized_font_for_face("Monospace", 400, false, 14.0)
        .expect("realized primary font");
    if !svc.materialized_font_has_char(&primary, '▶') {
        debug!("skipping: platform monospace font does not cover U+25B6");
        return;
    }

    let symbol = svc
        .materialized_font_for_realized_face_char('▶', selection)
        .expect("covered symbol font");
    assert_eq!(symbol.font.id, primary.font.id);
    assert_eq!(symbol.resolution, FontResolutionSource::FacePrimary);

    let (_, published_fonts) = svc
        .resolve_cluster_uncached("▶", selection)
        .expect("resolved symbol cluster");
    let published = published_fonts
        .iter()
        .find(|font| font.id == primary.font.id)
        .expect("selected primary is published for the cluster");
    assert_eq!(*published, primary.font);
}

#[test]
fn realized_face_font_cache_identity_includes_primary_and_fontset_base() {
    let svc = make_svc();
    let base = RealizedFaceFontSelection::new(
        PrimaryFontFamily::new("Primary A"),
        FontsetBaseFamily::new("Base A"),
        400,
        false,
        13.0,
    );
    let different_primary = RealizedFaceFontSelection::new(
        PrimaryFontFamily::new("Primary B"),
        FontsetBaseFamily::new("Base A"),
        400,
        false,
        13.0,
    );
    let different_base = RealizedFaceFontSelection::new(
        PrimaryFontFamily::new("Primary A"),
        FontsetBaseFamily::new("Base B"),
        400,
        false,
        13.0,
    );

    let key = svc.realized_face_font_cache_key(base);
    assert_ne!(key, svc.realized_face_font_cache_key(different_primary));
    assert_ne!(key, svc.realized_face_font_cache_key(different_base));
}

#[test]
fn realized_face_complex_run_shapes_with_the_exact_materialized_fontset_font() {
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(NoCandidateFontBackend));
    let selection = RealizedFaceFontSelection::new(
        PrimaryFontFamily::new("Symbols Nerd Font Mono"),
        FontsetBaseFamily::new("JetBrainsMono Nerd Font"),
        400,
        false,
        13.0,
    );

    let materialized = svc
        .materialized_font_for_realized_face_char('क', selection)
        .expect("materialized fontset font");
    assert_eq!(materialized.font.family, "JetBrainsMono Nerd Font");
    assert_eq!(
        materialized.resolution,
        FontResolutionSource::FontsetFallback
    );
    let exact_family = match svc
        .build_attrs_for_materialized_font(&materialized)
        .expect("outline font attributes")
        .family
    {
        cosmic_text::Family::Name(name) => name.to_string(),
        family => panic!("exact font must have a pinned named family, got {family:?}"),
    };

    let observed_family = std::sync::Arc::new(std::sync::Mutex::new(None));
    svc.shaper = Box::new(RecordingFamilyShaper {
        observed_family: std::sync::Arc::clone(&observed_family),
    });

    svc.shape_run_for_realized_face("क्", selection);

    assert_eq!(
        observed_family.lock().unwrap().as_deref(),
        Some(exact_family.as_str()),
        "complex-run measurement must reuse the exact materialized fontset font"
    );
}

// ---------------------------------------------------------------
// fill_ascii_widths
// ---------------------------------------------------------------

#[test]
fn fill_ascii_widths_all_positive_for_printable() {
    let mut svc = make_svc();
    let widths = svc.fill_ascii_widths("monospace", 400, false, 14.0);
    // Printable ASCII (32-126) should all have positive widths
    for cp in 32u32..127 {
        assert!(
            widths[cp as usize] > 0.0,
            "width for ASCII {} ('{}') should be positive, got {}",
            cp,
            char::from_u32(cp).unwrap(),
            widths[cp as usize]
        );
    }
}

#[test]
fn fill_ascii_widths_control_chars_have_fallback() {
    let mut svc = make_svc();
    let widths = svc.fill_ascii_widths("monospace", 400, false, 14.0);
    // Control chars (0-31) should have space-width fallback
    let space_w = widths[32]; // space
    for cp in 0u32..32 {
        assert!(
            widths[cp as usize] > 0.0,
            "control char {} should have positive fallback width",
            cp
        );
        assert!(
            (widths[cp as usize] - space_w).abs() < 0.01,
            "control char {} width ({}) should match space width ({})",
            cp,
            widths[cp as usize],
            space_w
        );
    }
}

#[test]
fn frame_cell_metrics_for_monospace_use_ascii_max_not_space() {
    let mut widths = [3.56f32; 128];
    widths[b'M' as usize] = 9.0;
    widths[b'W' as usize] = 10.0;

    let advances = FontAdvanceMetrics::from_ascii_widths(3.56, &widths);
    let cell = FrameCellMetrics::derive(
        true,
        13.0,
        FontVerticalMetrics {
            ascent: 12.0,
            descent: 4.0,
            line_height: 16.0,
        },
        advances,
    );

    assert_eq!(cell.column_width, 10.0);
    assert_eq!(cell.confidence, MetricConfidence::Degraded);
}

#[test]
fn frame_cell_metrics_for_proportional_use_ascii_average_not_space() {
    let mut widths = [5.0f32; 128];
    widths[b' ' as usize] = 3.0;
    widths[b'i' as usize] = 2.0;
    widths[b'W' as usize] = 11.0;

    let advances = FontAdvanceMetrics::from_ascii_widths(3.0, &widths);
    let cell = FrameCellMetrics::derive(
        false,
        13.0,
        FontVerticalMetrics {
            ascent: 12.0,
            descent: 4.0,
            line_height: 16.0,
        },
        advances,
    );

    assert_eq!(cell.column_width, advances.average_width);
    assert_ne!(cell.column_width, advances.space_width);
    assert_ne!(cell.column_width, advances.max_width);
}

#[test]
fn frame_cell_metrics_keeps_realized_line_height() {
    // A healthy backend answer must pass through untouched.
    let widths = [8.0f32; 128];
    let advances = FontAdvanceMetrics::from_ascii_widths(8.0, &widths);
    let cell = FrameCellMetrics::derive(
        true,
        14.0,
        FontVerticalMetrics {
            ascent: 12.0,
            descent: 4.0,
            line_height: 18.0,
        },
        advances,
    );
    assert_eq!(cell.line_height, 18.0);
    assert_eq!(cell.ascent, 12.0);
    assert_eq!(cell.descent, 4.0);
}

#[test]
fn frame_cell_metrics_falls_back_when_backend_reports_no_valid_advances() {
    let widths = [0.0f32; 128];

    let advances = FontAdvanceMetrics::from_ascii_widths(0.0, &widths);
    let cell = FrameCellMetrics::derive(
        true,
        13.0,
        FontVerticalMetrics {
            ascent: 12.0,
            descent: 4.0,
            line_height: 16.0,
        },
        advances,
    );

    assert_eq!(cell.column_width, 13.0 * 0.6);
    assert_eq!(cell.confidence, MetricConfidence::Degraded);
}

#[test]
fn degraded_cell_width_uses_the_effective_opened_size() {
    let advances = FontAdvanceMetrics::from_ascii_widths(0.0, &[0.0; 128]);
    let cell = derive_observed_frame_cell_metrics(
        true,
        12.6,
        GraphicFontSizePx::new(13.0),
        FontVerticalMetrics {
            ascent: 10.0,
            descent: 3.0,
            line_height: 13.0,
        },
        advances,
    );

    assert_eq!(cell.column_width, 13.0 * 0.6);
    assert_ne!(cell.column_width, 12.6 * 0.6);
}

#[test]
fn unrealized_gui_font_size_never_publishes_placeholder_cell_metrics() {
    let mut service = FontMetricsService::new();

    // Issue #282: an early GUI redisplay can observe the default face before
    // its requested pixel size has been realized.  This test deliberately
    // enters through the public measurement seam: backend-local 1px clamps
    // must not turn the unrealized state into a publishable 1x2/1x3 cell.
    let FrameCellGeometry::Graphic(geometry) = service.frame_cell_geometry(
        "monospace",
        400,
        false,
        0.0,
        FrameFontDomain::for_frame(true, 16.0),
    ) else {
        panic!("a window-system frame must produce graphic geometry");
    };
    assert_eq!(geometry.font_size.get(), 16.0);

    assert!(
        geometry.metrics.char_width > 1.0,
        "unrealized GUI font escaped as a {}px-wide cell",
        geometry.metrics.char_width
    );
    assert!(
        geometry.metrics.line_height > 3.0,
        "unrealized GUI font escaped as a {}px-tall cell",
        geometry.metrics.line_height
    );
}

#[test]
fn frame_cell_geometry_keeps_graphic_and_terminal_domains_distinct() {
    let mut service = FontMetricsService::new();

    assert!(matches!(
        service.frame_cell_geometry(
            "monospace",
            400,
            false,
            16.0,
            FrameFontDomain::for_frame(false, 16.0),
        ),
        FrameCellGeometry::TerminalCell
    ));
}

#[test]
fn frame_cell_geometry_reuses_the_effective_size_from_its_metric_observation() {
    let mut service = FontMetricsService::new();
    let requested_size = 12.6;
    let key = MetricsCacheKey::new(
        "monospace",
        400,
        false,
        requested_size,
        neomacs_display_protocol::geometry::DeviceScale::new(1.0).expect("unit scale"),
    );
    let effective_size = service
        .materialized_font_for_face("monospace", 400, false, requested_size)
        .and_then(|font| font.px_metrics)
        .map(|metrics| metrics.pixel_size as f32)
        .expect("test font must expose probed pixel metrics");

    let _ = service.font_metrics("monospace", 400, false, requested_size);
    // Simulate the exact font becoming temporarily unavailable after metrics
    // were observed.  Frame publication must consume the cached observation,
    // not perform a second selection and pair a different size with it.
    service.resolved_face_font_cache.insert(key, None);

    let FrameCellGeometry::Graphic(geometry) = service.frame_cell_geometry(
        "monospace",
        400,
        false,
        requested_size,
        FrameFontDomain::for_frame(true, requested_size),
    ) else {
        panic!("a window-system frame must produce graphic geometry");
    };

    assert_eq!(geometry.font_size.get(), effective_size);
}

#[cfg(target_os = "linux")]
#[test]
fn selected_font_probe_observes_vertical_and_advance_metrics_at_one_size() {
    let mut service = FontMetricsService::new();
    let family = "monospace";
    let requested_size = 12.6;
    let key = MetricsCacheKey::new(
        family,
        400,
        false,
        requested_size,
        neomacs_display_protocol::geometry::DeviceScale::new(1.0).expect("unit scale"),
    );
    service.resolved_face_font_cache.insert(key.clone(), None);

    let FrameCellGeometry::Graphic(geometry) = service.frame_cell_geometry(
        family,
        400,
        false,
        requested_size,
        FrameFontDomain::for_frame(true, requested_size),
    ) else {
        panic!("a window-system frame must produce graphic geometry");
    };
    let observation = service.metrics_cache[&key];
    assert_eq!(observation.source, FontMetricSource::SelectedFontProbe);

    let (font_id, _) = service.selected_font_id_and_space_width(family, 400, false, requested_size);
    let face = service
        .font_system
        .db()
        .face(font_id.expect("selected font"))
        .expect("selected face");
    let probe = crate::font::probe::probe_font_px_metrics(
        &fontdb_face_file(face).expect("file-backed selected face"),
        face.index,
        geometry.font_size.get() as u32,
        None,
    )
    .expect("selected face metrics");
    let expected = FrameCellMetrics::derive(
        service.font_resolver.family_prefers_monospace(family),
        geometry.font_size.get(),
        FontVerticalMetrics {
            ascent: probe.ascent as f32,
            descent: probe.descent as f32,
            line_height: probe.height as f32,
        },
        FontAdvanceMetrics::from_font_probe(probe),
    );

    assert_eq!(geometry.metrics.char_width, expected.column_width);
    assert_eq!(geometry.metrics.space_width, probe.space_width as f32);
}

#[test]
fn fill_ascii_widths_cached() {
    let mut svc = make_svc();
    let w1 = svc.fill_ascii_widths("monospace", 400, false, 14.0);
    let w2 = svc.fill_ascii_widths("monospace", 400, false, 14.0);
    // Second call should return exact same values from cache
    for i in 0..128 {
        assert_eq!(w1[i], w2[i], "cache mismatch at index {i}");
    }
}

#[test]
fn fill_ascii_widths_different_sizes_differ() {
    let mut svc = make_svc();
    let w14 = svc.fill_ascii_widths("monospace", 400, false, 14.0);
    let w28 = svc.fill_ascii_widths("monospace", 400, false, 28.0);
    // At a larger size, 'A' (index 65) should be wider
    assert!(
        w28[65] > w14[65],
        "28px A ({}) should be wider than 14px A ({})",
        w28[65],
        w14[65]
    );
}

// ---------------------------------------------------------------
// font_metrics
// ---------------------------------------------------------------

#[test]
fn font_metrics_positive_values() {
    let mut svc = make_svc();
    let m = svc.font_metrics("monospace", 400, false, 14.0);
    assert!(
        m.ascent > 0.0,
        "ascent should be positive, got {}",
        m.ascent
    );
    assert!(
        m.descent > 0.0,
        "descent should be positive, got {}",
        m.descent
    );
    assert!(
        m.line_height > 0.0,
        "line_height should be positive, got {}",
        m.line_height
    );
    assert!(
        m.char_width > 0.0,
        "char_width should be positive, got {}",
        m.char_width
    );
}

#[test]
fn font_metrics_line_height_gte_ascent() {
    let mut svc = make_svc();
    let m = svc.font_metrics("monospace", 400, false, 14.0);
    assert!(
        m.line_height >= m.ascent,
        "line_height ({}) should be >= ascent ({})",
        m.line_height,
        m.ascent
    );
}

#[test]
fn font_metrics_scales_with_size() {
    let mut svc = make_svc();
    let m14 = svc.font_metrics("monospace", 400, false, 14.0);
    let m28 = svc.font_metrics("monospace", 400, false, 28.0);
    assert!(
        m28.char_width > m14.char_width,
        "28px char_width ({}) should be > 14px ({})",
        m28.char_width,
        m14.char_width
    );
    assert!(
        m28.line_height > m14.line_height,
        "28px line_height ({}) should be > 14px ({})",
        m28.line_height,
        m14.line_height
    );
}

#[test]
fn font_metrics_cached() {
    let mut svc = make_svc();
    let m1 = svc.font_metrics("monospace", 400, false, 14.0);
    let m2 = svc.font_metrics("monospace", 400, false, 14.0);
    assert_eq!(m1.ascent, m2.ascent);
    assert_eq!(m1.descent, m2.descent);
    assert_eq!(m1.char_width, m2.char_width);
    assert_eq!(m1.line_height, m2.line_height);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn font_metrics_match_selected_font_face_table_metrics() {
    let mut svc = make_svc();
    let family = "monospace";
    let weight = 400;
    let italic = false;
    let font_size = 14.0;
    let fm = svc.font_metrics(family, weight, italic, font_size);

    let (font_id, _) = svc.selected_font_id_and_space_width(family, weight, italic, font_size);
    let expected = svc
        .font_metrics_from_selected_face(font_id.expect("selected font id"), font_size)
        .expect("selected font table metrics");

    assert!(
        (fm.line_height - expected.line_height).abs() < 0.01,
        "expected line height {:.3}, got {:.3}",
        expected.line_height,
        fm.line_height
    );
    assert!(
        (fm.ascent - expected.ascent).abs() < 0.01,
        "expected ascent {:.3}, got {:.3}",
        expected.ascent,
        fm.ascent
    );
    assert!(
        (fm.descent - expected.descent).abs() < 0.01,
        "expected descent {:.3}, got {:.3}",
        expected.descent,
        fm.descent
    );
}

#[cfg(target_os = "macos")]
#[test]
fn font_metrics_match_the_selected_core_text_face() {
    let mut svc = make_svc();
    let family = "monospace";
    let weight = 400;
    let italic = false;
    let font_size = 14.0;
    let expected = svc
        .platform_primary_match(family, weight, italic, font_size)
        .and_then(|matched| matched.pixel_metrics(font_size))
        .expect("CoreText selected-face metrics");

    let actual = svc.font_metrics(family, weight, italic, font_size);

    assert_eq!(actual.ascent, expected.ascent as f32);
    assert_eq!(actual.descent, expected.descent as f32);
    assert_eq!(actual.line_height, expected.height as f32);
    assert_eq!(actual.space_width, expected.space_width as f32);
}

#[cfg(target_os = "linux")]
#[test]
fn selected_face_vertical_metrics_use_the_gnu_freetype_probe() {
    let mut svc = make_svc();
    let font_size = 10.0;
    let fixture = svc
        .font_system
        .db()
        .faces()
        .filter_map(|face| {
            let file = fontdb_face_file(face)?;
            let probed = crate::font::probe::probe_font_px_metrics(
                &file,
                face.index,
                font_size as u32,
                None,
            )?;
            let table = svc
                .font_system
                .db()
                .with_face_data(face.id, |data, index| {
                    let parsed = TtfFace::parse(data, index).ok()?;
                    let scale = font_size / f32::from(parsed.units_per_em().max(1));
                    Some((
                        (f32::from(parsed.ascender()) * scale).ceil(),
                        (f32::from(-parsed.descender()) * scale).ceil(),
                    ))
                })
                .flatten()?;
            ((table.0 as i32, table.1 as i32) != (probed.ascent, probed.descent))
                .then_some((face.id, file, probed))
        })
        .next()
        .expect("installed font whose hinted FreeType metrics differ from raw TTF rounding");

    let actual = svc
        .font_metrics_from_selected_face(fixture.0, font_size)
        .expect("selected font metrics");
    assert_eq!(
        (actual.ascent as i32, actual.descent as i32),
        (fixture.2.ascent, fixture.2.descent),
        "layout must use GNU/Cairo-compatible hinted metrics for {}",
        fixture.1
    );
}

#[cfg(unix)]
struct FixedPrimaryFontBackend {
    file: String,
    face_index: u32,
    slant: FontSlant,
    size: crate::font_backend::PlatformFontSize,
}

#[cfg(unix)]
struct FixedCharFontBackend {
    matched: crate::font_backend::PlatformFontCandidate,
}

#[cfg(unix)]
struct FixedNativeMemoryFontBackend {
    candidate: crate::font_backend::PlatformFontCandidate,
    asset: neomacs_display_protocol::font::FontMemoryAsset,
}

struct NoCandidateFontBackend;

struct NativeMetricsPrimaryBackend {
    candidate: crate::font_backend::PlatformFontCandidate,
    metrics: crate::font_backend::PlatformFontDesignMetrics,
}

impl crate::font_backend::FontBackend for NativeMetricsPrimaryBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::CoreText
    }

    fn list_families(&self) -> Vec<crate::font_backend::FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_owned()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        true
    }

    fn list_candidates(
        &self,
        _query: &crate::font_backend::FontCandidateQuery,
    ) -> Vec<crate::font_backend::FontCandidate> {
        vec![crate::font_backend::FontCandidate {
            matched: self.candidate.clone(),
        }]
    }

    fn design_metrics(
        &self,
        _matched: &crate::font_backend::PlatformFontMatch,
    ) -> Option<crate::font_backend::PlatformFontDesignMetrics> {
        Some(self.metrics)
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

struct ChangingCatalogBackend {
    pending: std::sync::Arc<std::sync::atomic::AtomicBool>,
    advances: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

struct RecordingFamilyShaper {
    observed_family: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl crate::text_shaper::TextShaper for RecordingFamilyShaper {
    fn shape_run(
        &mut self,
        _font_system: &mut cosmic_text::FontSystem,
        _text: &str,
        attrs: &cosmic_text::Attrs<'static>,
        _font_size: f32,
        _line_height: f32,
    ) -> Vec<ShapedGlyph> {
        let family = match attrs.family {
            cosmic_text::Family::Name(name) => name.to_string(),
            cosmic_text::Family::Serif => "serif".to_string(),
            cosmic_text::Family::SansSerif => "sans-serif".to_string(),
            cosmic_text::Family::Cursive => "cursive".to_string(),
            cosmic_text::Family::Fantasy => "fantasy".to_string(),
            cosmic_text::Family::Monospace => "monospace".to_string(),
        };
        *self.observed_family.lock().unwrap() = Some(family);
        Vec::new()
    }
}

impl crate::font_backend::FontBackend for NoCandidateFontBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }

    fn list_families(&self) -> Vec<crate::font_backend::FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_string()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        true
    }

    fn list_candidates(
        &self,
        _query: &crate::font_backend::FontCandidateQuery,
    ) -> Vec<crate::font_backend::FontCandidate> {
        Vec::new()
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

impl crate::font_backend::FontBackend for ChangingCatalogBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }

    fn list_families(&self) -> Vec<crate::font_backend::FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_owned()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        true
    }

    fn list_candidates(
        &self,
        _query: &crate::font_backend::FontCandidateQuery,
    ) -> Vec<crate::font_backend::FontCandidate> {
        Vec::new()
    }

    fn advance_catalog_generation(&mut self) {
        self.advances
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        if self
            .pending
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            crate::font::catalog::FontCatalogChange::Changed
        } else {
            crate::font::catalog::FontCatalogChange::Unchanged
        }
    }
}

#[test]
fn font_metrics_consumes_native_catalog_changes_once_at_a_safe_point() {
    let pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let advances = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(ChangingCatalogBackend {
            pending: std::sync::Arc::clone(&pending),
            advances: std::sync::Arc::clone(&advances),
        }));
    let initial = svc.font_catalog_generation();

    assert!(!svc.synchronize_font_catalog().changed());
    pending.store(true, std::sync::atomic::Ordering::Release);
    let update = svc.synchronize_font_catalog();
    assert!(update.changed());
    assert_eq!(update.generation(), initial.next());
    assert_eq!(advances.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert!(!svc.synchronize_font_catalog().changed());
    assert_eq!(advances.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[cfg(unix)]
impl crate::font_backend::FontBackend for FixedCharFontBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }

    fn list_families(&self) -> Vec<crate::font_backend::FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_string()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        true
    }

    fn list_candidates(
        &self,
        _query: &crate::font_backend::FontCandidateQuery,
    ) -> Vec<crate::font_backend::FontCandidate> {
        vec![crate::font_backend::FontCandidate {
            matched: self.matched.clone(),
        }]
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

#[cfg(unix)]
impl crate::font_backend::FontBackend for FixedPrimaryFontBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }

    fn list_families(&self) -> Vec<crate::font_backend::FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_string()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        false
    }

    fn list_candidates(
        &self,
        query: &crate::font_backend::FontCandidateQuery,
    ) -> Vec<crate::font_backend::FontCandidate> {
        vec![crate::font_backend::FontCandidate {
            matched: platform_file_candidate(
                ResolvedFontIdentity::from_file(&self.file, self.face_index, None),
                crate::font_backend::PlatformFontMetadata {
                    foundry: None,
                    family: match &query.scope {
                        crate::font_backend::FontCandidateScope::Family(family)
                        | crate::font_backend::FontCandidateScope::NativeFallback {
                            base_family: family,
                        } => family.as_str().to_owned(),
                        crate::font_backend::FontCandidateScope::All => "Fixture".to_owned(),
                    },
                    weight: Some(query.requested_weight),
                    slant: if query.requested_slant.is_italic() {
                        FontSlant::Italic
                    } else {
                        self.slant
                    },
                    width: Some(query.requested_width),
                    spacing: Some(100),
                    design_metrics: None,
                    size: self.size,
                },
            ),
        }]
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

#[cfg(unix)]
impl crate::font_backend::FontBackend for FixedNativeMemoryFontBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::CoreText
    }

    fn list_families(&self) -> Vec<crate::font_backend::FontFamilyName> {
        Vec::new()
    }

    fn resolve_family(&self, family: &str) -> String {
        family.to_owned()
    }

    fn family_prefers_monospace(&self, _family: &str) -> bool {
        true
    }

    fn list_candidates(
        &self,
        _query: &crate::font_backend::FontCandidateQuery,
    ) -> Vec<crate::font_backend::FontCandidate> {
        vec![crate::font_backend::FontCandidate {
            matched: self.candidate.clone(),
        }]
    }

    fn finalize_match(
        &self,
        matched: crate::font_backend::PlatformFontCandidate,
    ) -> Option<crate::font_backend::PlatformFontMatch> {
        matches!(
            &matched.locator,
            crate::font_backend::PlatformFontCandidateLocator::Native
        )
        .then(|| matched.into_memory_match(self.asset.clone()))
        .flatten()
    }

    fn advance_catalog_generation(&mut self) {}

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

#[cfg(unix)]
#[test]
fn explicit_primary_font_uses_fontconfig_static_file_when_cosmic_would_fallback() {
    let mut svc = make_svc();
    let requested_family = "neomacs-missing-primary-font-fixture";
    let cosmic_fallback = svc
        .cosmic_probe_file(requested_family, 400, false)
        .expect("cosmic fallback file");
    let target = svc
        .font_system
        .db()
        .faces()
        .filter_map(fontdb_face_file)
        .find(|file| {
            file != &cosmic_fallback
                && crate::font::probe::named_instance_wght_values(file, 0).is_empty()
        })
        .expect("different static font file");
    svc.font_resolver
        .replace_backend(Box::new(FixedPrimaryFontBackend {
            file: target.clone(),
            face_index: 0,
            slant: FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Scalable,
        }));
    let resolved = svc
        .resolved_font_for_char(' ', requested_family, 400, false, 10.0)
        .expect("resolved primary font");
    assert_eq!(
        resolved.identity.file_path.as_deref(),
        Some(target.as_str()),
        "fontconfig's primary file is GNU's authority even when it is static"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn resolved_face_preserves_an_exact_freetype_bitmap_realization() {
    use neomacs_display_protocol::font::{FontReplay, GlyphSampling};

    let fixture = test_font_path(neomacs_test_fonts::spleen_2_2_0().pcf_gz());
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(FixedPrimaryFontBackend {
            file: fixture.clone(),
            face_index: 0,
            slant: FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Unknown,
        }));
    let resolved = svc
        .resolved_font_for_face("Spleen", 400, false, 16.0)
        .expect("the exact bitmap face is a drawable realization");

    assert_eq!(
        resolved.identity.file_path.as_deref(),
        Some(fixture.as_str())
    );
    assert!(matches!(
        resolved.replay,
        FontReplay::FreeTypeBitmap {
            sampling: GlyphSampling::Nearest,
            ..
        }
    ));
    assert_eq!(resolved.ascent_px, 12.0);
    assert_eq!(resolved.descent_px, 4.0);
    assert_eq!(resolved.space_advance_px, 8.0);
    assert_eq!(
        svc.char_width('\u{a9}', "Spleen", 400, false, 16.0),
        8.0,
        "non-ASCII measurement must use the same exact bitmap face"
    );
    let selected = svc
        .select_font_for_char('\u{a9}', "Spleen", 400, false, 16.0)
        .expect("bitmap glyph selection");
    assert_eq!(selected.resolved.identity, resolved.identity);
    assert!(selected.glyph_code.is_some());
}

#[cfg(target_os = "linux")]
#[test]
fn fixed_bitmap_clusters_publish_exact_freetype_glyph_ids() {
    let fixture = test_font_path(neomacs_test_fonts::spleen_2_2_0().bdf());
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(FixedPrimaryFontBackend {
            file: fixture,
            face_index: 0,
            slant: FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Unknown,
        }));

    let (glyphs, fonts) = svc
        .resolved_glyphs_for_cluster("A©", "Spleen", 400, false, 16.0)
        .expect("bitmap cluster must cross the exact glyph replay boundary");

    assert_eq!(glyphs.len(), 2);
    assert_eq!(fonts.len(), 1);
    assert!(matches!(
        fonts[0].replay,
        neomacs_display_protocol::font::FontReplay::FreeTypeBitmap { .. }
    ));
    assert_eq!(glyphs[0].cluster_start, 0);
    assert_eq!(glyphs[0].cluster_end, 1);
    assert_eq!(glyphs[1].cluster_start, 1);
    assert_eq!(glyphs[1].cluster_end, 3);
    assert!(glyphs.iter().all(|glyph| glyph.glyph_id.get() > 0));
}

#[cfg(target_os = "linux")]
#[test]
fn bitmap_cluster_never_substitutes_notdef_for_an_uncovered_scalar() {
    let fixture = test_font_path(neomacs_test_fonts::spleen_2_2_0().bdf());
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(FixedPrimaryFontBackend {
            file: fixture,
            face_index: 0,
            slant: FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Unknown,
        }));

    let (glyphs, _) = svc
        .resolved_glyphs_for_cluster("©好", "Spleen", 400, false, 16.0)
        .expect("an uncovered scalar must take an explicit fallback path");

    assert!(
        glyphs.iter().all(|glyph| glyph.glyph_id.get() != 0),
        "GNU's font driver reports an invalid code and retries fallback; it never publishes .notdef"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bitmap_realization_publishes_the_selected_strikes_effective_logical_size() {
    let fixture = test_font_path(neomacs_test_fonts::spleen_2_2_0().otb());
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(FixedPrimaryFontBackend {
            file: fixture,
            face_index: 0,
            slant: FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Unknown,
        }));

    let resolved = svc
        .resolved_font_for_face("Spleen", 400, false, 11.0)
        .expect("nearest fixed strike");

    assert_eq!(resolved.pixel_size, 16.0);
}

#[cfg(unix)]
#[test]
fn resolved_face_preserves_an_exact_decoded_woff_realization() {
    let fixture = test_font_path(neomacs_test_fonts::spleen_2_2_0().woff());
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(FixedPrimaryFontBackend {
            file: fixture.clone(),
            face_index: 0,
            slant: FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Scalable,
        }));
    let platform = svc
        .platform_primary_match("Spleen 8x16", 400, false, 16.0)
        .expect("shared source adapter accepts WOFF");
    assert!(
        svc.fontdb_face_for_platform_match(&platform).is_some(),
        "decoded binary face id must survive pinning"
    );

    let resolved = svc
        .resolved_font_for_face("Spleen 8x16", 400, false, 16.0)
        .expect("decoded WOFF must remain materializable after selection");

    assert_eq!(
        resolved.identity.file_path.as_deref(),
        Some(fixture.as_str())
    );
    assert_eq!(resolved.replay, swash_replay_for(&resolved.identity));
}

#[cfg(unix)]
#[test]
fn url_less_native_candidate_publishes_its_finalized_memory_asset() {
    use neomacs_display_protocol::font::{FontMemoryAsset, FontOutlineAsset};
    use std::sync::Arc;

    let path = neomacs_test_fonts::spleen_2_2_0().woff();
    let mut db = fontdb::Database::new();
    let decoded =
        neomacs_font_materializer::FontFileCache::open_file(&mut db, &path.to_string_lossy(), 0)
            .expect("decode downloaded WOFF fixture");
    let bytes = decoded
        .into_iter()
        .find_map(|id| match &db.face(id)?.source {
            fontdb::Source::SharedFile(_, bytes) => Some(bytes.as_ref().as_ref().to_vec()),
            fontdb::Source::File(_) | fontdb::Source::Binary(_) => None,
        })
        .expect("decoded standalone SFNT bytes");
    let identity = ResolvedFontIdentity::from_native_with_variations(
        FontBackendKind::CoreText,
        "coretext:test:Spleen#0".to_owned(),
        0,
        Some("Spleen-8x16".to_owned()),
        Vec::new(),
    );
    let asset = FontMemoryAsset::new(identity.stable_key.clone(), Arc::new(bytes), 0)
        .expect("native-memory fixture");
    let candidate = crate::font_backend::PlatformFontCandidate {
        identity: identity.clone(),
        locator: crate::font_backend::PlatformFontCandidateLocator::Native,
        metadata: crate::font_backend::PlatformFontMetadata {
            foundry: None,
            family: "Native Spleen".to_owned(),
            weight: Some(400),
            slant: FontSlant::Normal,
            width: Some(FontWidth::Normal),
            spacing: Some(100),
            design_metrics: None,
            size: crate::font_backend::PlatformFontSize::Scalable,
        },
    };
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(FixedNativeMemoryFontBackend {
            candidate,
            asset: asset.clone(),
        }));

    let resolved = svc
        .resolved_font_for_face("Native Spleen", 400, false, 16.0)
        .expect("materialize URL-less native winner");

    assert_eq!(resolved.identity, identity);
    assert_eq!(
        resolved.replay,
        FontReplay::Swash {
            asset: FontOutlineAsset::Memory(asset)
        }
    );
}

#[cfg(unix)]
#[test]
fn scalable_color_bitmap_keeps_exact_platform_identity_at_requested_size() {
    let fixture = test_font_path(neomacs_test_fonts::noto_color_emoji_2_051().to_owned());
    let identity = ResolvedFontIdentity::from_file(&fixture, 0, Some("NotoColorEmoji".into()));
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(FixedCharFontBackend {
            matched: platform_file_candidate(
                identity.clone(),
                crate::font_backend::PlatformFontMetadata {
                    foundry: None,
                    family: "Noto Color Emoji".to_owned(),
                    weight: Some(400),
                    slant: FontSlant::Normal,
                    width: Some(FontWidth::Normal),
                    spacing: Some(100),
                    design_metrics: None,
                    // Fontconfig reports Noto Color Emoji as scalable even
                    // though FreeType exposes one CBDT/CBLC bitmap strike.
                    size: crate::font_backend::PlatformFontSize::Scalable,
                },
            ),
        }));

    let resolved = svc
        .resolved_font_for_char('\u{1f600}', "monospace", 400, false, 14.0)
        .expect("GNU/Cairo can realize the scalable color bitmap face");

    assert_eq!(resolved.identity, identity);
    assert_eq!(resolved.replay, swash_replay_for(&resolved.identity));
    assert_eq!(resolved.pixel_size, 14.0);
}

fn fixture_record(
    id: ResolvedFontId,
    identity: &ResolvedFontIdentity,
    replay: &FontReplay,
    pixel_size: f32,
    family: &str,
) -> ResolvedFont {
    ResolvedFont {
        id,
        identity: identity.clone(),
        replay: replay.clone(),
        family: family.to_owned(),
        full_name: None,
        postscript_name: None,
        weight: 400,
        slant: FontSlantKind::Normal,
        width: 5,
        pixel_size,
        ascent_px: pixel_size,
        descent_px: 0.0,
        space_advance_px: pixel_size * 0.6,
        glyph_advance: ResolvedFontAdvance::PerGlyph,
    }
}

#[test]
fn resolved_font_ids_name_a_complete_realized_instance() {
    use neomacs_display_protocol::font::{BitmapStrikeKey, FontReplay, GlyphSampling};

    let mut svc = make_svc();
    let identity = ResolvedFontIdentity::from_file("/fonts/fixed.pcf", 0, None);
    let strike = |index| FontReplay::FreeTypeBitmap {
        asset: neomacs_display_protocol::font::FontFileAsset::new("/fonts/fixed.pcf", 0)
            .expect("fixture path"),
        strike: BitmapStrikeKey {
            index,
            x_ppem_26_6: i64::from(8 + index) << 6,
            y_ppem_26_6: i64::from(13 + index) << 6,
        },
        sampling: GlyphSampling::Nearest,
        spacing: neomacs_display_protocol::font::FixedFontSpacing::MonospaceOrCharacterCell,
    };
    let intern = |svc: &mut FontMetricsService, replay: FontReplay, size: f32| {
        svc.intern_resolved_font(identity.clone(), replay, size, |id, identity, replay| {
            fixture_record(id, &identity, &replay, size, "fixed")
        })
        .id
    };

    let first = intern(&mut svc, strike(0), 13.0);
    assert_eq!(first, intern(&mut svc, strike(0), 13.0));
    assert_ne!(first, intern(&mut svc, strike(1), 14.0));
    assert_ne!(
        intern(&mut svc, swash_replay_for(&identity), 13.0),
        intern(&mut svc, swash_replay_for(&identity), 14.0),
        "metrics-bearing protocol entries at distinct sizes cannot share an id"
    );
}

#[test]
fn interner_owns_the_record_published_under_an_id() {
    // Four builders realize records independently; the interner must hand
    // every later builder the first record for the key, or the id-keyed frame
    // table can carry two records for one id across frames.
    let mut svc = make_svc();
    let identity = ResolvedFontIdentity::from_file("/fonts/one.ttf", 0, None);
    let replay = swash_replay_for(&identity);
    let first = svc.intern_resolved_font(
        identity.clone(),
        replay.clone(),
        12.0,
        |id, identity, replay| fixture_record(id, &identity, &replay, 12.0, "First Family"),
    );
    let second = svc.intern_resolved_font(identity, replay, 12.0, |id, identity, replay| {
        fixture_record(id, &identity, &replay, 12.0, "Second Family")
    });
    assert_eq!(second, first);
    assert_eq!(second.family, "First Family");
}

#[test]
fn catalog_advance_republishes_records_under_stable_ids() {
    // A file replaced in place keeps its id (frames keep referencing it) but
    // must republish its metrics: GNU `font_clear_cache` closes the objects in
    // `FONT_OBJLIST_INDEX` when the font set changes.
    let pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let advances = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(ChangingCatalogBackend {
            pending: std::sync::Arc::clone(&pending),
            advances: std::sync::Arc::clone(&advances),
        }));
    let identity = ResolvedFontIdentity::from_file("/fonts/replaced.ttf", 0, None);
    let replay = swash_replay_for(&identity);
    let before = svc.intern_resolved_font(
        identity.clone(),
        replay.clone(),
        12.0,
        |id, identity, replay| fixture_record(id, &identity, &replay, 12.0, "Version 1"),
    );
    let pinned = svc.intern_resolved_font(
        identity.clone(),
        replay.clone(),
        12.0,
        |id, identity, replay| fixture_record(id, &identity, &replay, 12.0, "Version 2"),
    );
    assert_eq!(
        pinned, before,
        "within one generation the first record stands"
    );

    pending.store(true, std::sync::atomic::Ordering::Release);
    assert!(svc.synchronize_font_catalog().changed());
    let after = svc.intern_resolved_font(identity, replay, 12.0, |id, identity, replay| {
        fixture_record(id, &identity, &replay, 12.0, "Version 2")
    });
    assert_eq!(after.id, before.id, "the id survives the catalog advance");
    assert_eq!(
        after.family, "Version 2",
        "the record is rebuilt from the new file"
    );
}

#[test]
fn char_first_realization_publishes_the_face_record() {
    // The per-character builder realizes the file before any face does (a
    // fallback-only frame); the pinned record must be the face path's.
    let mut reference = make_svc();
    let Some(face) = reference.resolved_font_for_face("Monospace", 400, false, 14.0) else {
        return;
    };
    let Some(ch) = ['é', 'ü', 'ñ', 'ö'].into_iter().find(|ch| {
        reference
            .resolved_font_for_char(*ch, "Monospace", 400, false, 14.0)
            .is_some_and(|font| font.id == face.id)
    }) else {
        debug!("skipping: platform monospace font covers none of the probe characters");
        return;
    };

    let mut svc = make_svc();
    let via_char = svc
        .resolved_font_for_char(ch, "Monospace", 400, false, 14.0)
        .expect("covered character");
    let via_face = svc
        .resolved_font_for_face("Monospace", 400, false, 14.0)
        .expect("realized primary font");
    assert_eq!(via_face.id, via_char.id);
    assert_eq!(
        via_face, face,
        "char-first realization must not change the face record"
    );
}

fn native_metrics_fixture_service(
    variations: Vec<neomacs_display_protocol::font::FontVariationCoord>,
) -> (
    FontMetricsService,
    fontdb::ID,
    String,
    crate::font_backend::PlatformFontDesignMetrics,
) {
    let fixture = test_font_path(neomacs_test_fonts::spleen_2_2_0().woff());
    let design_metrics = crate::font_backend::PlatformFontDesignMetrics {
        units_per_em: 1000,
        ascent: 1010,
        descent: 300,
        line_gap: 0,
        max_advance: 600,
        space_advance: 600,
        average_advance: 600,
    };
    let mut svc = make_svc();
    let ids = FontFileCache::open_file(svc.font_system.db_mut(), &fixture, 0)
        .expect("decode pinned font fixture");
    let fontdb_id = *ids.first().expect("fixture has a fontdb face");
    let face = svc.font_system.db().face(fontdb_id).expect("fixture face");
    let family = face.families[0].0.clone();
    let identity = ResolvedFontIdentity::from_platform_file_with_variations(
        FontBackendKind::CoreText,
        &fixture,
        face.index,
        Some(face.post_script_name.clone()),
        variations,
    );
    svc.font_resolver
        .replace_backend(Box::new(NativeMetricsPrimaryBackend {
            candidate: platform_file_candidate(
                identity,
                crate::font_backend::PlatformFontMetadata {
                    foundry: None,
                    family: family.clone(),
                    weight: Some(400),
                    slant: FontSlant::Normal,
                    width: Some(FontWidth::Normal),
                    spacing: Some(100),
                    // Exercise the resolver's native-metric enrichment,
                    // not metadata pre-attached to a catalog candidate.
                    design_metrics: None,
                    size: crate::font_backend::PlatformFontSize::Scalable,
                },
            ),
            metrics: design_metrics,
        }));
    (svc, fontdb_id, family, design_metrics)
}

#[test]
fn shaping_first_realization_publishes_the_native_face_metrics() {
    // Catch table metrics being interned before the resolver attaches native
    // metrics. The fixture and backend do not depend on installed system fonts.
    for size in [10.6, 14.0] {
        let (mut reference, _, family, design_metrics) = native_metrics_fixture_service(Vec::new());
        let expected = reference
            .materialized_font_for_face(&family, 400, false, size)
            .expect("fixture primary face");
        let native = design_metrics.at_pixel_size(size).expect("native metrics");
        assert_eq!(expected.font.ascent_px, native.ascent as f32);
        assert_eq!(expected.font.space_advance_px, native.space_width as f32);

        let (mut svc, fontdb_id, _, _) = native_metrics_fixture_service(Vec::new());
        let shaping = svc
            .resolved_font_from_fontdb_id(fontdb_id, size)
            .expect("shaping-selected fixture");
        let via_face = svc
            .materialized_font_for_face(&family, 400, false, size)
            .expect("primary after shaping");
        assert_eq!(shaping, via_face.font, "one id must publish one record");
        assert_eq!(
            via_face.font, expected.font,
            "shaping-first must preserve native metrics at {size}px"
        );
        let handle_metrics = via_face.px_metrics.expect("native handle metrics");
        assert_eq!(via_face.font.ascent_px, handle_metrics.ascent as f32);
        assert_eq!(via_face.font.descent_px, handle_metrics.descent as f32);
        assert_eq!(
            via_face.font.space_advance_px,
            handle_metrics.space_width as f32
        );
        assert_eq!(
            via_face.font.glyph_advance,
            resolved_font_advance(
                neomacs_display_protocol::font::FixedFontSpacing::MonospaceOrCharacterCell,
                Some(handle_metrics),
            )
        );
    }
}

#[test]
fn shaping_metrics_do_not_come_from_another_variation_of_the_same_file() {
    let variation = neomacs_display_protocol::font::FontVariationCoord::try_new(
        u32::from_be_bytes(*b"wght"),
        700.0,
    )
    .expect("finite weight coordinate");
    let (mut svc, fontdb_id, family, _) = native_metrics_fixture_service(vec![variation]);
    let size = 10.6;
    let selected = svc.font_system.db().face(fontdb_id).expect("fixture face");
    let identity = svc.fontdb_face_identity(
        fontdb_face_file(selected).as_deref(),
        selected.index,
        Some(selected.post_script_name.clone()),
        &family,
    );
    let expected = FontMetricsService::probe_resolved_font_metrics(&identity, None, size)
        .or_else(|| svc.font_px_metrics_from_selected_face(fontdb_id, size, &[]))
        .expect("selected file metrics");
    let shaping = svc
        .resolved_font_from_fontdb_id(fontdb_id, size)
        .expect("selected face");
    assert_eq!(shaping.identity, identity);
    assert_eq!(
        shaping.ascent_px, expected.ascent as f32,
        "same-file native metrics belong to a different variation identity"
    );
    assert_eq!(shaping.space_advance_px, expected.space_width as f32);
}

#[test]
fn shaping_selected_face_reuses_the_realized_face_record() {
    // When shaping lands on the face's own fontdb face, it must publish the
    // already interned record even if its diagnostic family metadata differs.
    let mut svc = make_svc();
    let Some(primary) = svc.materialized_font_for_face("Monospace", 400, false, 14.0) else {
        return;
    };
    let Some(fontdb_id) = primary.source.fontdb_id() else {
        debug!("skipping: primary face is a bitmap font");
        return;
    };
    let Some(from_shaping) = svc.resolved_font_from_fontdb_id(fontdb_id, 14.0) else {
        debug!("skipping: fontdb face has no replayable file");
        return;
    };
    if from_shaping.id != primary.font.id {
        debug!("skipping: fontdb identity differs from the platform identity");
        return;
    }
    assert_eq!(from_shaping, primary.font);
}

#[cfg(target_os = "linux")]
#[test]
fn ascii_character_resolution_keeps_primary_face_that_lacks_the_glyph() {
    let mut svc = make_svc();
    let primary = svc.font_system.db().faces().find_map(|face| {
        let file = fontdb_face_file(face)?;
        let lacks_ascii_space = svc
            .font_system
            .db()
            .with_face_data(face.id, |data, face_index| {
                TtfFace::parse(data, face_index)
                    .ok()
                    .is_some_and(|parsed| parsed.glyph_index(' ').is_none())
            })
            .unwrap_or(false);
        lacks_ascii_space.then_some((file, face.index))
    });
    let Some((file, face_index)) = primary else {
        return;
    };
    let requested_family = "neomacs-primary-without-ascii-space-fixture";
    svc.font_resolver
        .replace_backend(Box::new(FixedPrimaryFontBackend {
            file: file.clone(),
            face_index,
            slant: FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Scalable,
        }));

    let expected_metrics = crate::font::probe::probe_font_px_metrics(&file, face_index, 10, None)
        .expect("primary font metrics");
    let resolved = svc
        .resolved_font_for_char(' ', requested_family, 400, false, 10.0)
        .expect("GNU assigns ASCII to the primary face even when its glyph is unavailable");

    assert_eq!(resolved.identity.file_path.as_deref(), Some(file.as_str()));
    assert_eq!(resolved.identity.file_face_index(), face_index);
    assert_eq!(
        svc.materialized_font_for_realized_face_char(
            ' ',
            RealizedFaceFontSelection::same_fontset(requested_family, 400, false, 10.0),
        )
        .expect("materialized primary face")
        .resolution,
        FontResolutionSource::FacePrimary
    );

    let selected = svc
        .select_font_for_char(' ', requested_family, 400, false, 10.0)
        .expect("font-at selection keeps the GNU ASCII face");
    assert_eq!(
        selected.resolved.identity.file_path.as_deref(),
        Some(file.as_str())
    );
    assert_eq!(
        svc.char_width(' ', requested_family, 400, false, 10.0),
        expected_metrics.space_width as f32,
        "GNU measures missing ASCII with the primary font's .notdef advance"
    );
}

#[cfg(unix)]
#[test]
fn font_at_preserves_reverse_slant_from_platform_selection() {
    let mut svc = make_svc();
    let Some((file, face_index)) = svc
        .font_system
        .db()
        .faces()
        .find_map(|face| fontdb_face_file(face).map(|file| (file, face.index)))
    else {
        return;
    };
    svc.font_resolver
        .replace_backend(Box::new(FixedPrimaryFontBackend {
            file,
            face_index,
            slant: FontSlant::ReverseOblique,
            size: crate::font_backend::PlatformFontSize::Scalable,
        }));

    let selected = svc
        .select_font_for_char(
            ' ',
            "neomacs-reverse-slant-primary-fixture",
            400,
            false,
            10.0,
        )
        .expect("font-at selection");

    assert_eq!(selected.slant, FontSlant::ReverseOblique);
}

#[cfg(target_os = "linux")]
#[test]
fn installed_symbols_only_primary_font_uses_fontconfig_metrics_without_ascii_fallback() {
    let family = "Symbols Nerd Font Mono";
    let Some(platform) = crate::font::fontconfig::find_font_for_spec(
        Some(family),
        None,
        None,
        Some(FontWeight::Normal),
        Some(FontSlant::Normal),
        None,
    ) else {
        return;
    };
    if !platform.family.eq_ignore_ascii_case(family) {
        return;
    }
    let Some(expected_file) = platform.file else {
        return;
    };

    let expected = crate::font::probe::probe_font_px_metrics(&expected_file, 0, 10, None)
        .expect("fontconfig primary font should be measurable");
    let mut svc = make_svc();
    let actual = svc.font_metrics(family, 400, false, 10.0);

    assert_eq!(actual.ascent, expected.ascent as f32);
    assert_eq!(actual.descent, expected.descent as f32);
    assert_eq!(actual.line_height, expected.height as f32);
    assert_eq!(actual.space_width, expected.space_width as f32);
    assert_eq!(actual.char_width, expected.max_width as f32);
}

// ---------------------------------------------------------------
// bold / italic variants
// ---------------------------------------------------------------

#[test]
fn char_width_bold_vs_normal() {
    let mut svc = make_svc();
    let w_normal = svc.char_width('A', "DejaVu Sans", 400, false, 14.0);
    let w_bold = svc.char_width('A', "DejaVu Sans", 700, false, 14.0);
    // Both should be positive — bold may or may not be wider depending on font
    assert!(w_normal > 0.0, "normal width should be positive");
    assert!(w_bold > 0.0, "bold width should be positive");
}

#[cfg(target_os = "linux")]
#[test]
fn installed_iosevka_digit_advance_is_fixed_across_weights() {
    let resolver = crate::font::resolver::FontResolver::new(Box::new(
        crate::font_backend::FontconfigBackend::default(),
    ));
    let Some(normal) = resolver.resolve_primary(
        "Iosevka",
        400,
        FontSlant::Normal,
        FontWidth::Normal,
        crate::font_backend::FontSelectionSize::new(
            13.0,
            neomacs_display_protocol::geometry::DeviceScale::new(1.0).expect("unit scale"),
        ),
    ) else {
        return;
    };
    let Some(bold) = resolver.resolve_primary(
        "Iosevka",
        700,
        FontSlant::Normal,
        FontWidth::Normal,
        crate::font_backend::FontSelectionSize::new(
            13.0,
            neomacs_display_protocol::geometry::DeviceScale::new(1.0).expect("unit scale"),
        ),
    ) else {
        return;
    };
    assert_eq!(normal.family(), "Iosevka");
    assert_eq!(bold.family(), "Iosevka");

    let mut svc = make_svc();
    let normal_width = svc.char_width('2', "Iosevka", 400, false, 10.0);
    let bold_width = svc.char_width('2', "Iosevka", 700, false, 10.0);
    let normal_realized = svc.select_font_for_char('2', "Iosevka", 400, false, 10.0);
    let bold_realized = svc.select_font_for_char('2', "Iosevka", 700, false, 10.0);

    assert_eq!(
        normal_width, bold_width,
        "Iosevka is fixed-pitch across weights: platform regular={normal:?}, platform bold={bold:?}, shaped regular={normal_width} {normal_realized:?}, shaped bold={bold_width} {bold_realized:?}"
    );
}

#[test]
fn char_width_italic() {
    let mut svc = make_svc();
    let w = svc.char_width('A', "monospace", 400, true, 14.0);
    assert!(w > 0.0, "italic width should be positive, got {w}");
}

// ---------------------------------------------------------------
// clear_caches
// ---------------------------------------------------------------

#[test]
fn clear_caches_empties_all() {
    let mut svc = make_svc();
    // Populate caches
    svc.fill_ascii_widths("monospace", 400, false, 14.0);
    svc.char_width('漢', "monospace", 400, false, 14.0);
    svc.font_metrics("monospace", 400, false, 14.0);
    svc.shape_run("漢字", "monospace", 400, false, 14.0);

    assert!(!svc.ascii_cache.is_empty());
    assert!(!svc.char_cache.is_empty());
    assert!(!svc.metrics_cache.is_empty());
    assert!(!svc.shaped_run_cache.is_empty());

    svc.clear_caches();

    assert!(svc.ascii_cache.is_empty());
    assert!(svc.char_cache.is_empty());
    assert!(svc.metrics_cache.is_empty());
    assert!(svc.shaped_run_cache.is_empty());
}

// ---------------------------------------------------------------
// shaped-run cache: dedupes the measure-pass + render-pass double shape
// ---------------------------------------------------------------

#[test]
fn shape_run_caches_runs_to_dedupe_double_shape() {
    let mut svc = make_svc();
    // The measure pass and the render pass shape the same run with the same
    // face; the second must be a cache hit so cosmic-text shapes it once.
    let first = svc.shape_run("hello", "monospace", 400, false, 16.0);
    assert_eq!(svc.shape_calls(), 1, "first shape_run must actually shape");
    let second = svc.shape_run("hello", "monospace", 400, false, 16.0);
    assert_eq!(
        svc.shape_calls(),
        1,
        "second identical shape_run must hit the cache, not reshape"
    );
    assert_eq!(
        first, second,
        "cached shaping must equal the freshly shaped run"
    );
    assert!(!svc.shaped_run_cache.is_empty());
}

#[test]
fn shape_run_cache_keys_on_face_identity_not_just_text() {
    let mut svc = make_svc();
    // Same text, different font size: distinct faces must NOT share a cache
    // entry, or one face's advances would bleed into the other (the exact
    // advance-bleed bug class a text-only key would introduce).
    let small = svc.shape_run("WW", "monospace", 400, false, 12.0);
    assert_eq!(svc.shape_calls(), 1);
    let large = svc.shape_run("WW", "monospace", 400, false, 24.0);
    assert_eq!(
        svc.shape_calls(),
        2,
        "a different face must miss the cache, not reuse another size's shaping"
    );
    assert_eq!(svc.shaped_run_cache.len(), 2);

    // Re-requesting each face now hits its own entry (no further shaping).
    let small2 = svc.shape_run("WW", "monospace", 400, false, 12.0);
    let large2 = svc.shape_run("WW", "monospace", 400, false, 24.0);
    assert_eq!(svc.shape_calls(), 2, "both faces are now cached");
    assert_eq!(small, small2);
    assert_eq!(large, large2);

    // Sanity that the two cached entries are genuinely different shapings.
    let small_w: f32 = small.iter().map(|g| g.x_advance).sum();
    let large_w: f32 = large.iter().map(|g| g.x_advance).sum();
    assert!(
        large_w > small_w,
        "24px run must be wider than 12px (small={small_w}, large={large_w})"
    );

    // The key must distinguish ALL four MetricsCacheKey fields, not just size:
    // a key bug dropping weight/italic/family would let a bold/italic/serif run
    // reuse the regular run's shaping (advance-bleed). Discriminate on
    // shape_calls (exact: a key collision would be a hit, so no increment),
    // NOT width — bold/italic width is font-dependent (see
    // char_width_bold_vs_normal) and unreliable as a discriminator.
    let base = svc.shape_calls();
    svc.shape_run("ZZ", "monospace", 400, false, 16.0);
    assert_eq!(
        svc.shape_calls(),
        base + 1,
        "new (text, face) is a fresh shape"
    );
    svc.shape_run("ZZ", "monospace", 700, false, 16.0);
    assert_eq!(
        svc.shape_calls(),
        base + 2,
        "different weight must be a distinct cache entry"
    );
    svc.shape_run("ZZ", "monospace", 400, true, 16.0);
    assert_eq!(
        svc.shape_calls(),
        base + 3,
        "different italic must be a distinct cache entry"
    );
    svc.shape_run("ZZ", "serif", 400, false, 16.0);
    assert_eq!(
        svc.shape_calls(),
        base + 4,
        "different family must be a distinct cache entry"
    );
}

#[test]
fn shape_run_cache_clears_on_overflow_keeping_newest() {
    let mut svc = make_svc();
    svc.set_shaped_run_cache_cap(4);
    // Fill the cache to the cap with distinct runs.
    for s in ["a", "b", "c", "d"] {
        svc.shape_run(s, "monospace", 400, false, 16.0);
    }
    assert_eq!(svc.shaped_run_cache.len(), 4);

    // The next distinct run trips the cap: clear-then-insert keeps the cache
    // bounded and holds only the newest entry.
    let e = svc.shape_run("e", "monospace", 400, false, 16.0);
    assert_eq!(
        svc.shaped_run_cache.len(),
        1,
        "overflow clears the cache before inserting the newest run"
    );

    // The clear happened BEFORE the newest insert, so the just-shaped run is
    // retained: an immediate re-request is a hit, not a reshape.
    let calls = svc.shape_calls();
    let e2 = svc.shape_run("e", "monospace", 400, false, 16.0);
    assert_eq!(
        svc.shape_calls(),
        calls,
        "the newest run is retained after the overflow clear"
    );
    assert_eq!(e, e2);
}

#[test]
fn shape_run_cache_cleared_by_clear_caches() {
    let mut svc = make_svc();
    svc.shape_run("abc", "monospace", 400, false, 16.0);
    assert_eq!(svc.shape_calls(), 1);
    svc.shape_run("abc", "monospace", 400, false, 16.0);
    assert_eq!(svc.shape_calls(), 1, "cached");

    svc.clear_caches();
    assert!(svc.shaped_run_cache.is_empty());

    // After a font-change clear, the run must reshape (no stale cached glyphs).
    svc.shape_run("abc", "monospace", 400, false, 16.0);
    assert_eq!(
        svc.shape_calls(),
        2,
        "after clear_caches the run must reshape"
    );
}

// ---------------------------------------------------------------
// char_width consistency: individual vs fill_ascii
// ---------------------------------------------------------------

#[test]
fn char_width_matches_fill_ascii() {
    let mut svc = make_svc();
    // Get widths via fill_ascii_widths
    let widths = svc.fill_ascii_widths("JetBrains Mono", 400, false, 14.0);

    // Clear caches so char_width computes fresh
    svc.clear_caches();

    // Check that char_width for individual chars matches
    for cp in 32u32..127 {
        let ch = char::from_u32(cp).unwrap();
        let individual = svc.char_width(ch, "JetBrains Mono", 400, false, 14.0);
        let eps = 0.01;
        assert!(
            (individual - widths[cp as usize]).abs() < eps,
            "char_width('{}') = {} but fill_ascii_widths[{}] = {} (diff={})",
            ch,
            individual,
            cp,
            widths[cp as usize],
            (individual - widths[cp as usize]).abs()
        );
    }
}

// ---------------------------------------------------------------
// Print diagnostics (not a real assertion test, but useful
// for visually inspecting font resolution)
// ---------------------------------------------------------------

#[test]
fn diagnostic_print_widths() {
    let mut svc = make_svc();
    let families = [
        "monospace",
        "JetBrains Mono",
        "DejaVu Sans Mono",
        "DejaVu Sans",
    ];
    for family in families {
        let w_a = svc.char_width('A', family, 400, false, 14.0);
        let w_m = svc.char_width('M', family, 400, false, 14.0);
        let w_i = svc.char_width('i', family, 400, false, 14.0);
        let m = svc.font_metrics(family, 400, false, 14.0);
        debug!(
            "[font_metrics] {family:20} @ 14px: A={w_a:.2} M={w_m:.2} i={w_i:.2} | \
             ascent={:.2} descent={:.2} line_h={:.2} char_w={:.2}",
            m.ascent, m.descent, m.line_height, m.char_width
        );
    }
}

// ---------------------------------------------------------------
// Layout/render boundary verification.  Layout publishes an exact
// (font, glyph, advance) binding for every visible scalar; rendering must
// replay that answer instead of independently measuring the outline.
// ---------------------------------------------------------------

/// Measure through the resolved frame-glyph contract used by the renderer.
///
/// The second service remains an argument because these tests historically
/// exercised independent render-side shaping.  It is deliberately unused
/// now: independently measuring an outline would discard GNU-compatible
/// device hinting.  The renderer consumes `ResolvedCharGlyph::advance_px`
/// verbatim, as `GlyphAtlas::try_fast_single_char_glyph` does.
fn measure_with_resolved_fontsystem(
    layout: &mut FontMetricsService,
    _renderer: &mut FontMetricsService,
    ch: char,
    requested_family: &str,
    weight: u16,
    italic: bool,
    font_size: f32,
) -> f32 {
    let selection =
        RealizedFaceFontSelection::same_fontset(requested_family, weight, italic, font_size);
    let selected = layout
        .select_font_for_realized_face_char(ch, selection)
        .unwrap_or_else(|| {
        panic!(
            "layout must publish an exact font for render-boundary test: {} U+{:04X} family={requested_family} weight={weight} italic={italic}",
            ch.escape_default(),
            ch as u32
        )
    });
    if selected.glyph_code.is_none() && ch.is_ascii() {
        // GNU keeps unavailable ASCII on the primary face and advances its
        // missing-glyph box by that face's hinted space width.
        return selected.resolved.space_advance_px;
    }
    let published = neomacs_display_protocol::font::ResolvedCharGlyph {
        resolved_font_id: selected.resolved.id,
        glyph_id: neomacs_display_protocol::font::ResolvedGlyphId::new(
            selected
                .glyph_code
                .unwrap_or_else(|| panic!("{} has no publishable glyph", ch.escape_default())),
        ),
        advance_px: layout.char_width_for_realized_face(ch, selection),
    };
    published.advance_px
}

#[test]
fn two_fontsystems_produce_identical_widths() {
    // FontMetricsService (layout thread)
    let mut svc = make_svc();

    // Independent font system (simulating the render thread).
    let mut renderer = make_svc();

    let test_cases: &[(&str, u16)] = &[
        ("JetBrains Mono", 400),
        ("JetBrains Mono", 700),
        ("DejaVu Sans Mono", 400),
        ("DejaVu Sans", 400),
        ("monospace", 400),
    ];

    for &(family_str, weight) in test_cases {
        for cp in 32u32..127 {
            let ch = char::from_u32(cp).unwrap();
            let layout_w = svc.char_width(ch, family_str, weight, false, 14.0);
            let render_w = measure_with_resolved_fontsystem(
                &mut svc,
                &mut renderer,
                ch,
                family_str,
                weight,
                false,
                14.0,
            );
            assert_eq!(
                layout_w, render_w,
                "WIDTH MISMATCH: '{}' (U+{:04X}) in {} w{}: layout={} render={}",
                ch, cp, family_str, weight, layout_w, render_w
            );
        }
    }
}

#[test]
fn two_fontsystems_identical_for_cjk() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    let cjk_chars = ['漢', '字', '日', '本', '語', '中', '文'];
    for &ch in &cjk_chars {
        let layout_w = svc.char_width(ch, "monospace", 400, false, 14.0);
        let render_w = measure_with_resolved_fontsystem(
            &mut svc,
            &mut renderer,
            ch,
            "monospace",
            400,
            false,
            14.0,
        );
        assert_eq!(
            layout_w, render_w,
            "CJK WIDTH MISMATCH: '{}' (U+{:04X}): layout={} render={}",
            ch, ch as u32, layout_w, render_w
        );
    }
}

#[test]
fn explicit_mono_family_cjk_fallback_stays_wider_than_ascii() {
    let mut svc = make_svc();
    let ascii = svc.char_width('a', "Noto Sans Mono", 400, false, 14.0);
    let cjk = svc.char_width('好', "Noto Sans Mono", 400, false, 14.0);
    assert!(
        cjk > ascii * 1.2,
        "explicit mono CJK fallback should stay wider than ASCII: ascii={ascii} cjk={cjk}"
    );
}

#[test]
fn explicit_mono_family_cjk_matches_renderer_across_face_matrix_sizes() {
    let mut svc = make_svc();
    let mut renderer = make_svc();
    let families = [
        "JetBrains Mono",
        "Hack",
        "DejaVu Sans Mono",
        "Noto Sans Mono",
    ];
    let sizes = [24.0_f32, 26.666666_f32, 32.0_f32, 42.666668_f32];
    let weights = [400_u16, 600_u16, 700_u16, 800_u16];

    for family in families {
        for size in sizes {
            for weight in weights {
                let layout_w = svc.char_width('好', family, weight, false, size);
                let render_w = measure_with_resolved_fontsystem(
                    &mut svc,
                    &mut renderer,
                    '好',
                    family,
                    weight,
                    false,
                    size,
                );
                assert!(
                    (layout_w - render_w).abs() <= 0.01,
                    "CJK renderer/layout mismatch for family={family} weight={weight} size={size}: layout={layout_w} render={render_w}"
                );
            }
        }
    }
}

#[test]
fn select_font_for_char_reports_realized_face_metadata() {
    let mut svc = make_svc();
    let expected = svc
        .font_resolver
        .resolve_for_char(
            "JetBrains Mono",
            '好',
            800,
            FontSlant::Normal,
            FontWidth::Normal,
            svc.selection_size(24.0),
        )
        .expect("native fallback face");
    let selected = svc
        .select_font_for_char('好', "JetBrains Mono", 800, false, 24.0)
        .expect("selected font for fallback char");
    assert_eq!(
        selected.resolved.weight,
        expected.metadata.weight.unwrap_or(400)
    );
    assert!(selected.metrics.pixel_size > 0);
    assert!(selected.glyph_code.is_some());
}

#[test]
fn select_font_for_char_preserves_resolved_weight_for_variable_family_reports() {
    let mut svc = make_svc();
    if !svc.font_system.db().faces().any(|face| {
        face.families
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("Noto Sans Mono"))
    }) {
        return;
    }

    let selected = svc
        .select_font_for_char('A', "Noto Sans Mono", 600, false, 24.0)
        .expect("selected font for variable family");
    assert_eq!(selected.resolved.weight, 600);
}

#[test]
fn select_font_for_char_preserves_resolved_family_for_fallback_reports() {
    let mut svc = make_svc();
    let resolved = svc.font_request_for_char(
        '好',
        RealizedFaceFontSelection::new(
            PrimaryFontFamily::new("Noto Sans Mono"),
            FontsetBaseFamily::new("Noto Sans Mono"),
            400,
            false,
            13.0,
        ),
    );
    let selected = svc
        .select_font_for_char('好', "Noto Sans Mono", 400, false, 24.0)
        .expect("selected font for fallback char");
    assert_eq!(selected.resolved.family, resolved.family);
}

#[test]
fn select_font_for_char_resolves_generic_ascii_family() {
    let mut svc = make_svc();
    let expected = svc.font_resolver.resolve_family("Monospace");
    let selected = svc
        .select_font_for_char('A', "Monospace", 400, false, 24.0)
        .expect("selected font for ascii char");
    assert_eq!(selected.resolved.family, expected);
}

#[test]
fn two_fontsystems_identical_for_missing_font() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    // Fonts that definitely don't exist on the system
    let fake_families = [
        "NonExistentFont-XYZ-12345",
        "Comic Sans MS", // unlikely on NixOS
        "Papyrus",       // unlikely on NixOS
        "ThisFontDoesNotExist",
        "", // empty string
    ];

    for family_str in fake_families {
        for cp in 32u32..127 {
            let ch = char::from_u32(cp).unwrap();
            let layout_w = svc.char_width(ch, family_str, 400, false, 14.0);
            let render_w = measure_with_resolved_fontsystem(
                &mut svc,
                &mut renderer,
                ch,
                family_str,
                400,
                false,
                14.0,
            );
            assert_eq!(
                layout_w, render_w,
                "MISSING FONT MISMATCH: '{}' (U+{:04X}) in '{}': layout={} render={}",
                ch, cp, family_str, layout_w, render_w
            );
        }
        // Also check that fallback produces positive widths (not zero/garbage)
        let w = svc.char_width('A', family_str, 400, false, 14.0);
        assert!(
            w > 0.0,
            "missing font '{}' should still produce positive width, got {}",
            family_str,
            w
        );
    }
}

#[test]
fn two_fontsystems_identical_across_weights() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    // CSS font weights: 100=Thin, 200=ExtraLight, 300=Light,
    // 400=Normal, 500=Medium, 600=SemiBold, 700=Bold, 800=ExtraBold, 900=Black
    let weights: &[u16] = &[100, 200, 300, 400, 500, 600, 700, 800, 900];
    let families = ["JetBrains Mono", "DejaVu Sans", "monospace"];

    for family in families {
        for &weight in weights {
            for cp in 32u32..127 {
                let ch = char::from_u32(cp).unwrap();
                let layout_w = svc.char_width(ch, family, weight, false, 14.0);
                let render_w = measure_with_resolved_fontsystem(
                    &mut svc,
                    &mut renderer,
                    ch,
                    family,
                    weight,
                    false,
                    14.0,
                );
                assert_eq!(
                    layout_w, render_w,
                    "WEIGHT MISMATCH: '{}' in {} w{}: layout={} render={}",
                    ch, family, weight, layout_w, render_w
                );
            }
        }
    }
}

#[test]
fn two_fontsystems_identical_across_styles() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    let families = [
        "JetBrains Mono",
        "DejaVu Sans Mono",
        "DejaVu Sans",
        "monospace",
    ];
    let styles: &[(bool, &str)] = &[(false, "normal"), (true, "italic")];
    let weights: &[u16] = &[400, 700];

    for family in families {
        for &weight in weights {
            for &(italic, style_name) in styles {
                for cp in 32u32..127 {
                    let ch = char::from_u32(cp).unwrap();
                    let layout_w = svc.char_width(ch, family, weight, italic, 14.0);
                    let render_w = measure_with_resolved_fontsystem(
                        &mut svc,
                        &mut renderer,
                        ch,
                        family,
                        weight,
                        italic,
                        14.0,
                    );
                    assert_eq!(
                        layout_w, render_w,
                        "STYLE MISMATCH: '{}' in {} w{} {}: layout={} render={}",
                        ch, family, weight, style_name, layout_w, render_w
                    );
                }
            }
        }
    }
}

#[test]
fn two_fontsystems_identical_at_multiple_sizes() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    for font_size in [10.0, 14.0, 18.0, 24.0, 36.0] {
        for cp in 32u32..127 {
            let ch = char::from_u32(cp).unwrap();
            let layout_w = svc.char_width(ch, "JetBrains Mono", 400, false, font_size);
            let render_w = measure_with_resolved_fontsystem(
                &mut svc,
                &mut renderer,
                ch,
                "JetBrains Mono",
                400,
                false,
                font_size,
            );
            assert_eq!(
                layout_w, render_w,
                "SIZE MISMATCH: '{}' @ {}px: layout={} render={}",
                ch, font_size, layout_w, render_w
            );
        }
    }
}

// ---------------------------------------------------------------
// Buffer size parameter: FontMetricsService uses font_size*4.0
// but rasterize_text() uses font_size*8.0.  Verify this doesn't
// affect glyph.w values.
// ---------------------------------------------------------------

#[test]
fn buffer_size_does_not_affect_width() {
    let mut fs = FontSystem::new();
    let font_size = 14.0;
    let line_height = font_size * 1.3;
    let metrics = safe_metrics(font_size, line_height);
    let attrs = Attrs::new().family(Family::Monospace).weight(Weight(400));

    for cp in 32u32..127 {
        let ch = char::from_u32(cp).unwrap();
        let text = String::from(ch);

        // Small buffer (font_size * 4.0) — as in FontMetricsService
        let mut buf_small = Buffer::new(&mut fs, metrics);
        buf_small.set_size(&mut fs, Some(font_size * 4.0), Some(font_size * 2.0));
        buf_small.set_text(&mut fs, &text, &attrs, cosmic_text::Shaping::Advanced, None);
        buf_small.shape_until_scroll(&mut fs, false);
        let w_small = buf_small
            .layout_runs()
            .flat_map(|r| r.glyphs.iter())
            .next()
            .map(|g| g.w)
            .unwrap_or(0.0);

        // Large buffer (font_size * 8.0) — as in rasterize_text()
        let mut buf_large = Buffer::new(&mut fs, metrics);
        buf_large.set_size(&mut fs, Some(font_size * 8.0), Some(font_size * 3.0));
        buf_large.set_text(&mut fs, &text, &attrs, cosmic_text::Shaping::Advanced, None);
        buf_large.shape_until_scroll(&mut fs, false);
        let w_large = buf_large
            .layout_runs()
            .flat_map(|r| r.glyphs.iter())
            .next()
            .map(|g| g.w)
            .unwrap_or(0.0);

        assert_eq!(
            w_small, w_large,
            "BUFFER SIZE AFFECTS WIDTH: '{}' (U+{:04X}): small_buf={} large_buf={}",
            ch, cp, w_small, w_large
        );
    }
}

// ---------------------------------------------------------------
// Extreme font sizes
// ---------------------------------------------------------------

#[test]
fn two_fontsystems_identical_at_extreme_sizes() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    for font_size in [1.0, 4.0, 6.0, 72.0, 144.0] {
        for &ch in &['A', 'M', 'i', '.', ' '] {
            let layout_w = svc.char_width(ch, "monospace", 400, false, font_size);
            let render_w = measure_with_resolved_fontsystem(
                &mut svc,
                &mut renderer,
                ch,
                "monospace",
                400,
                false,
                font_size,
            );
            assert_eq!(
                layout_w, render_w,
                "EXTREME SIZE MISMATCH: '{}' @ {}px: layout={} render={}",
                ch, font_size, layout_w, render_w
            );
            assert!(
                layout_w > 0.0,
                "'{}' @ {}px should have positive width, got {}",
                ch,
                font_size,
                layout_w
            );
        }
    }
}

// ---------------------------------------------------------------
// Emoji
// ---------------------------------------------------------------

#[test]
fn two_fontsystems_identical_for_emoji() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    let emoji = ['😀', '🎉', '❤', '👍', '🔥', '⭐', '✅', '🚀'];
    for &ch in &emoji {
        let layout_w = svc.char_width(ch, "monospace", 400, false, 14.0);
        let render_w = measure_with_resolved_fontsystem(
            &mut svc,
            &mut renderer,
            ch,
            "monospace",
            400,
            false,
            14.0,
        );
        assert_eq!(
            layout_w, render_w,
            "EMOJI MISMATCH: '{}' (U+{:04X}): layout={} render={}",
            ch, ch as u32, layout_w, render_w
        );
        assert!(
            layout_w > 0.0,
            "emoji '{}' should have positive width, got {}",
            ch,
            layout_w
        );
    }
}

// ---------------------------------------------------------------
// Zero-width and special Unicode characters
// ---------------------------------------------------------------

#[test]
fn two_fontsystems_identical_for_zero_width_chars() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    let special: &[(char, &str)] = &[
        ('\u{200B}', "zero-width space"),
        ('\u{200C}', "ZWNJ"),
        ('\u{200D}', "ZWJ"),
        ('\u{FEFF}', "BOM/ZWNBSP"),
        ('\u{00AD}', "soft hyphen"),
    ];

    for &(ch, name) in special {
        let layout_w = svc.char_width(ch, "monospace", 400, false, 14.0);
        let render_w = measure_with_resolved_fontsystem(
            &mut svc,
            &mut renderer,
            ch,
            "monospace",
            400,
            false,
            14.0,
        );
        assert_eq!(
            layout_w, render_w,
            "SPECIAL CHAR MISMATCH: {} (U+{:04X}): layout={} render={}",
            name, ch as u32, layout_w, render_w
        );
    }
}

// ---------------------------------------------------------------
// RTL characters (Arabic, Hebrew)
// ---------------------------------------------------------------

#[test]
#[tracing_test::traced_test]
fn two_fontsystems_identical_for_rtl() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    let rtl: &[(char, &str)] = &[
        ('א', "Hebrew Alef"),
        ('ב', "Hebrew Bet"),
        ('ع', "Arabic Ain"),
        ('م', "Arabic Meem"),
        ('ش', "Arabic Sheen"),
    ];

    for &(ch, name) in rtl {
        let layout_w = svc.char_width(ch, "monospace", 400, false, 14.0);
        let render_w = measure_with_resolved_fontsystem(
            &mut svc,
            &mut renderer,
            ch,
            "monospace",
            400,
            false,
            14.0,
        );
        assert_eq!(
            layout_w, render_w,
            "RTL MISMATCH: {} '{}' (U+{:04X}): layout={} render={}",
            name, ch, ch as u32, layout_w, render_w
        );
        assert!(
            layout_w > 0.0,
            "RTL char {} should have positive width, got {}",
            name,
            layout_w
        );
    }
}

// ---------------------------------------------------------------
// Combining marks / diacritics
// ---------------------------------------------------------------

#[test]
fn two_fontsystems_identical_for_combining_marks() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    // Standalone combining marks — these may have zero advance (expected),
    // but both systems must agree
    let combining: &[(char, &str)] = &[
        ('\u{0300}', "combining grave"),
        ('\u{0301}', "combining acute"),
        ('\u{0302}', "combining circumflex"),
        ('\u{0308}', "combining diaeresis"),
        ('\u{0327}', "combining cedilla"),
        ('\u{0303}', "combining tilde"),
    ];

    for &(ch, name) in combining {
        let layout_w = svc.char_width(ch, "monospace", 400, false, 14.0);
        let render_w = measure_with_resolved_fontsystem(
            &mut svc,
            &mut renderer,
            ch,
            "monospace",
            400,
            false,
            14.0,
        );
        assert_eq!(
            layout_w, render_w,
            "COMBINING MISMATCH: {} (U+{:04X}): layout={} render={}",
            name, ch as u32, layout_w, render_w
        );
    }
}

// ---------------------------------------------------------------
// Mixed :height faces within a single line
//
// Simulates a line like:  normal(14px) LARGE(28px) small(10px) bold(14px)
// Each character has a different face with different font_size/weight.
// The layout engine measures each displayed glyph through its current
// display face. Verify that rapid switching between sizes/weights/families
// produces identical results in both systems.
// ---------------------------------------------------------------

/// Simulate a line with mixed face attributes, as the layout engine
/// would call char_width() while iterating through characters.
#[test]
fn two_fontsystems_identical_mixed_heights_in_line() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    // Each tuple: (char, family, weight, italic, font_size)
    // Simulates a real line: "Hello WORLD tiny Bold"
    // where each word has a different :height face
    let line: &[(char, &str, u16, bool, f32)] = &[
        // "Hello" — normal face, 14px
        ('H', "JetBrains Mono", 400, false, 14.0),
        ('e', "JetBrains Mono", 400, false, 14.0),
        ('l', "JetBrains Mono", 400, false, 14.0),
        ('l', "JetBrains Mono", 400, false, 14.0),
        ('o', "JetBrains Mono", 400, false, 14.0),
        (' ', "JetBrains Mono", 400, false, 14.0),
        // "WORLD" — heading face, :height 2.0 → 28px
        ('W', "JetBrains Mono", 700, false, 28.0),
        ('O', "JetBrains Mono", 700, false, 28.0),
        ('R', "JetBrains Mono", 700, false, 28.0),
        ('L', "JetBrains Mono", 700, false, 28.0),
        ('D', "JetBrains Mono", 700, false, 28.0),
        (' ', "JetBrains Mono", 700, false, 28.0),
        // "tiny" — small face, :height 0.8 → 11.2px
        ('t', "JetBrains Mono", 400, false, 11.2),
        ('i', "JetBrains Mono", 400, false, 11.2),
        ('n', "JetBrains Mono", 400, false, 11.2),
        ('y', "JetBrains Mono", 400, false, 11.2),
        (' ', "JetBrains Mono", 400, false, 14.0),
        // "Bold" — bold italic, same size
        ('B', "JetBrains Mono", 700, true, 14.0),
        ('o', "JetBrains Mono", 700, true, 14.0),
        ('l', "JetBrains Mono", 700, true, 14.0),
        ('d', "JetBrains Mono", 700, true, 14.0),
        // Switch to proportional mid-line
        (' ', "DejaVu Sans", 400, false, 14.0),
        ('v', "DejaVu Sans", 400, false, 14.0),
        ('a', "DejaVu Sans", 400, false, 14.0),
        ('r', "DejaVu Sans", 400, false, 14.0),
        // Back to monospace, different size
        (' ', "JetBrains Mono", 400, false, 18.0),
        ('e', "JetBrains Mono", 400, false, 18.0),
        ('n', "JetBrains Mono", 400, false, 18.0),
        ('d', "JetBrains Mono", 400, false, 18.0),
    ];

    let mut layout_total = 0.0f32;
    let mut render_total = 0.0f32;

    for (i, &(ch, family, weight, italic, font_size)) in line.iter().enumerate() {
        let layout_w = svc.char_width(ch, family, weight, italic, font_size);
        let render_w = measure_with_resolved_fontsystem(
            &mut svc,
            &mut renderer,
            ch,
            family,
            weight,
            italic,
            font_size,
        );

        assert_eq!(
            layout_w,
            render_w,
            "MIXED LINE MISMATCH at pos {}: '{}' ({} w{} {} {}px): layout={} render={}",
            i,
            ch,
            family,
            weight,
            if italic { "italic" } else { "normal" },
            font_size,
            layout_w,
            render_w
        );

        layout_total += layout_w;
        render_total += render_w;
    }

    // Total line width must also match exactly
    assert_eq!(
        layout_total, render_total,
        "MIXED LINE TOTAL WIDTH MISMATCH: layout={} render={}",
        layout_total, render_total
    );
}

/// Same test but with org-mode-like headings: *, **, *** at :height 3.0, 2.0, 1.5
#[test]
fn two_fontsystems_identical_org_heading_sizes() {
    let mut svc = make_svc();
    let mut renderer = make_svc();

    // Simulates org-mode: "* H1  ** H2  *** H3  body"
    // with decreasing :height per heading level
    let segments: &[(&str, &str, u16, f32)] = &[
        ("* ", "JetBrains Mono", 700, 42.0), // :height 3.0 → 42px
        ("H1 ", "JetBrains Mono", 700, 42.0),
        ("** ", "JetBrains Mono", 700, 28.0), // :height 2.0 → 28px
        ("H2 ", "JetBrains Mono", 700, 28.0),
        ("*** ", "JetBrains Mono", 700, 21.0), // :height 1.5 → 21px
        ("H3 ", "JetBrains Mono", 700, 21.0),
        ("body ", "JetBrains Mono", 400, 14.0),  // normal
        ("code", "DejaVu Sans Mono", 400, 14.0), // inline code, different font
    ];

    for (seg_text, family, weight, font_size) in segments {
        for ch in seg_text.chars() {
            let layout_w = svc.char_width(ch, family, *weight, false, *font_size);
            let render_w = measure_with_resolved_fontsystem(
                &mut svc,
                &mut renderer,
                ch,
                family,
                *weight,
                false,
                *font_size,
            );
            assert_eq!(
                layout_w, render_w,
                "ORG HEADING MISMATCH: '{}' in {} w{} {}px: layout={} render={}",
                ch, family, weight, font_size, layout_w, render_w
            );
        }
    }
}

// --- shape_run (gstring foundation) ---

#[test]
fn shape_run_returns_per_glyph_clusters_for_ascii() {
    let mut svc = make_svc();
    let glyphs = svc.shape_run("abc", "monospace", 400, false, 16.0);
    assert_eq!(glyphs.len(), 3, "ascii 'abc' should shape to 3 glyphs");
    // Each ASCII char is its own one-byte cluster, in logical order.
    assert_eq!((glyphs[0].cluster_start, glyphs[0].cluster_end), (0, 1));
    assert_eq!((glyphs[1].cluster_start, glyphs[1].cluster_end), (1, 2));
    assert_eq!((glyphs[2].cluster_start, glyphs[2].cluster_end), (2, 3));
    // Positive advances; the pen advances left to right for LTR text.
    assert!(glyphs.iter().all(|g| g.x_advance > 0.0));
    assert!(glyphs[2].x >= glyphs[0].x);
}

#[test]
fn shape_run_empty_text_is_empty() {
    let mut svc = make_svc();
    assert!(svc.shape_run("", "monospace", 400, false, 16.0).is_empty());
}

// ---------------------------------------------------------------------------
// resolved_font_for_face / realize_frame_fonts (render-boundary Phase 1)
// ---------------------------------------------------------------------------

#[test]
fn resolved_font_for_face_yields_an_exact_replayable_identity() {
    use neomacs_display_protocol::font::FontOutlineAsset;

    let mut svc = make_svc();
    let font = svc
        .resolved_font_for_face("Monospace", 400, false, 24.0)
        .expect("monospace face resolves to a concrete font");
    match font
        .replay
        .outline_asset()
        .expect("scalable font has an outline replay asset")
    {
        FontOutlineAsset::File(asset) => {
            assert!(
                std::path::Path::new(asset.path()).is_absolute(),
                "font file path must be absolute: {}",
                asset.path()
            );
            assert_eq!(
                font.identity.stable_key,
                format!("{}#{}", asset.path(), asset.face_index())
            );
        }
        FontOutlineAsset::Memory(asset) => {
            assert_eq!(asset.key(), font.identity.stable_key);
            assert_eq!(asset.face_index(), font.identity.backend_selector());
            assert!(!asset.bytes().is_empty());
        }
    }
    assert!(font.ascent_px > 0.0, "resolved font carries real metrics");
}

#[test]
fn resolved_font_for_face_matches_char_selection_for_ascii() {
    // The face-level identity must be the font the ASCII metrics/selection
    // path uses — that agreement is the whole point of the boundary design.
    let mut svc = make_svc();
    let face_font = svc
        .resolved_font_for_face("Monospace", 400, false, 24.0)
        .expect("face font");
    let char_font = svc
        .select_font_for_char('A', "Monospace", 400, false, 24.0)
        .expect("char font");
    assert_eq!(
        face_font.identity.file_path,
        char_font.resolved.identity.file_path
    );
    assert_eq!(
        face_font.postscript_name,
        char_font.resolved.postscript_name
    );
    assert_eq!(face_font.family, char_font.resolved.family);
}

#[test]
fn portable_metric_fallback_stays_on_the_selected_face() {
    let mut svc = make_svc();
    let selected = svc
        .materialized_font_for_face("Monospace", 400, false, 24.0)
        .expect("selected face");
    let metrics = svc
        .font_px_metrics_from_selected_face(
            selected.source.fontdb_id().expect("Swash face"),
            24.0,
            &selected.font.identity.variation_coords,
        )
        .expect("metrics from exact selected face data");

    assert_eq!(metrics.pixel_size, 24);
    assert!(metrics.height > 0);
    assert!(metrics.max_width > 0);
    assert!(metrics.average_width > 0);
}

#[cfg(target_os = "linux")]
#[test]
#[tracing_test::traced_test]
fn resolved_font_for_face_preserves_platform_named_instance() {
    use neovm_core::face::{FontSlant, FontWeight};

    let Some(platform) = crate::font::fontconfig::find_font_for_spec(
        Some("Noto Sans"),
        None,
        None,
        Some(FontWeight::Bold),
        Some(FontSlant::Normal),
        None,
    ) else {
        tracing::info!("skipping: Noto Sans is not installed");
        return;
    };
    let Some(platform_file) = platform.file.as_deref() else {
        tracing::info!("skipping: platform did not expose the matched font file");
        return;
    };
    let platform_postscript = platform
        .postscript_name
        .expect("an installed Fontconfig match must preserve its PostScript name");
    if !platform_postscript.to_ascii_lowercase().contains("bold") {
        tracing::info!("skipping: Noto Sans has no distinct bold instance");
        return;
    }
    if platform.face_index >> 16 == 0 {
        // The property under test is FreeType named-instance SELECTOR
        // decoding. When a static Bold face is installed (common alongside
        // the variable family), fontconfig legitimately prefers it and the
        // match carries no instance selector — there is nothing to decode.
        tracing::info!("skipping: platform Bold match is a static face, not a named instance");
        return;
    }

    let mut svc = make_svc();
    // Frame face realization currently primes the platform file before the
    // finished-frame font table is materialized.  The exact realization
    // contract must survive that ordinary pipeline ordering.
    svc.resolve_family("Noto Sans", Some(platform_file));
    let regular = svc
        .resolved_font_for_face("Noto Sans", 400, false, 12.0)
        .expect("Noto Sans Regular resolves before its derived bold face");
    assert_ne!(
        regular.postscript_name.as_deref(),
        Some(platform_postscript.as_str()),
        "test setup requires distinct regular and bold instances"
    );
    let resolved = svc
        .resolved_font_for_face("Noto Sans", 700, false, 12.0)
        .expect("Noto Sans Bold resolves to a concrete font instance");
    let selected = svc
        .select_font_for_char('A', "Noto Sans", 700, false, 12.0)
        .expect("font-at style query resolves the same concrete instance");

    assert_eq!(
        resolved.postscript_name.as_deref(),
        Some(platform_postscript.as_str()),
        "the realized face must preserve the platform-selected named instance"
    );
    assert_eq!(resolved.weight, 700);
    assert_eq!(
        resolved
            .identity
            .variation_coords
            .iter()
            .find(|coord| coord.tag() == u32::from_be_bytes(*b"wght"))
            .map(|coord| coord.value()),
        Some(700.0),
        "the FreeType named-instance selector must decode to its weight axis"
    );
    assert_eq!(
        selected.resolved.postscript_name.as_deref(),
        Some(platform_postscript.as_str()),
        "character selection must reuse the face's primary realization"
    );
    assert_eq!(selected.resolved.weight, 700);
    let metrics = svc.font_metrics("Noto Sans", 700, false, 12.0);
    let explicit_weight = resolved
        .identity
        .variation_coords
        .iter()
        .find(|coord| coord.tag() == u32::from_be_bytes(*b"wght"))
        .map(|coord| coord.value());
    let exact_metrics = crate::font::probe::probe_font_px_metrics(
        platform_file,
        resolved
            .identity
            .freetype_selector()
            .expect("Fontconfig identity has a FreeType selector"),
        12,
        explicit_weight,
    )
    .expect("FreeType opens the exact named instance");
    assert_eq!(metrics.ascent, exact_metrics.ascent as f32);
    assert_eq!(metrics.descent, exact_metrics.descent as f32);
    assert_eq!(metrics.line_height, exact_metrics.height as f32);
    assert_eq!(
        resolved.identity.backend_selector(),
        platform.face_index,
        "the exact platform named-instance index must cross the realization boundary"
    );
    assert_ne!(
        regular.identity, resolved.identity,
        "regular and bold instances in one variable-font file need distinct exact identities"
    );
    assert_ne!(
        regular.id, resolved.id,
        "the frame font table must not alias regular and bold instances"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn scaled_noto_cjk_bold_ascii_advances_match_gnu_device_glyph_metrics() {
    // GNU Emacs 31's Cairo/FreeType display of the real journal heading at
    // 168 DPI reports these glyph widths through `posn-at-point`: its 23px
    // Noto Sans CJK SC Bold face advances "2,Sinm" by 12, 5, 13, 6, 13,
    // and 20 device pixels respectively.  A 1.75-scale Neomacs frame must
    // publish those same device advances back in logical pixels.
    let scale = neomacs_display_protocol::geometry::DeviceScale::new(1.75)
        .expect("1.75 is a valid display scale");
    let mut svc = make_svc();
    svc.set_device_scale(scale);
    let family = "Noto Sans CJK SC";
    let selected = svc
        .materialized_font_for_face(family, 700, false, 13.0)
        .expect("the journal's proportional bold font is installed");
    let Some(file) = selected.font.identity.file_path.as_deref() else {
        eprintln!("skipping: selected Noto CJK face has no file identity");
        return;
    };
    if !file.contains("NotoSansCJK-VF.otf.ttc") {
        eprintln!("skipping: GNU capture used NotoSansCJK-VF.otf.ttc, got {file}");
        return;
    }

    let widths = svc.fill_ascii_widths(family, 700, false, 13.0);
    for (character, device_advance) in [
        ('2', 12.0),
        (',', 5.0),
        ('S', 13.0),
        ('i', 6.0),
        ('n', 13.0),
        ('m', 20.0),
    ] {
        let expected = device_advance / scale.get();
        assert!(
            (widths[character as usize] - expected).abs() < 0.001,
            "{character:?}: GNU advance is {device_advance}px device / {} = {expected}px logical, got {}",
            scale.get(),
            widths[character as usize]
        );
    }
}

#[test]
fn resolved_font_ids_are_stable_across_cache_clear() {
    let mut svc = make_svc();
    let before = svc
        .resolved_font_for_face("Monospace", 400, false, 24.0)
        .expect("face font");
    svc.clear_caches();
    let after = svc
        .resolved_font_for_face("Monospace", 400, false, 24.0)
        .expect("face font after clear");
    assert_eq!(before.id, after.id, "interned id survives clear_caches");
    assert_eq!(before.identity, after.identity);

    // A different instance (bold) gets a different identity only when the
    // database actually has a distinct bold face; either way the regular
    // id maps back to the same identity.
    let again = svc
        .resolved_font_for_face("Monospace", 400, false, 24.0)
        .expect("face font repeat");
    assert_eq!(again.id, before.id);
}

#[test]
fn realize_frame_fonts_publishes_face_identity_and_table() {
    use neomacs_display_protocol::face::Face;
    use neomacs_display_protocol::glyph_matrix::FrameDisplayState;

    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    let mut default_face = Face::new(FaceId::new(0));
    default_face.font_family = "Monospace".to_string();
    default_face.font_size = 24.0;
    state.faces.insert(FaceId::new(0), default_face);
    let mut bold_face = Face::new(FaceId::new(21));
    bold_face.font_family = "Monospace".to_string();
    bold_face.font_size = 24.0;
    bold_face.font_weight = 700;
    state.faces.insert(FaceId::new(21), bold_face);

    let mut service = Some(make_svc());
    let generation = service
        .as_ref()
        .expect("GUI font service")
        .font_catalog_generation();
    realize_frame_fonts(&mut state, &mut service);
    assert_eq!(state.font_catalog_generation, generation);

    for face_id in [FaceId::new(0), FaceId::new(21)] {
        let face = &state.faces[&face_id];
        let font_id = face
            .default_resolved_font_id
            .unwrap_or_else(|| panic!("face {face_id} got a resolved font id"));
        let font = state
            .fonts
            .get(&font_id)
            .unwrap_or_else(|| panic!("frame font table has face {face_id}'s font"));
        assert_eq!(font.id, font_id);
        assert_eq!(
            face.font_file_path, font.identity.file_path,
            "font_file_path bridge mirrors the resolved identity"
        );
    }

    // TTY frames (no service) must stay untouched.
    let mut tty_state = FrameDisplayState::new(80, 24, 1.0, 1.0);
    tty_state
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));
    let mut no_service: Option<FontMetricsService> = None;
    realize_frame_fonts(&mut tty_state, &mut no_service);
    assert!(tty_state.fonts.is_empty());
    assert_eq!(
        tty_state.faces[&FaceId::new(0)].default_resolved_font_id,
        None
    );
}

#[cfg(target_os = "linux")]
#[test]
#[tracing_test::traced_test]
fn realize_frame_fonts_resolves_an_installed_symbols_only_primary_face() {
    use neomacs_display_protocol::face::Face;
    use neomacs_display_protocol::glyph_matrix::FrameDisplayState;

    let family = "Symbols Nerd Font Mono";
    let Some(platform) = crate::font::fontconfig::find_font_for_spec(
        Some(family),
        None,
        None,
        Some(FontWeight::Normal),
        Some(FontSlant::Normal),
        None,
    ) else {
        return;
    };
    if !platform.family.eq_ignore_ascii_case(family) {
        return;
    }
    let Some(expected_file) = platform.file else {
        return;
    };

    let face_id = FaceId::new(226);
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    let mut face = Face::new(face_id);
    face.font_family = family.to_string();
    face.font_size = 10.0;
    state.faces.insert(face_id, face);

    let mut service = Some(make_svc());
    realize_frame_fonts(&mut state, &mut service);

    let resolved_id = state.faces[&face_id]
        .default_resolved_font_id
        .expect("an installed symbols-only primary font crosses the GUI boundary");
    assert_eq!(
        state.fonts[&resolved_id].identity.file_path.as_deref(),
        Some(expected_file.as_str()),
        "primary realization must publish Fontconfig's exact symbols font"
    );
    assert!(
        !logs_contain("GUI face has no resolvable primary font"),
        "a resolvable installed font must not fall back to render-side font selection"
    );
}

/// Not a pass/fail perf gate (wall-clock asserts flake in CI) — this prints
/// the steady-state cost of the per-frame font realization pass so its
/// evaluator-thread overhead is measurable on demand:
/// `cargo nextest run -p neomacs-layout-engine realize_frame_fonts_steady --no-capture`
#[test]
fn realize_frame_fonts_steady_state_cost_probe() {
    use neomacs_display_protocol::face::Face;
    use neomacs_display_protocol::glyph_matrix::FrameDisplayState;

    let mut service = Some(make_svc());
    let build_state = || {
        let mut state = FrameDisplayState::new(120, 40, 8.0, 16.0);
        // 40 faces across a few realistic attr combos (default, bold,
        // italic, size variants) — a typical GUI frame's face table.
        for id in (0..40u32).map(FaceId::new) {
            let mut face = Face::new(id);
            face.font_family = "Monospace".to_string();
            face.font_size = 14.0 + (id.get() % 3) as f32;
            face.font_weight = if id.get() % 4 == 0 { 700 } else { 400 };
            state.faces.insert(id, face);
        }
        state
    };

    // Cold pass: pays the one-time probe-shape per unique attr combo.
    let cold_start = std::time::Instant::now();
    let mut state = build_state();
    realize_frame_fonts(&mut state, &mut service);
    let cold = cold_start.elapsed();
    assert!(
        state
            .faces
            .values()
            .all(|f| f.default_resolved_font_id.is_some())
    );

    // Steady state: what every subsequent redisplay pays on the eval thread.
    const FRAMES: u32 = 1000;
    let warm_start = std::time::Instant::now();
    for _ in 0..FRAMES {
        let mut state = build_state();
        realize_frame_fonts(&mut state, &mut service);
    }
    let warm = warm_start.elapsed();
    // Subtract the state-construction baseline so the printed number is the
    // realization pass alone.
    let base_start = std::time::Instant::now();
    for _ in 0..FRAMES {
        let state = build_state();
        std::hint::black_box(&state);
    }
    let base = base_start.elapsed();
    eprintln!(
        "realize_frame_fonts: cold(first frame, 40 faces) = {cold:?}; \
         steady state = {:?}/frame incl. state build, {:?}/frame realization only \
         ({FRAMES} frames, 40 faces/frame)",
        warm / FRAMES,
        warm.saturating_sub(base) / FRAMES
    );
}

#[test]
fn realize_frame_char_fonts_stamps_cjk_fallback() {
    use neomacs_display_protocol::face::Face;
    use neomacs_display_protocol::glyph_matrix::{
        FrameDisplayState, Glyph, GlyphArea, GlyphMatrix, WindowMatrixEntry,
    };
    use neomacs_display_protocol::types::Rect;

    let mut state = FrameDisplayState::new(20, 1, 8.0, 16.0);
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "Monospace".to_string();
    face.font_size = 14.0;
    state.faces.insert(FaceId::new(0), face);

    let mut matrix = GlyphMatrix::new(1, 20);
    neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).enabled = true;
    for (i, ch) in "a好b".chars().enumerate() {
        neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs
            [GlyphArea::Text as usize]
            .push(Glyph::char(ch, FaceId::new(0), i));
    }
    state.window_matrices.push(WindowMatrixEntry {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 160.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 160.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut service = Some(make_svc());
    realize_frame_fonts(&mut state, &mut service);

    // Every visible scalar carries its exact font/glyph answer. This makes
    // renderer replay a pure lookup for both primary ASCII and fallback CJK.
    let by_char = state
        .char_fonts
        .get(&FaceId::new(0))
        .expect("face 0 has char fallback entries");
    assert!(by_char.contains_key(&'a'));
    let binding = by_char.get(&'好').copied().expect("好 resolved");
    let font_id = binding.resolved_font_id;
    let font = state
        .fonts
        .get(&font_id)
        .expect("char fallback font published in frame font table");
    assert_eq!(font.id, font_id);
    // The layout-side selection agrees with the char-selection oracle path.
    let selected = service
        .as_mut()
        .unwrap()
        .select_font_for_char('好', "Monospace", 400, false, 14.0)
        .expect("char font");
    assert_eq!(
        font.identity.file_path,
        selected.resolved.identity.file_path
    );
}

#[cfg(target_os = "linux")]
#[test]
#[tracing_test::traced_test]
fn resolved_font_for_char_preserves_platform_collection_face() {
    let platform =
        crate::font::fontconfig::match_font_for_char("Monospace", '好', true, 400, false)
            .expect("installed CJK fallback");
    let Some(platform_file) = platform.file.as_deref() else {
        tracing::info!("skipping: fontconfig CJK fallback has no file identity");
        return;
    };
    if platform.face_index == 0 {
        tracing::info!("skipping: installed CJK fallback is not a collection face");
        return;
    }

    let mut svc = make_svc();
    let resolved = svc
        .resolved_font_for_char('好', "Monospace", 400, false, 14.0)
        .expect("resolved CJK fallback");

    assert_eq!(resolved.identity.file_path.as_deref(), Some(platform_file));
    assert_eq!(
        resolved.identity.backend_selector(),
        platform.face_index,
        "the layout answer must preserve Fontconfig's exact TTC face"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn resolved_font_for_char_treats_platform_identity_as_authoritative() {
    let platform =
        crate::font::fontconfig::match_font_for_char("Monospace", '好', true, 400, false)
            .expect("installed CJK fallback");
    let platform_file = platform.file.expect("fontconfig file identity");
    let identity = ResolvedFontIdentity::from_file_with_variations(
        &platform_file,
        platform.face_index,
        platform.postscript_name.clone(),
        platform.variation_coords,
    );
    let expected_identity = identity.clone();

    let mut svc = make_svc();
    svc.font_resolver
        .replace_backend(Box::new(FixedCharFontBackend {
            matched: platform_file_candidate(
                identity,
                crate::font_backend::PlatformFontMetadata {
                    foundry: None,
                    // Native backends may publish a selector/display name
                    // unknown to fontdb. The exact identity must still win.
                    family: "neomacs-platform-display-alias".to_string(),
                    weight: platform.weight,
                    slant: platform.slant,
                    width: Some(FontWidth::Normal),
                    spacing: None,
                    design_metrics: None,
                    size: crate::font_backend::PlatformFontSize::Scalable,
                },
            ),
        }));

    let resolved = svc
        .resolved_font_for_char('好', "Monospace", 400, false, 14.0)
        .expect("resolved exact platform fallback");

    assert_eq!(resolved.identity, expected_identity);
}

#[test]
fn representative_char_policy_matches_render_side_expectations() {
    use crate::composition::representative_char_for_cluster;
    assert_eq!(representative_char_for_cluster("abc"), None);
    assert_eq!(representative_char_for_cluster("a好b"), Some('好'));
    // Emoji presentation selector forces the canonical emoji probe.
    assert_eq!(
        representative_char_for_cluster("1\u{FE0F}\u{20E3}"),
        Some('\u{1F600}')
    );
    // Joiners alone don't determine a font.
    assert_eq!(representative_char_for_cluster("a\u{200D}b"), None);
}

#[test]
fn resolved_glyphs_for_cluster_publishes_exact_glyphs() {
    let mut svc = make_svc();
    // A combining-mark cluster: base 'e' + COMBINING ACUTE ACCENT. Shapes to
    // one or two glyphs depending on the font; every glyph must reference an
    // interned font in the returned font list.
    let (glyphs, fonts) = svc
        .resolved_glyphs_for_cluster("e\u{0301}", "Monospace", 400, false, 24.0)
        .expect("cluster shapes");
    assert!(!glyphs.is_empty());
    for glyph in &glyphs {
        assert!(
            fonts.iter().any(|f| f.id == glyph.resolved_font_id),
            "glyph font {:?} present in returned fonts",
            glyph.resolved_font_id
        );
        assert!(glyph.x_advance >= 0.0);
    }
    // Deterministic across calls (cache or not).
    let (again, _) = svc
        .resolved_glyphs_for_cluster("e\u{0301}", "Monospace", 400, false, 24.0)
        .expect("cluster shapes again");
    assert_eq!(glyphs, again);
}

#[test]
fn realize_frame_fonts_publishes_shaped_clusters_for_composites() {
    use neomacs_display_protocol::face::Face;
    use neomacs_display_protocol::glyph_matrix::{
        FrameDisplayState, Glyph, GlyphArea, GlyphMatrix, GlyphType, WindowMatrixEntry,
    };
    use neomacs_display_protocol::types::Rect;

    let mut state = FrameDisplayState::new(20, 1, 8.0, 16.0);
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "Monospace".to_string();
    face.font_size = 14.0;
    state.faces.insert(FaceId::new(0), face);

    let mut matrix = GlyphMatrix::new(1, 20);
    neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).enabled = true;
    let mut composite = Glyph::char('e', FaceId::new(0), 0);
    composite.glyph_type = GlyphType::Composite {
        text: "e\u{0301}".into(),
    };
    neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs
        [GlyphArea::Text as usize]
        .push(composite);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 160.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 160.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut service = Some(make_svc());
    realize_frame_fonts(&mut state, &mut service);

    let glyphs = state
        .shaped_clusters
        .get(&FaceId::new(0))
        .and_then(|by_text| by_text.get("e\u{0301}"))
        .expect("composite cluster published");
    assert!(!glyphs.is_empty());
    for glyph in glyphs {
        assert!(
            state.fonts.contains_key(&glyph.resolved_font_id),
            "cluster glyph font in frame font table"
        );
    }
}

#[test]
fn pin_file_as_family_forces_cosmic_to_that_exact_file() {
    let variable = "/nix/store/7lrhms8rphrd8ywphjbvjyll57pkim64-noto-fonts-2025.11.01/share/fonts/noto/NotoSans[wdth,wght].ttf";
    if !std::path::Path::new(variable).exists() {
        eprintln!("skipping: {variable} not present");
        return;
    }
    let mut svc = make_svc();

    // Baseline: a plain bold "Noto Sans" request. With a static
    // NotoSans-Bold.ttf present, cosmic may pick the static file.
    let baseline = svc
        .select_font_for_char('n', "Noto Sans", 700, false, 24.0)
        .and_then(|s| s.resolved.identity.file_path);

    // Pin the VARIABLE file under a synthetic family and shape through it.
    let synthetic = svc
        .pin_file_as_family(variable, 0)
        .expect("pin the variable file");
    let attrs = cosmic_text::Attrs::new()
        .family(cosmic_text::Family::Name(synthetic))
        .weight(cosmic_text::Weight(700));
    let metrics = safe_metrics(24.0, 24.0 * 1.3);
    let mut buffer = cosmic_text::Buffer::new(&mut svc.font_system, metrics);
    buffer.set_size(&mut svc.font_system, Some(96.0), Some(48.0));
    buffer.set_text(
        &mut svc.font_system,
        "n",
        &attrs,
        cosmic_text::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(&mut svc.font_system, false);
    let font_id = buffer
        .layout_runs()
        .find_map(|run| run.glyphs.iter().next())
        .map(|g| g.physical((0.0, 0.0), 1.0).cache_key.font_id)
        .expect("shaped a glyph");
    let pinned_file = svc
        .font_system
        .db()
        .face(font_id)
        .and_then(fontdb_face_file);

    assert_eq!(
        pinned_file.as_deref(),
        Some(variable),
        "pinned shaping must select the exact variable file (baseline picked {baseline:?})"
    );

    // Re-pinning the same (file, index) reuses the synthetic family.
    let synthetic2 = svc.pin_file_as_family(variable, 0).unwrap();
    assert_eq!(synthetic, synthetic2);
}

#[test]
fn pin_file_as_family_opens_deterministic_woff_selected_by_fontconfig() {
    let webfont = test_font_path(neomacs_test_fonts::spleen_2_2_0().woff());

    let mut svc = make_svc();
    assert!(
        svc.pin_file_as_family(&webfont, 0).is_some(),
        "the exact-font path must use the same container decoder as ordinary font loading"
    );
}
