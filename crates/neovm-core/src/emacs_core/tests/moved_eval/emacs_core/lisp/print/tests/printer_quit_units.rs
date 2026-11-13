//! Issue #590: a quit requested while the printer traverses must reach the
//! handler surrounding the print call instead of returning partial output.
//! GNU checks `maybe_quit` at `print_obj` (`src/print.c:2281-2283`), including
//! after a Lisp unreadable-object callback and before visiting the next object.

#[test]
fn printer_delivers_quit_during_traversal_before_returning_partial_output() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    // Two unreadable markers: the callback records each visit and requests quit
    // on the first. GNU polls at print_obj, so the second marker is never
    // visited. A printer that polls only once after the traversal would report
    // two visits, and one that does not poll at all would return the string.
    let result = context
        .eval_str(
            r#"(progn
                 (defvar neovm--issue590-visits 0)
                 (let ((print-circle nil)
                     (neovm--issue590-visits 0)
                     (print-unreadable-function
                      (lambda (_object _escape)
                        (setq neovm--issue590-visits (1+ neovm--issue590-visits))
                        (setq quit-flag t)
                        "Q")))
                 (let ((printed (condition-case nil
                                    (prin1-to-string (vector (make-marker) (make-marker)))
                                  (quit 'caught))))
                   (prin1-to-string (list printed neovm--issue590-visits)))))"#,
        )
        .expect("quit must reach the handler surrounding the print call");
    assert_eq!(
        result.as_lisp_string().map(|s| s.as_bytes().to_vec()),
        Some(b"(caught 1)".to_vec())
    );
}
