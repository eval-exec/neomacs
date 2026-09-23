use super::*;

#[test]
fn popup_placement_flips_to_the_opposite_side_before_shifting_into_viewport() {
    let placement = PopupPlacement::new(
        Rect::new(80.0, 90.0, 10.0, 10.0),
        PopupPreferredSide::Below,
        Point::ZERO,
        PopupConstraintPolicy::FlipAndShift { padding: 4.0 },
    );

    let resolved = placement.resolve(Size::new(30.0, 40.0), Rect::new(0.0, 0.0, 100.0, 100.0));

    assert_eq!(resolved.side(), PopupPreferredSide::Above);
    assert_eq!(resolved.origin(), Point::new(66.0, 50.0));
}

#[test]
fn popup_placement_preserves_unconstrained_explicit_coordinates() {
    let placement = PopupPlacement::at(Point::new(120.0, 140.0));

    let resolved = placement.resolve(Size::new(30.0, 40.0), Rect::new(0.0, 0.0, 100.0, 100.0));

    assert_eq!(resolved.side(), PopupPreferredSide::AtAnchor);
    assert_eq!(resolved.origin(), Point::new(120.0, 140.0));
}

#[test]
fn constrained_at_anchor_coordinates_shift_without_inventing_a_side() {
    let placement = PopupPlacement::new(
        Rect::new(95.0, 98.0, 0.0, 0.0),
        PopupPreferredSide::AtAnchor,
        Point::new(2.0, 3.0),
        PopupConstraintPolicy::Shift { padding: 4.0 },
    );

    let resolved = placement.resolve(Size::new(30.0, 40.0), Rect::new(0.0, 0.0, 100.0, 100.0));

    assert_eq!(resolved.side(), PopupPreferredSide::AtAnchor);
    assert_eq!(resolved.origin(), Point::new(66.0, 56.0));
}
