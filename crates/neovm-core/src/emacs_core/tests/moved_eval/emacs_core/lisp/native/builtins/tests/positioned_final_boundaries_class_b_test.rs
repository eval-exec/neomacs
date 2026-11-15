use crate::emacs_core::print::print_value;
fn answer(form: &str) -> String {
    print_value(
        &crate::test_utils::new_context()
            .eval_str(form)
            .expect("positioned final boundary"),
    )
}

#[test]
fn positioned_image_library_type_follows_mode() {
    assert_eq!(
        answer(
            r#"(let ((type (position-symbol 'png 0)))
                          (list (let ((symbols-with-pos-enabled nil)) (init-image-library type))
                                (let ((symbols-with-pos-enabled t))
                                  (eq (init-image-library type) (init-image-library 'png)))))"#
        ),
        "(nil t)"
    );
}

#[test]
fn positioned_fringe_alignment_and_frame_symbol_checks_follow_mode() {
    assert_eq!(
        answer(
            r#"(let ((top (position-symbol 'top 0))
                            (variable (position-symbol 'issue554-void-state 1)))
                          (mapcar (lambda (enabled)
                            (let ((symbols-with-pos-enabled enabled))
                              (list (condition-case err
                                      (progn (define-fringe-bitmap 'issue554-fringe [0] 1 8 (list top t)) t)
                                      (error (car err)))
                                    (condition-case err (frame-or-buffer-changed-p variable)
                                      (error (car err)))))) '(nil t)))"#
        ),
        "((error wrong-type-argument) (t void-variable))"
    );
}

/// Re-homed from the eval unit-test tree (assertion byte-identical): the
/// completion-flags path calls `kill-buffer`, a primitive core owns by R0
/// placement, so the test executes in core's real runtime instead of a
/// test-only eval stand-in.
#[test]
fn positioned_completion_flags_follow_eq_policy() {
    assert_eq!(
        answer(
            r#"(let ((buffer (get-buffer-create "issue554-complete-target")))
                          (unwind-protect
                            (let ((symbols-with-pos-enabled t))
                              (list (internal-complete-buffer "issue554-complete-target" nil
                                                              (position-symbol 'lambda 0))
                                    (eq (car (internal-complete-buffer "" nil
                                                                      (position-symbol 'metadata 1))) 'metadata)
                                    (equal (internal-complete-buffer "issue554-complete-target" nil
                                                                     (position-symbol t 2))
                                           '("issue554-complete-target"))))
                            (kill-buffer buffer)))"#
        ),
        "(t t t)"
    );
}
