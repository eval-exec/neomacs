use crate::display_row::geometry::DisplayRowTextAreaOrigin;
use crate::display_row::transition::{DisplayRowOverflowTransitionPlan, VisualWrapBreak};
use crate::display_row::walk_state::{
    DisplayRowTextOverflowDecision, SpecialTextRowOverflowDecision, TextRowTransitionStatePolicy,
    WordWrapBreakCandidate,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplaySourceTextCharOverflowAction {
    Fits,
    Truncate {
        transition: DisplayRowOverflowTransitionPlan,
    },
    WordWrap {
        break_candidate: WordWrapBreakCandidate,
        transition: DisplayRowOverflowTransitionPlan,
    },
    CharacterWrap {
        transition: DisplayRowOverflowTransitionPlan,
    },
}

impl DisplaySourceTextCharOverflowAction {
    pub(crate) fn for_decision(decision: DisplayRowTextOverflowDecision) -> Self {
        match decision {
            DisplayRowTextOverflowDecision::Fits => Self::Fits,
            DisplayRowTextOverflowDecision::Truncate => Self::Truncate {
                transition: DisplayRowOverflowTransitionPlan::truncation(
                    TextRowTransitionStatePolicy::truncation(),
                ),
            },
            DisplayRowTextOverflowDecision::WordWrap { break_candidate } => Self::WordWrap {
                break_candidate,
                transition: DisplayRowOverflowTransitionPlan::visual_wrap(
                    VisualWrapBreak::AtWordBoundary,
                    TextRowTransitionStatePolicy::visual_wrap(),
                ),
            },
            DisplayRowTextOverflowDecision::CharacterWrap => Self::CharacterWrap {
                transition: DisplayRowOverflowTransitionPlan::visual_wrap(
                    VisualWrapBreak::MidElement,
                    TextRowTransitionStatePolicy::character_wrap(),
                ),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplaySourceSpecialCharOverflowAction {
    Fits,
    Truncate {
        transition: DisplayRowOverflowTransitionPlan,
    },
    Wrap {
        transition: DisplayRowOverflowTransitionPlan,
    },
}

impl DisplaySourceSpecialCharOverflowAction {
    pub(crate) fn for_decision(decision: SpecialTextRowOverflowDecision) -> Self {
        match decision {
            SpecialTextRowOverflowDecision::Fits => Self::Fits,
            SpecialTextRowOverflowDecision::Truncate => Self::Truncate {
                transition: DisplayRowOverflowTransitionPlan::truncation(
                    TextRowTransitionStatePolicy::special_truncation(),
                ),
            },
            SpecialTextRowOverflowDecision::Wrap => Self::Wrap {
                transition: DisplayRowOverflowTransitionPlan::visual_wrap(
                    VisualWrapBreak::MidElement,
                    TextRowTransitionStatePolicy::special_visual_wrap(),
                ),
            },
        }
    }
}

/// GNU `it->current_x` and `it->last_visible_x` for one `produce_*` call.
///
/// Both are window-local: pixels from the left edge of the window's text
/// area, not from the frame's (src/dispextern.h:2785-2791, emacs-31.0.90:
/// "last_visible_x == pixel width of W + first_visible_x").  The row writer
/// keeps frame-absolute positions, so the conversion happens here, once,
/// through [`DisplayRowTextAreaOrigin`]; a policy that compared against a
/// frame-absolute edge would be wrong in every window but the leftmost.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowLocalRowExtent {
    current_x_px: f32,
    last_visible_x_px: f32,
}

impl WindowLocalRowExtent {
    pub(crate) fn from_frame_coordinates(
        origin: DisplayRowTextAreaOrigin,
        x_px: f32,
        right_edge_px: f32,
    ) -> Self {
        Self {
            current_x_px: origin.window_local(x_px),
            last_visible_x_px: origin.window_local(right_edge_px),
        }
    }

    pub(crate) fn last_visible_x_px(self) -> f32 {
        self.last_visible_x_px
    }

    /// `it->last_visible_x - it->current_x`: how much of the row is left.
    pub(crate) fn remaining_px(self) -> f32 {
        self.last_visible_x_px - self.current_x_px
    }
}

/// What GNU does with an xwidget glyph that would extend past the right
/// edge of the text area.
///
/// `produce_xwidget_glyph` (src/xdisp.c:32575-32579, emacs-31.0.90) decides
/// this at production time, before `display_line` measures the glyph:
///
/// ```c
///   /* Automatically crop wide image glyphs at right edge so we can
///      draw the cursor on same display row.  */
///   crop = it->pixel_width - (it->last_visible_x - it->current_x);
///   if (crop > 0 && (it->hpos == 0 || it->pixel_width > it->last_visible_x / 4))
///     it->pixel_width -= crop;
/// ```
///
/// A glyph that starts the row, or is wider than a quarter of the window's
/// visible width, has its layout advance cropped so it fits exactly and is
/// shown partially rather than not at all -- `display_line` then keeps it
/// and `x_draw_xwidget_glyph_string` clips the widget, whose own size is
/// untouched (src/xwidget.c:2841-2849).
///
/// This is the xwidget rule only.  `produce_image_glyph` has its own
/// (src/xdisp.c:32457-32473), which also weighs word wrap, the line-number
/// prefix and the frame's column width, and it is not ported here.
///
/// What this port does NOT do, relative to the GNU function:
///
/// - **`LeaveWhole` drops the glyph.** In GNU a narrow mid-row widget that
///   does not fit is continued onto the next row or truncated by
///   `display_line` (the test at src/xdisp.c:26221-26224, the branch that
///   restores the position and emits the continuation glyph at
///   :26416-26434); this row builder has no
///   remainder for a media replacement, so the row's
///   `RejectOverflowingGlyph` policy consumes the covered text and emits
///   nothing.  The pre-existing behavior, narrowed by this rule to widgets
///   GNU would also leave whole.
/// - **No room at all.** With `hpos == 0` GNU still crops when nothing of
///   the row is left, producing a glyph of zero or negative width
///   (`clip_to_bounds (-1, …)`, :32600); here `visible_width_px > 0.0`
///   guards the crop and such a glyph is dropped instead.
/// - **Box line widths.** GNU adds `box_vertical_line_width` to
///   `it->pixel_width` before computing `crop` (:32557-32571); the width
///   passed here is the widget's, so a boxed widget's threshold and advance
///   are narrower than GNU's by the box.  Xwidgets have no positive-box
///   expansion in this port yet (only images do).
/// - **Horizontal scrolling.** GNU's `current_x` and `last_visible_x` both
///   carry `first_visible_x` (src/xdisp.c:3507); this port scrolls by
///   skipping columns, so [`WindowLocalRowExtent`] is hscroll-free.  The
///   remaining width agrees; the quarter-width threshold is smaller than
///   GNU's by a quarter of the scrolled-off pixels.  A widget that
///   straddles `first_visible_x` is produced by GNU and kept with a
///   negative `row->x`; here the skip phase consumes the character that
///   carries it as a plain glyph (`consume_step_char` in
///   `buffer_source/row_lifecycle.rs`), so it never reaches this rule and
///   is not shown at all.
/// - **`it->hpos == 0` under horizontal scrolling.** GNU's `hpos` counts
///   only glyphs past `first_visible_x` (`maybe_produce_line_number`,
///   :25705-25706); `at_row_start` here means "nothing written before this
///   glyph", which agrees only because the skip phase writes nothing before
///   the first visible glyph.
/// - **Ascent and descent.** The same GNU function splits the widget's
///   height evenly (`it->ascent = it->descent = xw->height/2`,
///   :32546-32547); this port gives a media replacement a full-height
///   ascent (`display_replacement_ascent`).  Not a crop matter, listed
///   because it is in the function this rule is taken from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayXwidgetOverflowAction {
    Fits,
    CropAdvanceToVisibleWidth {
        visible_width_px: f32,
    },
    /// GNU leaves the glyph whole; the row's overflow policy decides.
    LeaveWhole,
}

impl DisplayXwidgetOverflowAction {
    pub(crate) fn for_xwidget(
        width_px: f32,
        extent: WindowLocalRowExtent,
        at_row_start: bool,
    ) -> Self {
        let visible_width_px = extent.remaining_px();
        let crop = width_px - visible_width_px;
        if crop <= 0.0 {
            return Self::Fits;
        }
        if visible_width_px > 0.0 && (at_row_start || width_px > extent.last_visible_x_px() / 4.0) {
            Self::CropAdvanceToVisibleWidth { visible_width_px }
        } else {
            Self::LeaveWhole
        }
    }
}

#[cfg(test)]
#[path = "display_source_overflow_test.rs"]
mod tests;
