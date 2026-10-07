//! Mode-line exits cross the void frontend callback only after state restoration.

use super::*;
use crate::buffer::EmacsByteRange;
use std::cell::Cell;
use std::rc::Rc;

fn context() -> Context {
    // nextest isolates tests in separate processes. This immutable process
    // knob contains no Lisp state and can be shared by independent mutators.
    if std::env::var_os("NEOVM_MODE_LINE_FLOW").is_none() {
        unsafe { std::env::set_var("NEOVM_MODE_LINE_FLOW", "1") };
    }
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer = eval.buffers.current_buffer_id().expect("current buffer");
    eval.frames.create_frame("mode-line-flow", 640, 384, buffer);
    eval
}

#[test]
fn redisplay_mode_line_exit_restores_restrictions_and_callback() {
    let mut eval = context();
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
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
    let expected = Value::string("first mode-line exit");
    eval.push_specpdl_root(expected);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        let buffer = eval
            .buffers
            .get(buffer_id)
            .expect("buffer during redisplay");
        assert_eq!(buffer.point_min_emacs_byte_pos().get(), 0);
        assert_eq!(buffer.point_max_emacs_byte_pos().get(), 6);
        eval.defer_mode_line_display_flow(Flow::throw(Value::symbol("mode-line-exit"), expected));
        eval.defer_mode_line_display_flow(Flow::throw(
            Value::symbol("mode-line-exit"),
            Value::string("later exit"),
        ));
    }));

    let caught = eval
        .eval_str("(catch 'mode-line-exit (redisplay t))")
        .expect("deferred exit reaches the enclosing catch");
    assert_eq!(
        caught, expected,
        "the first non-local exit owns the callback"
    );
    assert!(!eval.has_mode_line_display_flow());
    assert!(eval.redisplay_fn.is_some());
    assert!(eval.last_redisplay_signature.is_none());
    let buffer = eval.buffers.get(buffer_id).expect("buffer after redisplay");
    assert_eq!(buffer.point_min_emacs_byte_pos().get(), 2);
    assert_eq!(buffer.point_max_emacs_byte_pos().get(), 4);
    eval.restore_specpdl_roots(roots);
}

#[test]
fn redisplay_mode_line_flow_force_respects_inhibit_redisplay() {
    let mut eval = context();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    eval.redisplay_fn = Some(Box::new(move |_| observed.set(observed.get() + 1)));
    eval.set_variable("inhibit-redisplay", Value::T);

    eval.redisplay_with_force(true)
        .expect("inhibited forced redisplay succeeds");
    assert_eq!(calls.get(), 0);
    assert!(eval.redisplay_fn.is_some());
}
