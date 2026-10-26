use super::{CompletedEobOverlayMotionExtent, EobOverlayMotionCoverage, WindowDisplaySnapshot};
use crate::buffer::LispCharPos1;

#[test]
fn completed_eob_overlay_extent_is_relative_to_its_first_physical_row() {
    let anchor = LispCharPos1::new(4);
    // "abc" followed by "\nX\n" ends two rows after its first overlay row,
    // including the empty final row that need not be emitted for painting.
    let extent = CompletedEobOverlayMotionExtent::from_physical_rows(anchor, 7, 9).unwrap();
    let translated = CompletedEobOverlayMotionExtent::from_physical_rows(anchor, 107, 109).unwrap();
    assert_eq!(extent.anchor(), anchor);
    assert_eq!(extent.following_rows(), 2);
    assert_eq!(extent, translated);
}

#[test]
fn completed_eob_overlay_extent_refuses_a_reversed_physical_interval() {
    assert!(
        CompletedEobOverlayMotionExtent::from_physical_rows(LispCharPos1::new(4), 9, 7).is_none()
    );
}

#[test]
fn snapshot_identity_includes_eob_overlay_completion_evidence() {
    let plain = WindowDisplaySnapshot::default();
    let mut measured = plain.clone();
    measured.eob_overlay_motion = EobOverlayMotionCoverage::Incomplete {
        anchor: LispCharPos1::new(4),
    };
    assert_ne!(plain, measured);
    let incomplete = measured.clone();
    measured.eob_overlay_motion = EobOverlayMotionCoverage::Complete(
        CompletedEobOverlayMotionExtent::from_physical_rows(LispCharPos1::new(4), 0, 2).unwrap(),
    );
    assert_ne!(incomplete, measured);
}
