//! Oracle parity tests for advanced `apply` usage:
//! rest args, multiple argument spreading, apply with builtins,
//! and apply in higher-order patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// apply with various argument patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_builtin_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 15""#];
    crate::common::assert_oracle_parity_expect("(apply #'+ '(1 2 3 4 5))", expect);
    let expect = expect_test::expect![r#""OK 120""#];
    crate::common::assert_oracle_parity_expect("(apply #'* '(1 2 3 4 5))", expect);
    let expect = expect_test::expect![r#""OK 9""#];
    crate::common::assert_oracle_parity_expect("(apply #'max '(3 1 4 1 5 9))", expect);
    let expect = expect_test::expect![r#""OK 1""#];
    crate::common::assert_oracle_parity_expect("(apply #'min '(3 1 4 1 5 9))", expect);
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect("(apply #'concat '(\"a\" \"b\" \"c\"))", expect);
    let expect = expect_test::expect![r#""OK (1 2 3)""#];
    crate::common::assert_oracle_parity_expect("(apply #'list '(1 2 3))", expect);
    let expect = expect_test::expect![r#""OK [1 2 3]""#];
    crate::common::assert_oracle_parity_expect("(apply #'vector '(1 2 3))", expect);
}

#[test]
fn oracle_prop_apply_with_leading_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 15""#];
    // apply with individual args before the list
    crate::common::assert_oracle_parity_expect("(apply #'+ 1 2 '(3 4 5))", expect);
    let expect = expect_test::expect![r#""OK (a b c d e)""#];
    crate::common::assert_oracle_parity_expect("(apply #'list 'a 'b '(c d e))", expect);
    let expect = expect_test::expect![[r#""OK \"xyz\"""#]];
    crate::common::assert_oracle_parity_expect("(apply #'concat \"x\" '(\"y\" \"z\"))", expect);
}

#[test]
fn oracle_prop_apply_with_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 17""#];
    crate::common::assert_oracle_parity_expect(
        "(apply (lambda (a b c) (+ (* a b) c)) '(3 4 5))",
        expect,
    );
    let expect = expect_test::expect![r#""OK (1 2 3 4)""#];
    crate::common::assert_oracle_parity_expect(
        "(apply (lambda (a &rest r) (cons a r)) 1 '(2 3 4))",
        expect,
    );
}

#[test]
fn oracle_prop_apply_with_optional_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(apply (lambda (a &optional b c)
                         (list a (or b 'default-b) (or c 'default-c)))
                       '(1))";
    let expect = expect_test::expect![r#""OK (1 default-b default-c)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    let form = "(apply (lambda (a &optional b c)
                         (list a (or b 'default-b) (or c 'default-c)))
                       '(1 2))";
    let expect = expect_test::expect![r#""OK (1 2 default-c)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    let form = "(apply (lambda (a &optional b c)
                         (list a (or b 'default-b) (or c 'default-c)))
                       '(1 2 3))";
    let expect = expect_test::expect![r#""OK (1 2 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_apply_with_rest_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(apply (lambda (&rest args) (length args)) '(a b c d))";
    let expect = expect_test::expect![r#""OK 4""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    let form = "(apply (lambda (x &rest args)
                         (cons x (length args)))
                       1 2 '(3 4 5))";
    let expect = expect_test::expect![r#""OK (1 . 4)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_apply_empty_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(apply #'+ '())", expect);
    let expect = expect_test::expect![r#""OK 1""#];
    crate::common::assert_oracle_parity_expect("(apply #'* '())", expect);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect("(apply #'list '())", expect);
}

// ---------------------------------------------------------------------------
// Complex: apply in functional patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_variadic_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compose functions using apply
    let form = "(let ((pipe
                       (lambda (fns)
                         (lambda (val)
                           (let ((result val))
                             (dolist (f fns)
                               (setq result (funcall f result)))
                             result)))))
                  (let ((transform
                         (funcall pipe
                                  (list (lambda (x) (* x 2))
                                        (lambda (x) (+ x 1))
                                        (lambda (x) (* x x))))))
                    (list (funcall transform 3)
                          (funcall transform 5))))";
    let expect = expect_test::expect![r#""OK (49 121)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_apply_dispatch_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use apply with a dispatch table
    let form = "(let ((ops (list (cons '+ #'+)
                                (cons '- #'-)
                                (cons '* #'*)
                                (cons 'max #'max))))
                  (let ((dispatch
                         (lambda (op args)
                           (let ((fn (cdr (assq op ops))))
                             (if fn
                                 (apply fn args)
                               (signal 'error
                                       (list \"unknown op\")))))))
                    (list (funcall dispatch '+ '(1 2 3))
                          (funcall dispatch '* '(2 3 4))
                          (funcall dispatch 'max '(5 2 8 1)))))";
    let expect = expect_test::expect![r#""OK (6 24 8)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_apply_spread_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use apply to spread a list as args to format
    let form = r#"(let ((args-list '("hello %s, you are %d" "world" 42)))
                    (apply #'format args-list))"#;
    let expect = expect_test::expect![[r#""OK \"hello world, you are 42\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
