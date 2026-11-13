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
