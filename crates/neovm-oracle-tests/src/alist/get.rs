//! Oracle parity tests for `alist-get` and association list patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_alist_get_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 2""#];
    crate::common::assert_oracle_parity_expect("(alist-get 'b '((a . 1) (b . 2) (c . 3)))", expect);
}

#[test]
fn oracle_prop_alist_get_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect("(alist-get 'z '((a . 1) (b . 2)))", expect);
}

#[test]
fn oracle_prop_alist_get_with_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK default""#];
    crate::common::assert_oracle_parity_expect("(alist-get 'z '((a . 1)) 'default)", expect);
}

#[test]
fn oracle_prop_alist_get_first_match_wins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 1""#];
    crate::common::assert_oracle_parity_expect("(alist-get 'a '((a . 1) (a . 2) (a . 3)))", expect);
}

#[test]
fn oracle_prop_alist_get_with_equal_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(alist-get "key" '(("key" . "val") ("other" . "x"))
                              nil nil 'equal)"#;
    let expect = expect_test::expect![[r#""OK \"val\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_setf_alist_get_remove_uses_eql_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let* ((default (list 'same))
        (al (list (cons 'k 'old))))
   (setf (alist-get 'k al default t) (list 'same))
   al)
 (let* ((default (copy-sequence "same"))
        (al (list (cons 'k 'old))))
   (setf (alist-get 'k al default t) (copy-sequence "same"))
   al)
 (let ((al (list (cons 'k 'old))))
   (setf (alist-get 'k al 7 t) 7)
   al)
 (let ((al (list (cons 'k 'old))))
   (setf (alist-get 'k al 'gone t) 'gone)
   al))
"#;
    let expect = expect_test::expect![[r#""OK (((k same)) ((k . \"same\")) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_assoc_vs_assq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // assq uses eq, assoc uses equal
    let form = r#"(list
                    (assq 'a '((a . 1) (b . 2)))
                    (assoc "hello" '(("hello" . 1) ("world" . 2)))
                    (assq "hello" '(("hello" . 1) ("world" . 2))))"#;
    let expect = expect_test::expect![[r#""OK ((a . 1) (\"hello\" . 1) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_assoc_with_test_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(assoc "HELLO" '(("hello" . 1) ("world" . 2))
                         'string-equal)"#;
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rassq_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (b . 2)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(rassq 2 '((a . 1) (b . 2) (c . 3)))", expect);
    assert_ok_eq("(b . 2)", &o, &n);
}

#[test]
fn oracle_prop_rassq_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(rassq 99 '((a . 1) (b . 2)))", expect);
    assert_ok_eq("nil", &o, &n);
}
