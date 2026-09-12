//! Select one opened startup font for frame geometry and Lisp face seeding.

use super::{
    BootstrapDisplayConfig, BootstrapFrameMetrics, FontMetricsService, FontWeight,
    FrameFontRequest, SelectedFontInfo,
};
use neovm_core::emacs_core::display_host::SystemFontRole;

pub(super) struct StartupFont {
    pub(super) selected: SelectedFontInfo,
    pub(super) metrics: BootstrapFrameMetrics,
}

impl StartupFont {
    /// Desktop settings are a suggestion when user configuration has not run.
    /// Explicit Lisp frame/default-face settings continue through normal font
    /// realization afterwards; no redisplay-time reapplication can erase them.
    pub(super) fn select(display: &BootstrapDisplayConfig) -> Option<Self> {
        let mut metrics = FontMetricsService::new();
        let desktop = display
            .system_fonts
            .get(SystemFontRole::Monospace)
            .and_then(|name| FrameFontRequest::from_name(name.as_str()))
            .and_then(|request| {
                let size = display
                    .font_sizing()
                    .font_size_px_for_request(request.size())?;
                let family = request.face().family_runtime_string_owned()?;
                metrics.select_font_for_char(
                    'M',
                    &family,
                    request
                        .face()
                        .weight
                        .unwrap_or(FontWeight::NORMAL)
                        .css_weight(),
                    request.face().slant.is_some_and(|slant| slant.is_italic()),
                    size.get(),
                )
            });
        let selected = desktop.or_else(|| {
            metrics.select_font_for_char(
                'M',
                "Monospace",
                400,
                false,
                display.font_sizing().face_height_to_layout_pixels(100),
            )
        })?;
        let frame_metrics = BootstrapFrameMetrics {
            char_width: selected.metrics.average_width.max(1) as f32,
            char_height: selected.metrics.height.max(1) as f32,
            font_pixel_size: selected.metrics.pixel_size.max(1) as f32,
        };
        Some(Self {
            selected,
            metrics: frame_metrics,
        })
    }
}
