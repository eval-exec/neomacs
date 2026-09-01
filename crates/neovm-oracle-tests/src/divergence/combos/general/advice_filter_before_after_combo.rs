//! Deep combo: advice + :filter + :before + :after + :around + override.
//! Tests advice system combinations with function call chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_advice_before_modifies_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (invalid-function (closure (t) (lambda (args) (list (1+ (car args)))) my-fn-fa))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-fn (x) (* x 10))\n\
         (define-advice my-fn (:filter-args (lambda (args)\n\
         (list (1+ (car args)))))\n\
         my-fn-fa)\n\
         (list (my-fn 5)\n\
         (advice--p (ad-get 'my-fn))))",
        expect,
    );
}

#[test]
fn deficiency_advice_after_accesses_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Unrecognized name spec ‘(push (list x (my-square--my-square x)) after-log)’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar after-log nil)\n\
         (defun my-square (x) (* x x))\n\
         (define-advice my-square (:after (x &rest _)\n\
         (push (list x (my-square--my-square x)) after-log))\n\
         my-square-logger)\n\
         (my-square 7)\n\
         (list (my-square 3)\n\
         (nreverse after-log)))",
        expect,
    );
}

#[test]
fn deficiency_advice_around_wraps_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Unrecognized name spec ‘(* 2 (funcall fn a b))’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-add (a b) (+ a b))\n\
         (define-advice my-add (:around (fn a b)\n\
         (* 2 (funcall fn a b)))\n\
         my-add-doubler)\n\
         (list (my-add 3 4)\n\
         (my-add 10 20)))",
        expect,
    );
}

#[test]
fn deficiency_advice_remove_restores_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Unrecognized name spec ‘(* 10 (funcall fn x))’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-test (x) (1+ x))\n\
         (define-advice my-test (:around (fn x)\n\
         (* 10 (funcall fn x)))\n\
         my-test-multiply)\n\
         (let ((advised (my-test 5)))\n\
         (advice-remove 'my-test 'my-test-multiply)\n\
         (list advised (my-test 5))))",
        expect,
    );
}

#[test]
fn deficiency_advice_override_replaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Unrecognized name spec ‘(- x 50)’\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-base (x) (+ x 100))\n\
         (define-advice my-base (:override (x)\n\
         (- x 50))\n\
         my-base-override)\n\
         (list (my-base 10)))",
        expect,
    );
}

#[test]
fn deficiency_multiple_advice_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Unrecognized name spec ‘(push 'before advice-log)’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar advice-log nil)\n\
         (defun my-chain (x) (push 'original advice-log) x)\n\
         (define-advice my-chain (:before (x)\n\
         (push 'before advice-log))\n\
         my-chain-before)\n\
         (define-advice my-chain (:after (x)\n\
         (push 'after advice-log))\n\
         my-chain-after)\n\
         (my-chain 42)\n\
         (nreverse advice-log))",
        expect,
    );
}

#[test]
fn deficiency_advice_on_builtin_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (define-advice 1+ (:filter-return (ret))\n\
         (if (numberp ret) (* ret 2) ret))\n\
         (list (1+ 5)\n\
         (1+ -1)))",
        expect,
    );
}

#[test]
fn deficiency_malformed_builtin_advice_signals_invalid_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK invalid-function""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case nil\n\
         (progn\n\
         (define-advice car (:filter-return (lambda (ret)\n\
         (if (numberp ret) (* ret 2) ret)))\n\
         car-double)\n\
         (car '(5 . rest)))\n\
         (invalid-function 'invalid-function))",
        expect,
    );
}

#[test]
fn deficiency_advice_with_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Unrecognized name spec ‘(setq call-count (1+ call-count))’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar call-count 0)\n\
         (defun my-counted (x) x)\n\
         (define-advice my-counted (:around (fn x)\n\
         (setq call-count (1+ call-count))\n\
         (funcall fn x))\n\
         my-counted-counter)\n\
         (my-counted 1)\n\
         (my-counted 2)\n\
         (my-counted 3)\n\
         (list call-count (my-counted 4)))",
        expect,
    );
}

#[test]
fn deficiency_advice_named_vs_anonymous() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-plain (x) x)\n\
         (advice-add 'my-plain :around\n\
         (lambda (fn x) (funcall fn (* x 2))))\n\
         (list (my-plain 5)))",
        expect,
    );
}

#[test]
fn deficiency_nested_advice_around_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Unrecognized name spec ‘(push 'outer nested-log)’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar nested-log nil)\n\
         (defun my-nested (x) (push 'base nested-log) x)\n\
         (define-advice my-nested (:around (fn x)\n\
         (push 'outer nested-log) (funcall fn x))\n\
         my-nested-outer)\n\
         (define-advice my-nested (:around (fn x)\n\
         (push 'inner nested-log) (funcall fn x))\n\
         my-nested-inner)\n\
         (my-nested 42)\n\
         (nreverse nested-log))",
        expect,
    );
}
