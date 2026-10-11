use super::*;

#[test]
fn gde_review_overlay_lists_full_region_intersection() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    let result = eval.eval_str(
        r#"(list
(with-temp-buffer
(insert "ab")
(dolist (spec '((3 3 end-empty) (1 1 begin-empty) (2 3 nonempty)))
(overlay-put (make-overlay (car spec) (cadr spec)) 'tag (nth 2 spec)))
(list (mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists)))
(save-restriction (narrow-to-region 2 2)
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists))))))
(with-temp-buffer (overlay-put (make-overlay 1 1) 'tag 'empty-buffer)
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists))))
(with-temp-buffer (insert "abcdef")
(overlay-put (make-overlay 4 7) 'tag 'collapsed-end)
(overlay-put (make-overlay 2 3) 'tag 'prefix)
(delete-region 4 7)
(mapcar (lambda (o) (overlay-get o 'tag)) (car (overlay-lists)))))"#,
    );
    let actual = crate::emacs_core::format_eval_result_with_eval(&eval, &result);
    // This expectation was generated directly from GNU Emacs 31.1.
    assert_eq!(
        actual,
        r#"OK (((begin-empty nonempty) (begin-empty nonempty)) (empty-buffer) (prefix))"#
    );
}
