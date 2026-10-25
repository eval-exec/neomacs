use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

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

/// A Rust panic through the frontend callback must not evaluate Lisp while
/// unwinding (P4.11): the transaction's Drop retires the pushed bindings
/// storage-only, so neither the unwind form nor the variable watcher runs.
#[test]
fn redisplay_transaction_panic_fallback_never_calls_panicking_lisp() {
    fn panic_on_recovery_lisp(eval: &mut Context, _: Vec<Value>) -> EvalResult {
        eval.obarray
            .set_symbol_value("redisplay-recovery-lisp-ran", Value::T);
        panic!("Lisp callback must not run during Rust panic recovery");
    }
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = Context::new();
    let sentinel = crate::emacs_core::intern::intern("redisplay-recovery-lisp-ran");
    let symbol = crate::emacs_core::intern::intern("redisplay-recovery-watched");
    eval.obarray.set_symbol_value_id(sentinel, Value::NIL);
    eval.obarray.set_symbol_value_id(symbol, Value::fixnum(10));
    let count = eval.specpdl.len();
    eval.register_subr(SubrSpec::new(
        "redisplay-recovery-panic",
        NativeFn::ContextVec(panic_on_recovery_lisp),
        SubrArity::new(0, None),
    ));
    eval.redisplay_fn = Some(Box::new(move |eval| {
        eval.try_specbind(symbol, Value::fixnum(20))
            .expect("frontend binds saved value");
        crate::emacs_core::advice::builtin_add_variable_watcher(
            eval,
            vec![
                Value::from_sym_id(symbol),
                Value::symbol("redisplay-recovery-panic"),
            ],
        )
        .expect("register recovery watcher");
        let cleanup = Value::list(vec![Value::symbol("redisplay-recovery-panic")]);
        eval.specpdl.push(SpecBinding::UnwindProtect {
            forms: cleanup,
            lexenv: eval.lexenv,
        });
        panic!("simulate frontend panic");
    }));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = eval.redisplay_with_force_flow(true);
    }));
    assert!(outcome.is_err());
    assert_eq!(eval.obarray.symbol_value_id_or_nil(sentinel), Value::NIL);
    assert_eq!(
        eval.obarray.symbol_value_id_or_nil(symbol),
        Value::fixnum(10)
    );
    assert_eq!(eval.specpdl.len(), count);
    assert!(!eval.gnu_redisplay_hooks.active.is_active());
}

// Direct window.el commits and preparatory redisplay share GNU's physical
// current-matrix invalidation. No test below manufactures terminal cell data.
fn direct_mini_commit_fixture() -> (Context, FrameId, WindowId, i64, i64) {
    let mut eval = Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let (mini, root_height, mini_height) = {
        let state = eval.frames.get_mut(frame).expect("frame");
        state.char_width = 1.0;
        state.char_height = 1.0;
        state.shrink_mini_window();
        let mini = state.minibuffer_window.expect("separate mini");
        (
            mini,
            state.root_window().bounds().height as i64,
            state.find_window(mini).unwrap().bounds().height as i64,
        )
    };
    eval.gnu_redisplay_hooks.redisplay_frames.clear();
    eval.gnu_redisplay_hooks.frame_window_change.clear();
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    (eval, frame, mini, root_height, mini_height)
}

fn stage_direct_mini_commit(
    eval: &mut Context,
    frame: FrameId,
    mini: WindowId,
    root_height: i64,
    mini_height: i64,
    delta: i64,
) {
    let state = eval.frames.get_mut(frame).unwrap();
    state
        .root_window_mut()
        .set_new_pixel(Some(root_height - delta));
    state
        .find_window_mut(mini)
        .unwrap()
        .set_new_pixel(Some(mini_height + delta));
}

#[test]
fn direct_mini_commit_on_dirty_tty_publishes_geometry_and_physical_redraw() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    eval.gnu_mark_frame_redisplay(frame);
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    assert_eq!(
        crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
            &mut eval,
            vec![Value::make_window(mini.0)],
        )
        .expect("valid direct commit"),
        Value::T,
    );
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.contains(&frame));
    assert!(
        eval.gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert!(
        eval.gnu_take_tty_frame_redraw(frame),
        "GNU garbages an already-dirty TTY frame at the successful shared commit"
    );
    assert!(
        !eval.gnu_take_tty_frame_redraw(frame),
        "one prepared repaint consumes the request"
    );
}

#[test]
fn direct_mini_commit_on_clean_tty_publishes_without_forcing_physical_redraw() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
        &mut eval,
        vec![Value::make_window(mini.0)],
    )
    .expect("valid clean commit");
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.contains(&frame));
    assert!(
        eval.gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
}

#[test]
fn direct_mini_commit_on_gui_does_not_request_a_tty_repaint() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    eval.frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    eval.gnu_mark_frame_redisplay(frame);
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
        &mut eval,
        vec![Value::make_window(mini.0)],
    )
    .expect("valid GUI commit");
    assert!(
        eval.gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
}

#[test]
fn refused_direct_mini_commit_publishes_neither_geometry_nor_physical_redraw() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    eval.gnu_mark_frame_redisplay(frame);
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    eval.frames
        .get_mut(frame)
        .unwrap()
        .find_window_mut(mini)
        .unwrap()
        .set_new_pixel(Some(mini_height + 2));
    assert!(
        crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
            &mut eval,
            vec![Value::make_window(mini.0)],
        )
        .is_err()
    );
    assert!(
        !eval
            .gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    let state = eval.frames.get(frame).unwrap();
    assert_eq!(state.root_window().bounds().height as i64, root_height);
    assert_eq!(
        state.find_window(mini).unwrap().bounds().height as i64,
        mini_height
    );
}

#[test]
fn prepared_mini_noop_does_not_publish_a_commit_or_physical_redraw() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, _, mini_height) = direct_mini_commit_fixture();
    eval.gnu_mark_frame_redisplay(frame);
    assert_eq!(
        eval.apply_prepared_minibuffer_resize(frame, mini, mini_height as f32)
            .expect("unchanged height"),
        Value::NIL
    );
    assert!(
        !eval
            .gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
}

#[test]
fn prepared_mini_commit_uses_the_same_primitive_publication_once() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, _, mini_height) = direct_mini_commit_fixture();
    // Only the achievable root-size Lisp seam is a fixture; the public
    // commit primitive, its validation, publication and redraw are real.
    eval.eval_str("(fset 'window--resize-root-window-vertically '(lambda (root delta &rest ignored) (set-window-new-pixel root (+ (window-pixel-height root) delta)) delta))")
        .expect("deterministic achievable-size policy");
    eval.gnu_mark_frame_redisplay(frame);
    assert_eq!(
        eval.apply_prepared_minibuffer_resize(frame, mini, mini_height as f32 + 1.0)
            .expect("prepared commit"),
        Value::T
    );
    assert!(
        eval.gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert!(eval.gnu_take_tty_frame_redraw(frame));
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
}

#[test]
fn legacy_direct_mini_commit_requests_physical_redraw_without_gnu_hook_flags() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    let generation_before = eval.redisplay_generation;
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.is_empty());
    crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
        &mut eval,
        vec![Value::make_window(mini.0)],
    )
    .expect("actual Legacy mini geometry commit");
    assert_ne!(eval.redisplay_generation, generation_before);
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.is_empty());
    assert!(eval.gnu_redisplay_hooks.frame_window_change.is_empty());
    assert!(
        eval.gnu_take_tty_frame_redraw(frame),
        "the default product policy must repaint the invalidated physical current matrix too"
    );
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
}

#[test]
fn legacy_direct_mini_noop_preserves_physical_matrix_and_hook_policy() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 0);
    let generation_before = eval.redisplay_generation;
    assert_eq!(
        crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
            &mut eval,
            vec![Value::make_window(mini.0)],
        )
        .expect("conserving zero-delta primitive commit"),
        Value::T
    );
    assert_eq!(eval.redisplay_generation, generation_before);
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.is_empty());
}

#[test]
fn legacy_gui_mini_commit_invalidates_layout_without_a_tty_redraw() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    eval.frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    let generation_before = eval.redisplay_generation;
    crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
        &mut eval,
        vec![Value::make_window(mini.0)],
    )
    .expect("GUI mini commit");
    assert_ne!(eval.redisplay_generation, generation_before);
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.is_empty());
}

#[test]
fn legacy_refused_mini_commit_does_not_invalidate_or_queue_physical_redraw() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    eval.frames
        .get_mut(frame)
        .unwrap()
        .find_window_mut(mini)
        .unwrap()
        .set_new_pixel(Some(mini_height + 2));
    let generation_before = eval.redisplay_generation;
    assert!(
        crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
            &mut eval,
            vec![Value::make_window(mini.0)],
        )
        .is_err()
    );
    assert_eq!(eval.redisplay_generation, generation_before);
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.is_empty());
}
