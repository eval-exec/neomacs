//! Oracle parity tests for `condition-case` error data structure.
//!
//! These strict tests use primitive `signal` directly.  GNU defines
//! `error` and `cadr` in lisp/subr.el, so those full-runtime paths live
//! in condition-case-error-data-via-binary-semantics.rs.
//!
//! GNU src/eval.c: error data is a cons `(ERROR-SYMBOL . DATA)`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_condition_case_error_symbol_is_car_of_err() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    // GNU eval.c `signal_or_quit` binds VAR to `(ERROR-SYMBOL . SIGNAL-DATA)`.
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (signal 'error '("test"))
       (error (car err)))"#,
        expect,
    );
    assert_ok_eq("error", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_error_message_is_first_data_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"my message\"""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (signal 'error '("my message"))
       (error (car (cdr err))))"#,
        expect,
    );
    assert_ok_eq("\"my message\"", &oracle, &neovm);
}

#[test]
fn oracle_condition_case_error_data_is_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err
         (signal 'error '("msg"))
       (error (listp err)))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}
