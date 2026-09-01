//! Divergence tests: complex interop - eval/funcall/load chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eval_nested_defun_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable f)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (eval '(defun test-compose-fn-xxx (f g)
           (lambda (x) (funcall f (funcall g x)))))
  (let ((inc (lambda (x) (1+ x)))
        (dbl (lambda (x) (* x 2))))
    (let ((inc-then-dbl (test-compose-fn-xxx dbl inc))
          (dbl-then-inc (test-compose-fn-xxx inc dbl)))
      (list (funcall inc-then-dbl 3)
            (funcall dbl-then-inc 3)
            (funcall (test-compose-fn-xxx #'1+ #'1+) 0))))) ",
        expect,
    );
}

#[test]
fn divergence_apply_partial_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 2 21 0 25 60)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((add5 (apply-partially '+ 5))
        (mul3 (apply-partially '* 3)))
  (list (funcall add5 10)
        (funcall add5 -3)
        (funcall mul3 7)
        (funcall mul3 0)
        (apply add5 '(20))
        (apply mul3 '(4 5)))) ",
        expect,
    );
}

#[test]
fn deficiency_mapcan_nconc_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6) t (1 10 2 20 3 30) t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((input '((1 2) (3 4) (5 6)))
        (result (mapcan #'copy-sequence input))
        (doubled (mapcan (lambda (x) (list x (* x 10))) '(1 2 3))))
  (list result
        (equal result '(1 2 3 4 5 6))
        doubled
        (equal doubled '(1 10 2 20 3 30)))) ",
        expect,
    );
}

#[test]
fn divergence_defalias_indirect_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 #<subr car> #<subr car> t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defalias 'test-chain-a-xxx 'car)
  (defalias 'test-chain-b-xxx 'test-chain-a-xxx)
  (defalias 'test-chain-c-xxx 'test-chain-b-xxx)
  (list (funcall 'test-chain-c-xxx '(10 20 30))
        (indirect-function 'test-chain-c-xxx)
        (indirect-function 'test-chain-b-xxx)
        (eq (indirect-function 'test-chain-c-xxx)
            (indirect-function 'car)))) ",
        expect,
    );
}

#[test]
fn divergence_closure_over_let_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (33 6)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((fns nil))
  (let* ((a 1)
         (b (+ a 1))
         (c (+ b 1)))
    (push (lambda () (+ a b c)) fns)
    (let* ((a 10)
           (b 20))
      (push (lambda () (+ a b c)) fns)))
  (list (funcall (nth 0 fns))
        (funcall (nth 1 fns)))) ",
        expect,
    );
}

#[test]
fn divergence_eval_defmacro_then_use() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (eval '(defmacro test-thrice-xxx (expr)
           \\`(list ,expr ,expr ,expr)))
  (let ((counter 0))
    (test-thrice-xxx (cl-incf counter)))) ",
        expect,
    );
}

#[test]
fn divergence_funcall_compose_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 34)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-safe-div-xxx (a b)
    (condition-case err
        (/ a b)
      (arith-error 'div-error)))
  (defun test-safe-sqrt-xxx (x)
    (condition-case err
        (sqrt x)
      (args-out-of-range 'sqrt-error)))
  (list (test-safe-div-xxx 10 3)
        (test-safe-div-xxx 10 0)
        (> (test-safe-sqrt-xxx 16) 3.9)
        (test-safe-sqrt-xxx -1)))) ",
        expect,
    );
}

#[test]
fn deficiency_obarray_map_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-remove-duplicates)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((syms '(test-ob-a-xxx test-ob-b-xxx test-ob-c-xxx)))
  (dolist (s syms) (set s (intern (symbol-name s))))
  (let ((found nil))
    (mapatoms (lambda (s)
                (when (string-match \"test-ob-\" (symbol-name s))
                  (push (intern (symbol-name s)) found))))
    (list (length (cl-remove-duplicates found))
          (>= (length (cl-remove-duplicates found)) 3)
          (member 'test-ob-a-xxx found)))) ",
        expect,
    );
}

#[test]
fn divergence_dynamic_binding_through_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (global (local local) global)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-dyn-eval-xxx 'global)
  (defun test-dyn-access-xxx () test-dyn-eval-xxx)
  (list (test-dyn-access-xxx)
        (let ((test-dyn-eval-xxx 'local))
          (list (test-dyn-access-xxx)
                (eval 'test-dyn-eval-xxx)))
        (test-dyn-access-xxx))) ",
        expect,
    );
}

#[test]
fn deficiency_setf_generalized_with_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([0 10 0 30 0] 0 10 0 30 0 0 [1 11 1 31 1])""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((v [0 0 0 0 0])
        (indices '(1 3)))
  (dolist (i indices)
    (setf (aref v i) (* i 10)))
  (list v
        (aref v 0) (aref v 1) (aref v 2) (aref v 3) (aref v 4)
        (apply #'aref v '(2))
        (apply #'vector (mapcar #'1+ (append v nil))))) ",
        expect,
    );
}
