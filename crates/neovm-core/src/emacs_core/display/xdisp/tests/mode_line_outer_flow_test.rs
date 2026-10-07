//! GNU's outer redisplay handler catches the `error` family, not all signals.

use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::eval::save_scratch_gc_roots;

#[test]
fn mode_line_outer_binding_signals_preserve_non_error_conditions_and_scopes() {
    if std::env::var_os("NEOVM_MODE_LINE_FLOW").is_none() {
        unsafe { std::env::set_var("NEOVM_MODE_LINE_FLOW", "1") };
    }
    crate::test_utils::init_test_tracing();
    let mut observed = Vec::new();
    for operation in ["let", "unlet"] {
        for condition in ["quit", "fx2-derived-quit", "fx2-non-error", "error"] {
            let mut eval = Context::new();
            eval.set_variable("noninteractive", Value::NIL);
            eval.set_variable("inhibit-quit", Value::NIL);
            let bid = eval.buffers.current_buffer_id().expect("current buffer");
            eval.frames.create_frame("fx2-outer-signal", 80, 24, bid);
            eval.eval_str(
                "(put 'fx2-derived-quit 'error-conditions '(fx2-derived-quit quit))
                 (put 'fx2-non-error 'error-conditions '(fx2-non-error))
                 (setq fx2-outer-format '(\"before\" (:eval \"body\") \"after\"))",
            )
            .expect("condition hierarchy and format");
            eval.redisplay_fn = Some(Box::new(|eval| {
                eval.set_variable("fx2-outer-callback-hit", Value::T);
                let frame = eval.frames.selected_frame().expect("selected frame");
                let window = Value::make_window(frame.selected_window.0);
                let format = eval
                    .obarray
                    .symbol_value("fx2-outer-format")
                    .copied()
                    .unwrap();
                format_mode_line_for_display_with_sources(eval, format, window, Value::NIL, 80);
            }));
            let roots = save_scratch_gc_roots();
            let specpdl = eval.specpdl.len();
            let selected = eval.frames.selected_frame().unwrap().selected_window;
            let expression = format!(
                "(let ((watcher (lambda (symbol new-value operation where)
                                 (if (eq operation '{operation})
                                     (progn
                                       (setq fx2-outer-watched operation)
                                       (signal '{condition} nil))))))
                   (unwind-protect
                       (progn
                         (add-variable-watcher 'inhibit-quit watcher)
                         (condition-case err (redisplay t) (t (car err))))
                     (remove-variable-watcher 'inhibit-quit watcher)))"
            );
            let result = eval
                .eval_str(&expression)
                .expect("outer handler receives signal");
            assert!(eval.eval_str("fx2-outer-callback-hit").unwrap().is_t());
            assert_eq!(
                eval.eval_str("fx2-outer-watched").unwrap().as_symbol_name(),
                Some(operation)
            );
            observed.push((
                operation,
                condition,
                result.as_symbol_name().unwrap().to_owned(),
            ));
            assert_eq!(eval.specpdl.len(), specpdl, "{operation} {condition}");
            assert_eq!(save_scratch_gc_roots(), roots, "{operation} {condition}");
            assert_eq!(eval.buffers.current_buffer_id(), Some(bid));
            assert_eq!(
                eval.frames.selected_frame().unwrap().selected_window,
                selected
            );
            assert!(eval.redisplay_fn.is_some(), "redisplay callback restored");
            assert!(
                !eval.has_mode_line_display_flow(),
                "outer handler drained original Flow"
            );
            // GNU's unlet watcher runs before storing the restored value;
            // a signal there leaves the inhibition at t even after popping.
            let inhibition = eval.eval_str("inhibit-quit").unwrap();
            assert_eq!(inhibition.is_t(), operation == "unlet");
        }
    }
    for (operation, condition, observed) in observed {
        // GNU xdisp.c's outer `list_of_error` catches only error ancestry;
        // the inner Qt handler is absent during binding and unbinding.
        let expected = if condition == "error" { "t" } else { condition };
        assert_eq!(
            observed, expected,
            "{operation} watcher signaling {condition}"
        );
    }
}
