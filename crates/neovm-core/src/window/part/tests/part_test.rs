#[test]
fn a_bottom_divider_reports_the_posn_its_drag_command_is_bound_to() {
    // GNU binds `[bottom-divider down-mouse-1]` to `mouse-drag-mode-line`
    // (lisp/mouse.el:3825) and sets `Qbottom_divider` for the part
    // (src/keyboard.c:5983). Reporting anything else — this used to say
    // `horizontal-scroll-bar` — means that binding never matches, so
    // dragging a bottom divider to resize windows does nothing at all.
    assert_eq!(
        window_part_of_region(neomacs_display_protocol::PresentedRegionKind::BottomDivider)
            .and_then(WindowPart::area_symbol),
        Some("bottom-divider")
    );
}

#[test]
fn a_right_divider_is_not_reported_as_a_vertical_line() {
    // GNU sets `Qright_divider` (src/keyboard.c:5976). Both symbols happen
    // to be bound to `mouse-drag-vertical-line`, so the drag worked by
    // accident, but a user keymap on `[right-divider ...]` did not — and
    // `posn-area` lied about where the click was.
    assert_eq!(
        window_part_of_region(neomacs_display_protocol::PresentedRegionKind::RightDivider)
            .and_then(WindowPart::area_symbol),
        Some("right-divider")
    );
}

#[test]
fn every_presented_region_either_names_a_window_part_or_is_a_frame_bar() {
    // The exhaustive match is what makes a new presented region a
    // compile-time prompt to decide which it is. This asserts the runtime
    // half: that the only regions with no window part are the three frame
    // bars, which GNU answers with the frame rather than a window.
    use neomacs_display_protocol::PresentedRegionKind as Kind;
    for kind in [
        Kind::TextBody,
        Kind::LeftMargin,
        Kind::RightMargin,
        Kind::LeftFringe,
        Kind::RightFringe,
        Kind::LeftScrollBar,
        Kind::RightScrollBar,
        Kind::HorizontalScrollBar,
        Kind::TabLine,
        Kind::HeaderLine,
        Kind::ModeLine,
        Kind::RightDivider,
        Kind::BottomDivider,
    ] {
        assert!(
            window_part_of_region(kind).is_some(),
            "{kind:?} is part of a window and must name a WindowPart"
        );
    }
    for kind in [Kind::MenuBar, Kind::ToolBar, Kind::CompactBar, Kind::TabBar] {
        assert!(
            window_part_of_region(kind).is_none(),
            "{kind:?} replaces the window with the frame in GNU's posn"
        );
    }
}
use super::*;

/// An 80x22 terminal window with a mode line, laid out below a one-line
/// menu bar: the geometry every probe in `scripts/below-content-audit.el`
/// runs against.
fn tty_window(header: i64, mode: i64) -> WindowPartGeometry {
    WindowPartGeometry::new(
        Rect::new(0.0, 1.0, 80.0, 22.0),
        0,
        header,
        mode,
        0,
        80,
        0,
        0,
        1,
        true,
    )
}

#[test]
fn the_bottom_row_of_a_window_with_a_mode_line_is_the_mode_line() {
    let geometry = tty_window(0, 1);
    assert_eq!(geometry.classify(0, 22), Some(WindowPart::ModeLine));
    assert_eq!(geometry.classify(0, 21), Some(WindowPart::Text));
    // One row further down is the next window's, not this one's.
    assert_eq!(geometry.classify(0, 23), None);
}

#[test]
fn the_top_row_of_a_window_with_a_header_line_is_the_header_line() {
    // GNU's `posn-at-x-y` doc string: "the text area includes the
    // header-line and the tab-line of the window", so a Y of 0 in such a
    // window is the header line and not the first line of text.
    let geometry = tty_window(1, 1);
    assert_eq!(geometry.classify(0, 1), Some(WindowPart::HeaderLine));
    assert_eq!(geometry.classify(0, 2), Some(WindowPart::Text));
}

#[test]
fn a_window_with_no_mode_line_has_text_in_its_bottom_row() {
    let geometry = tty_window(0, 0);
    assert_eq!(geometry.classify(0, 22), Some(WindowPart::Text));
    assert_eq!(geometry.classify(0, 23), None);
}

#[test]
fn a_chrome_coordinate_carries_no_text_area_witness() {
    // The type-level half of ledger 205's residual 2: the buffer-position
    // lookup is reachable only from the arm GNU reaches it from.
    let geometry = tty_window(1, 1);
    assert!(matches!(
        geometry.resolve(5, 22),
        Some(WindowCoordinate::ChromeLine {
            line: WindowChromeLine::ModeLine,
            window_x: 5,
            window_y: 21,
        })
    ));
    assert!(matches!(
        geometry.resolve(5, 1),
        Some(WindowCoordinate::ChromeLine {
            line: WindowChromeLine::HeaderLine,
            window_y: 0,
            ..
        })
    ));
    let Some(WindowCoordinate::Buffer { at, .. }) = geometry.resolve(5, 3) else {
        panic!("a text coordinate must carry a text-area witness");
    };
    // `y2 = wy` goes to the walk; `yret` is what the posn reports.
    assert_eq!(
        (at.text_area_x(), at.window_y(), at.text_area_y()),
        (5, 2, 1)
    );
}

#[test]
fn the_last_column_of_a_non_rightmost_terminal_window_is_the_border() {
    let mut geometry = tty_window(0, 1);
    geometry.rightmost = false;
    geometry.right_x = 56;
    geometry.text_area_width = 56;
    assert_eq!(geometry.classify(55, 5), Some(WindowPart::VerticalBorder));
    assert_eq!(geometry.classify(54, 5), Some(WindowPart::Text));
}
