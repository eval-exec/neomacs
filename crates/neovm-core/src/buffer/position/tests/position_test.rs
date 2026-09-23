use super::*;

#[test]
fn text_position_bounds_keep_char_and_byte_anchor_pairs_together() {
    let mut bounds = TextPositionBounds::new(TextPositionAnchor::from_usize(20, 40));

    bounds.consider_char_anchor(CharPos0::new(10), TextPositionAnchor::from_usize(5, 7));
    bounds.consider_char_anchor(CharPos0::new(10), TextPositionAnchor::from_usize(15, 27));
    bounds.consider_char_anchor(CharPos0::new(10), TextPositionAnchor::from_usize(12, 24));

    assert_eq!(bounds.below(), TextPositionAnchor::from_usize(5, 7));
    assert_eq!(bounds.above(), TextPositionAnchor::from_usize(12, 24));
    assert_eq!(
        bounds.nearest_char_anchor(CharPos0::new(10)),
        TextPositionAnchor::from_usize(12, 24)
    );

    let mut byte_bounds = TextPositionBounds::new(TextPositionAnchor::from_usize(20, 40));
    byte_bounds.consider_byte_anchor(
        EmacsBytePos::new(30),
        TextPositionAnchor::from_usize(11, 29),
    );
    byte_bounds.consider_byte_anchor(
        EmacsBytePos::new(30),
        TextPositionAnchor::from_usize(13, 33),
    );

    assert_eq!(byte_bounds.below(), TextPositionAnchor::from_usize(11, 29));
    assert_eq!(byte_bounds.above(), TextPositionAnchor::from_usize(13, 33));
    assert_eq!(
        byte_bounds.byte_below_distance(EmacsBytePos::new(30)),
        EmacsByteLen::new(1)
    );
    assert_eq!(
        byte_bounds.byte_above_distance(EmacsBytePos::new(30)),
        EmacsByteLen::new(3)
    );
    assert_eq!(
        byte_bounds.nearest_byte_anchor(EmacsBytePos::new(30)),
        TextPositionAnchor::from_usize(11, 29)
    );
}

#[test]
fn text_position_hint_contributes_backend_anchor_without_exposing_storage_shape() {
    let hint = TextPositionHint::from_anchor(TextPositionAnchor::from_usize(8, 12));

    let mut char_bounds = TextPositionBounds::new(TextPositionAnchor::from_usize(20, 40));
    hint.consider_char_anchor(&mut char_bounds, CharPos0::new(10));
    assert_eq!(char_bounds.below(), TextPositionAnchor::from_usize(8, 12));

    let mut byte_bounds = TextPositionBounds::new(TextPositionAnchor::from_usize(20, 40));
    hint.consider_byte_anchor(&mut byte_bounds, EmacsBytePos::new(10));
    assert_eq!(byte_bounds.above(), TextPositionAnchor::from_usize(8, 12));
}
