//! Oracle parity tests for dynamic binding via `let` with `defvar`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_let_dynamic_basic_rebinding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar makes a special/dynamic variable; let rebinds it dynamically
    let form = "(progn
                  (defvar neovm--test-dyn-var 10)
                  (let ((neovm--test-dyn-var 42))
                    neovm--test-dyn-var))";
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_prop_let_dynamic_restore_after_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defvar neovm--test-dyn-v2 'original)
                  (let ((neovm--test-dyn-v2 'rebound))
                    nil)
                  neovm--test-dyn-v2)";
    let expect = expect_test::expect![[r#""OK original""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("original", &o, &n);
}

#[test]
fn oracle_prop_let_dynamic_visible_in_called_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dynamic binding is visible in called functions
    let form = "(progn
                  (defvar neovm--test-dyn-v3 'global)
                  (fset 'neovm--test-read-dyn (lambda () neovm--test-dyn-v3))
                  (unwind-protect
                      (list (funcall 'neovm--test-read-dyn)
                            (let ((neovm--test-dyn-v3 'local))
                              (funcall 'neovm--test-read-dyn))
                            (funcall 'neovm--test-read-dyn))
                    (fmakunbound 'neovm--test-read-dyn)))";
    let expect = expect_test::expect![[r#""OK (global local global)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(global local global)", &o, &n);
}

#[test]
fn oracle_prop_let_dynamic_nested_rebinding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defvar neovm--test-dyn-v4 0)
                  (let ((neovm--test-dyn-v4 1))
                    (let ((neovm--test-dyn-v4 2))
                      (let ((neovm--test-dyn-v4 3))
                        neovm--test-dyn-v4))))";
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_let_dynamic_unwind_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dynamic binding is restored even when error occurs
    let form = "(progn
                  (defvar neovm--test-dyn-v5 'safe)
                  (condition-case nil
                      (let ((neovm--test-dyn-v5 'danger))
                        (signal 'error '(\"boom\")))
                    (error nil))
                  neovm--test-dyn-v5)";
    let expect = expect_test::expect![[r#""OK safe""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("safe", &o, &n);
}

#[test]
fn oracle_prop_let_dynamic_setq_affects_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defvar neovm--test-dyn-v6 'initial)
                  (let ((neovm--test-dyn-v6 'rebound))
                    (setq neovm--test-dyn-v6 'mutated)
                    neovm--test-dyn-v6))";
    let expect = expect_test::expect![[r#""OK mutated""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("mutated", &o, &n);
}
