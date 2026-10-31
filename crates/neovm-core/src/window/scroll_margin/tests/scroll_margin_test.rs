use super::ScrollMargin;

/// GNU `window_scroll_margin` (src/window.c:6207) as a table:
/// `(scroll-margin, maximum-scroll-margin, window-lines) -> margin`.
#[test]
fn effective_margin_matches_gnu_window_scroll_margin() {
    let gnu = |lines: i64, maximum: Option<f64>, window_lines: i64| {
        ScrollMargin::from_lisp_values(lines, maximum).lines(window_lines)
    };
    assert_eq!(gnu(999, Some(0.5), 28), 13, "capped at (lines - 1) / 2");
    assert_eq!(gnu(999, Some(0.5), 49), 24, "half of an odd window");
    assert_eq!(gnu(999, None, 28), 7, "default maximum is 0.25");
    assert_eq!(gnu(999, Some(0.25), 23), 5, "fraction beats the half cap");
    assert_eq!(gnu(5, Some(0.5), 28), 5, "a smaller request is untouched");
    assert_eq!(
        gnu(5, Some(0.0), 28),
        0,
        "a zero fraction disables the margin"
    );
    assert_eq!(gnu(-10, Some(0.5), 28), 0, "negative requests are zero");
    assert_eq!(gnu(999, Some(0.5), 2), 0, "no room outside the margins");
    assert_eq!(gnu(999, Some(0.5), 1), 0);
    assert_eq!(gnu(999, Some(0.5), 0), 0);
}

/// GNU ignores non-float `maximum-scroll-margin` values and clamps the rest.
#[test]
fn maximum_fraction_is_normalized_like_gnu() {
    let effective = |maximum: Option<f64>| ScrollMargin::from_lisp_values(999, maximum).lines(40);
    assert_eq!(
        effective(None),
        10,
        "non-float values leave the 0.25 default"
    );
    assert_eq!(
        effective(Some(0.75)),
        19,
        "fractions above 0.5 clamp, still bounded by (lines - 1) / 2"
    );
    assert_eq!(effective(Some(-1.0)), 0, "fractions below 0 clamp");
    assert_eq!(effective(Some(0.3)), 12, "fractions in range pass through");
}

#[test]
fn pixel_margin_uses_whole_window_lines_like_gnu() {
    let margin = ScrollMargin::from_lisp_values(999, Some(0.5));
    assert_eq!(margin.pixels(560, 20), 260, "28 lines, 13 margin lines");
    // A partial bottom line is not a full margin line (GNU divides first).
    assert_eq!(margin.pixels(550, 20), 260, "27 whole lines");
    assert_eq!(margin.pixels(0, 20), 0);
}

#[test]
fn last_usable_line_is_the_bottom_margin_row() {
    let half = ScrollMargin::from_lisp_values(999, Some(0.5));
    assert_eq!(half.lines(28), 13);
    assert_eq!(half.last_usable_line(28), 14);
    assert_eq!(ScrollMargin::default().last_usable_line(28), 27);
    assert_eq!(half.last_usable_line(1), 0);
    assert_eq!(half.last_usable_line(0), 0);
}

#[test]
fn zero_request_skips_the_margin_entirely() {
    let none = ScrollMargin::default();
    assert!(none.is_zero());
    assert_eq!(none.lines(40), 0);
    assert_eq!(none.pixels(800, 20), 0);
    assert!(!ScrollMargin::from_lisp_values(1, Some(0.5)).is_zero());
}

#[test]
fn non_finite_fractions_fall_back_to_the_default() {
    let effective = |maximum: f64| ScrollMargin::from_lisp_values(999, Some(maximum)).lines(40);
    assert_eq!(effective(f64::NAN), 10);
    assert_eq!(effective(f64::INFINITY), 10);
}
