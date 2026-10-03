//! GNU's window error handler blocks outer handlers and debugger entry.

use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::eval::save_scratch_gc_roots;

#[test]
fn mode_line_outer_binding_errors_block_outer_handlers_and_debugger() {
    if std::env::var_os("NEOVM_MODE_LINE_FLOW").is_none() {
        unsafe { std::env::set_var("NEOVM_MODE_LINE_FLOW", "1") };
    }
    crate::test_utils::init_test_tracing();
    let mut observations = Vec::new();
    for phase in ["let", "unlet"] {
        let mut eval = Context::new();
        eval.set_variable("noninteractive", Value::NIL);
        let bid = eval.buffers.current_buffer_id().expect("current buffer");
        eval.frames.create_frame("fx2-outer-handlers", 80, 24, bid);
        eval.eval_str(&format!(
            r#"
          (fset 'ignore '(lambda (&rest args) nil))
          (setq fx2-outer-fired nil fx2-handler-calls nil fx2-debugger-calls nil)
          (setq fx2-outer-watcher
                (lambda (symbol value operation where)
                  (if fx2-outer-fired nil
                      (if (eq operation '{phase})
                          (progn
                            (setq fx2-outer-fired t)
                            (signal 'error '("outside Qt watcher")))))))
          (setq fx2-outer-format
                (list "before"
                      (list :eval
                            (list 'progn
                                  (list 'add-variable-watcher
                                        '(quote inhibit-redisplay)
                                        (list 'quote fx2-outer-watcher))
                                  "armed"))
                      '(:eval "body") "after"))
        "#
        ))
        .expect("one-shot watcher and format");
        eval.redisplay_fn = Some(Box::new(|eval| {
            eval.set_variable("fx2-callback-hit", Value::T);
            let window = eval.frames.selected_frame().unwrap().selected_window;
            let format = eval
                .obarray
                .symbol_value("fx2-outer-format")
                .copied()
                .unwrap();
            format_mode_line_for_display_with_sources(
                eval,
                format,
                Value::make_window(window.0),
                Value::NIL,
                80,
            );
        }));
        let roots = save_scratch_gc_roots();
        let specpdl = eval.specpdl.len();
        let conditions = eval.condition_stack_len();
        let value = eval
            .eval_str(
                r#"
          (let ((debug-on-error t) (debug-on-signal nil) (debug-ignored-errors nil)
                (debugger (lambda (&rest args)
                            (setq fx2-debugger-calls (cons 'debugger fx2-debugger-calls)))))
            (let ((result
                   (unwind-protect
                       (handler-bind-1
                        (lambda () (redisplay t)) 'error
                        (lambda (err)
                          (setq fx2-handler-calls (cons 'outer fx2-handler-calls))))
                     (remove-variable-watcher 'inhibit-redisplay fx2-outer-watcher))))
              (list result fx2-callback-hit fx2-outer-fired fx2-handler-calls fx2-debugger-calls)))
        "#,
            )
            .expect("ordinary redisplay error is consumed");
        let fields = list_to_vec(&value).expect("observations");
        assert!(fields[0].is_t(), "redisplay returns t");
        assert!(fields[1].is_t(), "native callback ran");
        assert!(fields[2].is_t(), "{phase} watcher signaled");
        observations.push((phase, fields[3].is_nil(), fields[4].is_nil()));
        assert_eq!(eval.specpdl.len(), specpdl);
        assert_eq!(eval.condition_stack_len(), conditions);
        assert_eq!(save_scratch_gc_roots(), roots);
        assert!(eval.redisplay_fn.is_some());
        assert!(!eval.has_mode_line_display_flow());
    }
    // Active GNU outer-window-handler-results.el: both phases return t,
    // watcher fired, outer handler calls nil, debugger calls nil.
    for (phase, handler_suppressed, debugger_suppressed) in observations {
        assert!(
            handler_suppressed,
            "{phase} watcher reached outer handler-bind"
        );
        assert!(debugger_suppressed, "{phase} watcher reached debugger");
    }
}
