use super::ENVS;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_gd_b_safe_call_debug_on_signal_can_override_qt_debugger_suppression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar gd-b-safe-debug nil)
  (defvar gd-b-safe-armed nil)
  (let ((old (symbol-function 'substitute-command-keys)) out)
    (unwind-protect
        (dolist (mode '(interpreted bytecode))
          (setq gd-b-safe-armed nil)
          (let ((callback (lambda (text)
                            (if gd-b-safe-armed
                                (signal 'error '("safe failure"))
                              text))))
            (fset 'substitute-command-keys
                  (if (eq mode 'bytecode) (byte-compile callback) callback)))
          (dotimes (_ 128)
            (error-message-string '(wrong-type-argument integerp nope)))
          (setq gd-b-safe-armed t)
          (dolist (override '(nil t))
            (setq gd-b-safe-debug nil internal-when-entered-debugger -1)
            (let ((debug-on-error t)
                  (debug-on-signal override)
                  (debugger (lambda (&rest args) (push args gd-b-safe-debug))))
              (push (list mode override
                          (error-message-string '(wrong-type-argument integerp nope))
                          gd-b-safe-debug)
                    out))))
      (fset 'substitute-command-keys old))
    (nreverse out)))"#;
    crate::common::assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            "\"OK ((interpreted nil \\\"Wrong type argument: integerp, nope\\\" nil) (interpreted t \\\"Wrong type argument: integerp, nope\\\" ((error (error \\\"safe failure\\\")))) (bytecode nil \\\"Wrong type argument: integerp, nope\\\" nil) (bytecode t \\\"Wrong type argument: integerp, nope\\\" ((error (error \\\"safe failure\\\")))))\""
        ]],
    );
}
