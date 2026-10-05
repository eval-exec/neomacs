use super::*;

#[test]
fn oracle_prop_gde_review_multibyte_after_contracted_starts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
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
(nreverse out)))"#;
    let expected = expect_test::expect![[r#""OK ((1 5 2 3 4) (1 5 2 3 4) ((1 5 2 3 4) (3 4)))""#]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
