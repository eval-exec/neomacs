//! Transactional per-window layout outcomes.
//!
//! A window may begin an attempt with retained or face-estimated chrome
//! metrics.  Shaping the tab/header/mode rows produces their actual intrinsic
//! metrics.  A mismatch is a layout invalidation: callers must discard the
//! attempt and retry before publishing any body/cursor/spatial output.

use crate::display_frame_output::WindowFrameGeometry;
use crate::types::{FrameParams, WindowParams};
use neomacs_display_protocol::types::Rect;
use neovm_core::window::PresentedWindowRegions;
use neovm_core::window::WindowDisplaySnapshot;

/// The vertical metrics that partition one leaf window's body from its chrome.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct WindowChromeMetrics {
    pub(crate) tab_line_height: f32,
    pub(crate) header_line_height: f32,
    pub(crate) mode_line_height: f32,
}

impl WindowChromeMetrics {
    pub(crate) fn from_params(params: &WindowParams) -> Self {
        Self {
            tab_line_height: params.tab_line_height,
            header_line_height: params.header_line_height,
            mode_line_height: params.mode_line_height,
        }
    }

    pub(crate) fn from_snapshot(snapshot: &WindowDisplaySnapshot) -> Self {
        Self {
            tab_line_height: snapshot.tab_line_height.max(0) as f32,
            header_line_height: snapshot.header_line_height.max(0) as f32,
            mode_line_height: snapshot.mode_line_height.max(0) as f32,
        }
    }

    /// Seed a new attempt with accepted metrics while respecting the current
    /// window's wants-* decision.  A newly enabled row has no retained positive
    /// metric, so it keeps the bridge's face estimate for its first attempt.
    pub(crate) fn seed_params(self, params: &mut WindowParams) {
        params.tab_line_height =
            retained_or_estimated(params.tab_line_height, self.tab_line_height);
        params.header_line_height =
            retained_or_estimated(params.header_line_height, self.header_line_height);
        params.mode_line_height =
            retained_or_estimated(params.mode_line_height, self.mode_line_height);
    }

    fn is_stable_with(self, measured: Self) -> bool {
        metric_is_stable(self.tab_line_height, measured.tab_line_height)
            && metric_is_stable(self.header_line_height, measured.header_line_height)
            && metric_is_stable(self.mode_line_height, measured.mode_line_height)
    }
}

/// The complete physical partition of one leaf window.
///
/// This is a layout-domain value, not a renderer reconstruction.  Body rows,
/// chrome, scroll bars, hit testing, popup anchors, and evaluator queries must
/// all consume rectangles projected from this same value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowLayoutBox {
    chrome: WindowChromeMetrics,
    regions: PresentedWindowRegions,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub(crate) struct WindowDividerLayout {
    right_edge: f32,
    bottom_edge: f32,
    right_width: f32,
    bottom_height: f32,
}

impl WindowDividerLayout {
    pub(crate) fn resolve(
        params: &WindowParams,
        frame_params: &FrameParams,
        geometry: WindowFrameGeometry,
    ) -> Self {
        let right_width = if !params.is_minibuffer()
            && !geometry.is_rightmost
            && frame_params.right_divider_width > 0
        {
            frame_params.right_divider_width as f32
        } else {
            0.0
        };
        let bottom_height = if !params.is_minibuffer()
            && !geometry.is_bottommost
            && frame_params.bottom_divider_width > 0
        {
            frame_params.bottom_divider_width as f32
        } else {
            0.0
        };
        Self {
            right_edge: geometry.right_edge,
            bottom_edge: geometry.bottom_edge,
            right_width,
            bottom_height,
        }
    }

    #[cfg(test)]
    pub(crate) fn without_dividers(params: &WindowParams) -> Self {
        Self {
            right_edge: params.bounds.x + params.bounds.width,
            bottom_edge: params.bounds.y + params.bounds.height,
            right_width: 0.0,
            bottom_height: 0.0,
        }
    }
}

/// Exact physical partition under which retained rows were produced.
///
/// Equality is the cache reuse gate.  Keeping the whole partition together
/// prevents a new margin, fringe ordering, scroll bar, or chrome height from
/// accidentally reusing rows whose origin/clip belonged to an older layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowPartitionSignature {
    regions: PresentedWindowRegions,
}

impl WindowPartitionSignature {
    pub(crate) fn from_layout_box(layout_box: WindowLayoutBox) -> Self {
        Self {
            regions: layout_box.regions(),
        }
    }

    pub(crate) fn text_body(self) -> Rect {
        self.regions.text_body
    }

    #[cfg(test)]
    pub(crate) fn from_regions(regions: PresentedWindowRegions) -> Self {
        Self { regions }
    }

    #[cfg(test)]
    pub(crate) fn regions_mut(&mut self) -> &mut PresentedWindowRegions {
        &mut self.regions
    }
}

impl WindowLayoutBox {
    pub(crate) fn resolve(
        params: &WindowParams,
        chrome: WindowChromeMetrics,
        dividers: WindowDividerLayout,
    ) -> Self {
        let outer = params.bounds;
        let tab_h = chrome.tab_line_height.max(0.0);
        let header_h = chrome.header_line_height.max(0.0);
        let mode_h = chrome.mode_line_height.max(0.0);
        let right_divider_w = dividers.right_width.max(0.0).min(outer.width.max(0.0));
        let bottom_divider_h = dividers.bottom_height.max(0.0).min(outer.height.max(0.0));
        let content_width = (outer.width - right_divider_w).max(0.0);
        let content_height = (outer.height - bottom_divider_h).max(0.0);
        let hscroll_h = if params.horizontal_scroll_bar {
            params.scroll_bar_pixel_height.max(0.0)
        } else {
            0.0
        };
        let body_y = outer.y + tab_h + header_h;
        let body_h = (content_height - tab_h - header_h - hscroll_h - mode_h).max(0.0);
        let body = Rect::new(
            params.text_bounds.x,
            body_y,
            (params.text_bounds.width - right_divider_w).max(0.0),
            body_h,
        );
        let band = |x: f32, width: f32| (width > 0.0).then(|| Rect::new(x, body_y, width, body_h));

        let left_sb_w = if params.vertical_scroll_bar_side.as_deref() == Some("left") {
            params.scroll_bar_pixel_width.max(0.0)
        } else {
            0.0
        };
        let right_sb_w = if params.vertical_scroll_bar_side.as_deref() == Some("right") {
            params.scroll_bar_pixel_width.max(0.0)
        } else {
            0.0
        };

        let mut left_x = outer.x;
        let left_scroll_bar = band(left_x, left_sb_w);
        left_x += left_sb_w;
        let (left_fringe, left_margin) = if params.fringes_outside_margins {
            let fringe = band(left_x, params.left_fringe_width);
            left_x += params.left_fringe_width.max(0.0);
            let margin = band(left_x, params.left_margin_width);
            (fringe, margin)
        } else {
            let margin = band(left_x, params.left_margin_width);
            left_x += params.left_margin_width.max(0.0);
            let fringe = band(left_x, params.left_fringe_width);
            (fringe, margin)
        };

        let mut right_x = body.x + body.width;
        let (right_fringe, right_margin) = if params.fringes_outside_margins {
            let margin = band(right_x, params.right_margin_width);
            right_x += params.right_margin_width.max(0.0);
            let fringe = band(right_x, params.right_fringe_width);
            right_x += params.right_fringe_width.max(0.0);
            (fringe, margin)
        } else {
            let fringe = band(right_x, params.right_fringe_width);
            right_x += params.right_fringe_width.max(0.0);
            let margin = band(right_x, params.right_margin_width);
            right_x += params.right_margin_width.max(0.0);
            (fringe, margin)
        };
        let right_scroll_bar = band(right_x, right_sb_w);

        let optional_rect = |rect: Rect| (rect.width > 0.0 && rect.height > 0.0).then_some(rect);
        let tab_line = optional_rect(Rect::new(outer.x, outer.y, content_width, tab_h));
        let header_line =
            optional_rect(Rect::new(outer.x, outer.y + tab_h, content_width, header_h));
        let horizontal_scroll_bar = optional_rect(Rect::new(
            outer.x,
            body_y + body_h,
            content_width,
            hscroll_h,
        ));
        let mode_line = optional_rect(Rect::new(
            outer.x,
            outer.y + content_height - mode_h,
            content_width,
            mode_h,
        ));

        let right_divider = optional_rect(Rect::new(
            dividers.right_edge - dividers.right_width,
            outer.y,
            right_divider_w,
            content_height,
        ));
        let bottom_divider = optional_rect(Rect::new(
            outer.x,
            dividers.bottom_edge - bottom_divider_h,
            content_width,
            bottom_divider_h,
        ));

        Self {
            chrome,
            regions: PresentedWindowRegions {
                outer,
                text_body: body,
                left_margin_columns: params.left_margin_columns,
                right_margin_columns: params.right_margin_columns,
                left_margin,
                right_margin,
                left_fringe,
                right_fringe,
                left_scroll_bar,
                right_scroll_bar,
                horizontal_scroll_bar,
                tab_line,
                header_line,
                mode_line,
                right_divider,
                bottom_divider,
            },
        }
    }

    pub(crate) fn chrome(self) -> WindowChromeMetrics {
        self.chrome
    }

    pub(crate) fn regions(self) -> PresentedWindowRegions {
        self.regions
    }

    pub(crate) fn body(self) -> Rect {
        self.regions.text_body
    }
}

fn retained_or_estimated(estimated: f32, retained: f32) -> f32 {
    if estimated > 0.0 && retained > 0.0 {
        retained
    } else {
        estimated.max(0.0)
    }
}

fn metric_is_stable(assumed: f32, measured: f32) -> bool {
    // Measured display rows cross the layout boundary as integral logical
    // pixels (see `stable_pixel_ceil`).  Keep convergence exact here: treating
    // two distinct allocations as equivalent would let the body and chrome
    // publish rectangles derived from different partitions.
    assumed == measured
}

/// Publication outcome for one leaf-window attempt.
#[derive(Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum WindowLayoutOutcome {
    /// The window had no materializable body for this attempt.
    Skipped,
    /// Body, chrome, cursor, and spatial geometry use the same metrics.
    Stable(WindowLayoutBox),
    /// Shaping discovered different intrinsic metrics.  The containing frame
    /// must discard the attempt and immediately relayout with `measured`.
    NeedsRelayout {
        assumed: WindowChromeMetrics,
        measured: WindowChromeMetrics,
    },
}

impl WindowLayoutOutcome {
    pub(crate) fn from_measurement(
        layout_box: WindowLayoutBox,
        measured: WindowChromeMetrics,
    ) -> Self {
        let assumed = layout_box.chrome();
        if assumed.is_stable_with(measured) {
            Self::Stable(layout_box)
        } else {
            Self::NeedsRelayout { assumed, measured }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::metric_is_stable;

    #[test]
    fn chrome_convergence_requires_the_same_canonical_pixel_height() {
        assert!(metric_is_stable(17.0, 17.0));
        assert!(!metric_is_stable(17.0, 17.001));
    }
}
