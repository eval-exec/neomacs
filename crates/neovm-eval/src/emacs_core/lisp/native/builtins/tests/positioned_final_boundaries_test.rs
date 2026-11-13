use crate::emacs_core::{eval::Context, print::print_value};

fn answer(form: &str) -> String {
    print_value(
        &Context::new(crate::test_utils::unit_runtime())
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
