//! Divergence tests: advice, hooks, and buffer-local variables.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_advice_add_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-test-fn (x) (* x 2))
  (advice-add 'my-test-fn :around
    (lambda (fn &rest args)
      (apply fn args)))
  (let ((r1 (my-test-fn 5)))
    (advice-remove 'my-test-fn
      (lambda (fn &rest args)
        (apply fn args)))
    (let ((r2 (my-test-fn 5)))
      (list r1 r2))))"#,
        expect,
    );
}

#[test]
fn divergence_advice_before() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((fn 42) (before 42))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((log nil))
  (defun my-adv-fn (x) (push (list 'fn x) log))
  (advice-add 'my-adv-fn :before
    (lambda (x) (push (list 'before x) log)))
  (my-adv-fn 42)
  log)"#,
        expect,
    );
}

#[test]
fn divergence_advice_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((after 42) (fn 42))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((log nil))
  (defun my-adv-fn2 (x) (push (list 'fn x) log))
  (advice-add 'my-adv-fn2 :after
    (lambda (x) (push (list 'after x) log)))
  (my-adv-fn2 42)
  log)"#,
        expect,
    );
}

#[test]
fn divergence_run_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (second first)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((log nil))
  (defvar my-test-hook nil)
  (add-hook 'my-test-hook (lambda () (push 'first log)))
  (add-hook 'my-test-hook (lambda () (push 'second log)) t)
  (run-hooks 'my-test-hook)
  log)"#,
        expect,
    );
}

#[test]
fn divergence_run_hook_with_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((got 42))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((log nil))
  (defvar my-test-hook2 nil)
  (add-hook 'my-test-hook2 (lambda (x) (push (list 'got x) log)))
  (run-hook-with-args 'my-test-hook2 42)
  log)"#,
        expect,
    );
}

#[test]
fn divergence_buffer_local_setq_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 20 10 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf1 (get-buffer-create " *test-bl1*"))
        (buf2 (get-buffer-create " *test-bl2*")))
  (unwind-protect
      (progn
        (with-current-buffer buf1
          (setq-local my-test-var 10))
        (with-current-buffer buf2
          (setq-local my-test-var 20))
        (list
         (buffer-local-value 'my-test-var buf1)
         (buffer-local-value 'my-test-var buf2)
         (with-current-buffer buf1 my-test-var)
         (with-current-buffer buf2 my-test-var)))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}

#[test]
fn divergence_make_variable_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 200 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-bl-var 0)
  (make-variable-buffer-local 'my-bl-var)
  (let ((buf1 (get-buffer-create " *test-mvbl1*"))
        (buf2 (get-buffer-create " *test-mvbl2*")))
    (unwind-protect
        (progn
          (with-current-buffer buf1 (setq my-bl-var 100))
          (with-current-buffer buf2 (setq my-bl-var 200))
          (list (buffer-local-value 'my-bl-var buf1)
                (buffer-local-value 'my-bl-var buf2)
                (default-value 'my-bl-var)))
      (kill-buffer buf1)
      (kill-buffer buf2))))"#,
        expect,
    );
}

#[test]
fn divergence_kill_local_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-kl-var 99)
  (make-variable-buffer-local 'my-kl-var)
  (setq my-kl-var 100)
  (kill-local-variable 'my-kl-var)
  my-kl-var)"#,
        expect,
    );
}

#[test]
fn divergence_default_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (50 100)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-dv-var 0)
  (setq-default my-dv-var 50)
  (let ((buf1 (get-buffer-create " *test-dv1*")))
    (unwind-protect
        (progn
          (with-current-buffer buf1 (setq-local my-dv-var 100))
          (list (default-value 'my-dv-var)
                (buffer-local-value 'my-dv-var buf1)))
      (kill-buffer buf1))))"#,
        expect,
    );
}
