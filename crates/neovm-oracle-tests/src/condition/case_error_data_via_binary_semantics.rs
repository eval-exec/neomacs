//! Oracle parity tests for `condition-case` error data via binary.
//!
//! Uses `eval_oracle_and_neovm` which spawns the actual
//! Neomacs release binary, giving access to the full Lisp library and
//! matching the GNU Emacs binary-based evaluation on the oracle side.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_condition_case_car_err_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    // This previously returned void-function in-process but may work
    // correctly when run through the release binary.
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (error "test")
       (error (car err)))"#,
        expect,
    );
    assert_ok_eq("error", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_cadr_err_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"my message\"""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (error "my message")
       (error (cadr err)))"#,
        expect,
    );
    assert_ok_eq("\"my message\"", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_error_data_cons_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (error "msg")
       (error (consp err)))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_re_signal_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK outer-caught""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case nil
         (condition-case nil
             (error "inner")
           (arith-error 'wrong-handler))
       (error 'outer-caught))"#,
        expect,
    );
    assert_ok_eq("outer-caught", &oracle, &neovm);
}
