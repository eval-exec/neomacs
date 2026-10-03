use super::*;

#[test]
fn explicit_policy_is_required_and_invalid_values_preserve_baseline() {
    for value in [
        None,
        Some(OsStr::new("")),
        Some(OsStr::new("off")),
        Some(OsStr::new("unknown")),
    ] {
        assert_eq!(parse_redisplay_hooks(value), RedisplayHookPolicy::Legacy);
    }
    for value in ["on", "1", "true", "yes"] {
        assert_eq!(
            parse_redisplay_hooks(Some(OsStr::new(value))),
            RedisplayHookPolicy::Gnu
        );
    }
}

#[cfg(unix)]
#[test]
fn non_unicode_policy_is_baseline() {
    use std::os::unix::ffi::OsStrExt;
    assert_eq!(
        parse_redisplay_hooks(Some(OsStr::from_bytes(&[0xff]))),
        RedisplayHookPolicy::Legacy
    );
}

#[test]
fn pending_targets_join_until_transaction_reset() {
    let mut scope = PendingScope::None;
    scope.raise(PendingScope::Some);
    scope.raise(PendingScope::None);
    assert_eq!(scope, PendingScope::Some);
    scope.raise(PendingScope::All);
    scope.raise(PendingScope::Some);
    assert_eq!(scope, PendingScope::All);
}

#[test]
fn callback_and_active_guard_restore_during_rust_unwind() {
    use std::cell::Cell;
    use std::rc::Rc;
    let mut eval = Context::new();
    let restored = Rc::new(Cell::new(false));
    let observed = restored.clone();
    eval.redisplay_fn = Some(Box::new(move |_| observed.set(true)));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eval.gnu_redisplay_hooks.active = RedisplayActiveOwner::RedisplayTransaction;
        let binding_count = eval.specpdl.len();
        let restrictions = eval.buffers.reset_outermost_restrictions();
        let callback = eval.redisplay_fn.take();
        let _guard = RedisplayTransaction {
            eval: &mut eval,
            restrictions: Some(restrictions),
            callback,
            preparation: None,
            binding_count,
        };
        panic!("simulated frontend unwind");
    }));
    assert!(outcome.is_err());
    assert!(!eval.gnu_redisplay_hooks.active.is_active());
    let mut callback = eval.redisplay_fn.take().expect("restored frontend");
    callback(&mut eval);
    assert!(restored.get());
}

#[test]
fn exceptional_layout_payload_remains_owned_until_taken() {
    let mut eval = Context::new();
    let payload = Value::string("fresh layout throw");
    eval.defer_redisplay_hook_flow(Flow::throw(Value::symbol("layout-test"), payload));
    assert!(eval.redisplay_hook_flow_pending());
    eval.defer_redisplay_hook_flow(Flow::throw(Value::symbol("later"), Value::NIL));
    let flow = eval
        .gnu_redisplay_hooks
        .layout_flow
        .take()
        .expect("first flow retained");
    assert_eq!(flow.as_throw().expect("throw").value, payload);
    assert!(!eval.redisplay_hook_flow_pending());
}

#[test]
fn physical_redraw_waits_for_exact_rendered_frame_not_hook_or_display_ack() {
    let mut state = RedisplayHookOwnership::default();
    let first = FrameId(1);
    let second = FrameId(2);
    state.request_physical_redraw(first);
    state.request_physical_redraw(second);
    state.redisplay_frames.insert(first);
    state.acknowledge_frame_target(first);
    assert!(!state.redisplay_frames.contains(&first));
    assert!(state.take_physical_redraw(first));
    assert!(!state.take_physical_redraw(first));
    assert!(state.take_physical_redraw(second));
}

#[test]
fn committed_scroll_guard_keeps_outer_active_owner_and_restores_idle_owner() {
    let mut eval = Context::new();
    {
        let guard = eval.gnu_guard_committed_scroll();
        assert!(guard.eval.gnu_redisplay_hooks.active.is_active());
    }
    assert!(!eval.gnu_redisplay_hooks.active.is_active());
    eval.gnu_redisplay_hooks.active = RedisplayActiveOwner::RedisplayTransaction;
    {
        let _guard = eval.gnu_guard_committed_scroll();
    }
    assert!(eval.gnu_redisplay_hooks.active.is_active());
}

#[test]
fn redisplay_policy_guard_restores_nested_numeric_selector_on_unwind() {
    let initial = gnu_redisplay_hooks_enabled();
    {
        let _outer = RedisplayHookPolicyGuard::legacy();
        assert!(!gnu_redisplay_hooks_enabled());
        {
            let _inner = RedisplayHookPolicyGuard::gnu();
            assert!(gnu_redisplay_hooks_enabled());
        }
        assert!(!gnu_redisplay_hooks_enabled());
        let result = std::panic::catch_unwind(|| {
            let _inner = RedisplayHookPolicyGuard::gnu();
            assert!(gnu_redisplay_hooks_enabled());
            panic!("exercise numeric redisplay policy cleanup");
        });
        assert!(result.is_err());
        assert!(!gnu_redisplay_hooks_enabled());
    }
    assert_eq!(gnu_redisplay_hooks_enabled(), initial);
}

fn accepted_body_fixture() -> (
    Context,
    FrameId,
    WindowId,
    crate::window::WindowDisplaySnapshot,
    HookWindowDimensions,
) {
    use crate::window::{PresentedWindowRegions, WindowDisplaySnapshot};
    use neomacs_display_protocol::types::Rect as TransportRect;
    let mut eval = Context::new();
    let buffer = eval.buffers.current_buffer_id().expect("buffer");
    let frame_id = eval
        .frames
        .create_frame("accepted-body-hook-epoch", 800, 600, buffer);
    let frame = eval.frames.get(frame_id).expect("frame");
    let window = frame.selected_window;
    let bounds = *frame.find_window(window).expect("window").bounds();
    let snapshot = WindowDisplaySnapshot {
        window_id: window,
        regions_materialized: true,
        regions: PresentedWindowRegions {
            outer: TransportRect::new(bounds.x, bounds.y, bounds.width, bounds.height),
            text_body: TransportRect::new(bounds.x, bounds.y, bounds.width, bounds.height - 1.0),
            mode_line: Some(TransportRect::new(
                bounds.x,
                bounds.y + bounds.height - 1.0,
                bounds.width,
                1.0,
            )),
            ..PresentedWindowRegions::default()
        },
        mode_line_height: 1,
        ..WindowDisplaySnapshot::default()
    };
    eval.frames
        .get_mut(frame_id)
        .expect("frame")
        .prepare_live_window_presentation(
            crate::window::geometry::PresentationId::new(1),
            vec![snapshot.clone()],
        )
        .expect("prepare initial accepted body");
    let (body_width, body_height) = crate::emacs_core::window_cmds::hook_window_body_dimensions(
        &eval.frames,
        &eval.buffers,
        frame_id,
        window,
    )
    .expect("accepted body dimensions");
    let old = HookWindowDimensions {
        total_width: bounds.width,
        total_height: bounds.height,
        body_width,
        body_height,
    };
    eval.gnu_redisplay_hooks.dimensions.insert(window, old);
    eval.gnu_redisplay_hooks.frame_window_change.clear();
    (eval, frame_id, window, snapshot, old)
}

fn remove_accepted_mode_line(snapshot: &mut crate::window::WindowDisplaySnapshot) {
    snapshot.regions.text_body.height += 1.0;
    snapshot.regions.mode_line = None;
    snapshot.mode_line_height = 0;
}

#[test]
fn accepted_chrome_body_size_change_marks_next_hook_epoch_without_overwriting_old_dimensions() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, window, mut snapshot, old) = accepted_body_fixture();
    // No output changed: a successful acceptance is not itself a size change.
    eval.note_gnu_frame_display_accepted(frame, [window]);
    assert!(
        !eval
            .gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    remove_accepted_mode_line(&mut snapshot);
    eval.frames
        .get_mut(frame)
        .expect("frame")
        .prepare_live_window_presentation(
            crate::window::geometry::PresentationId::new(2),
            vec![snapshot],
        )
        .expect("prepare mode-line removal");
    eval.note_gnu_frame_display_accepted(frame, [window]);
    assert!(
        eval.gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame),
        "GNU init_iterator publishes changed body dimensions for the following hook pass"
    );
    assert_eq!(
        eval.gnu_redisplay_hooks.dimensions.get(&window),
        Some(&old),
        "accepted display cannot replace the old window-change epoch"
    );
}

#[test]
fn failed_body_presentation_keeps_previous_dimensions_and_has_no_new_hook_epoch() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, window, mut snapshot, old) = accepted_body_fixture();
    remove_accepted_mode_line(&mut snapshot);
    let result = eval
        .frames
        .get_mut(frame)
        .expect("frame")
        .prepare_live_window_presentation(
            crate::window::geometry::PresentationId::new(1),
            vec![snapshot],
        );
    assert!(
        result.is_err(),
        "a changed publication cannot reuse an accepted identity"
    );
    // The frontend does not acknowledge failed preparation. Even replaying the
    // previously accepted output or accepting no body cannot announce the failed
    // candidate's body dimensions.
    eval.note_gnu_frame_display_accepted(frame, [window]);
    eval.note_gnu_frame_display_accepted(frame, []);
    assert!(
        !eval
            .gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert_eq!(eval.gnu_redisplay_hooks.dimensions.get(&window), Some(&old));
}

#[test]
fn legacy_body_acceptance_preserves_window_change_ownership() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, window, mut snapshot, old) = accepted_body_fixture();
    remove_accepted_mode_line(&mut snapshot);
    eval.frames
        .get_mut(frame)
        .expect("frame")
        .prepare_live_window_presentation(
            crate::window::geometry::PresentationId::new(2),
            vec![snapshot],
        )
        .expect("prepare legacy mode-line removal");
    eval.note_gnu_frame_display_accepted(frame, [window]);
    assert!(
        !eval
            .gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert_eq!(eval.gnu_redisplay_hooks.dimensions.get(&window), Some(&old));
}
