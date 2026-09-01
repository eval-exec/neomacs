//! Oracle parity tests for GNU mode-hook runner semantics.
//!
//! GNU implements `run-mode-hooks` and `delay-mode-hooks` in `lisp/subr.el`.
//! The runner has mode-specific sequencing beyond `run-hooks`: delayed hooks
//! are queued per buffer, then flushed before the explicit hooks, followed by
//! `after-change-major-mode-hook` and delayed derived-mode `:after-hook` forms.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_run_mode_hooks_delayed_order_and_after_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--rmh-a nil)
  (defvar neovm--rmh-b nil)
  (defvar neovm--rmh-c nil)
  (defvar neovm--rmh-d nil)
  (let ((log nil)
        (old-change change-major-mode-after-body-hook)
        (old-after after-change-major-mode-hook))
    (unwind-protect
        (progn
          (setq change-major-mode-after-body-hook
                (list (lambda () (push 'change-major-mode-after-body log))))
          (setq after-change-major-mode-hook
                (list (lambda () (push 'after-change-major-mode log))))
          (setq neovm--rmh-a (list (lambda () (push 'a log))))
          (setq neovm--rmh-b (list (lambda () (push 'b log))))
          (setq neovm--rmh-c (list (lambda () (push 'c log))))
          (setq neovm--rmh-d (list (lambda () (push 'd log))))
          (with-temp-buffer
            (setq-local delayed-after-hook-functions
                        (list (lambda () (push 'after-hook-2 log))
                              (lambda () (push 'after-hook-1 log))))
            (let ((delayed-state
                   (delay-mode-hooks
                     (run-mode-hooks 'neovm--rmh-a)
                     (run-mode-hooks 'neovm--rmh-b 'neovm--rmh-c)
                     (list log
                           delayed-mode-hooks
                           delay-mode-hooks
                           (local-variable-p 'delay-mode-hooks)))))
              (list delayed-state
                    (run-mode-hooks 'neovm--rmh-d)
                    (nreverse log)
                    delayed-mode-hooks
                    delayed-after-hook-functions
                    delay-mode-hooks
                    (local-variable-p 'delay-mode-hooks)))))
      (setq change-major-mode-after-body-hook old-change)
      (setq after-change-major-mode-hook old-after)
      (makunbound 'neovm--rmh-a)
      (makunbound 'neovm--rmh-b)
      (makunbound 'neovm--rmh-c)
      (makunbound 'neovm--rmh-d))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil (neovm--rmh-c neovm--rmh-d) t t) nil (change-major-mode-after-body a b c d after-change-major-mode after-hook-1 after-hook-2) nil nil nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delay_mode_hooks_macroexpansion_and_dynamic_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (list
   (macroexpand '(delay-mode-hooks (list delay-mode-hooks)))
   (local-variable-p 'delay-mode-hooks)
   (let ((inside
          (delay-mode-hooks
            (list delay-mode-hooks
                  (local-variable-p 'delay-mode-hooks)
                  (assq 'delay-mode-hooks (buffer-local-variables))))))
     (list inside
           delay-mode-hooks
           (local-variable-p 'delay-mode-hooks)
           (assq 'delay-mode-hooks (buffer-local-variables))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((progn (make-local-variable 'delay-mode-hooks) (let ((delay-mode-hooks t)) (list delay-mode-hooks))) nil ((t t (delay-mode-hooks . t)) nil t (delay-mode-hooks)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
