use super::*;

#[test]
fn setter_under_temporary_limit_only_reapplies_its_target() {
    let mut state = FrameOpacityState::default();
    state.accept(1, [0.3; 2], 0.0);
    state.accept(2, [0.5; 2], 0.0);
    state.focus(1, true);
    state.set_lower_limit(0.8);
    state.accept(2, [0.5; 2], 0.8);
    state.set_lower_limit(0.0);
    assert_eq!(state.applied(1), Some(0.3));
    assert_eq!(state.applied(2), Some(0.8));
}

#[test]
fn unchanged_highlight_and_stale_focus_do_not_reapply_policy() {
    let mut state = FrameOpacityState::default();
    for frame in [1, 2, 3] {
        state.accept(frame, [0.7, 0.3], 0.0);
    }
    state.focus(1, true);
    state.set_redirects(vec![(1, Some(2))]);
    state.set_lower_limit(0.9);
    state.set_redirects(vec![(1, Some(2)), (3, Some(1))]);
    state.focus(1, true);
    state.focus(3, false);
    assert_eq!(state.applied(1), Some(0.3));
    assert_eq!(state.applied(2), Some(0.7));
    assert_eq!(state.applied(3), Some(0.3));
    state.set_redirects(vec![(1, Some(3))]);
    assert_eq!(state.applied(1), Some(0.3));
    assert_eq!(state.applied(2), Some(0.9));
    assert_eq!(state.applied(3), Some(0.9));
}

#[test]
fn accepted_numeric_then_nil_survives_absent_and_replaced_scenes() {
    let mut state = FrameOpacityState::default();
    state.focus(1, true);
    for frame in [1, 2] {
        state.accept(frame, [0.5; 2], 0.2);
        state.accept(frame, [-1.0; 2], 0.2);
        assert_eq!(state.applied(frame), Some(0.5));
    }
    state.focus(1, false);
    state.focus(2, true);
    assert_eq!(state.applied(1), Some(0.5));
    assert_eq!(state.applied(2), Some(0.5));
}

#[test]
fn lower_limit_changes_apply_at_focus_without_replaying_nil() {
    let mut state = FrameOpacityState::default();
    state.accept(1, [0.8, 0.3], 0.2);
    state.focus(1, true);
    state.set_lower_limit(0.6);
    assert_eq!(state.applied(1), Some(0.8));
    state.focus(1, false);
    assert_eq!(state.applied(1), Some(0.6));
    state.accept(1, [-1.0; 2], 0.6);
    state.set_lower_limit(0.9);
    state.focus(1, true);
    assert_eq!(state.applied(1), Some(0.6));
}

#[test]
fn nil_component_retains_last_scalar_not_previous_policy_component() {
    let mut state = FrameOpacityState::default();
    state.focus(1, true);
    state.accept(1, [0.8, 0.4], 0.2);
    state.focus(1, false);
    state.accept(1, [-1.0, 0.6], 0.2);
    state.focus(1, true);
    assert_eq!(state.applied(1), Some(0.6));
    state.accept(1, [-1.0; 2], 0.2);
    state.focus(1, false);
    assert_eq!(state.applied(1), Some(0.6));
}

#[test]
fn redirect_clear_dead_target_and_late_leave_use_effective_highlight() {
    let mut state = FrameOpacityState::default();
    for frame in [1, 2, 3] {
        state.accept(frame, [0.8, 0.4], 0.2);
    }
    state.focus(1, true);
    assert_eq!((state.applied(1), state.applied(2)), (Some(0.8), Some(0.4)));
    state.set_redirects(vec![(1, Some(2))]);
    assert_eq!((state.applied(1), state.applied(2)), (Some(0.4), Some(0.8)));
    state.set_redirects(vec![(1, None)]);
    assert_eq!((state.applied(1), state.applied(2)), (Some(0.8), Some(0.4)));
    state.set_redirects(vec![(1, Some(2)), (2, Some(3))]);
    assert_eq!(state.applied(2), Some(0.8)); // one hop, not recursive
    state.retire(2);
    assert_eq!(state.applied(1), Some(0.8));
    state.focus(3, true);
    state.focus(1, false);
    assert_eq!(state.applied(3), Some(0.8));
}
