use crate::{
    DisplayFrameId, FrameDisplayState, ParentFrameRect, PresentationId, PresentedFramePlacement,
    PresentedHitIndex, SealedFramePresentation,
};

fn coherent_state(revision: u64) -> FrameDisplayState {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.presentation_id = PresentationId::new(revision);
    state.frame_placement = PresentedFramePlacement::new(
        DisplayFrameId::new(7),
        state.presentation_id,
        None,
        ParentFrameRect::new(0.0, 0.0, 640.0, 384.0).unwrap(),
        0,
    );
    state.presented_hit_index =
        PresentedHitIndex::from_parts(state.presentation_id, vec![], vec![]).unwrap();
    state
}

#[test]
fn sealed_presentation_exposes_read_only_coherent_transport() {
    let sealed = SealedFramePresentation::seal(coherent_state(41)).unwrap();

    assert_eq!(sealed.presentation(), PresentationId::new(41));
    assert_eq!(sealed.state().presentation_id, PresentationId::new(41));
}

#[test]
fn sealed_presentation_rejects_a_stale_frame_placement() {
    let mut state = coherent_state(41);
    state.frame_placement = PresentedFramePlacement::new(
        DisplayFrameId::new(7),
        PresentationId::new(40),
        None,
        ParentFrameRect::new(0.0, 0.0, 640.0, 384.0).unwrap(),
        0,
    );

    assert!(SealedFramePresentation::seal(state).is_err());
}
