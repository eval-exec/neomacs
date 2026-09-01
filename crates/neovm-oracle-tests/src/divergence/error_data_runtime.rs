//! Error parity: condition-case error symbol + data + message text for
//! wrong-type-argument, arith-error, void-function/variable, wrong-number-
//! of-arguments, args-out-of-range, custom signal/user-error/error, no-catch
//! throw, and cl-assert/cl-the.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn err_args_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (wrong-number-of-arguments wrong-number-of-arguments wrong-number-of-arguments)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (funcall (lambda (a b) (+ a b)) 1) (error (car e)))
        (condition-case e (car) (error (car e)))
        (condition-case e (cons 1) (error (car e))))"##,
        expect,
    );
}

#[test]
fn err_arith() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((arith-error) 1.0e+INF)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (/ 1 0) (error (cons (car e) (cdr e))))
        (condition-case e (/ 1.0 0) (error (cons (car e) (cdr e)))))"##,
        expect,
    );
}

#[test]
fn err_cl_assert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (assert-failed wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (condition-case e (cl-assert (= 1 2)) (cl-assertion-failed 'assert-failed) (error (car e)))
      (condition-case e (cl-the integer "x") (error (car e))))"##,
        expect,
    );
}

#[test]
fn err_message_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Wrong type argument: number-or-marker-p, \\\"x\\\"\" \"Arithmetic error\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (+ 1 "x") (error (error-message-string e)))
        (condition-case e (/ 1 0) (error (error-message-string e))))"##,
        expect,
    );
}

#[test]
fn err_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1 (args-out-of-range \"ab\" 0 9) (wrong-type-argument characterp -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (nth -1 '(1 2)) (error (cons (car e) (cdr e))))
        (condition-case e (substring "ab" 0 9) (error (cons (car e) (cdr e))))
        (condition-case e (char-to-string -1) (error (cons (car e) (cdr e)))))"##,
        expect,
    );
}

#[test]
fn err_throw_no_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nocatch no-such-tag 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (throw 'no-such-tag 42) (no-catch (cons 'nocatch (cdr e))) (error (car e)))"##,
        expect,
    );
}

#[test]
fn err_user_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (other \"custom 42\" \"plain msg\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (signal 'my-custom-err '(1 2 3)) (my-custom-err (cons 'caught (cdr e))) (error 'other))
        (condition-case e (user-error "custom %d" 42) (user-error (cadr e)))
        (condition-case e (error "plain %s" "msg") (error (cadr e))))"##,
        expect,
    );
}

#[test]
fn err_void() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((void-function neo-no-such-fn-xyz) (void-variable neo-no-such-var-xyz))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (funcall 'neo-no-such-fn-xyz) (error (cons (car e) (cdr e))))
        (condition-case e (symbol-value 'neo-no-such-var-xyz) (error (cons (car e) (cdr e)))))"##,
        expect,
    );
}

#[test]
fn err_wrong_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments foo 3) wrong-number-of-arguments)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (signal 'wrong-number-of-arguments '(foo 3)) (error (cons (car e) (cdr e))))
        (condition-case e (let ((x)) (setq x)) (error (car e))))"##,
        expect,
    );
}

#[test]
fn err_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument number-or-marker-p \"x\") (wrong-type-argument listp 5) (args-out-of-range [1 2] 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (+ 1 "x") (error (cons (car e) (cdr e))))
        (condition-case e (car 5) (error (cons (car e) (cdr e))))
        (condition-case e (aref [1 2] 9) (error (cons (car e) (cdr e)))))"##,
        expect,
    );
}
