//! Select one opened startup font for frame geometry and Lisp face seeding.

use super::{
    BootstrapDisplayConfig, BootstrapFrameMetrics, FontMetricsService, FontWeight,
    FrameFontRequest, FrontendKind, SelectedFontInfo,
};
use neomacs_display_runtime::font_defaults::InitialFontFamilyMatch;
use neovm_core::emacs_core::display_host::FrameFontSize;

pub(super) struct StartupFont {
    selected: SelectedFontInfo,
    metrics: BootstrapFrameMetrics,
}

/// A GUI bootstrap always owns an opened font. TTY bootstrap never scans the
/// font catalog and cannot accidentally seed a graphical Lisp font object.
pub(super) enum BootstrapFont {
    Gui(StartupFont),
    Tty,
}

impl BootstrapFont {
    pub(super) fn select(display: &BootstrapDisplayConfig) -> Option<Self> {
        match display.frontend() {
            FrontendKind::Gui => StartupFont::select(display).map(Self::Gui),
            FrontendKind::Tty => Some(Self::Tty),
        }
    }

    pub(super) fn metrics(&self) -> BootstrapFrameMetrics {
        match self {
            Self::Gui(font) => font.metrics(),
            Self::Tty => BootstrapFrameMetrics::TTY,
        }
    }
}

impl StartupFont {
    pub(super) fn metrics(&self) -> BootstrapFrameMetrics {
        self.metrics
    }

    pub(super) fn into_selected(self) -> SelectedFontInfo {
        self.selected
    }

    /// Desktop settings are a suggestion when user configuration has not run.
    /// Explicit Lisp frame/default-face settings continue through normal font
    /// realization afterwards; no redisplay-time reapplication can erase them.
    pub(super) fn select(display: &BootstrapDisplayConfig) -> Option<Self> {
        let mut metrics = FontMetricsService::new();
        let selected = display.font_defaults.select(|candidate| {
            let request = FrameFontRequest::from_name(candidate.name())?;
            let requested_size = match request.size() {
                FrameFontSize::Default => candidate.default_size().font_size(),
                explicit => explicit,
            };
            let size = display
                .font_sizing()
                .font_size_px_for_request(requested_size)?;
            let family = request.face().family_runtime_string_owned()?;
            if candidate.family_match() == InitialFontFamilyMatch::RequireNamedFamily {
                // A generic font fallback must not make an absent named
                // candidate appear to have opened successfully.
                let query = neomacs_layout_engine::font::resolver::FontEntityQuery::new(
                    neomacs_layout_engine::font_backend::FontFamilyName::new(&family),
                );
                metrics.resolve_font_entity(&query)?;
            }
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
