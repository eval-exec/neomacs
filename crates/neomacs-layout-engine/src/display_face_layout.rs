use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::width::DisplayRowCharWidthPolicy;
use crate::font::metrics::FontMetricsService;
use crate::neovm_bridge::ResolvedFace;

pub(crate) struct DisplayHeightFaceBasis<'a> {
    pub(crate) canonical_face: &'a ResolvedFace,
    pub(crate) base_face: &'a ResolvedFace,
    pub(crate) fallback_metrics: DisplayRowFallbackMetrics,
}

/// The measurement capability available at this stage of face resolution.
/// A graphical measurer cannot be requested without supplying the service;
/// deferred graphical resolution is distinct from terminal cell geometry.
pub(crate) enum HeightFaceMeasurement<'a> {
    ConcreteFont(&'a mut FontMetricsService),
    LogicalCells,
    DeferredFont,
}

pub(crate) fn height_adjusted_face(
    source: &ResolvedFace,
    basis: DisplayHeightFaceBasis<'_>,
    factor: f32,
    measurement: HeightFaceMeasurement<'_>,
) -> Option<ResolvedFace> {
    if !factor.is_finite() || factor <= 0.0 {
        return None;
    }
    let fallback = basis.fallback_metrics;
    if fallback.char_width() <= 1.0 && fallback.row_height() <= 1.0 {
        return None;
    }

    let canonical = basis.canonical_face;
    let canonical_font_size = positive_f32(canonical.font_size)
        .or_else(|| positive_f32(basis.base_face.font_size))
        .or_else(|| positive_f32(source.font_size))
        .unwrap_or_else(|| fallback.row_height().max(1.0));
    let canonical_line_height = positive_f32(canonical.font_line_height)
        .or_else(|| positive_f32(fallback.row_height()))
        .unwrap_or(canonical_font_size);
    let canonical_ascent = positive_f32(canonical.font_ascent)
        .or_else(|| positive_f32(fallback.ascent()))
        .unwrap_or(canonical_line_height * 0.8)
        .min(canonical_line_height);
    let canonical_char_width = DisplayRowCharWidthPolicy::new(canonical_font_size * 0.5)
        .width_or_measured(
            canonical.measured_char_width_px(),
            Some(fallback.char_width()),
        );

    let mut resolved = source.clone();
    resolved.font_size = (canonical_font_size * factor).max(1.0);
    resolved.font_line_height = (canonical_line_height * factor).max(1.0);
    resolved.font_ascent = (canonical_ascent * factor)
        .max(1.0)
        .min(resolved.font_line_height);
    // GNU re-measures the font after a relative `:height`
    // (`font_load_for_lface` -> `x_set_font`), so the scaled advance is the
    // font engine's answer at the SCALED pixel size, not the canonical advance
    // times the factor. Scaling only approximates it and drifts whenever the
    // advance is not linear in the size; keep it only as the no-measurement
    // fallback (TTY / font service absent).
    let measured_char_width = match measurement {
        HeightFaceMeasurement::ConcreteFont(service) => Some(
            service
                .font_metrics(
                    &resolved.font_family,
                    resolved.font_weight,
                    resolved.italic,
                    resolved.font_size,
                )
                .char_width,
        ),
        HeightFaceMeasurement::LogicalCells | HeightFaceMeasurement::DeferredFont => None,
    }
    .filter(|width| width.is_finite() && *width > 0.0);
    resolved.set_measured_char_width_px(
        measured_char_width
            .unwrap_or(canonical_char_width * factor)
            .max(1.0),
    );
    Some(resolved)
}

fn positive_f32(value: f32) -> Option<f32> {
    (value.is_finite() && value > 0.0).then_some(value)
}

#[cfg(test)]
#[path = "display_face_layout_test.rs"]
mod tests;
