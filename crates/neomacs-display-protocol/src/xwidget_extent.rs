//! The intrinsic size of an inline xwidget, kept apart from the glyph that
//! places it.
//!
//! GNU keeps three extents for one xwidget glyph and never conflates them:
//!
//! - the widget's own size, `xw->width` / `xw->height`, which is what the
//!   native view is sized to (`x_draw_xwidget_glyph_string` sizes the view
//!   from `xww->width` and only clips it, src/xwidget.c:2841-2849 in
//!   emacs-31.0.90);
//! - the glyph's layout advance, `glyph->pixel_width`, which
//!   `produce_xwidget_glyph` may crop at the right edge of the text area
//!   (src/xdisp.c:32577-32579);
//! - the visible clip, the window's text area (`window_box (s->w, xv->area,
//!   …)`, src/xwidget.c:2841).
//!
//! [`XwidgetContentExtent`] is the first of these.  The glyph matrix and the
//! frame glyph carry it next to the cropped advance so the native placement
//! reads the widget size from here and never from the layout width.

/// GNU `xw->width` / `xw->height`: the pixel size the xwidget was created
/// with, and therefore the size of the native web view's content area.
///
/// Both dimensions are finite and strictly positive; a widget with no area
/// has no native view to place.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XwidgetContentExtent {
    width_px: f32,
    height_px: f32,
}

impl XwidgetContentExtent {
    /// `None` unless both dimensions are finite and strictly positive.
    #[must_use]
    pub fn new(width_px: f32, height_px: f32) -> Option<Self> {
        let valid = |value: f32| value.is_finite() && value > 0.0;
        (valid(width_px) && valid(height_px)).then_some(Self {
            width_px,
            height_px,
        })
    }

    #[must_use]
    pub const fn width_px(self) -> f32 {
        self.width_px
    }

    #[must_use]
    pub const fn height_px(self) -> f32 {
        self.height_px
    }
}

#[cfg(test)]
#[path = "xwidget_extent_test.rs"]
mod tests;
