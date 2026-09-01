//! Divergence tests: dynamic binding + closure + eval + obarray combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_closure_over_dynbound_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 7 15 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-dyn-x-xxx 0)
  (defun test-make-adder-xxx ()
    (let ((test-dyn-x-xxx 10))
      (list (lambda () test-dyn-x-xxx)
            (lambda (n) (+ test-dyn-x-xxx n))
            (eval '(+ test-dyn-x-xxx 5)))))
  (let ((adders (test-make-adder-xxx)))
    (list (funcall (car adders))
          (funcall (cadr adders) 7)
          (caddr adders)
          test-dyn-x-xxx))) "#,
        expect,
    );
}

#[test]
fn divergence_unintern_reintern_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 42 42 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-uir-sym-xxx 42)
  (let ((closure (let ((test-uir-sym-xxx 100))
                   (lambda () test-uir-sym-xxx))))
    (let ((v1 (funcall closure)))
      (unintern 'test-uir-sym-xxx obarray)
      (defvar test-uir-sym-xxx 99)
      (let ((v2 (funcall closure))
            (v3 test-uir-sym-xxx))
        (list v1 v2 v3
              (= v1 100)
              (= v3 99)))))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_defun_captures_dynvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 77 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-edc-val-xxx 0)
  (let ((test-edc-val-xxx 55))
    (eval '(defun test-edc-get-xxx () test-edc-val-xxx)))
  (let ((v1 (test-edc-get-xxx)))
    (let ((test-edc-val-xxx 77))
      (list v1 (test-edc-get-xxx)
            (= v1 0)
            (= (test-edc-get-xxx) 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_closures_eval_mutating() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-nc-counter-xxx 0)
  (let ((test-nc-counter-xxx 1))
    (let ((inner (lambda ()
                   (cl-incf test-nc-counter-xxx)
                   test-nc-counter-xxx)))
      (let ((wrapper (lambda ()
                       (list (funcall inner)
                             (funcall inner)
                             (eval 'test-nc-counter-xxx)))))
        (let ((r1 (funcall wrapper)))
          (list r1
                test-nc-counter-xxx
                (= test-nc-counter-xxx 0)))))) "#,
        expect,
    );
}

#[test]
fn divergence_obarray_map_closure_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-ob-count-xxx 0)
  (let ((syms nil))
    (mapatoms (lambda (s)
                (when (and (fboundp s)
                           (string-match "^test-ob-" (symbol-name s)))
                  (push s syms)
                  (cl-incf test-ob-count-xxx))))
    (list (>= test-ob-count-xxx 1)
          (>= (length syms) 1)
          (memq 'test-ob-count-xxx syms)))) "#,
        expect,
    );
}

#[test]
fn divergence_set_symbol_value_eval_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 2 3 4 5) t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-rt-val-xxx nil)
  (set 'test-rt-val-xxx '(1 2 3))
  (let ((v1 (eval 'test-rt-val-xxx)))
    (set 'test-rt-val-xxx (append v1 '(4 5)))
    (let ((v2 (eval 'test-rt-val-xxx)))
      (list v1 v2
            (equal v1 '(1 2 3))
            (equal v2 '(1 2 3 4 5))
            (= (length v2) 5))))) "#,
        expect,
    );
}

#[test]
fn divergence_let_dynamic_shadow_eval_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 (20 30 20) 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-shd-x-xxx 10)
  (list
   (eval 'test-shd-x-xxx)
   (let ((test-shd-x-xxx 20))
     (list (eval 'test-shd-x-xxx)
           (let ((test-shd-x-xxx 30))
             (eval 'test-shd-x-xxx))
           (eval 'test-shd-x-xxx)))
   (eval 'test-shd-x-xxx))) "#,
        expect,
    );
}

#[test]
fn divergence_multiple_closures_share_dynvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 200 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-sc-shared-xxx 0)
  (let ((test-sc-shared-xxx 100)
        (reader (lambda () test-sc-shared-xxx))
        (writer (lambda (v) (setq test-sc-shared-xxx v))))
    (let ((r1 (funcall reader)))
      (funcall writer 200)
      (let ((r2 (funcall reader)))
        (list r1 r2
              (= r1 100)
              (= r2 200)))))) "#,
        expect,
    );
}

#[test]
fn divergence_fset_eval_lambda_captures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 99 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-fs-x-xxx 0)
  (let ((test-fs-x-xxx 42))
    (fset 'test-fs-fn-xxx (eval '(lambda () test-fs-x-xxx))))
  (let ((v1 (test-fs-fn-xxx)))
    (let ((test-fs-x-xxx 99))
      (list v1 (test-fs-fn-xxx)
            (= v1 0)
            (= (test-fs-fn-xxx) 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_obarray_intern_soft_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (intern "test-is-xxx")
  (let ((s1 (intern-soft "test-is-xxx"))
        (s2 (intern-soft "test-is-xxx" obarray)))
    (unintern "test-is-xxx" obarray)
    (let ((s3 (intern-soft "test-is-xxx"))
          (s4 (intern "test-is-xxx")))
      (let ((s5 (intern-soft "test-is-xxx")))
        (list (symbolp s1) (eq s1 s2) (null s3)
              (eq s4 s5) (symbolp s4)))))) "#,
        expect,
    );
}
