//! Native filtering may legitimately leave no scalable faces. Public layout
//! requests must retain their missing-font contracts rather than panic inside
//! cosmic-text or rediscover fonts that the native configuration rejected.
use super::*;

fn empty_service() -> FontMetricsService {
    FontMetricsService::from_font_system_and_resolver(
        FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new()),
        crate::font::resolver::FontResolver::new(Box::new(NoCandidateFontBackend)),
    )
}

#[test]
fn empty_catalog_shaper_returns_no_glyphs() {
    use crate::text_shaper::{CosmicTextShaper, TextShaper};
    let mut fonts = FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new());
    for text in ["", "startup", "\u{2800}", "\u{1f347}\u{200d}\u{1f347}"] {
        assert!(
            CosmicTextShaper
                .shape_run(&mut fonts, text, &Attrs::new(), 16.0, 20.0)
                .is_empty()
        );
        assert!(
            fonts.db().is_empty(),
            "shaping must not bypass native filtering"
        );
    }
}

#[test]
fn empty_catalog_primary_probe_reports_no_font() {
    let mut service = empty_service();
    assert!(
        service
            .cosmic_primary_probe("monospace", 400, false)
            .is_none()
    );
    assert_eq!(
        service.selected_font_id_and_space_width("monospace", 400, false, 16.0),
        (None, 0.0)
    );
    assert!(service.font_system.db().is_empty());
}

#[test]
fn empty_catalog_ascii_measurements_use_finite_degraded_metrics() {
    let mut service = empty_service();
    let widths = service.fill_ascii_widths("monospace", 400, false, 16.0);
    assert!(widths.iter().all(|width| width.is_finite() && *width > 0.0));
    assert!(service.font_system.db().is_empty());
}

#[test]
fn empty_catalog_non_ascii_selection_reports_no_font() {
    let mut service = empty_service();
    assert!(
        service
            .select_font_for_char('\u{2800}', "monospace", 400, false, 16.0)
            .is_none()
    );
    assert!(service.font_system.db().is_empty());
}

#[test]
fn empty_catalog_non_ascii_measurement_uses_finite_degraded_metrics() {
    let mut service = empty_service();
    let width = service.char_width('\u{2800}', "monospace", 400, false, 16.0);
    assert!(width.is_finite() && width > 0.0);
    assert!(service.font_system.db().is_empty());
}

#[test]
fn empty_catalog_startup_metrics_stay_finite_without_font_substitution() {
    let mut service = empty_service();
    let metrics = service.font_metrics("monospace", 400, false, 16.0);
    for positive in [
        metrics.ascent,
        metrics.line_height,
        metrics.char_width,
        metrics.space_width,
    ] {
        assert!(positive.is_finite() && positive > 0.0, "{metrics:?}");
    }
    assert!(metrics.descent.is_finite() && metrics.descent >= 0.0);
    assert!(
        service
            .shape_run("startup", "monospace", 400, false, 16.0)
            .is_empty()
    );
    assert!(service.font_system.db().is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn empty_scalable_catalog_keeps_exact_bitmap_fonts_available() {
    let path = neomacs_test_fonts::spleen_2_2_0()
        .pcf_gz()
        .to_str()
        .unwrap()
        .to_owned();
    let mut service = FontMetricsService::from_font_system_and_resolver(
        FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new()),
        crate::font::resolver::FontResolver::new(Box::new(FixedPrimaryFontBackend {
            file: path,
            face_index: 0,
            slant: FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Unknown,
        })),
    );
    let font = service
        .resolved_font_for_face("Spleen", 400, false, 16.0)
        .expect("native selected bitmap source remains usable without scalable faces");
    assert!(matches!(font.replay, FontReplay::FreeTypeBitmap { .. }));
    let widths = service.fill_ascii_widths("Spleen", 400, false, 16.0);
    assert!(widths.iter().all(|width| width.is_finite() && *width > 0.0));
    assert!(service.font_system.db().is_empty());
}
