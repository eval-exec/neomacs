//! A mode-line element can change the list tail that is rendered next.

use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::eval::save_scratch_gc_roots;

fn replacing_tail(generational: bool, display: bool) {
    unsafe {
        std::env::set_var(
            "NEOVM_GC_GENERATIONAL",
            if generational { "1" } else { "0" },
        );
    }
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.gc_stress = false;
    eval.tagged_heap.set_gc_threshold(usize::MAX);
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
    let before_gc = eval.gc_count;
    let before_roots = save_scratch_gc_roots();
    let rendered = if display {
        let format = eval
            .obarray
            .symbol_value("review-mode-line-live-format")
            .copied()
            .expect("installed mode-line format");
        format_mode_line_for_display_with_sources(&mut eval, format, Value::NIL, Value::NIL, 80)
            .value()
    } else {
        eval.eval_str("(format-mode-line review-mode-line-live-format 0)")
            .expect("format changed live tail")
    };
    assert_eq!(rendered.as_utf8_str(), Some("AC"));
    assert_eq!(eval.gc_count, before_gc + 1);
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

#[test]
fn mode_line_split_callback_follows_replaced_live_tail() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.gc_stress = false;
    let format = eval
        .eval_str("'(\"A\" (:eval nil) \"B\")")
        .expect("callback format");
    let mut callbacks = 0;
    let before_roots = save_scratch_gc_roots();
    let rendered = finish_format_mode_line_in_state_with_eval(
        &eval.obarray,
        &[],
        &eval.frames,
        &mut eval.buffers,
        &eval.processes,
        &[format, Value::fixnum(0)],
        |_, _| {
            callbacks += 1;
            format
                .cons_cdr()
                .set_cdr(Value::list(vec![Value::string("C")]));
            Ok(Value::NIL)
        },
    )
    .expect("split callback changes tail");
    assert_eq!(rendered.as_utf8_str(), Some("AC"));
    assert_eq!(callbacks, 1);
    assert_eq!(save_scratch_gc_roots(), before_roots);
}

#[test]
fn mode_line_live_spine_stops_at_improper_tail() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    let rendered = eval
        .eval_str("(format-mode-line '(\"A\" . \"B\") 0)")
        .expect("improper mode-line list");
    assert_eq!(rendered.as_utf8_str(), Some("A"));
}

#[test]
fn mode_line_live_spine_stops_on_cycles_like_gnu_safe_iterator() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    // GNU FOR_EACH_TAIL_SAFE compares at doubling Brent checkpoints.
    // It renders four elements for a two-cell cycle and five for a three-cell cycle.
    for (elements, last_tail, expected) in [
        ("(list \"A\")", "fmt", "A"),
        ("(list \"A\" \"B\")", "(cdr fmt)", "ABAB"),
        ("(list \"A\" \"B\" \"C\")", "(cdr (cdr fmt))", "ABCAB"),
    ] {
        let form =
            format!("(let ((fmt {elements})) (setcdr {last_tail} fmt) (format-mode-line fmt 0))");
        let rendered = eval.eval_str(&form).expect("cyclic mode-line list");
        assert_eq!(rendered.as_utf8_str(), Some(expected));
    }
}

fn collecting_checkpoint_format(eval: &mut Context) -> (Value, usize, usize) {
    eval.eval_str(
        "(setq review-mode-line-checkpoint-format
               (list \"A\" \"B\" \"C\"
                     '(:eval (progn
                               (setcdr (cdr review-mode-line-checkpoint-format) nil)
                               (garbage-collect)
                               \"E\"))))",
    )
    .expect("install collecting checkpoint format");
    let format = eval
        .obarray
        .symbol_value("review-mode-line-checkpoint-format")
        .copied()
        .expect("installed checkpoint format");
    // GNU's first doubling transition checkpoints C after displaying B.
    // Preserve only bits here, so this test does not itself root that cons.
    let checkpoint = format.cons_cdr().cons_cdr();
    (format, checkpoint.bits(), checkpoint.cons_car().bits())
}

fn assert_checkpoint_retained(eval: &Context, checkpoint_bits: usize, car_bits: usize) {
    let checkpoint = Value::from_bits(checkpoint_bits);
    assert!(
        eval.tagged_heap.owns_heap_value_for_test(checkpoint),
        "the detached Brent checkpoint cons was collected during :eval"
    );
    // Ownership guards the cons-cell read. Collection statistics can reuse a
    // swept slot; compare car bits too, without accessing any string payload.
    assert_eq!(
        checkpoint.cons_car().bits(),
        car_bits,
        "the detached Brent checkpoint cons was reused after collection"
    );
}

fn checkpoint_survives_detaching_eval(generational: bool, display: bool) {
    unsafe {
        std::env::set_var(
            "NEOVM_GC_GENERATIONAL",
            if generational { "1" } else { "0" },
        );
    }
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.gc_stress = false;
    eval.tagged_heap.set_gc_threshold(usize::MAX);
    eval.set_variable("noninteractive", Value::NIL);
    eval.gc_collect_exact();
    let (format, checkpoint_bits, car_bits) = collecting_checkpoint_format(&mut eval);
    let before_gc = eval.gc_count;
    let before_roots = save_scratch_gc_roots();
    let rendered = if display {
        format_mode_line_for_display_with_sources(&mut eval, format, Value::NIL, Value::NIL, 80)
            .value()
    } else {
        eval.eval_str("(format-mode-line review-mode-line-checkpoint-format 0)")
            .expect("format with a detaching :eval")
    };
    assert_eq!(rendered.as_utf8_str(), Some("ABCE"));
    assert_eq!(eval.gc_count, before_gc + 1);
    assert_eq!(save_scratch_gc_roots(), before_roots);
    assert_checkpoint_retained(&eval, checkpoint_bits, car_bits);
}

#[test]
fn gc_mode_line_brent_checkpoint_survives_detaching_eval_gen0() {
    checkpoint_survives_detaching_eval(false, false);
}

#[test]
fn gc_mode_line_brent_checkpoint_survives_detaching_eval_gen1() {
    checkpoint_survives_detaching_eval(true, false);
}

#[test]
fn gc_mode_line_display_brent_checkpoint_survives_detaching_eval_gen0() {
    checkpoint_survives_detaching_eval(false, true);
}

#[test]
fn gc_mode_line_display_brent_checkpoint_survives_detaching_eval_gen1() {
    checkpoint_survives_detaching_eval(true, true);
}

fn split_checkpoint_survives_collection(generational: bool) {
    unsafe {
        std::env::set_var(
            "NEOVM_GC_GENERATIONAL",
            if generational { "1" } else { "0" },
        );
    }
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.gc_stress = false;
    eval.tagged_heap.set_gc_threshold(usize::MAX);
    eval.gc_collect_exact();
    let (format, checkpoint_bits, car_bits) = collecting_checkpoint_format(&mut eval);
    // Separate manager copies let the callback collect through Context without
    // aliasing the state references borrowed by the split walker.
    let obarray = eval.obarray.clone();
    let buffers = eval.buffers.clone();
    let processes = crate::emacs_core::process::ProcessManager::new();
    let pctx = ModeLinePercentContext::default();
    let before_roots = save_scratch_gc_roots();
    let before_gc = eval.gc_count;
    let mut callbacks = 0;
    let rendered = {
        let roots = mode_line_gc::ScratchRoots::new();
        let mut result = ModeLineRendered::default();
        format_mode_line_recursive_in_state_with_eval_rooted(
            &obarray,
            &[],
            &buffers,
            &processes,
            &pctx,
            &format,
            &mut result,
            0,
            false,
            &mut |_, _| {
                callbacks += 1;
                format.cons_cdr().set_cdr(Value::NIL);
                eval.gc_collect_exact();
                assert_checkpoint_retained(&eval, checkpoint_bits, car_bits);
                Ok(Value::string("E"))
            },
            &roots,
        )
        .expect("split walker with a collecting callback");
        result.into_value(ModeLineFaceSpec {
            no_props: true,
            face: None,
        })
    };
    assert_eq!(rendered.as_utf8_str(), Some("ABCE"));
    assert_eq!(callbacks, 1);
    assert_eq!(eval.gc_count, before_gc + 1);
    assert_eq!(save_scratch_gc_roots(), before_roots);
    assert_checkpoint_retained(&eval, checkpoint_bits, car_bits);
}

#[test]
fn gc_mode_line_split_brent_checkpoint_survives_collection_gen0() {
    split_checkpoint_survives_collection(false);
}

#[test]
fn gc_mode_line_split_brent_checkpoint_survives_collection_gen1() {
    split_checkpoint_survives_collection(true);
}
