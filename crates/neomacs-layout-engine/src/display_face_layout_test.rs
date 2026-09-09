use super::*;
use crate::display_row::metrics::DisplayRowFallbackMetrics;

fn face(family: &str, size: f32) -> ResolvedFace {
    let mut face = ResolvedFace::default();
    face.font_family = family.to_string();
    face.font_weight = 400;
    face.italic = false;
    face.font_size = size;
    face.font_line_height = size * 1.2;
    face.font_ascent = size * 0.8;
    face.set_measured_char_width_px(8.0);
    face
}

fn basis<'a>(canonical: &'a ResolvedFace) -> DisplayHeightFaceBasis<'a> {
    DisplayHeightFaceBasis {
        canonical_face: canonical,
        base_face: canonical,
        fallback_metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    }
}

/// GNU re-measures the font at the scaled size (`font_load_for_lface` ->
/// `x_set_font`), so with a font service the face must carry the service's
/// advance for the SCALED pixel size; only the no-service fallback may scale
/// the canonical advance by the height factor.
#[test]
fn height_adjusted_face_measures_the_scaled_font_when_a_service_is_available() {
    let _eval = neovm_core::emacs_core::Context::new();
    let canonical = face("Monospace", 13.0);
    let mut service = FontMetricsService::new();

    let measured = height_adjusted_face(
        &canonical,
        basis(&canonical),
        1.3,
        HeightFaceMeasurement::ConcreteFont(&mut service),
    )
    .expect("scaled face");
    let expected = service
        .font_metrics(
            &measured.font_family,
            measured.font_weight,
            measured.italic,
            measured.font_size,
        )
        .char_width;
    assert_eq!(
        measured.measured_char_width_px(),
        expected,
        "the scaled face must carry the font service's advance for the scaled size"
    );

    let fallback = height_adjusted_face(
        &canonical,
        basis(&canonical),
        1.3,
        HeightFaceMeasurement::DeferredFont,
    )
    .expect("scaled face");
    assert_eq!(
        fallback.measured_char_width_px(),
        8.0 * 1.3,
        "without a service the canonical advance scales by the height factor"
    );
}
