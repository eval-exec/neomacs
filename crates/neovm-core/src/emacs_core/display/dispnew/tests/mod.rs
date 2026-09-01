use super::pure::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::value::Value;

#[test]
fn redraw_frame_nil_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_redraw_frame(&mut ctx, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn redraw_frame_rejects_non_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_redraw_frame(&mut ctx, vec![Value::string("not-a-frame")]);
    assert!(result.is_err());
}

#[test]
fn redraw_display_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_redraw_display(vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn ding_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_ding(vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn ding_with_arg_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_ding(vec![Value::T]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn open_termscript_signals_tty_error() {
    crate::test_utils::init_test_tracing();
    let result = builtin_open_termscript(vec![Value::NIL]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Current frame is not on a tty device")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn send_string_to_terminal_rejects_non_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_send_string_to_terminal(&mut eval, vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn send_string_to_terminal_accepts_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_send_string_to_terminal(&mut eval, vec![Value::string("hello")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_show_cursor_tracks_visibility() {
    crate::test_utils::init_test_tracing();
    reset_dispnew_thread_locals();
    let mut eval = crate::emacs_core::eval::Context::new();
    let fid = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let wid = eval.frames.get(fid).expect("frame").selected_window;
    let default_visible = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert_eq!(default_visible, Value::T);
    assert!(
        eval.frames
            .get(fid)
            .and_then(|frame| frame.find_window(wid))
            .and_then(|window| window.display())
            .is_some_and(|display| !display.cursor_off_p)
    );

    builtin_internal_show_cursor(&mut eval, vec![Value::NIL, Value::NIL]).unwrap();
    let hidden = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert!(hidden.is_nil());
    assert!(
        eval.frames
            .get(fid)
            .and_then(|frame| frame.find_window(wid))
            .and_then(|window| window.display())
            .is_some_and(|display| display.cursor_off_p && !display.last_cursor_off_p)
    );

    builtin_internal_show_cursor(&mut eval, vec![Value::NIL, Value::T]).unwrap();
    let visible = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert_eq!(visible, Value::T);
    assert!(
        eval.frames
            .get(fid)
            .and_then(|frame| frame.find_window(wid))
            .and_then(|window| window.display())
            .is_some_and(|display| !display.cursor_off_p && !display.last_cursor_off_p)
    );
}

#[test]
fn internal_show_cursor_rejects_non_window() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_internal_show_cursor(&mut eval, vec![Value::fixnum(1), Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn force_window_update_no_arg_returns_t() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result =
        crate::emacs_core::window_cmds::builtin_force_window_update(&mut eval, vec![]).unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn force_window_update_non_window_arg_returns_nil() {
    // A non-window, non-displayed-buffer OBJECT yields nil (GNU
    // `Fforce_window_update' returns t only for nil / a live window / a
    // buffer shown in some window).
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result =
        crate::emacs_core::window_cmds::builtin_force_window_update(&mut eval, vec![Value::T])
            .unwrap();
    assert!(result.is_nil());
}

#[test]
fn force_window_update_nil_arg_returns_t() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result =
        crate::emacs_core::window_cmds::builtin_force_window_update(&mut eval, vec![Value::NIL])
            .unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn force_window_update_live_window_returns_t() {
    // GNU returns t for a live window OBJECT (oracle test cx409); the prior
    // stub wrongly returned nil for any non-nil OBJECT.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let selected =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();
    let result =
        crate::emacs_core::window_cmds::builtin_force_window_update(&mut eval, vec![selected])
            .unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn eval_internal_show_cursor_per_window_state() {
    crate::test_utils::init_test_tracing();
    reset_dispnew_thread_locals();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let selected =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();
    let other = crate::emacs_core::builtins::dispatch_builtin(
        &mut eval,
        "split-window-internal",
        vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL],
    )
    .unwrap()
    .unwrap();

    // Both start visible
    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![selected]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![other]).unwrap(),
        Value::T
    );

    // Hide selected window cursor
    builtin_internal_show_cursor(&mut eval, vec![Value::NIL, Value::NIL]).unwrap();
    assert!(
        builtin_internal_show_cursor_p(&mut eval, vec![selected])
            .unwrap()
            .is_nil()
    );
    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![other]).unwrap(),
        Value::T
    );
}

#[test]
fn frame_z_order_lessp_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_frame_z_order_lessp(vec![Value::NIL, Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn frame_z_order_lessp_requires_two_args() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_frame_z_order_lessp(vec![]).is_err());
    assert!(builtin_frame_z_order_lessp(vec![Value::NIL]).is_err());
}
