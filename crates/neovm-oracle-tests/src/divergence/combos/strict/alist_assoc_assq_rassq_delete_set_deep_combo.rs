//! Strict combo oracle probes, batch 293: alist operations deep. assoc-defaults,
//! assq-delete-all / rassq-delete-all / alist-delete, copy-alist isolation, and
//! assoc-string case-fold variants.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_assoc_defaults_with_test_recurse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((al '(("a" . 1) ("b" . 2) ("c" . 3))))
  (list (assoc-default "b" al)
        (assoc-default "B" al)
        (assoc-default "B" al nil t)
        (assoc-default "z" al nil t)
        (assoc-default 2 (mapcar (lambda (c) (cons (cdr c) (car c))) al))))
"##;
    let expect = expect_test::expect![[r#""OK (2 nil nil nil \"b\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_assq_rassq_delete_all_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((al (copy-alist '((a . 1) (b . 2) (c . 3) (b . 4))))
       (delq (assq-delete-all 'b al))
       (rdelq (rassq-delete-all 2 al)))
  (list delq
        rdelq
        al
        (eq delq al)))
"##;
    let expect =
        expect_test::expect![[r#""OK (((a . 1) (c . 3)) ((a . 1) (c . 3)) ((a . 1) (c . 3)) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_assoc_string_case_fold_with_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((al '(("apple" . 1) ("banana" . 2))))
  (list (assoc-string "apple" al)
        (assoc-string "APPLE" al)
        (assoc-string "APPLE" al t)
        (assoc-string "cherry" al)
        (assoc-string "cherry" al nil 'missing)
        (assoc-string 2 al)
        (rassoc "apple" al)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments assoc-string 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
