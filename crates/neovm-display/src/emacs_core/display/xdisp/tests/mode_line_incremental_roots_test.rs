//! Incremental accumulator roots outlive nested elements and unwind as a walk.
use super::*;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::save_scratch_gc_roots;
use neovm_value::error::{FlowKind, FlowResultExt};

#[test]
fn gc_mode_line_rehomed_predecessor_plist_survives_eval() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut eval, false);
    eval.set_gc_threshold(usize::MAX);
    eval.set_variable("noninteractive", Value::NIL);
    let before_roots = save_scratch_gc_roots();
    let rendered = eval
        .eval_str(
            "(progn
           (setq review-mode-line-format
                 '((:propertize \"A\" review-property 73)
                   (:propertize \"B\" review-property 74)
                   (:eval (progn (garbage-collect) \"C\"))))
           (format-mode-line review-mode-line-format))",
        )
        .expect("collecting mode-line");
    assert_eq!(save_scratch_gc_roots(), before_roots);
    assert_eq!(rendered.as_utf8_str(), Some("ABC"));
    let runs = rendered
        .as_lisp_string()
        .unwrap()
        .intervals()
        .object_interval_plist_runs_for_char_len(CharLen::new(3));
    for (run, expected) in runs.iter().take(2).zip([73, 74]) {
        let plist = run.plist();
        mode_line_gc_roots::assert_owned_property(
            &eval,
            plist,
            Value::symbol("review-property"),
            Value::fixnum(expected),
        );
    }
    assert!(runs.len() >= 2);
}

#[test]
fn gc_mode_line_split_accumulator_roots_restore_on_throw() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut eval, false);
    let format = eval
        .eval_str(
            "'((:propertize \"A\" review-property 73)
           (1 (:propertize ((:eval t)) face bold)))",
        )
        .expect("nested collecting format");
    let before = save_scratch_gc_roots();
    let tag = Value::symbol("review-mode-line-throw");
    let (ml_obarray, ml_owner, ml_frames, ml_buffers, ml_processes) =
        eval.mode_line_formatting_inputs_mut();
    let result = finish_format_mode_line_in_state_with_eval(
        neovm_eval::emacs_core::eval::VariableView::new(ml_obarray, ml_owner),
        &[],
        ml_frames,
        ml_buffers,
        ml_processes,
        &[format],
        |_, _| {
            assert!(save_scratch_gc_roots() > before);
            Err(Flow::throw(tag, Value::fixnum(42)))
        },
    );
    assert!(matches!(result.kinded(), Err(FlowKind::Throw(..))));
    assert_eq!(
        save_scratch_gc_roots(),
        before,
        "the entire walk's roots unwind"
    );
}
