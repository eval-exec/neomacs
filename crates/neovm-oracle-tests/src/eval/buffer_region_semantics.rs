//! Oracle parity tests for `eval-buffer` and `eval-region`.
//!
//! GNU implements `eval-buffer` and `eval-region` in `src/lread.c`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_eval_buffer_returns_nil_for_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-evalbuf*"))
  (erase-buffer)
  (eval-buffer))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_eval_buffer_evaluates_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-evalbuf2*"))
  (erase-buffer)
  (insert "(setq neovm--test-evalbuf-result 42)")
  (eval-buffer)
  neovm--test-evalbuf-result)"#,
        expect,
    );
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_eval_region_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-evalreg*"))
  (erase-buffer)
  (insert "99")
  (eval-region (point-min) (point-max)))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_eval_buffer_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(eval-buffer 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
