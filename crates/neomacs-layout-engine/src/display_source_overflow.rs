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

/// What GNU does with a media replacement -- an image, video, xwidget or
/// surface glyph -- that would extend past the right edge of the text area.
///
/// `produce_image_glyph` (src/xdisp.c:32582-32598) and
/// `produce_xwidget_glyph` (:32700-32704) decide this at production time,
/// before `display_line` measures the glyph:
///
/// ```c
///   crop = it->pixel_width - (it->last_visible_x - it->current_x);
///   if (crop > 0 && (it->hpos == 0 || it->pixel_width > it->last_visible_x / 4))
///     it->pixel_width -= crop;
/// ```
///
/// A glyph that starts the row, or is wider than a quarter of the visible
/// width, is cropped so it fits exactly and is shown partially rather than
/// not at all -- `display_line` then keeps it (:26254-26310) and the row's
/// clip does the rest.  A narrower glyph in the middle of a row is left
/// whole for `display_line`'s continuation or truncation handling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayMediaReplacementOverflowAction {
    Fits,
    CropToVisibleWidth {
        visible_width_px: f32,
    },
    /// GNU leaves the glyph whole; the row's overflow policy decides.
    LeaveWhole,
}

impl DisplayMediaReplacementOverflowAction {
    pub(crate) fn for_replacement(
        width_px: f32,
        x_px: f32,
        right_edge_px: f32,
        at_row_start: bool,
    ) -> Self {
        let visible_width_px = right_edge_px - x_px;
        let crop = width_px - visible_width_px;
        if crop <= 0.0 {
            return Self::Fits;
        }
        if visible_width_px > 0.0 && (at_row_start || width_px > right_edge_px / 4.0) {
            Self::CropToVisibleWidth { visible_width_px }
        } else {
            Self::LeaveWhole
        }
    }
}

#[cfg(test)]
#[path = "display_source_overflow_test.rs"]
mod tests;
