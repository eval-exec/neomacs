//! Redisplay-hook tests whose fixture is the display window surface.
//!
//! `window_cmds` and `crate::window` live above eval. Eval cannot name
//! that crate. Test names and assertions are unchanged. Frame and buffer
//! managers are the public accessors because those fields are `pub(crate)`.
//! The bare harness is core's `new_context`: eval's unit-runtime module
//! stays `cfg(test)`.

use neovm_display::window::{DisplayFrameExt, DisplayFrameManagerExt};
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_value::display_vocab::FrameId;
use neovm_value::value::Value;

use crate::emacs_core::eval::redisplay_hooks::HookWindowDimensions;
use crate::emacs_core::eval::{Context, RedisplayHookPolicyGuard};

fn accepted_body_fixture() -> (
    Context,
    FrameId,
    WindowId,
    crate::window::WindowDisplaySnapshot,
    HookWindowDimensions,
) {
    use crate::window::{PresentedWindowRegions, WindowDisplaySnapshot};
    use neomacs_display_protocol::types::Rect as TransportRect;
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.buffer_manager().current_buffer_id().expect("buffer");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("accepted-body-hook-epoch", 800, 600, buffer);
    let frame = eval.frame_manager().get(frame_id).expect("frame");
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
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .prepare_live_window_presentation(
            crate::window::geometry::PresentationId::new(1),
            vec![snapshot.clone()],
        )
        .expect("prepare initial accepted body");
    let (body_width, body_height) = crate::emacs_core::window_cmds::hook_window_body_dimensions(
        eval.frame_manager(),
        eval.buffer_manager(),
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
    eval.gnu_redisplay_hooks_mut()
        .dimensions
        .insert(window, old);
    eval.gnu_redisplay_hooks_mut().frame_window_change.clear();
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
            .gnu_redisplay_hooks()
            .frame_window_change
            .contains(&frame)
    );
    remove_accepted_mode_line(&mut snapshot);
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .prepare_live_window_presentation(
            crate::window::geometry::PresentationId::new(2),
            vec![snapshot],
        )
        .expect("prepare mode-line removal");
    eval.note_gnu_frame_display_accepted(frame, [window]);
    assert!(
        eval.gnu_redisplay_hooks()
            .frame_window_change
            .contains(&frame),
        "GNU init_iterator publishes changed body dimensions for the following hook pass"
    );
    assert_eq!(
        eval.gnu_redisplay_hooks().dimensions.get(&window),
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
        .frame_manager_mut()
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
            .gnu_redisplay_hooks()
            .frame_window_change
            .contains(&frame)
    );
    assert_eq!(
        eval.gnu_redisplay_hooks().dimensions.get(&window),
        Some(&old)
    );
}

#[test]
fn legacy_body_acceptance_preserves_window_change_ownership() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, window, mut snapshot, old) = accepted_body_fixture();
    remove_accepted_mode_line(&mut snapshot);
    eval.frame_manager_mut()
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
            .gnu_redisplay_hooks()
            .frame_window_change
            .contains(&frame)
    );
    assert_eq!(
        eval.gnu_redisplay_hooks().dimensions.get(&window),
        Some(&old)
    );
}

// Direct window.el commits and preparatory redisplay share GNU's physical
// current-matrix invalidation. No test below manufactures terminal cell data.
fn direct_mini_commit_fixture() -> (Context, FrameId, WindowId, i64, i64) {
    let mut eval = crate::test_utils::new_context();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let (mini, root_height, mini_height) = {
        let state = eval.frame_manager_mut().get_mut(frame).expect("frame");
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
    eval.clear_frame_redisplay_marks();
    eval.gnu_redisplay_hooks_mut().frame_window_change.clear();
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
    let state = eval.frame_manager_mut().get_mut(frame).unwrap();
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
    assert!(eval.frame_redisplay_pending(frame));
    assert!(
        eval.gnu_redisplay_hooks()
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
    assert!(eval.frame_redisplay_pending(frame));
    assert!(
        eval.gnu_redisplay_hooks()
            .frame_window_change
            .contains(&frame)
    );
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
}

#[test]
fn direct_mini_commit_on_gui_does_not_request_a_tty_repaint() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    eval.frame_manager_mut()
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
        eval.gnu_redisplay_hooks()
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
    eval.frame_manager_mut()
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
            .gnu_redisplay_hooks()
            .frame_window_change
            .contains(&frame)
    );
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    let state = eval.frame_manager().get(frame).unwrap();
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
        eval.apply_prepared_minibuffer_resize(
            frame,
            mini,
            neomacs_display_protocol::types::Px(mini_height as f32)
        )
        .expect("unchanged height"),
        crate::window::PreparedMiniResizeOutcome::Unchanged
    );
    assert!(
        !eval
            .gnu_redisplay_hooks()
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
        eval.apply_prepared_minibuffer_resize(
            frame,
            mini,
            neomacs_display_protocol::types::Px(mini_height as f32 + 1.0)
        )
        .expect("prepared commit"),
        crate::window::PreparedMiniResizeOutcome::Committed
    );
    assert!(
        eval.gnu_redisplay_hooks()
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
    let generation_before = eval.redisplay_generation();
    assert!(eval.gnu_redisplay_hooks_redisplay_frames_empty());
    crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
        &mut eval,
        vec![Value::make_window(mini.0)],
    )
    .expect("actual Legacy mini geometry commit");
    assert_ne!(eval.redisplay_generation(), generation_before);
    assert!(eval.gnu_redisplay_hooks_redisplay_frames_empty());
    assert!(eval.gnu_redisplay_hooks().frame_window_change.is_empty());
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
    let generation_before = eval.redisplay_generation();
    assert_eq!(
        crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
            &mut eval,
            vec![Value::make_window(mini.0)],
        )
        .expect("conserving zero-delta primitive commit"),
        Value::T
    );
    assert_eq!(eval.redisplay_generation(), generation_before);
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    assert!(eval.gnu_redisplay_hooks_redisplay_frames_empty());
}

#[test]
fn legacy_gui_mini_commit_invalidates_layout_without_a_tty_redraw() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    let generation_before = eval.redisplay_generation();
    crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
        &mut eval,
        vec![Value::make_window(mini.0)],
    )
    .expect("GUI mini commit");
    assert_ne!(eval.redisplay_generation(), generation_before);
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    assert!(eval.gnu_redisplay_hooks_redisplay_frames_empty());
}

#[test]
fn legacy_refused_mini_commit_does_not_invalidate_or_queue_physical_redraw() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, mini, root_height, mini_height) = direct_mini_commit_fixture();
    stage_direct_mini_commit(&mut eval, frame, mini, root_height, mini_height, 1);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(mini)
        .unwrap()
        .set_new_pixel(Some(mini_height + 2));
    let generation_before = eval.redisplay_generation();
    assert!(
        crate::emacs_core::window_cmds::builtin_resize_mini_window_internal(
            &mut eval,
            vec![Value::make_window(mini.0)],
        )
        .is_err()
    );
    assert_eq!(eval.redisplay_generation(), generation_before);
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
    assert!(eval.gnu_redisplay_hooks_redisplay_frames_empty());
}

#[test]
fn prepared_mini_resize_refuses_an_ordinary_window_before_lisp_staging() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, _, _) = direct_mini_commit_fixture();
    eval.eval_str("(progn (setq prepared-mini-invalid-target-calls 0) (fset 'window--resize-root-window-vertically '(lambda (&rest ignored) (setq prepared-mini-invalid-target-calls (1+ prepared-mini-invalid-target-calls)) 0)))")
        .expect("observable root-sizing policy");
    let state = eval.frame_manager().get(frame).expect("frame");
    let ordinary = state.root_window().id();
    let root_before = *state.root_window().bounds();
    let mini_before = *state.find_window(mini).expect("mini").bounds();
    let generation_before = eval.redisplay_generation();
    let result = eval.apply_prepared_minibuffer_resize(
        frame,
        ordinary,
        neomacs_display_protocol::types::Px(root_before.height + 1.0),
    );
    assert_eq!(
        eval.eval_str("prepared-mini-invalid-target-calls")
            .expect("policy call count"),
        Value::fixnum(0),
        "an ordinary window must be refused before invoking arbitrary Lisp sizing policy"
    );
    assert_eq!(
        result.expect("invalid mini target is an unchanged request"),
        crate::window::PreparedMiniResizeOutcome::Unchanged
    );
    let state = eval.frame_manager().get(frame).expect("frame");
    assert_eq!(*state.root_window().bounds(), root_before);
    assert_eq!(
        *state.find_window(mini).expect("mini").bounds(),
        mini_before
    );
    assert!(state.root_window().new_pixel().is_none());
    assert!(state.find_window(mini).expect("mini").new_pixel().is_none());
    assert_eq!(eval.redisplay_generation(), generation_before);
    assert!(eval.gnu_redisplay_hooks_redisplay_frames_empty());
    assert!(eval.gnu_redisplay_hooks().frame_window_change.is_empty());
    assert!(!eval.gnu_take_tty_frame_redraw(frame));
}
