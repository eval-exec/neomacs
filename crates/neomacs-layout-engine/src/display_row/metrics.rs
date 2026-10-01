//! Shared row metric values used across display source and row rendering paths.

use crate::font::metrics::FontMetrics;
use crate::types::{FrameParams, WindowParams};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowMeasuredFaceMetrics {
    char_width: f32,
    row_height: f32,
    ascent: f32,
    space_width: f32,
}

impl DisplayRowMeasuredFaceMetrics {
    pub(crate) fn new(char_width: f32, row_height: f32, ascent: f32, space_width: f32) -> Self {
        Self {
            char_width,
            row_height,
            ascent,
            space_width,
        }
    }

    pub(crate) fn char_width(self) -> f32 {
        self.char_width
    }

    pub(crate) fn row_height(self) -> f32 {
        self.row_height
    }

    pub(crate) fn ascent(self) -> f32 {
        self.ascent
    }

    pub(crate) fn space_width(self) -> f32 {
        self.space_width
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowFallbackMetrics {
    pub(crate) char_width: f32,
    pub(crate) row_height: f32,
    pub(crate) ascent: f32,
}

impl DisplayRowFallbackMetrics {
    pub(crate) fn from_default_face_extents(char_width: f32, row_height: f32, ascent: f32) -> Self {
        Self {
            char_width,
            row_height,
            ascent,
        }
    }

    pub(crate) fn from_window_defaults(params: &WindowParams) -> Self {
        Self::from_default_face_extents(params.char_width, params.char_height, params.font_ascent)
    }

    pub(crate) fn from_frame_defaults(params: &FrameParams, ascent: f32) -> Self {
        Self::from_default_face_extents(params.char_width, params.char_height, ascent)
    }

    pub(crate) fn from_font_metrics(metrics: FontMetrics) -> Self {
        Self::from_default_face_extents(metrics.char_width, metrics.line_height, metrics.ascent)
    }

    pub(crate) fn from_measured_face(metrics: DisplayRowMeasuredFaceMetrics) -> Self {
        Self::from_default_face_extents(
            metrics.char_width(),
            metrics.row_height(),
            metrics.ascent(),
        )
    }

    pub(crate) fn with_row_height(self, row_height: f32) -> Self {
        Self::from_default_face_extents(self.char_width(), row_height, self.ascent())
    }

    pub(crate) fn with_extents(self, char_width: f32, row_height: f32) -> Self {
        Self::from_default_face_extents(char_width, row_height, self.ascent())
    }

    pub(crate) fn char_width(self) -> f32 {
        self.char_width
    }

    pub(crate) fn row_height(self) -> f32 {
        self.row_height
    }

    pub(crate) fn ascent(self) -> f32 {
        self.ascent
    }
}

/// GNU newline height resolution, shared by buffer, pushed-string and worker
/// rows. A factor uses the frame font; an integer keeps the newline face.
/// Height added by either form goes above the baseline, preserving descent.
pub(crate) struct DisplayLineHeightMetrics {
    pub height: f32,
    pub ascent: f32,
    pub newline_height: f32,
    pub newline_ascent: f32,
}

pub(crate) fn resolve_line_height(
    policy: crate::display_item::DisplayLineHeightPolicy,
    content: (f32, f32),
    face: (f32, f32),
    default: (f32, f32),
) -> DisplayLineHeightMetrics {
    use crate::display_item::DisplayLineHeightPolicy;
    let (font, minimum) = match policy {
        DisplayLineHeightPolicy::Scale(factor) => (default, (default.0 * factor).trunc()),
        DisplayLineHeightPolicy::Pixels(pixels) => (face, pixels),
        _ => (face, face.0),
    };
    // An unrepresentable scaled request cannot produce finite row geometry.
    // Retain the chosen font's normal extent rather than publishing infinity.
    let minimum = if minimum.is_finite() { minimum } else { font.0 };
    let mut descent = (font.0 - font.1).max(0.0);
    let mut ascent = font.1.max(minimum - descent);
    let content_descent = (content.0 - content.1).max(0.0);
    if policy == DisplayLineHeightPolicy::ContentOnly {
        // xdisp.c's constrain_row_ascent_descent_p newline branch.
        if descent > content_descent {
            ascent += descent - content_descent;
            descent = content_descent;
        }
        if ascent > content.1 {
            descent = content_descent.min(descent + ascent - content.1);
            ascent = content.1;
        }
    }
    let row_ascent = content.1.max(ascent);
    DisplayLineHeightMetrics {
        height: row_ascent + content_descent.max(descent),
        ascent: row_ascent,
        newline_height: ascent + descent,
        newline_ascent: ascent,
    }
}
