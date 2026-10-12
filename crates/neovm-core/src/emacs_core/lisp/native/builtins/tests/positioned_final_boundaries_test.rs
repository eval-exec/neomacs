use crate::emacs_core::{eval::Context, print::print_value};

fn answer(form: &str) -> String {
    print_value(
        &Context::new()
            .eval_str(form)
            .expect("positioned final boundary"),
    )
}

#[test]
fn positioned_sort_option_keywords_and_designators_follow_mode() {
    assert_eq!(
        answer(
            r#"(let ((key (position-symbol :key 0))
                            (identity (position-symbol 'identity 1))
                            (lessp (position-symbol :lessp 2))
                            (value< (position-symbol 'value< 3)))
                          (mapcar (lambda (enabled)
                            (let ((symbols-with-pos-enabled enabled))
                              (condition-case err
                                (sort [3 1 2] key identity lessp value<
                                      (position-symbol :reverse 4) nil
                                      (position-symbol :in-place 5) nil)
                                (error (car err))))) '(nil t)))"#
        ),
        "(error [1 2 3])"
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
