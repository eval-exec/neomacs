use super::*;

#[test]
fn oracle_prop_gde_review_multibyte_numeric_begin_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
(insert "жжжabcdefghij")
(let ((i 0) out)
(dolist (r '((5 8) (5 9) (5 10) (2 9) (5 7)))
(let ((o (make-overlay (car r) (cadr r)))) (overlay-put o 'tag (setq i (1+ i)))))
(dolist (flag '(nil t nil t))
(set-buffer-multibyte flag)
(push (mapcar (lambda (o) (list (overlay-get o 'tag) (overlay-start o) (overlay-end o)))
(overlays-in (point-min) (point-max))) out))
(nreverse out)))"#;
    let expected = expect_test::expect![[
        r#""OK (((4 3 12) (1 8 11) (2 8 12) (3 8 13) (5 8 10)) ((4 2 9) (5 5 7) (3 5 10) (2 5 9) (1 5 8)) ((4 3 12) (1 8 11) (2 8 12) (3 8 13) (5 8 10)) ((4 2 9) (5 5 7) (3 5 10) (2 5 9) (1 5 8)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}
