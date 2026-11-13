//! Issue #590: a quit requested while the printer traverses must reach the
//! handler surrounding the print call instead of returning partial output.
//! GNU checks `maybe_quit` at `print_obj` (`src/print.c:2281-2283`), including
//! after a Lisp unreadable-object callback and before visiting the next object.

#[test]
fn printer_delivers_quit_during_traversal_before_returning_partial_output() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let result = context
        .eval_str(
            r#"(let ((print-circle nil)
                     (print-unreadable-function
                      (lambda (_object _escape) (setq quit-flag t) "Q")))
                 (condition-case nil
                     (prin1-to-string (vector (make-marker) '(after)))
                   (quit 'caught)))"#,
        )
        .expect("quit must reach the handler surrounding the print call");
    assert_eq!(result, neovm_value::value::Value::symbol("caught"));
}
