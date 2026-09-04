use super::{DisplayXwidgetOverflowAction, WindowLocalRowExtent};
use crate::display_row::geometry::DisplayRowTextAreaOrigin;
use neomacs_display_protocol::{Px, XwidgetLayoutAdvance};

fn advance(px: f32) -> XwidgetLayoutAdvance {
    XwidgetLayoutAdvance::new(Px(px)).expect("positive finite layout advance")
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
