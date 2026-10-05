use super::ENVS;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_gd_b_safe_call_qt_barrier_blocks_outer_handlers_and_preserves_signal_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar gd-b-safe-events nil)
  (defvar gd-b-safe-hooks nil)
  (defvar gd-b-safe-debug nil)
  (defvar gd-b-safe-armed nil)
  (let ((old (symbol-function 'substitute-command-keys)) out)
    (unwind-protect
        (dolist (mode '(interpreted bytecode))
          (setq gd-b-safe-events nil gd-b-safe-hooks nil gd-b-safe-debug nil
                gd-b-safe-armed nil)
          (let ((callback
                 (lambda (text)
                   (if gd-b-safe-armed
                       (handler-bind-1
                        (lambda () (signal 'error '("safe failure")))
                        '(error)
                        (lambda (_)
                          (push (list 'inner inhibit-redisplay inhibit-debugger)
                                gd-b-safe-events)))
                     text))))
            (fset 'substitute-command-keys
                  (if (eq mode 'bytecode) (byte-compile callback) callback)))
          (dotimes (_ 128)
            (error-message-string '(wrong-type-argument integerp nope)))
          (setq gd-b-safe-armed t)
          (let ((signal-hook-function
                 (lambda (symbol data)
                   (push (list symbol data inhibit-redisplay inhibit-debugger)
                         gd-b-safe-hooks)))
                (debug-on-error t)
                (debugger (lambda (&rest args) (push args gd-b-safe-debug))))
            (push
             (list mode
                   (handler-bind-1
                    (lambda ()
                      (error-message-string '(wrong-type-argument integerp nope)))
                    '(error) (lambda (_) (push 'outer gd-b-safe-events)))
                   gd-b-safe-events gd-b-safe-hooks gd-b-safe-debug)
             out)))
      (fset 'substitute-command-keys old))
    (nreverse out)))"#;
    crate::common::assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            "\"OK ((interpreted \\\"Wrong type argument: integerp, nope\\\" ((inner t nil)) ((error (\\\"safe failure\\\") t nil)) nil) (bytecode \\\"Wrong type argument: integerp, nope\\\" ((inner t nil)) ((error (\\\"safe failure\\\") t nil)) nil))\""
        ]],
    );
}
