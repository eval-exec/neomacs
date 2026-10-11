use super::*;

#[test]
fn gde_review_multibyte_numeric_begin_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(with-temp-buffer
(insert "жжжabcdefghij")
(let ((i 0) out)
(dolist (r '((5 8) (5 9) (5 10) (2 9) (5 7)))
(let ((o (make-overlay (car r) (cadr r)))) (overlay-put o 'tag (setq i (1+ i)))))
(dolist (flag '(nil t nil t))
(set-buffer-multibyte flag)
(push (mapcar (lambda (o) (list (overlay-get o 'tag) (overlay-start o) (overlay-end o)))
(overlays-in (point-min) (point-max))) out))
(nreverse out)))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    // This expectation was generated directly from GNU Emacs 31.1.
    assert_eq!(
        actual,
        r#"OK (((4 3 12) (1 8 11) (2 8 12) (3 8 13) (5 8 10)) ((4 2 9) (5 5 7) (3 5 10) (2 5 9) (1 5 8)) ((4 3 12) (1 8 11) (2 8 12) (3 8 13) (5 8 10)) ((4 2 9) (5 5 7) (3 5 10) (2 5 9) (1 5 8)))"#
    );
}

#[test]
fn gde_review_multibyte_after_contracted_starts() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(with-temp-buffer
(insert "aжbжcжdжeжfжgжhжiжjж")
(let ((i 0) out)
(dolist (r '((2 10) (4 10) (6 12) (8 12) (3 5)))
(let ((o (make-overlay (car r) (cadr r)))) (overlay-put o 'tag (setq i (1+ i)))))
(set-buffer-multibyte nil) (delete-region 3 15)
(push (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-in 1 (point-max))) out)
(set-buffer-multibyte t)
(push (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-in 1 (point-max))) out)
(goto-char 3) (insert-before-markers "zz")
(push (list (mapcar (lambda (o) (overlay-get o 'tag)) (overlays-in 1 (point-max)))
(mapcar (lambda (o) (overlay-get o 'tag)) (overlays-at 5))) out)
(nreverse out)))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    // This expectation was generated directly from GNU Emacs 31.1.
    assert_eq!(
        actual,
        r#"OK ((1 5 2 3 4) (1 5 2 3 4) ((1 5 2 3 4) (3 4)))"#
    );
}
