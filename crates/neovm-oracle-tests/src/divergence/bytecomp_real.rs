//! Divergence tests: real bytecomp behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_byte_compile_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 6 1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((src (lambda (x) (+ x 1)))
        (compiled (byte-compile src)))
  (list (compiled-function-p compiled)
        (funcall compiled 5)
        (funcall compiled 0)
        (funcall compiled -1))) ",
        expect,
    );
}

#[test]
fn divergence_byte_compile_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (11 0 9 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-bc-fn-xxx (a b) (+ a (* b 2)))
  (list (test-bc-fn-xxx 3 4)
        (test-bc-fn-xxx 0 0)
        (test-bc-fn-xxx -1 5)
        (compiled-function-p (symbol-function 'test-bc-fn-xxx)))) ",
        expect,
    );
}

#[test]
fn divergence_byte_compile_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments eval 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((env '((x . 10)))
        (fn (eval '\\`(lambda (y) (+ x y)) lexical-binding)))
  (list (compiled-function-p fn)
        (funcall fn 5)
        (funcall fn 0))) ",
        expect,
    );
}

#[test]
fn divergence_byte_compile_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 120 3628800)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-bc-fact-xxx (n)
    (if (<= n 1) 1 (* n (test-bc-fact-xxx (1- n)))))
  (list (test-bc-fact-xxx 0)
        (test-bc-fact-xxx 1)
        (test-bc-fact-xxx 5)
        (test-bc-fact-xxx 10))) ",
        expect,
    );
}

#[test]
fn divergence_byte_code_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((fn (make-byte-code 514 \"\\300\\301\\042\" [1 2] 2)))
  (list (byte-code-function-p fn)
        (compiled-function-p fn))) ",
        expect,
    );
}

#[test]
fn divergence_disassemble_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-dis-fn-xxx (x) (list x (1+ x)))
  (let ((dis (with-output-to-string
               (disassemble 'test-dis-fn-xxx))))
    (list (> (length dis) 0)
          (string-match \"byte-code\" dis)))) ",
        expect,
    );
}

#[test]
fn divergence_macroexp_macroexpand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((if t (progn 'yes)) (if nil nil 'ok) (progn (setq x 1) (setq x 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (macroexp--expand-all '(when t 'yes))
  (macroexp--expand-all '(unless nil 'ok))
  (macroexp--expand-all '(progn (setq x 1) (setq x 2)))) ",
        expect,
    );
}

#[test]
fn divergence_eval_lexical_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((lexical-binding t))
  (list (eval '(let ((x 5))
                (funcall (lambda () x)))))
  (list (eval '(let ((x 5))
                (let ((f (lambda () x)))
                  (funcall f)))))) ",
        expect,
    );
}

#[test]
fn divergence_closure_print_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((fn (let ((x 42)) (lambda () x)))
        (printed (prin1-to-string fn)))
  (list (stringp printed)
        (string-match \"closure\" printed))) ",
        expect,
    );
}

#[test]
fn divergence_optimized_integer_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 24 \"abc\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-opt-add-xxx () (+ 1 2 3 4 5))
  (defun test-opt-mul-xxx () (* 2 3 4))
  (defun test-opt-concat-xxx () (concat \"a\" \"b\" \"c\"))
  (list (test-opt-add-xxx)
        (test-opt-mul-xxx)
        (test-opt-concat-xxx)
        (compiled-function-p (symbol-function 'test-opt-add-xxx)))) ",
        expect,
    );
}
