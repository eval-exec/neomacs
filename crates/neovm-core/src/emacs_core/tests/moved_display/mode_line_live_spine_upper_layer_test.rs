//! mode_line_live_spine_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::save_scratch_gc_roots;
use neovm_value::value::Value;

fn replacing_tail(generational: bool, display: bool) {
    unsafe {
        std::env::set_var(
            "NEOVM_GC_GENERATIONAL",
            if generational { "1" } else { "0" },
        );
    }
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut eval, false);
    eval.set_gc_threshold(usize::MAX);
    eval.set_variable("noninteractive", Value::NIL);
    eval.gc_collect_exact();
    eval.eval_str(
        "(setq review-mode-line-live-format
               (list \"A\"
                     '(:eval (progn
                               (setcdr (cdr review-mode-line-live-format)
                                       (list (concat \"C\")))
                               (garbage-collect)
                               \"\"))
                     (concat \"B\")))",
    )
    .expect("install live mode-line format");
    let before_gc = eval.gc_count_for_test();
    let before_roots = save_scratch_gc_roots();
    let rendered = if display {
        let format = eval
            .obarray_view()
            .symbol_value_copied("review-mode-line-live-format")
            .expect("installed mode-line format");
        format_mode_line_for_display_with_sources(&mut eval, format, Value::NIL, Value::NIL, 80)
            .value()
    } else {
        eval.eval_str("(format-mode-line review-mode-line-live-format 0)")
            .expect("format changed live tail")
    };
    assert_eq!(rendered.as_utf8_str(), Some("AC"));
    assert_eq!(eval.gc_count_for_test(), before_gc + 1);
    assert_eq!(save_scratch_gc_roots(), before_roots);
}

#[test]
fn gc_mode_line_replaced_tail_is_rendered_gen0() {
    replacing_tail(false, false);
}

#[test]
fn gc_mode_line_replaced_tail_is_rendered_gen1() {
    replacing_tail(true, false);
}

#[test]
fn gc_mode_line_display_replaced_tail_is_rendered_gen0() {
    replacing_tail(false, true);
}

#[test]
fn gc_mode_line_display_replaced_tail_is_rendered_gen1() {
    replacing_tail(true, true);
}
