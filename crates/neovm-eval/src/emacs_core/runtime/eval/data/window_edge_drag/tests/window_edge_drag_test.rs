use super::*;

fn grab(button: MouseButton, region: PresentedRegionKind) -> WindowEdgeGrab {
    WindowEdgeGrab::of(button, region).expect("this region drags a window edge")
}

fn session_of(drag: &WindowEdgeDrag) -> InteractionSessionId {
    match drag.origin() {
        PresentationOrigin::InteractiveResize { session } => session,
        other => panic!("expected a drag to be running, found {other:?}"),
    }
}

#[test]
fn a_press_on_a_mode_line_or_a_divider_grabs_a_window_edge() {
    // These are the regions GNU binds `down-mouse-1` on to a `mouse-drag-*`
    // command that resizes a window. If one stopped counting as a grab, drags
    // on that edge would animate against the user's hand.
    for region in [
        PresentedRegionKind::ModeLine,
        PresentedRegionKind::HeaderLine,
        PresentedRegionKind::TabLine,
        PresentedRegionKind::RightDivider,
        PresentedRegionKind::BottomDivider,
    ] {
        assert!(
            WindowEdgeGrab::of(MouseButton::Left, region).is_some(),
            "{region:?} drags a window edge"
        );
    }
}

#[test]
fn a_press_on_text_or_chrome_grabs_no_window_edge() {
    // The grab is the only way to open a session, so if text counted the
    // compositor would stop animating for the whole of an ordinary click-drag
    // selection.
    for region in [
        PresentedRegionKind::TextBody,
        PresentedRegionKind::LeftFringe,
        PresentedRegionKind::RightMargin,
        PresentedRegionKind::HorizontalScrollBar,
        PresentedRegionKind::MenuBar,
        PresentedRegionKind::ToolBar,
        PresentedRegionKind::TabBar,
    ] {
        assert!(
            WindowEdgeGrab::of(MouseButton::Left, region).is_none(),
            "{region:?} drags no window edge"
        );
    }
}

#[test]
fn a_presentation_composed_while_no_edge_is_held_is_ordinary() {
    // The default has to be "animate": every redisplay that is not a drag
    // passes through here.
    let drag = WindowEdgeDrag::default();
    assert_eq!(drag.origin(), PresentationOrigin::Ordinary);
}

#[test]
fn a_presentation_composed_while_an_edge_is_held_suppresses_layout_motion() {
    // This is the whole point of the step: every commit the drag makes must
    // install where the hand put it.
    let mut drag = WindowEdgeDrag::default();
    drag.grabbed(grab(MouseButton::Left, PresentedRegionKind::ModeLine));
    assert!(drag.origin().suppresses_layout_motion());
}

#[test]
fn releasing_the_button_that_grabbed_the_edge_ends_the_drag() {
    // The release is the drag's only ordinary terminator. If it did not end
    // the session, the editor would never animate again.
    let mut drag = WindowEdgeDrag::default();
    drag.grabbed(grab(MouseButton::Left, PresentedRegionKind::RightDivider));
    drag.released(MouseButton::Left);
    assert_eq!(drag.origin(), PresentationOrigin::Ordinary);
}

#[test]
fn releasing_a_different_button_leaves_the_drag_running() {
    // `mouse-drag-line`'s transient map reacts to the button that started the
    // drag; a middle-click during the drag must not drop the divider.
    let mut drag = WindowEdgeDrag::default();
    drag.grabbed(grab(MouseButton::Left, PresentedRegionKind::ModeLine));
    drag.released(MouseButton::Middle);
    assert!(drag.origin().suppresses_layout_motion());
}

#[test]
fn a_second_press_during_a_drag_does_not_start_a_second_drag() {
    // The edge is already in hand. Minting a new session here would leave the
    // first one's id unmatched by the release that ends the drag.
    let mut drag = WindowEdgeDrag::default();
    drag.grabbed(grab(MouseButton::Left, PresentedRegionKind::ModeLine));
    let first = session_of(&drag);
    drag.grabbed(grab(MouseButton::Right, PresentedRegionKind::BottomDivider));
    assert_eq!(session_of(&drag), first);
    drag.released(MouseButton::Left);
    assert_eq!(drag.origin(), PresentationOrigin::Ordinary);
}

#[test]
fn two_consecutive_drags_are_stamped_with_different_sessions() {
    // Reusing an id would make the previous drag's commits indistinguishable
    // from this one's, which is exactly what naming the causal extent is for.
    let mut drag = WindowEdgeDrag::default();
    drag.grabbed(grab(MouseButton::Left, PresentedRegionKind::ModeLine));
    let first = session_of(&drag);
    drag.released(MouseButton::Left);
    drag.grabbed(grab(MouseButton::Left, PresentedRegionKind::ModeLine));
    let second = session_of(&drag);
    assert!(second > first, "session ids advance, they are never reused");
}

#[test]
fn losing_the_pointer_without_a_release_ends_the_drag() {
    // A frame that loses focus mid-drag never sees the button come up. Without
    // this the session would stay open and every later presentation would
    // install instantly, for the rest of the session.
    let mut drag = WindowEdgeDrag::default();
    drag.grabbed(grab(MouseButton::Left, PresentedRegionKind::RightDivider));
    drag.abandoned();
    assert_eq!(drag.origin(), PresentationOrigin::Ordinary);
}
