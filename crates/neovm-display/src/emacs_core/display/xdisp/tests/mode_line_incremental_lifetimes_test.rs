//! Independent d5 lifetime checks using the upstream incremental root owner.
//! No alternate root policy, root-slot instrumentation, or GC cache is added.

use super::*;
use neovm_eval::emacs_core::eval::save_scratch_gc_roots;

use crate::test_utils::mode_line_incremental_lifetimes_test_upper_layer_helpers::assert_live_interval_plists;

fn context() -> neovm_eval::emacs_core::eval::Context {
    crate::test_utils::mode_line_incremental_lifetimes_test_upper_layer_helpers::context(|| {
        neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime())
    })
}

#[test]
fn mode_line_incremental_top_format_and_face_survive_replacement_during_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = context();
    let format = eval
        .eval_str(
            "(setq d5-root-format
           (list '(:eval (progn (setq d5-root-format nil d5-root-face nil)
                               (garbage-collect) \"X\")) (copy-sequence \"Y\")))",
        )
        .expect("format");
    let face = eval
        .eval_str("(setq d5-root-face (list :foreground (copy-sequence \"red\")))")
        .expect("face");
    let before = save_scratch_gc_roots();
    let gc = eval.gc_count_for_test();
    let output =
        builtin_format_mode_line_ctx(&mut eval, &vec![format, face]).expect("format-mode-line");
    assert_eq!(eval.gc_count_for_test(), gc + 1);
    assert_eq!(save_scratch_gc_roots(), before);
    assert_live_interval_plists(&eval, output);
    assert_eq!(output.as_utf8_str(), Some("XY"));
    let table = get_string_text_properties_table_for_value(output).expect("properties");
    let result_face = table
        .get_property_at_char_pos(CharPos0::ZERO, Value::symbol("face"))
        .expect("default face");
    assert_eq!(result_face, face, "preserve face object identity");
    assert!(eval.tagged_heap().owns_heap_value_for_test(result_face));
    let face_value = result_face.cons_cdr().cons_car();
    assert!(eval.tagged_heap().owns_heap_value_for_test(face_value));
    assert_eq!(face_value.as_utf8_str(), Some("red"));
}

#[test]
fn mode_line_incremental_resolved_symbol_survives_replacement_during_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = context();
    let format = eval
        .eval_str(
            "(progn
           (put 'd5-scalar-format 'risky-local-variable t)
           (setq d5-scalar-format
             (list '(:eval (progn (setq d5-scalar-format nil)
                                  (garbage-collect) \"X\"))
                   (copy-sequence \"Y\")))
           'd5-scalar-format)",
        )
        .expect("resolved format setup");
    let before = save_scratch_gc_roots();
    let gc = eval.gc_count_for_test();
    let value = builtin_format_mode_line_ctx(&mut eval, &vec![format]).expect("resolved formatter");
    assert_eq!(value.as_utf8_str(), Some("XY"));
    assert_eq!(eval.gc_count_for_test(), gc + 1);
    assert_eq!(
        eval.obarray_view().symbol_value_copied("d5-scalar-format"),
        Some(Value::NIL)
    );
    assert_eq!(save_scratch_gc_roots(), before);
}

#[test]
fn mode_line_incremental_split_panic_releases_accumulator_owners() {
    crate::test_utils::init_test_tracing();
    let mut eval = context();
    let format = eval
        .eval_str(
            "(setq d5-panic-format
          '((:propertize \"A\" face bold)
            (1 (:propertize ((:eval d5-panic)) face italic))))",
        )
        .expect("nested split format");
    let before = save_scratch_gc_roots();
    let mut calls = 0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (ml_obarray, ml_owner, ml_frames, ml_buffers, ml_processes) =
            eval.mode_line_formatting_inputs_mut();
        let _ = finish_format_mode_line_in_state_with_eval(
            neovm_eval::emacs_core::eval::VariableView::new(ml_obarray, ml_owner),
            &[],
            ml_frames,
            ml_buffers,
            ml_processes,
            &[format],
            |_, _| {
                calls += 1;
                panic!("deliberate split-state evaluator panic")
            },
        );
    }));
    assert!(result.is_err());
    assert_eq!(calls, 1, "panic must originate in the supplied evaluator");
    assert_eq!(
        save_scratch_gc_roots(),
        before,
        "Rust unwind leaked formatter roots"
    );
}

#[derive(Debug, PartialEq, Eq)]
struct Rendering {
    bytes: Vec<u8>,
    multibyte: bool,
    chars: usize,
    properties: Vec<std::collections::HashMap<Value, Value>>,
}
