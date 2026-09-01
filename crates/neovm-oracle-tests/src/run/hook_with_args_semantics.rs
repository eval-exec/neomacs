//! Oracle parity tests for GNU hook runner primitive semantics.
//!
//! GNU implements `run-hook-with-args`, `run-hook-with-args-until-success`,
//! and `run-hook-with-args-until-failure` in `src/eval.c` through the shared
//! `run_hook_with_args` helper.  These tests pin function-valued hooks,
//! stop-on-success/failure return values, and local `t` splicing of global
//! hooks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_run_hook_with_args_function_value_and_final_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil)
      (hook (lambda (x y)
              (push (list 'hook x y) log)
              'ignored-return)))
  (list
   (run-hook-with-args 'hook 1 2)
   (nreverse log)))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_run_hook_until_success_and_failure_stop_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((success-log nil)
      (failure-log nil)
      (success-hook
       (list
        (lambda (x) (push (list 's1 x) success-log) nil)
        (lambda (x) (push (list 's2 x) success-log) 'success-value)
        (lambda (x) (push (list 's3 x) success-log) 'too-late)))
      (failure-hook
       (list
        (lambda (x) (push (list 'f1 x) failure-log) t)
        (lambda (x) (push (list 'f2 x) failure-log) nil)
        (lambda (x) (push (list 'f3 x) failure-log) t))))
  (list
   (run-hook-with-args-until-success 'success-hook 10)
   (nreverse success-log)
   (run-hook-with-args-until-failure 'failure-hook 20)
   (nreverse failure-log)
   (let ((empty-hook nil))
     (list (run-hook-with-args-until-success 'empty-hook)
           (run-hook-with-args-until-failure 'empty-hook)))))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil t nil (nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_run_hook_with_args_local_t_ordering_for_stop_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--rhwa-success nil)
  (defvar neovm--rhwa-failure nil)
  (let ((success-log nil)
        (failure-log nil))
    (unwind-protect
        (progn
          (setq neovm--rhwa-success
                (list
                 (lambda (x) (push (list 'global-s x) success-log) 'global-success)))
          (setq neovm--rhwa-failure
                (list
                 (lambda (x) (push (list 'global-f x) failure-log) nil)))
          (with-temp-buffer
            (setq-local neovm--rhwa-success
                        (list
                         (lambda (x) (push (list 'local-s-before x) success-log) nil)
                         t
                         (lambda (x) (push (list 'local-s-after x) success-log) 'late)))
            (setq-local neovm--rhwa-failure
                        (list
                         (lambda (x) (push (list 'local-f-before x) failure-log) t)
                         t
                         (lambda (x) (push (list 'local-f-after x) failure-log) nil)))
            (list
             (run-hook-with-args-until-success 'neovm--rhwa-success 1)
             (nreverse success-log)
             (run-hook-with-args-until-failure 'neovm--rhwa-failure 2)
             (nreverse failure-log))))
      (makunbound 'neovm--rhwa-success)
      (makunbound 'neovm--rhwa-failure))))
"#;

    let expect = expect_test::expect![[
        r#""OK (global-success ((local-s-before 1) (global-s 1)) nil ((local-f-before 2) (global-f 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
