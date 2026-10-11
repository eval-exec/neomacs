//! Main's deferred mode-line exits also cross the GNU hook transaction.
//! Fixtures own one exclusive Context; their scalar policy guard is private
//! to the test thread and introduces no production Lisp-state cache.

use super::*;
use crate::buffer::EmacsByteRange;
use std::cell::Cell;
use std::rc::Rc;

fn context() -> Context {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer = eval.buffers.current_buffer_id().expect("current buffer");
    eval.frames
        .create_frame("hook-mode-line-flow", 640, 384, buffer);
    eval
}

#[test]
fn gnu_mode_line_exit_restores_callback_restrictions_and_reentry() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = context();
    let buffer_id = eval.buffers.current_buffer_id().expect("buffer");
    eval.buffers
        .get_mut(buffer_id)
        .expect("buffer")
        .insert("abcdef");
    let _ = eval.buffers.internal_labeled_narrow_to_emacs_byte_range(
        buffer_id,
        EmacsByteRange::from_usize(1, 5),
        Value::symbol("outer"),
    );
    let _ = eval.buffers.internal_labeled_narrow_to_emacs_byte_range(
        buffer_id,
        EmacsByteRange::from_usize(2, 4),
        Value::symbol("inner"),
    );
    let roots = eval.save_specpdl_roots();
    let expected = Value::string("first GNU mode-line exit");
    eval.push_specpdl_root(expected);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed.set(observed.get() + 1);
        assert!(eval.gnu_redisplay_transaction_active());
        let buffer = eval.buffers.get(buffer_id).expect("redisplay buffer");
        assert_eq!(buffer.point_min_emacs_byte_pos().get(), 0);
        assert_eq!(buffer.point_max_emacs_byte_pos().get(), 6);
        if observed.get() == 1 {
            eval.defer_mode_line_display_flow(Flow::throw(
                Value::symbol("hook-mode-line-exit"),
                expected,
            ));
            eval.defer_mode_line_display_flow(Flow::throw(
                Value::symbol("hook-mode-line-exit"),
                Value::NIL,
            ));
        } else {
            crate::test_utils::mock_redisplay::accept_all_frames(eval);
        }
    }));

    let caught = eval
        .eval_str("(catch 'hook-mode-line-exit (redisplay t))")
        .expect("the first deferred exit reaches the enclosing catch");
    assert_eq!(caught, expected);
    assert_eq!(calls.get(), 1);
    assert!(!eval.has_mode_line_display_flow());
    assert!(eval.redisplay_fn.is_some());
    assert!(!eval.gnu_redisplay_transaction_active());
    assert!(eval.last_redisplay_signature.is_none());
    let buffer = eval.buffers.get(buffer_id).expect("restored buffer");
    assert_eq!(buffer.point_min_emacs_byte_pos().get(), 2);
    assert_eq!(buffer.point_max_emacs_byte_pos().get(), 4);

    eval.redisplay_with_force(true)
        .expect("the restored callback can display again");
    assert_eq!(calls.get(), 2);
    assert!(eval.last_redisplay_signature.is_some());
    assert!(!eval.gnu_redisplay_transaction_active());
    eval.restore_specpdl_roots(roots);
}

#[test]
fn gnu_mode_line_exit_prevents_signature_commit_after_callback_acceptance() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = context();
    eval.redisplay_fn = Some(Box::new(|eval| {
        crate::test_utils::mock_redisplay::accept_all_frames(eval);
        eval.defer_mode_line_display_flow(Flow::throw(
            Value::symbol("accepted-mode-line-exit"),
            Value::fixnum(17),
        ));
    }));

    let flow = eval
        .redisplay()
        .expect_err("callback acceptance cannot swallow an exit");
    let thrown = flow.as_throw().expect("throw preserved");
    assert_eq!(thrown.tag, Value::symbol("accepted-mode-line-exit"));
    assert_eq!(thrown.value, Value::fixnum(17));
    assert!(eval.gnu_redisplay_hooks.accepted_serial > 0);
    assert!(eval.last_redisplay_signature.is_none());
    assert!(!eval.has_mode_line_display_flow());
    assert!(eval.redisplay_fn.is_some());
    assert!(!eval.gnu_redisplay_transaction_active());
}

#[test]
fn gnu_pending_mode_line_exit_precedes_inhibited_or_rendererless_skip() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    for renderer in [false, true] {
        let mut eval = context();
        let calls = Rc::new(Cell::new(0));
        if renderer {
            let observed = calls.clone();
            eval.redisplay_fn = Some(Box::new(move |_| observed.set(observed.get() + 1)));
            eval.set_variable("inhibit-redisplay", Value::T);
        }
        eval.defer_mode_line_display_flow(Flow::throw(
            Value::symbol("pending-mode-line-exit"),
            Value::fixnum(23),
        ));

        let flow = eval
            .redisplay_for_input_wait()
            .expect_err("a pending exit precedes redisplay skip decisions");
        assert_eq!(flow.as_throw().expect("throw").value, Value::fixnum(23));
        assert_eq!(calls.get(), 0);
        assert!(!eval.has_mode_line_display_flow());
        assert!(!eval.gnu_redisplay_transaction_active());
    }
}

#[test]
fn legacy_hook_policy_preserves_fallible_mode_line_callback_exit() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let mut eval = context();
    eval.redisplay_fn = Some(Box::new(|eval| {
        eval.defer_mode_line_display_flow(Flow::throw(
            Value::symbol("legacy-mode-line-exit"),
            Value::fixnum(31),
        ));
    }));

    let flow = eval
        .redisplay()
        .expect_err("legacy hook policy retains main's mode-line flow handling");
    assert_eq!(flow.as_throw().expect("throw").value, Value::fixnum(31));
    assert!(!eval.has_mode_line_display_flow());
    assert!(eval.redisplay_fn.is_some());
    assert!(eval.last_redisplay_signature.is_none());
}
