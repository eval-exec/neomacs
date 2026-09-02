use super::DisplayMediaReplacementOverflowAction;

/// GNU `produce_xwidget_glyph` (src/xdisp.c:32700-32704): the crop applies
/// to a glyph that starts the row, or one wider than a quarter of the
/// visible width; anything narrower mid-row is left for `display_line`.
#[test]
fn media_wider_than_the_remaining_row_is_cropped_by_gnus_rule() {
    // Fits: no crop needed.
    assert_eq!(
        DisplayMediaReplacementOverflowAction::for_replacement(100.0, 8.0, 312.0, false),
        DisplayMediaReplacementOverflowAction::Fits
    );
    assert_eq!(
        DisplayMediaReplacementOverflowAction::for_replacement(304.0, 8.0, 312.0, false),
        DisplayMediaReplacementOverflowAction::Fits,
        "exactly filling the row is a fit"
    );
    // Wider than a quarter of the visible width: cropped to what is left.
    assert_eq!(
        DisplayMediaReplacementOverflowAction::for_replacement(600.0, 8.0, 312.0, false),
        DisplayMediaReplacementOverflowAction::CropToVisibleWidth {
            visible_width_px: 304.0
        }
    );
    // Starts the row: cropped however narrow it is.
    assert_eq!(
        DisplayMediaReplacementOverflowAction::for_replacement(40.0, 300.0, 312.0, true),
        DisplayMediaReplacementOverflowAction::CropToVisibleWidth {
            visible_width_px: 12.0
        }
    );
    // Narrow and mid-row: GNU does not crop (`display_line` continues the
    // line instead).
    assert_eq!(
        DisplayMediaReplacementOverflowAction::for_replacement(40.0, 300.0, 312.0, false),
        DisplayMediaReplacementOverflowAction::LeaveWhole
    );
    // Nothing visible at all: nothing to crop to.
    assert_eq!(
        DisplayMediaReplacementOverflowAction::for_replacement(600.0, 312.0, 312.0, true),
        DisplayMediaReplacementOverflowAction::LeaveWhole
    );
}
