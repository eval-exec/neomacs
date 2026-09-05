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

// =======================================================================
// Presentation origin
// =======================================================================

#[test]
fn a_freshly_built_presentation_is_ordinary_and_animates() {
    // Nothing has to opt in to normal transition policy; only an interactive
    // resize opts out of it.
    let state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    assert_eq!(
        state.origin,
        crate::presentation_origin::PresentationOrigin::Ordinary
    );
    assert!(!state.origin.suppresses_layout_motion());
}

#[test]
fn a_sealed_presentation_carries_the_origin_it_was_built_with() {
    use crate::presentation_origin::{InteractionSessionId, PresentationOrigin};

    let session = InteractionSessionId::FIRST;
    let mut state = coherent_state(1);
    state.origin = PresentationOrigin::InteractiveResize { session };
    let sealed = SealedFramePresentation::seal(state).expect("coherent state seals");
    assert!(sealed.state().origin.suppresses_layout_motion());
    assert!(sealed.state().origin.belongs_to(session));
}

// =======================================================================
// Window identity
// =======================================================================

/// A presented window with an ordinary, non-placeholder identity.
fn presented_window(id: i64) -> crate::frame_glyphs::WindowInfo {
    crate::frame_glyphs::WindowInfo {
        window_id: crate::types::DisplayWindowId::new(id),
        buffer_id: 7,
        window_start: 1,
        window_end: 100,
        buffer_size: 1000,
        buffer_modiff: crate::presentation_origin::BufferModiff::new(1),
        bounds: crate::types::Rect::new(0.0, 0.0, 800.0, 600.0),
        geometry: crate::PresentedWindowGeometry::default(),
        line_number_field: None,
        mode_line_height: 20.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_name: String::from("scratch"),
        buffer_file_name: String::new(),
        modified: false,
    }
}

#[test]
fn sealing_rejects_a_presented_window_carrying_the_zero_placeholder() {
    // Zero means "no window" elsewhere in the display engine (chrome slots, the
    // glyph-builder cursor, media surfaces with no host). A *presented window*
    // may not use it: continuity keys on identity, so two unrelated entries
    // both claiming zero would pair as one window that moved.
    let mut state = coherent_state(1);
    state.window_infos.push(presented_window(4));
    state.window_infos.push(presented_window(0));

    assert_eq!(
        SealedFramePresentation::seal(state).err(),
        Some(super::SealFramePresentationError::PlaceholderWindowIdentity { index: 1 }),
        "the offending entry is named by index"
    );
}

#[test]
fn sealing_accepts_ordinary_window_identities() {
    let mut state = coherent_state(1);
    state.window_infos.push(presented_window(4));
    state.window_infos.push(presented_window(9));
    assert!(SealedFramePresentation::seal(state).is_ok());
}
