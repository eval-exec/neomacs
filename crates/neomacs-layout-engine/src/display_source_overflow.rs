use crate::display_row::append_context::DisplayRowLineWrap;
use crate::display_row::geometry::DisplayRowTextAreaOrigin;
use crate::display_row::transition::{DisplayRowOverflowTransitionPlan, VisualWrapBreak};
use crate::display_row::walk_state::{
    DisplayRowTextOverflowDecision, SpecialTextRowOverflowDecision, TextRowTransitionStatePolicy,
    WordWrapBreakCandidate,
};
use neomacs_display_protocol::{
    GeometryError, ImageLayoutAdvance, LogicalPixels, Px, XwidgetLayoutAdvance,
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
    current_x_px: LogicalPixels,
    last_visible_x_px: LogicalPixels,
}

impl WindowLocalRowExtent {
    pub(crate) fn from_frame_coordinates(
        origin: DisplayRowTextAreaOrigin,
        x_px: f32,
        right_edge_px: f32,
    ) -> Result<Self, GeometryError> {
        let current_x_px = LogicalPixels::new(origin.window_local(x_px))?;
        let last_visible_x_px = LogicalPixels::new(origin.window_local(right_edge_px))?;
        if current_x_px.get() < 0.0 || current_x_px.get() > last_visible_x_px.get() {
            return Err(GeometryError::InvalidGeometry);
        }
        Ok(Self {
            current_x_px,
            last_visible_x_px,
        })
    }

    pub(crate) fn last_visible_x_px(self) -> f32 {
        self.last_visible_x_px.get()
    }

    /// `it->last_visible_x - it->current_x`: how much of the row is left.
    pub(crate) fn remaining_px(self) -> f32 {
        self.last_visible_x_px.get() - self.current_x_px.get()
    }

    fn remaining_advance(self) -> Option<XwidgetLayoutAdvance> {
        XwidgetLayoutAdvance::new(Px(self.remaining_px()))
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
/// This is the xwidget rule only.  `produce_image_glyph` has its own, ported
/// separately as [`DisplayImageOverflowAction`]: it also weighs word wrap, the
/// line-number prefix and the frame's column width, and it crops the glyph's
/// source slice along with its advance.
///
/// What this port does NOT do, relative to the GNU function:
///
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
        advance: XwidgetLayoutAdvance,
    },
    /// GNU leaves the glyph whole; the row's overflow policy decides.
    LeaveWhole,
}

impl DisplayXwidgetOverflowAction {
    pub(crate) fn for_xwidget(
        layout_advance: XwidgetLayoutAdvance,
        extent: WindowLocalRowExtent,
        at_row_start: bool,
    ) -> Self {
        let width_px = layout_advance.px().get();
        let visible_width_px = extent.remaining_px();
        let crop = width_px - visible_width_px;
        if crop <= 0.0 {
            return Self::Fits;
        }
        if (at_row_start || width_px > extent.last_visible_x_px() / 4.0)
            && let Some(advance) = extent.remaining_advance()
        {
            Self::CropAdvanceToVisibleWidth { advance }
        } else {
            Self::LeaveWhole
        }
    }
}

/// What GNU does with an inline *image* glyph that would extend past the right
/// edge of the text area.
///
/// `produce_image_glyph` (src/xdisp.c:32492-32509, emacs-31.1) decides this at
/// production time, before `display_line` measures the row:
///
/// ```c
///   /* Automatically crop wide image glyphs at right edge so we can draw
///      the cursor on same display row.  But don't do that under
///      word-wrap, unless the image starts at column zero, because
///      wrapping correctly needs the real pixel width of the image.  */
///   if ((it->line_wrap != WORD_WRAP
///        || it->hpos == (0 + (it->lnum_width ? it->lnum_width + 2 : 0))
///        /* Always crop images larger than the window-width, minus 1 space.  */
///        || it->pixel_width > (it->last_visible_x - it->lnum_pixel_width
///                              - FRAME_COLUMN_WIDTH (it->f)))
///       && (crop = it->pixel_width - (it->last_visible_x - it->current_x),
///           crop > 0)
///       && (it->hpos == (0 + (it->lnum_width ? it->lnum_width + 2 : 0))
///           || it->pixel_width > it->last_visible_x / 4))
///     {
///       it->pixel_width -= crop;
///       slice.width -= crop;
///     }
/// ```
///
/// A glyph that starts the row, or is wider than a quarter of the window's
/// visible width, has its layout advance -- and with it the visible part of its
/// source slice -- cropped so the image ends exactly at the right edge instead
/// of disappearing.  `display_line` then keeps the glyph, which is why
/// `it->what == IT_IMAGE` is one of the conditions that end a truncating row
/// (:26585-26598).
///
/// This is the image rule only.  `produce_xwidget_glyph` has its own
/// ([`DisplayXwidgetOverflowAction`]).
///
/// What this port decides differently from the function above:
///
/// - **Which rows word-wrap.** The word-wrap disjunct is tested through the
///   row's resolved [`DisplayRowLineWrap`], which separates GNU's `WORD_WRAP`
///   from `WINDOW_WRAP`; `LineWrapMode` alone collapses the two and would
///   apply the clause to every wrapping row.  GNU measured, 720 px text area,
///   a 400 px image starting at 396: WORD_WRAP leaves the glyph whole and
///   `display_line` moves it to the next row (row 0 ends at 369), WINDOW_WRAP
///   crops it to 720 and keeps it on row 0.
/// - **No room at all.** With `hpos == 0` GNU still crops when nothing of the
///   row is left, producing a zero- or negative-width glyph
///   (`clip_to_bounds (-1, …)`, :32529); here a non-positive advance is
///   [`Self::LeaveWhole`] and the row's overflow policy drops the glyph, the
///   same guard [`DisplayXwidgetOverflowAction`] documents.
/// - **Box line widths.** GNU adds `box_vertical_line_width` to
///   `it->pixel_width` before computing `crop` (:32473-32490); the width this
///   rule sees is the media replacement's own.
/// - **Horizontal scrolling.** As for the xwidget rule, `current_x` and
///   `last_visible_x` carry no `first_visible_x`; the remaining width agrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayImageOverflowAction {
    Fits,
    /// Crop both halves of GNU's edit: the layout advance
    /// ([`neomacs_display_protocol::ImageLayoutAdvance`]) and, by the same
    /// number of pixels, the glyph's source slice.
    CropToVisibleWidth {
        advance: ImageLayoutAdvance,
    },
    /// GNU leaves the glyph whole; the row's overflow policy decides.
    LeaveWhole,
}

impl DisplayImageOverflowAction {
    /// `layout_advance_px` is GNU's `it->pixel_width` for the image: the
    /// image's own width plus its horizontal margins.  `line_wrap` is the
    /// row's resolved `it->line_wrap` (see
    /// [`crate::display_row::append_context::DisplayRowLineWrap`]); without it
    /// the word-wrap disjunct below cannot be evaluated and the rule silently
    /// becomes a strict subset of GNU's.
    pub(crate) fn for_image(
        layout_advance_px: f32,
        extent: WindowLocalRowExtent,
        at_row_start: bool,
        char_width_px: f32,
        line_number_width_px: f32,
        line_wrap: DisplayRowLineWrap,
    ) -> Self {
        let crop = layout_advance_px - extent.remaining_px();
        if crop <= 0.0 {
            return Self::Fits;
        }
        // "Always crop images larger than the window-width, minus 1 space."
        //
        // The whole clause is the FIRST DISJUNCT of GNU's condition:
        //
        //   if ((it->line_wrap != WORD_WRAP
        //        || it->hpos == (0 + (it->lnum_width ? it->lnum_width + 2 : 0))
        //        || it->pixel_width > (it->last_visible_x - it->lnum_pixel_width
        //                              - FRAME_COLUMN_WIDTH (it->f)))
        //       && ...)
        //
        // so a row that does not word-wrap skips it entirely.  That is what
        // makes a TRUNCATING row crop a mid-row image --
        // GNU Emacs 31.1, Xvfb, 720 px text area, 9 px column, 64 columns of
        // text (current_x 576) and a 181 px image: the glyph ends at 720
        // (row->pixel_width 720), while the same image at 180 px is left whole
        // (row->pixel_width 756).  A WINDOW_WRAP row crops too (a 400 px image
        // at 396 ends at 720), and only WORD_WRAP holds the glyph's real width
        // back so `display_line` can move it.
        let wider_than_a_row_of_its_own = layout_advance_px
            > extent.last_visible_x_px() - line_number_width_px.max(0.0) - char_width_px.max(0.0);
        if line_wrap.is_word_wrap() && !at_row_start && !wider_than_a_row_of_its_own {
            return Self::LeaveWhole;
        }
        // "`it->pixel_width > it->last_visible_x / 4`".
        if !at_row_start && !(layout_advance_px > extent.last_visible_x_px() / 4.0) {
            return Self::LeaveWhole;
        }
        match ImageLayoutAdvance::new(Px(extent.remaining_px())) {
            Some(advance) => Self::CropToVisibleWidth { advance },
            None => Self::LeaveWhole,
        }
    }
}

#[cfg(test)]
#[path = "display_source_overflow/tests/display_source_overflow_test.rs"]
mod tests;
