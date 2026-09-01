//! Divergence tests: complex eval + error + dynamic scope combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_nested_condition_unwind_catch_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((body cleanup) 101)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((log nil)
        (x 0))
  (catch 'done
    (unwind-protect
        (condition-case err
            (progn
              (push 'body log)
              (setq x (1+ x))
              (throw 'done (list 'thrown x)))
          (error
           (push 'caught log)
           (list 'caught err)))
      (push 'cleanup log)
      (setq x (+ x 100))))
  (list (nreverse log) x)) ",
        expect,
    );
}

#[test]
fn divergence_dynamic_rebinding_through_funcall_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a 0) (b 20 (a 20)) (b 20 (a 20)) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-chain-var-xxx 0)
  (defun test-chain-a-xxx ()
    (list 'a test-chain-var-xxx))
  (defun test-chain-b-xxx ()
    (let ((test-chain-var-xxx 20))
      (list 'b test-chain-var-xxx (test-chain-a-xxx))))
  (defun test-chain-c-xxx ()
    (let ((test-chain-var-xxx 30))
      (funcall (symbol-function 'test-chain-b-xxx))))
  (list (test-chain-a-xxx)
        (test-chain-b-xxx)
        (test-chain-c-xxx)
        test-chain-var-xxx)) ",
        expect,
    );
}

#[test]
fn divergence_deeply_nested_unwind_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable log)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((log nil))
  (ignore-errors
    (unwind-protect
        (condition-case _
            (unwind-protect
                (condition-case _
                    (error \"inner\")
                  (error (push 'inner-handler log) (error \"re-raised\")))
              (push 'middle-cleanup log))
          (error
           (push 'inner-handler log)))
          (error
           (push 'outer-caught log)))
      (push 'outer-cleanup log)))
  (nreverse log)) ",
        expect,
    );
}

#[test]
fn divergence_closure_over_dynamic_vars_with_setq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b c) c a a)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-clo-var-xxx 'global)
  (let ((fns nil))
    (dolist (val '(a b c))
      (push (let ((v val))
              (lambda ()
                (setq test-clo-var-xxx v)
                test-clo-var-xxx))
            fns))
    (setq fns (nreverse fns))
    (let ((results (mapcar #'funcall fns)))
      (list results
            test-clo-var-xxx
            (funcall (nth 0 fns))
            test-clo-var-xxx)))) ",
        expect,
    );
}

#[test]
fn divergence_funcall_apply_rest_optional_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 nil nil 0) (1 2 nil 0) (1 2 (3 4 5) 3) (10 nil nil 0) (10 20 (30) 1) (99 88 (77 66) 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-opt-rest-xxx (a &optional b &rest c)
    (condition-case err
        (list a b c (length c))
      (error (list 'err err))))
  (list (test-opt-rest-xxx 1)
        (test-opt-rest-xxx 1 2)
        (test-opt-rest-xxx 1 2 3 4 5)
        (apply #'test-opt-rest-xxx '(10))
        (apply #'test-opt-rest-xxx 10 '(20 30))
        (funcall #'test-opt-rest-xxx 99 88 77 66))) ",
        expect,
    );
}

#[test]
fn divergence_catch_from_mapcar_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (found 7)""#]];
    crate::common::assert_oracle_parity_expect(
        "(catch 'found
  (mapcar (lambda (x)
            (when (> x 5)
              (throw 'found (list 'found x))))
          '(1 3 7 2 9))) ",
        expect,
    );
}

#[test]
fn divergence_dotimes_with_errors_and_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (0 (cleanup . 0) 1 (cleanup . 1) 2 (cleanup . 2) 3 (cleanup . 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let ((log nil))
  (ignore-errors
    (dotimes (i 10)
      (unwind-protect
          (progn
            (push i log)
            (when (= i 3)
              (error \"stop at 3\")))
        (push (cons 'cleanup i) log))))
  (nreverse log)) ",
        expect,
    );
}

#[test]
fn divergence_advised_function_in_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (110 (caught (error \"too big: 15\")) 14)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-adv-fn-xxx (x)
    (if (> x 10) (error \"too big: %d\" x) (* x 2)))
  (advice-add 'test-adv-fn-xxx :filter-return
               (lambda (r) (if (numberp r) (+ r 100) r)))
  (let ((r1 (condition-case err
                (test-adv-fn-xxx 5)
              (error (list 'caught err))))
        (r2 (condition-case err
                (test-adv-fn-xxx 15)
              (error (list 'caught err)))))
    (advice-remove 'test-adv-fn-xxx
                    (lambda (r) (if (numberp r) (+ r 100) r)))
    (list r1 r2 (test-adv-fn-xxx 7)))) ",
        expect,
    );
}

#[test]
fn divergence_cl_block_return_from_nested_flet() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-flet)""#]];
    crate::common::assert_oracle_parity_expect(
        "(cl-flet ((check (x) (when (> x 5) (cl-return-from outer x))))
  (cl-block outer
    (list (check 3)
          (check 7)
          'not-reached))) ",
        expect,
    );
}

#[test]
fn divergence_buffer_local_with_let_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((before buffer global) (inside let-bound global) (modified modified global) (cleanup modified global) (after buffer global))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq test-blv-xxx 'global)
  (make-variable-buffer-local 'test-blv-xxx)
  (setq test-blv-xxx 'buffer)
  (let ((result nil))
    (push (list 'before test-blv-xxx (default-value 'test-blv-xxx)) result)
    (let ((test-blv-xxx 'let-bound))
      (push (list 'inside test-blv-xxx (default-value 'test-blv-xxx)) result)
      (unwind-protect
          (progn
            (setq test-blv-xxx 'modified)
            (push (list 'modified test-blv-xxx (default-value 'test-blv-xxx)) result))
        (push (list 'cleanup test-blv-xxx (default-value 'test-blv-xxx)) result)))
    (push (list 'after test-blv-xxx (default-value 'test-blv-xxx)) result)
    (nreverse result))) ",
        expect,
    );
}
