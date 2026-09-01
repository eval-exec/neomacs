//! Divergence tests: closure, lexical binding, and eval edge cases.
//!
//! Tests for closure capture semantics, lexical/dynamic interaction,
//! eval with lexical-binding flag, and function introspection.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_closure_captures_outer_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK 20""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((x 10))
  (funcall
   (let ((x 20))
     (lambda () x))))"#, expect);
}

#[test]
fn divergence_closure_mutates_captured_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK 3""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((counter 0))
  (let ((inc (lambda () (setq counter (1+ counter)))))
    (funcall inc)
    (funcall inc)
    (funcall inc)
    counter))"#, expect);
}

#[test]
fn divergence_let_parallel_vs_let_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (11 2)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (let ((a 10)) (let ((a 1) (a (1+ a))) a))
  (let ((a 10)) (let* ((a 1) (a (1+ a))) a)))"#, expect);
}

#[test]
fn divergence_lexical_shadow_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (200 300 200 200)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dyn-var 100)
  (let ((dyn-var 200))
    (list dyn-var
          (let ((dyn-var 300)) dyn-var)
          dyn-var
          (eval 'dyn-var))))"#, expect);
}

#[test]
fn divergence_closure_over_loop_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (0 1 2 3 4)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((fns nil))
  (dotimes (i 5)
    (push (lambda () i) fns))
  (mapcar #'funcall (nreverse fns)))"#, expect);
}

#[test]
fn divergence_funcall_with_and_rest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (1 (2 3 4))""#]];
crate::common::assert_oracle_parity_expect(r#"(funcall (lambda (a &rest b) (list a b)) 1 2 3 4)"#, expect);
}

#[test]
fn divergence_funcall_with_and_optional() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK ((1 nil nil) (1 2 nil) (1 2 3))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (funcall (lambda (a &optional b c) (list a b c)) 1)
  (funcall (lambda (a &optional b c) (list a b c)) 1 2)
  (funcall (lambda (a &optional b c) (list a b c)) 1 2 3))"#, expect);
}

#[test]
fn divergence_function_type_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (t nil t t nil)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (subrp (symbol-function 'car))
  (byte-code-function-p (symbol-function 'car))
  (functionp 'car)
  (functionp (lambda (x) x))
  (commandp 'car))"#, expect);
}

#[test]
fn divergence_apply_spreads_last_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (15 0 (1 2 3 4))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (apply #'+ 1 2 '(3 4 5))
  (apply #'+ nil)
  (apply #'list 1 2 '(3 4)))"#, expect);
}

#[test]
fn divergence_mapcar_mapc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK ((2 3 4 5 6) (c b a))""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (mapcar #'1+ '(1 2 3 4 5))
  (let ((acc nil))
    (mapc (lambda (x) (push x acc)) '(a b c))
    acc))"#, expect);
}

#[test]
fn divergence_recursive_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK 3628800""#]];
crate::common::assert_oracle_parity_expect(
        r#"(letrec ((fact (lambda (n)
                  (if (<= n 1) 1 (* n (funcall fact (1- n)))))))
  (funcall fact 10))"#, expect);
}

#[test]
fn divergence_dyn_wind_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (outer-cleanup inner-cleanup body)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((log nil))
  (condition-case nil
      (unwind-protect
          (unwind-protect
              (progn
                (push 'body log)
                (signal 'error "test"))
            (push 'inner-cleanup log))
        (push 'outer-cleanup log))
    (error nil))
  log)"#, expect);
}

#[test]
fn divergence_catch_throw_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (from-inner 42)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (catch 'outer
    (catch 'inner
      (throw 'outer 'from-inner))
    'not-reached)
  (catch 'tag
    (throw 'tag 42)))"#, expect);
}

#[test]
fn divergence_eval_with_lexical_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (5 5)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (eval '(let ((x 5)) (funcall (lambda () x))) t)
  (eval '(let ((x 5)) (funcall (lambda () x))) nil))"#, expect);
}

#[test]
fn divergence_closure_with_docstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (42 \"docstring\")""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((fn (lambda (x) "docstring" (1+ x))))
  (list (funcall fn 41) (documentation fn)))"#, expect);
}
