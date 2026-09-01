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
