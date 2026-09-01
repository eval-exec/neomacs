use super::*;
use neomacs_display_protocol::{TransitionAxis, TransitionDirection};

#[test]
fn slide_offsets_follow_the_typed_axis_and_direction() {
    let (old, new) = slide_offsets(
        TransitionAxis::Horizontal,
        TransitionDirection::Forward,
        100.0,
        0.25,
    );
    assert_eq!(old, [-25.0, 0.0]);
    assert_eq!(new, [75.0, 0.0]);

    let (old, new) = slide_offsets(
        TransitionAxis::Vertical,
        TransitionDirection::Backward,
        100.0,
        0.25,
    );
    assert_eq!(old, [0.0, 25.0]);
    assert_eq!(new, [0.0, -75.0]);
}
