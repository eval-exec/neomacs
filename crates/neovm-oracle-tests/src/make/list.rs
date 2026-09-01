//! Oracle parity tests for `make-list`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_make_list_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 0 0 0 0)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(make-list 5 0)", expect);
    assert_ok_eq("(0 0 0 0 0)", &o, &n);

    let expect = expect_test::expect![[r#""OK (x x x)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(make-list 3 'x)", expect);
    assert_ok_eq("(x x x)", &o, &n);
}

#[test]
fn oracle_prop_make_list_zero_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(make-list 0 'anything)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_make_list_negative_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Emacs signals error for negative length
    let form = "(make-list -1 'x)";
    let (oracle, neovm) = eval_oracle_and_neovm(form);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_make_list_fixnat_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/alloc.c:Fmake_list validates LENGTH with CHECK_FIXNAT, so all
    // invalid length classes signal `wrong-type-argument' with `wholenump'.
    let form = r#"
(list
 (condition-case err
     (make-list -1 'x)
   (error (list (car err) (cdr err))))
 (condition-case err
     (make-list 1.2 'x)
   (error (list (car err) (cdr err))))
 (condition-case err
     (make-list nil 'x)
   (error (list (car err) (cdr err))))
 (let* ((cell (list 'shared))
        (made (make-list 3 cell)))
   (list made
         (eq (car made) cell)
         (eq (car made) (cadr made))
         (eq (cadr made) (caddr made)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (wholenump -1)) (wrong-type-argument (wholenump 1.2)) (wrong-type-argument (wholenump nil)) (((shared) (shared) (shared)) t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_list_with_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(make-list 4 nil)", expect);
    assert_ok_eq("(nil nil nil nil)", &o, &n);
}

#[test]
fn oracle_prop_make_list_with_complex_init() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Same object repeated — all elements eq
    let form = "(let ((lst (make-list 3 '(a b))))
                  (eq (car lst) (cadr lst)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_make_list_length_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(length (make-list 10 42))", expect);
    assert_ok_eq("10", &o, &n);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_make_list_length(
        len in 0usize..20usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(length (make-list {} 'x))", len);
        let expected = format!("OK {}", len);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), expected.as_str());
        prop_assert_eq!(oracle.as_str(), expected.as_str());
    }
}
