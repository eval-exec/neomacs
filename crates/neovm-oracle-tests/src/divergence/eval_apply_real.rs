//! Divergence tests: real eval/apply behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eval_defun_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (12 0 2 (closure (t) (x) (+ x (* x x))) \"Return X plus its square.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-eval-nested-xxx (x)
    \"Return X plus its square.\"
    (+ x (* x x)))
  (list (test-eval-nested-xxx 3)
        (test-eval-nested-xxx 0)
        (test-eval-nested-xxx -2)
        (symbol-function 'test-eval-nested-xxx)
        (documentation 'test-eval-nested-xxx))) ",
        expect,
    );
}

#[test]
fn divergence_apply_with_spread() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 3 (a b c) [1 2 3 4 5] t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (apply '+ 1 2 '(3 4))
  (apply '+ 1 '(2))
  (apply 'list '(a b c))
  (apply 'vector 1 2 '(3 4 5))
  (= (apply '* 2 3 '(4 5)) 120)) ",
        expect,
    );
}

#[test]
fn divergence_funcall_with_lambda_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 11 3 12)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((make-counter
       (lambda (start)
         (let ((count start))
           (lambda () (setq count (1+ count)))))))
  (let ((c1 (funcall make-counter 0))
        (c2 (funcall make-counter 10)))
    (list (funcall c1)
          (funcall c1)
          (funcall c2)
          (funcall c1)
          (funcall c2)))) ",
        expect,
    );
}

#[test]
fn divergence_recursive_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable fib)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((fib
       (lambda (n self)
         (if (< n 2) n
           (+ (funcall self (- n 1) self)
              (funcall self (- n 2) self)))))))
  (list (funcall fib 0 fib)
        (funcall fib 1 fib)
        (funcall fib 5 fib)
        (funcall fib 10 fib))) ",
        expect,
    );
}

#[test]
fn divergence_condition_case_signal_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp \"not-a-list\")) (args-out-of-range (\"abc\" 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (condition-case err
      (car \"not-a-list\")
    (wrong-type-argument
     (list (car err) (cdr err))))
  (condition-case err
      (aref \"abc\" 10)
    (args-out-of-range
     (list (car err) (cdr err))))) ",
        expect,
    );
}

#[test]
fn divergence_unwind_protect_cleanup_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (result (cleanup body))""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((x nil))
  (list
   (unwind-protect
       (progn (push 'body x) 'result)
     (push 'cleanup x))
   x)) ",
        expect,
    );
}

#[test]
fn divergence_nested_condition_unwind_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cleanup handler body)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((log nil))
  (ignore-errors
    (unwind-protect
        (condition-case err
            (progn (push 'body log) (error \"test\"))
          (error (push 'handler log) (signal (car err) (cdr err))))
      (push 'cleanup log)))
  log) ",
        expect,
    );
}

#[test]
fn divergence_defmacro_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defmacro test-swap-xxx (a b)
    \\`(let ((tmp ,a)) (setq ,a ,b) (setq ,b tmp)))
  (let ((x 1) (y 2))
    (test-swap-xxx x y)
    (list x y
          (macroexpand '(test-swap-xxx x y))))) ",
        expect,
    );
}

#[test]
fn divergence_backquote_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((a 1) (b '(2 3)))
  (list
   \\`(a ,a ,@b)
   (equal \\`(a ,a ,@b) '(a 1 2 3))
   \\`(list ,(+ 1 2) ,@(mapcar '1+ '(4 5)))
   (equal \\`(list ,(+ 1 2) ,@(mapcar '1+ '(4 5)))
          '(list 3 5 6)))) ",
        expect,
    );
}

#[test]
fn divergence_setf_generalized() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([10 99 30] 10 99 30)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((v [10 20 30]))
  (setf (aref v 1) 99)
  (list v
        (aref v 0)
        (aref v 1)
        (aref v 2))) ",
        expect,
    );
}
