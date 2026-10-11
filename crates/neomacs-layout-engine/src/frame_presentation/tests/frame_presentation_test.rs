use super::*;
use neomacs_display_protocol::{
    DisplayFrameId, FrameDisplayState, FrameRect, InteractionId, ParentFrameRect, PresentationId,
    PresentedFramePlacement, PresentedPointerRegion, PresentedPointerSourceMap,
};

fn resolved_state(revision: u64) -> FrameDisplayState {
    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    state.presentation_id = PresentationId::new(revision);
    state.frame_placement = PresentedFramePlacement::new(
        DisplayFrameId::new(7),
        state.presentation_id,
        None,
        ParentFrameRect::new(0.0, 0.0, 640.0, 384.0).unwrap(),
        0,
    );
    state
}

#[test]
fn composer_seals_all_spatial_products_under_one_revision() {
    let resolved = ResolvedFrame::new(resolved_state(41)).expect("coherent resolved frame");

    let sealed = PresentationComposer::compose(
        resolved,
        PresentationInputs::new(&[], PresentedResizeEdge::Trailing),
    )
    .expect("valid presentation");

    assert_eq!(sealed.presentation(), PresentationId::new(41));
    assert_eq!(sealed.state().presentation_id, PresentationId::new(41));
    assert_eq!(
        sealed.state().presented_hit_index.presentation(),
        PresentationId::new(41)
    );
}

#[test]
fn composer_replaces_any_presealed_pointer_projection() {
    let mut state = resolved_state(41);
    state.presented_pointer_source = PresentedPointerSourceMap::new(
        vec![PresentedPointerRegion::new(
            FrameRect::new(0.0, 0.0, 8.0, 16.0).unwrap(),
            Some(InteractionId::new(99)),
            None,
        )],
        vec![],
    );
    let resolved = ResolvedFrame::new(state).expect("coherent resolved frame");

    let sealed = PresentationComposer::compose(
        resolved,
        PresentationInputs::new(&[], PresentedResizeEdge::Trailing),
    )
    .expect("valid presentation");

    assert!(
        sealed.state().presented_pointer_source.is_empty(),
        "pointer metadata must be derived while sealing, not trusted from a mutable builder"
    );
}

#[test]
fn resolved_frame_rejects_mismatched_placement_revision() {
    let mut state = resolved_state(41);
    state.frame_placement = PresentedFramePlacement::new(
        DisplayFrameId::new(7),
        PresentationId::new(40),
        None,
        ParentFrameRect::new(0.0, 0.0, 640.0, 384.0).unwrap(),
        0,
    );

    assert_eq!(
        ResolvedFrame::new(state).unwrap_err(),
        PresentationComposeError::StaleFramePlacement {
            frame: DisplayFrameId::new(7),
            expected: PresentationId::new(41),
            available: PresentationId::new(40),
        }
    );
}

#[test]
fn resolved_frame_requires_a_real_revision() {
    let state = FrameDisplayState::new(80, 24, 8.0, 16.0);

    assert_eq!(
        ResolvedFrame::new(state).unwrap_err(),
        PresentationComposeError::MissingRevision
    );
}
