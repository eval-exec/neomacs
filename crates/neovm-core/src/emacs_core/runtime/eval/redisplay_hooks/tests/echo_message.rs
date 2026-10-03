//! These fixtures exercise builtin_message, the real Context transaction,
//! geometry planning/commit, and display acceptance. Only glyph measurement is
//! supplied by the typed frontend seam; the real renderer is covered by the
//! existing live GNU echo-grow/clear oracle. State is exclusive to one Context.
use super::super::*;
use std::cell::Cell;
use std::rc::Rc;

fn fixture() -> (Context, FrameId, WindowId, Rc<Cell<f32>>, Rc<Cell<usize>>) {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    let buffer = eval.buffers.current_buffer_id().expect("root buffer");
    let frame = eval.frames.create_frame("message-mini", 80, 25, buffer);
    let mini = {
        let state = eval.frames.get_mut(frame).expect("frame");
        state.char_width = 1.0;
        state.char_height = 1.0;
        state.shrink_mini_window();
        state.minibuffer_window.expect("mini")
    };
    for name in [
        "window-buffer-change-functions",
        "window-size-change-functions",
        "window-selection-change-functions",
        "window-state-change-functions",
        "window-configuration-change-hook",
        "window-state-change-hook",
        "inhibit-redisplay",
        "inhibit-quit",
        "set-message-function",
        "clear-message-function",
        "inhibit-message",
        "pre-redisplay-function",
    ] {
        eval.obarray.set_symbol_value(name, Value::NIL);
    }
    let desired = Rc::new(Cell::new(1.0));
    let measured = desired.clone();
    eval.redisplay_prepare_fn = Some(Box::new(move |eval, request| {
        assert_eq!(request.source, RedisplayMiniGeometrySource::EchoArea);
        assert_eq!(eval.buffers.current_buffer_id(), Some(request.buffer));
        let resized = {
            let state = eval.frames.get_mut(request.frame).expect("live frame");
            if let Some(plan) = state.plan_mini_window_resize(measured.get(), 3.0) {
                state.apply_mini_window_resize(plan);
                true
            } else {
                false
            }
        };
        if resized {
            eval.gnu_publish_mini_geometry_changed(request.frame);
        }
        Ok(Value::NIL)
    }));
    let paints = Rc::new(Cell::new(0));
    let painted = paints.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        painted.set(painted.get() + 1);
        let windows = eval.gnu_window_order(frame);
        eval.note_gnu_frame_display_accepted(frame, windows);
    }));
    eval.redisplay_with_force_flow(true)
        .expect("initial accepted frame");
    paints.set(0);
    eval.eval_str(
        r#"(progn
      (setq d5-message-pres nil)
      (setq pre-redisplay-function
       (lambda (targets) (setq d5-message-pres (cons targets d5-message-pres)))))"#,
    )
    .expect("pre callback");
    (eval, frame, mini, desired, paints)
}

fn pre_targets(eval: &Context) -> Vec<Value> {
    let pres = eval
        .obarray
        .symbol_value("d5-message-pres")
        .copied()
        .expect("pre log");
    crate::emacs_core::value::list_to_vec(&pres).expect("pre observations")
}

fn assert_frame_targets(eval: &Context, frame: FrameId, mini: WindowId, targets: Value) {
    let main = eval.frames.get(frame).expect("frame").selected_window;
    let windows = crate::emacs_core::value::list_to_vec(&targets).expect("frame targets");
    assert_eq!(
        windows,
        vec![Value::make_window(mini.0), Value::make_window(main.0)]
    );
}

#[test]
fn gnu_message_growth_prepares_before_pre_callback_and_rearms_next_pass() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, desired, paints) = fixture();
    desired.set(3.0);
    let value = eval
        .eval_str(r#"(message "first\nsecond\nthird\nfourth")"#)
        .expect("growing message");
    assert_eq!(value.as_utf8_str(), Some("first\nsecond\nthird\nfourth"));
    assert_eq!(paints.get(), 1, "resize displays before message returns");
    let pre = pre_targets(&eval);
    assert_eq!(pre.len(), 1);
    assert_frame_targets(&eval, frame, mini, pre[0]);
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.contains(&frame));
    eval.redisplay_with_force_flow(true)
        .expect("explicit next pass");
    let pre = pre_targets(&eval);
    assert_eq!(pre.len(), 2);
    assert_frame_targets(&eval, frame, mini, pre[0]);
    assert!(!eval.gnu_redisplay_hooks.redisplay_frames.contains(&frame));
}

#[test]
fn gnu_message_clear_prepares_shrink_before_pre_and_rearms_next_pass() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, desired, paints) = fixture();
    desired.set(3.0);
    eval.eval_str(r#"(message "first\nsecond\nthird\nfourth")"#)
        .expect("grow");
    eval.redisplay_with_force_flow(true)
        .expect("accept armed next pass");
    eval.obarray.set_symbol_value("d5-message-pres", Value::NIL);
    paints.set(0);
    desired.set(1.0);
    assert_eq!(eval.eval_str("(message nil)").expect("clear"), Value::NIL);
    assert_eq!(paints.get(), 1);
    assert_eq!(
        eval.frames
            .get(frame)
            .expect("frame")
            .find_window(mini)
            .expect("mini")
            .bounds()
            .height,
        1.0
    );
    let pre = pre_targets(&eval);
    assert_eq!(pre.len(), 1);
    assert_frame_targets(&eval, frame, mini, pre[0]);
    assert!(eval.gnu_redisplay_hooks.redisplay_frames.contains(&frame));
    eval.redisplay_with_force_flow(true)
        .expect("next explicit pass");
    assert_eq!(pre_targets(&eval).len(), 2);
}

#[test]
fn gnu_same_height_and_inhibited_messages_do_not_run_complete_redisplay() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, _, _, desired, paints) = fixture();
    eval.eval_str(r#"(message "single line")"#)
        .expect("same height");
    assert_eq!(paints.get(), 0);
    assert!(pre_targets(&eval).is_empty());
    desired.set(3.0);
    eval.eval_str(r#"(let ((inhibit-redisplay t)) (message "first\nsecond\nthird"))"#)
        .expect("display inhibited");
    eval.eval_str(r#"(let ((inhibit-message t)) (message "inhibited"))"#)
        .expect("message inhibited");
    assert_eq!(paints.get(), 0);
    assert!(pre_targets(&eval).is_empty());
}

#[test]
fn gnu_message_sizing_throw_restores_source_callback_and_propagates_flow() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, mini, _, paints) = fixture();
    let caller = eval.buffers.current_buffer_id();
    let source = eval
        .frames
        .get(frame)
        .expect("frame")
        .find_window(mini)
        .and_then(Window::buffer_id);
    let serial = eval.gnu_redisplay_hooks.accepted_serial;
    eval.redisplay_prepare_fn = Some(Box::new(|_, _| {
        Err(Flow::throw(
            Value::symbol("message-sizing"),
            Value::symbol("interrupted"),
        ))
    }));
    let transferred = eval
        .eval_str(r#"(catch 'message-sizing (message "first\nsecond\nthird") 'not-thrown)"#)
        .expect("caught sizing transfer");
    assert_eq!(transferred, Value::symbol("interrupted"));
    assert_eq!(eval.buffers.current_buffer_id(), caller);
    assert_eq!(
        eval.frames
            .get(frame)
            .expect("frame")
            .find_window(mini)
            .and_then(Window::buffer_id),
        source
    );
    assert!(eval.redisplay_prepare_fn.is_some());
    assert!(eval.redisplay_fn.is_some());
    assert!(!eval.gnu_redisplay_hooks.active.is_active());
    assert_eq!(eval.gnu_redisplay_hooks.accepted_serial, serial);
    assert_eq!(paints.get(), 0);
}

#[test]
fn legacy_message_growth_preserves_deferred_redisplay_policy() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, _, _, desired, paints) = fixture();
    paints.set(0);
    desired.set(3.0);
    eval.eval_str(r#"(message "first\nsecond\nthird")"#)
        .expect("legacy message");
    assert_eq!(paints.get(), 0);
    assert!(pre_targets(&eval).is_empty());
}
