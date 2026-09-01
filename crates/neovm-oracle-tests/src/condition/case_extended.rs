//! Oracle parity tests for `condition-case` with thorough coverage.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_condition_case_multiple_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list
                  (condition-case err
                      (/ 1 0)
                    (wrong-type-argument 'wta)
                    (arith-error 'arith)
                    (error 'generic))
                  (condition-case err
                      (car 1)
                    (wrong-type-argument 'wta)
                    (arith-error 'arith)
                    (error 'generic))
                  (condition-case err
                      (signal 'file-error '(\"not found\"))
                    (wrong-type-argument 'wta)
                    (arith-error 'arith)
                    (error 'generic)))";
    let expect = expect_test::expect![[r#""OK (arith wta generic)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_no_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    // When no error occurs, body value is returned
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(condition-case err (+ 1 2) (error 'oops))",
        expect,
    );
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_condition_case_var_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // err variable binds the error data
    let form = "(condition-case err
                  (signal 'wrong-type-argument '(numberp \"hello\"))
                  (wrong-type-argument
                   (list 'caught (car err) (cadr err) (caddr err))))";
    let expect = expect_test::expect![[r#""OK (caught wrong-type-argument numberp \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_nil_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // nil as variable means don't bind
    let form = "(condition-case nil
                  (car 1)
                  (wrong-type-argument 'caught-it))";
    let expect = expect_test::expect![[r#""OK caught-it""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("caught-it", &o, &n);
}

#[test]
fn oracle_prop_condition_case_handler_body_multiple_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Handler body can have multiple forms
    let form = "(let ((log nil))
                  (condition-case err
                      (/ 1 0)
                    (arith-error
                     (setq log (cons 'first log))
                     (setq log (cons 'second log))
                     (setq log (cons (car err) log))
                     (nreverse log))))";
    let expect = expect_test::expect![[r#""OK (first second arith-error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_nested_different_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(condition-case outer-err
                  (condition-case inner-err
                      (progn
                        (/ 1 0))
                    (wrong-type-argument
                     'inner-wta))
                  (arith-error
                   (list 'outer-arith (car outer-err))))";
    let expect = expect_test::expect![[r#""OK (outer-arith arith-error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_rethrow_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Catch, log, and re-signal
    let form = "(let ((logged nil))
                  (condition-case err
                      (condition-case inner
                          (signal 'error '(\"original\"))
                        (error
                         (setq logged (cdr inner))
                         (signal (car inner) (cdr inner))))
                    (error
                     (list 'final logged (cdr err)))))";
    let expect = expect_test::expect![[r#""OK (final (\"original\") (\"original\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_with_unwind_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((resource nil))
                  (condition-case nil
                      (unwind-protect
                          (progn
                            (setq resource 'acquired)
                            (/ 1 0))
                        (setq resource 'released))
                    (arith-error nil))
                  resource)";
    let expect = expect_test::expect![[r#""OK released""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("released", &o, &n);
}

#[test]
fn oracle_prop_condition_case_debug_on_error_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common pattern: wrap with condition-case only sometimes
    let form = "(let ((safe-mode t))
                  (if safe-mode
                      (condition-case err
                          (/ 1 0)
                        (error (list 'error (car err))))
                    (/ 1 0)))";
    let expect = expect_test::expect![[r#""OK (error arith-error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_condition_case_signal_in_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Signal a new error from within a handler
    let form = "(condition-case outer
                  (condition-case inner
                      (car 1)
                    (wrong-type-argument
                     (signal 'error
                             (list \"wrapped\" (cdr inner)))))
                  (error (list 'final (car outer) (cadr outer))))";
    let expect = expect_test::expect![[r#""OK (final error \"wrapped\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
