//! Divergence tests: eval, apply, funcall edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eval_nested_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((x 'outer))
  (list x
        (let ((x 'inner))
          (eval 'x))
        (eval 'x)))"#, expect);
}

#[test]
fn divergence_funcall_interactively() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (nil nil)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (called-interactively-p 'interactive)
  (interactive-p))"#, expect);
}

#[test]
fn divergence_function_quoting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (42 42 t t)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((fn1 (function (lambda (x) (1+ x))))
        (fn2 #'(lambda (x) (1+ x))))
  (list (funcall fn1 41)
        (funcall fn2 41)
        (functionp fn1)
        (functionp fn2)))"#, expect);
}

#[test]
fn divergence_defalias_and_fset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (1 car my-alias-fn nil)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defalias 'my-alias-fn #'car)
  (list (my-alias-fn '(1 2 3))
        (symbol-function 'my-alias-fn)
        (fmakunbound 'my-alias-fn)
        (fboundp 'my-alias-fn)))"#, expect);
}

#[test]
fn divergence_special_form_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (t t t nil t)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(list
  (special-form-p (symbol-function 'if))
  (special-form-p (symbol-function 'let))
  (special-form-p (symbol-function 'condition-case))
  (special-form-p (symbol-function 'car))
  (special-form-p (symbol-function 'and)))"#, expect);
}

#[test]
fn divergence_setq_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (50 100 50)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-sdq-var 0)
  (setq-default my-sdq-var 50)
  (let ((buf1 (get-buffer-create " *test-sdq1*"))
        (buf2 (get-buffer-create " *test-sdq2*")))
    (unwind-protect
        (progn
          (with-current-buffer buf1
            (setq-local my-sdq-var 100))
          (list (default-value 'my-sdq-var)
                (buffer-local-value 'my-sdq-var buf1)
                (buffer-local-value 'my-sdq-var buf2)))
      (kill-buffer buf1)
      (kill-buffer buf2))))"#, expect);
}

#[test]
fn divergence_dynamic_binding_with_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK nil""#]];
crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-dyn-var 100)
  (let ((my-dyn-var 200))
    (list my-dyn-var
          (eval 'my-dyn-var)
          (let ((my-dyn-var 300))
            (list my-dyn-var (eval 'my-dyn-var)))))"#, expect);
}

#[test]
fn divergence_backtrace_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK nil""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((frames nil))
  (condition-case err
      (letrec ((f (lambda (n)
                    (if (= n 0)
                        (signal 'error "bottom")
                      (funcall f (1- n))))))
        (funcall f 3))
    (error
     (let ((bt (with-output-to-string
                  (backtrace))))
       (if (> (length bt) 0) 'has-backtrace 'no-backtrace))))"#, expect);
}

#[test]
fn divergence_obarray_intern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((ob (make-obarray 13)))
  (intern "hello" ob)
  (intern "world" ob)
  (list (intern-soft "hello" ob)
        (intern-soft "world" ob)
        (intern-soft "missing" ob)
        (let (count)
          (mapatoms (lambda (s) (push s count)) ob)
          (length count))))"#, expect);
}

#[test]
fn divergence_unintern_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments unintern 1)""#]];
crate::common::assert_oracle_parity_expect(
        r#"(let ((sym (intern "test-unintern-me")))
  (list (intern-soft "test-unintern-me")
        (unintern "test-unintern-me")
        (intern-soft "test-unintern-me")))"#, expect);
}
