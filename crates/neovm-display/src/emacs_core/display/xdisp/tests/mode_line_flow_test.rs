//! Active GNU `display_mode_element` catches signals, but lets throws escape.

use super::*;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::save_scratch_gc_roots;

fn context() -> Context {
    crate::test_utils::mode_line_flow_test_upper_layer_helpers::context(|| {
        neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime())
    })
}

#[test]
fn mode_line_throw_reaches_enclosing_catch() {
    let mut eval = context();
    let roots = save_scratch_gc_roots();
    let specpdl = eval.specpdl().len();
    // Captured from active GNU 31.1 in the peer's review-oracle-active-result.txt.
    let value = eval
        .eval_str(r#"(catch 'escaped (format-mode-line '("A" (:eval (throw 'escaped "escaped")) "B") 0))"#)
        .expect("mode-line throw reaches catch");
    assert_eq!(value.as_utf8_str(), Some("escaped"));
    assert_eq!(save_scratch_gc_roots(), roots);
    assert_eq!(eval.specpdl().len(), specpdl);
}

#[test]
fn mode_line_split_throw_restores_buffer_and_roots() {
    let mut eval = context();
    let saved_buffer = eval.edit.buffers.current_buffer_id();
    let target = eval.edit.buffers.create_buffer("fx2-target");
    let format = eval.eval_str("'(:eval nil)").expect("format");
    let roots = save_scratch_gc_roots();
    let (ml_obarray, ml_owner, ml_frames, ml_buffers, ml_processes) =
        eval.mode_line_formatting_inputs_mut();
    let result = finish_format_mode_line_in_state_with_eval(
        neovm_eval::emacs_core::eval::VariableView::new(ml_obarray, ml_owner),
        &[],
        ml_frames,
        ml_buffers,
        ml_processes,
        &[
            format,
            Value::fixnum(0),
            Value::NIL,
            Value::make_buffer(target),
        ],
        |_, _| {
            Err(Flow::throw(
                Value::symbol("escaped"),
                Value::string("escaped"),
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(eval.edit.buffers.current_buffer_id(), saved_buffer);
    assert_eq!(save_scratch_gc_roots(), roots);
}

#[test]
fn mode_line_split_signal_is_demoted_like_gnu() {
    let mut eval = context();
    let format = eval
        .eval_str(r#"'("before" (:eval nil) "after")"#)
        .expect("format");
    let (ml_obarray, ml_owner, ml_frames, ml_buffers, ml_processes) =
        eval.mode_line_formatting_inputs_mut();
    let result = finish_format_mode_line_in_state_with_eval(
        neovm_eval::emacs_core::eval::VariableView::new(ml_obarray, ml_owner),
        &[],
        ml_frames,
        ml_buffers,
        ml_processes,
        &[format, Value::fixnum(0)],
        |_, _| {
            Err(signal(
                LispCondition::Error,
                vec![Value::string("fx2 signal")],
            ))
        },
    )
    .expect("GNU catches signals in :eval and continues formatting");
    // Active GNU oracle: format-signal-outer-condition-case in results.el.
    assert_eq!(result.as_utf8_str(), Some("beforeafter"));
}
