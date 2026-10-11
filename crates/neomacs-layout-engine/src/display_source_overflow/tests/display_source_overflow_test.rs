use super::{DisplayImageOverflowAction, DisplayXwidgetOverflowAction, WindowLocalRowExtent};
use crate::display_row::append_context::DisplayRowLineWrap;
use crate::display_row::geometry::DisplayRowTextAreaOrigin;
use neomacs_display_protocol::{Px, XwidgetLayoutAdvance};

fn advance(px: f32) -> XwidgetLayoutAdvance {
    XwidgetLayoutAdvance::new(Px(px)).expect("positive finite layout advance")
}

fn image_advance(px: f32) -> neomacs_display_protocol::ImageLayoutAdvance {
    neomacs_display_protocol::ImageLayoutAdvance::new(Px(px)).expect("positive finite advance")
}

/// A window at the frame's left edge: window-local and frame-absolute
/// coordinates coincide, so this pins the rule itself.
fn leftmost_window(x_px: f32, right_edge_px: f32) -> WindowLocalRowExtent {
    WindowLocalRowExtent::from_frame_coordinates(
        DisplayRowTextAreaOrigin::row_local(),
        x_px,
        right_edge_px,
    )
    .expect("valid leftmost-window extent")
}

/// `produce_xwidget_glyph`, src/xdisp.c:32577-32579 (emacs-31.0.90), with
/// GNU's numbers: a 320 px TTY frame with one reserved column has
/// `last_visible_x` 312, and the widget after a one-cell "a" sits at 8.
#[test]
fn an_xwidget_wider_than_the_remaining_row_is_cropped_by_gnus_rule() {
    assert_eq!(
        DisplayXwidgetOverflowAction::for_xwidget(
            advance(100.0),
            leftmost_window(8.0, 312.0),
            false,
        ),
        DisplayXwidgetOverflowAction::Fits
    );
    assert_eq!(
        DisplayXwidgetOverflowAction::for_xwidget(
            advance(304.0),
            leftmost_window(8.0, 312.0),
            false,
        ),
        DisplayXwidgetOverflowAction::Fits,
        "crop == 0 is not a crop"
    );
    // Mid-row, wider than a quarter of the visible width: crop = 600 - 304.
    assert_eq!(
        DisplayXwidgetOverflowAction::for_xwidget(
            advance(600.0),
            leftmost_window(8.0, 312.0),
            false,
        ),
        DisplayXwidgetOverflowAction::CropAdvanceToVisibleWidth {
            advance: advance(304.0)
        }
    );
    // At hpos 0 the width does not matter.
    assert_eq!(
        DisplayXwidgetOverflowAction::for_xwidget(
            advance(40.0),
            leftmost_window(300.0, 312.0),
            true,
        ),
        DisplayXwidgetOverflowAction::CropAdvanceToVisibleWidth {
            advance: advance(12.0)
        }
    );
    // Narrow (40 <= 312 / 4) and mid-row: GNU leaves it whole.
    assert_eq!(
        DisplayXwidgetOverflowAction::for_xwidget(
            advance(40.0),
            leftmost_window(300.0, 312.0),
            false,
        ),
        DisplayXwidgetOverflowAction::LeaveWhole
    );
    // Nothing of the row is left; a zero-width crop would produce no glyph.
    assert_eq!(
        DisplayXwidgetOverflowAction::for_xwidget(
            advance(600.0),
            leftmost_window(312.0, 312.0),
            true,
        ),
        DisplayXwidgetOverflowAction::LeaveWhole
    );
}

/// GNU's quarter-width predicate compares against `it->last_visible_x`,
/// which is window-local (src/dispextern.h:2785-2791).  In a right-hand
/// split the frame-absolute right edge is about twice the window's width,
/// so comparing against it would leave a 300 px widget whole (300 <= 1592/4)
/// where GNU crops it (300 > 792/4).
#[test]
fn the_quarter_width_rule_uses_the_windows_own_width_in_a_right_hand_split() {
    // Right window of a 1600 px frame: text area at frame x 800, one
    // reserved column, so `last_visible_x` is 792 window-local; the widget
    // follows 70 cells of text and sits at window-local 560.
    let right_window = WindowLocalRowExtent::from_frame_coordinates(
        DisplayRowTextAreaOrigin::at_frame_x(800.0).expect("finite text-area origin"),
        1360.0,
        1592.0,
    )
    .expect("valid right-window extent");
    assert_eq!(right_window.last_visible_x_px(), 792.0);
    assert_eq!(right_window.remaining_px(), 232.0, "current_x is 560");

    assert_eq!(
        DisplayXwidgetOverflowAction::for_xwidget(advance(300.0), right_window, false),
        DisplayXwidgetOverflowAction::CropAdvanceToVisibleWidth {
            advance: advance(232.0)
        }
    );
    // The same widget at the same window-local place in the leftmost window
    // gets the same answer: the rule does not know where the window is.
    assert_eq!(
        DisplayXwidgetOverflowAction::for_xwidget(
            advance(300.0),
            leftmost_window(560.0, 792.0),
            false,
        ),
        DisplayXwidgetOverflowAction::CropAdvanceToVisibleWidth {
            advance: advance(232.0)
        }
    );
}

/// The line-number prefix is inside GNU's text area (`it->current_x` counts
/// it), so the origin is the text area's left edge, not the content's.
#[test]
fn the_text_area_origin_is_not_moved_by_a_line_number_prefix() {
    let origin = DisplayRowTextAreaOrigin::at_frame_x(100.0).expect("finite text-area origin");
    // content_x = 100 + 32 px of line numbers; the pen is 8 px past that.
    assert_eq!(origin.window_local(140.0), 40.0);
    assert_eq!(
        DisplayRowTextAreaOrigin::row_local().window_local(140.0),
        140.0
    );
}

#[test]
fn row_extent_rejects_non_finite_and_inverted_frame_geometry() {
    assert!(DisplayRowTextAreaOrigin::at_frame_x(f32::NAN).is_err());

    let origin = DisplayRowTextAreaOrigin::at_frame_x(100.0).expect("finite text-area origin");
    assert!(WindowLocalRowExtent::from_frame_coordinates(origin, f32::INFINITY, 180.0).is_err());
    assert!(WindowLocalRowExtent::from_frame_coordinates(origin, 181.0, 180.0).is_err());
    assert!(WindowLocalRowExtent::from_frame_coordinates(origin, 99.0, 180.0).is_err());
}

/// `produce_image_glyph`, src/xdisp.c:32492-32509 (emacs-31.1), with GNU's
/// numbers: a 320 px frame with one reserved column has `last_visible_x` 312
/// and an 8 px column, and the image after a one-cell "a" sits at 8.
///
/// The row here is a CHROME row (`DisplayRowLineWrap::chrome_row`), which GNU
/// truncates: `it->line_wrap == TRUNCATE` makes the word-wrap clause's first
/// disjunct true, so the quarter-width floor is the only width test left.
#[test]
fn an_image_wider_than_the_remaining_row_is_cropped_by_gnus_rule() {
    let truncating = DisplayRowLineWrap::Truncate;
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            100.0,
            leftmost_window(8.0, 312.0),
            false,
            8.0,
            0.0,
            truncating
        ),
        DisplayImageOverflowAction::Fits
    );
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            304.0,
            leftmost_window(8.0, 312.0),
            false,
            8.0,
            0.0,
            truncating
        ),
        DisplayImageOverflowAction::Fits,
        "crop == 0 is not a crop"
    );
    // Wider than a row of its own (600 > 312 - 0 - 8): crop = 600 - 304.
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            600.0,
            leftmost_window(8.0, 312.0),
            false,
            8.0,
            0.0,
            truncating
        ),
        DisplayImageOverflowAction::CropToVisibleWidth {
            advance: image_advance(304.0)
        }
    );
    // At hpos 0 the width does not matter.
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            40.0,
            leftmost_window(300.0, 312.0),
            true,
            8.0,
            0.0,
            truncating
        ),
        DisplayImageOverflowAction::CropToVisibleWidth {
            advance: image_advance(12.0)
        }
    );
    // Mid-row and narrower than a row of its own, but wider than a quarter of
    // it: under WORD_WRAP GNU leaves it whole so `display_line` can wrap it to
    // the next row.
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            200.0,
            leftmost_window(300.0, 312.0),
            false,
            8.0,
            0.0,
            DisplayRowLineWrap::WordWrap
        ),
        DisplayImageOverflowAction::LeaveWhole
    );
    // The same image on a truncating row: the word-wrap clause is the first
    // disjunct and this row does not word-wrap, so 200 > 312/4 = 78 crops.
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            200.0,
            leftmost_window(300.0, 312.0),
            false,
            8.0,
            0.0,
            truncating
        ),
        DisplayImageOverflowAction::CropToVisibleWidth {
            advance: image_advance(12.0)
        }
    );
    // Narrow mid-row image, past the quarter-width floor as well: no row crops
    // it, whatever its wrap method.
    for line_wrap in [
        DisplayRowLineWrap::Truncate,
        DisplayRowLineWrap::WindowWrap,
        DisplayRowLineWrap::WordWrap,
    ] {
        assert_eq!(
            DisplayImageOverflowAction::for_image(
                40.0,
                leftmost_window(300.0, 312.0),
                false,
                8.0,
                0.0,
                line_wrap
            ),
            DisplayImageOverflowAction::LeaveWhole,
            "{line_wrap:?}: 40 <= 312/4"
        );
    }
    // Nothing of the row is left; a zero-width crop would produce no glyph.
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            600.0,
            leftmost_window(312.0, 312.0),
            true,
            8.0,
            0.0,
            truncating
        ),
        DisplayImageOverflowAction::LeaveWhole
    );
}

/// The word-wrap clause, measured on GNU Emacs 31.1 under Xvfb.
///
/// A 752x720 frame, a 720 px text area and a 9 px column; 64 "x" (current_x
/// 576) then a 181 px image.  The crop fires only above `last_visible_x / 4`
/// = 180: GNU reports the first row's used width as 756 for a 180 px image and
/// 720 for a 181 px image (`window-lines-pixel-dimensions`), and the same
/// boundary holds for the truncated row in `tmp/midrow-image/crop-truncate.txt`.
#[test]
fn a_truncating_row_crops_a_mid_row_image_wider_than_a_quarter_of_the_row() {
    let truncating = DisplayRowLineWrap::Truncate;
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            180.0,
            leftmost_window(576.0, 720.0),
            false,
            9.0,
            0.0,
            truncating
        ),
        DisplayImageOverflowAction::LeaveWhole,
        "180 == 720/4 is not wider than a quarter; GNU's row ends at 756"
    );
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            181.0,
            leftmost_window(576.0, 720.0),
            false,
            9.0,
            0.0,
            truncating
        ),
        DisplayImageOverflowAction::CropToVisibleWidth {
            advance: image_advance(144.0)
        },
        "181 > 180; GNU's row ends at exactly 720"
    );
}

/// GNU crops a WINDOW_WRAP row's mid-row image too: the clause is only skipped
/// for WORD_WRAP, and `LineWrapMode::Wrap` collapses the two.
///
/// Measured, GNU Emacs 31.1, 720 px text area, 9 px column, 40 "x" then
/// " zzz" (current_x 396) and a 400 px image.  With `word-wrap` nil the image
/// ends at 720 on the first row (crop); with `word-wrap` t the row ends at 369
/// -- before the word -- and the image is whole on the next row
/// (tmp/midrow-image/wrap-modes.txt, cases 1 and 3).
#[test]
fn a_window_wrapping_row_crops_where_a_word_wrapping_row_does_not() {
    let extent = leftmost_window(396.0, 720.0);
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            400.0,
            extent,
            false,
            9.0,
            0.0,
            DisplayRowLineWrap::WordWrap
        ),
        DisplayImageOverflowAction::LeaveWhole,
        "400 <= 720 - 0 - 9 keeps its real width under WORD_WRAP"
    );
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            400.0,
            extent,
            false,
            9.0,
            0.0,
            DisplayRowLineWrap::WindowWrap
        ),
        DisplayImageOverflowAction::CropToVisibleWidth {
            advance: image_advance(324.0)
        },
        "WINDOW_WRAP crops: 400 > 180 and 400 - 324 > 0"
    );
}

/// The same WORD_WRAP row crops when the image is wider than a row of its own
/// ("Always crop images larger than the window-width, minus 1 space"):
/// measured on GNU 31.1, a 900 px image at current_x 396 ends at 720 on the
/// first row and the word stays with it.
#[test]
fn a_word_wrapping_row_still_crops_an_image_wider_than_a_row_of_its_own() {
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            900.0,
            leftmost_window(396.0, 720.0),
            false,
            9.0,
            0.0,
            DisplayRowLineWrap::WordWrap
        ),
        DisplayImageOverflowAction::CropToVisibleWidth {
            advance: image_advance(324.0)
        },
        "900 > 720 - 0 - 9"
    );
}

/// "Always crop images larger than the window-width, minus 1 space" subtracts
/// the line-number field too, so an image that fits the bare row but not the
/// row left after the numbers is still cropped.
#[test]
fn the_row_minus_one_space_threshold_subtracts_the_line_number_field() {
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            300.0,
            leftmost_window(200.0, 312.0),
            false,
            8.0,
            0.0,
            DisplayRowLineWrap::WordWrap
        ),
        DisplayImageOverflowAction::LeaveWhole,
        "300 <= 312 - 0 - 8"
    );
    assert_eq!(
        DisplayImageOverflowAction::for_image(
            300.0,
            leftmost_window(200.0, 312.0),
            false,
            8.0,
            32.0,
            DisplayRowLineWrap::WordWrap
        ),
        DisplayImageOverflowAction::CropToVisibleWidth {
            advance: image_advance(112.0)
        },
        "300 > 312 - 32 - 8"
    );
}
