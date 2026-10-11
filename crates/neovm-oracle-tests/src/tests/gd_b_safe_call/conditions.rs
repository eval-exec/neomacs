use super::ENVS;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_gd_b_safe_call_catches_all_conditions_and_preserves_throws() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (put 'gd-b-safe-condition 'error-conditions '(gd-b-safe-condition))
  (let ((old (symbol-function 'substitute-command-keys)) out)
    (unwind-protect
        (progn
          (dolist (condition '(error quit gd-b-safe-condition))
            (fset 'substitute-command-keys
                  (lambda (_) (signal condition '(payload))))
            (push (list condition
                        (error-message-string '(wrong-type-argument integerp nope)))
                  out))
          (fset 'substitute-command-keys
                (lambda (_) (throw 'gd-b-safe-escape 'escaped)))
          (push (catch 'gd-b-safe-escape
                  (error-message-string '(wrong-type-argument integerp nope))
                  'swallowed)
                out))
      (fset 'substitute-command-keys old))
    (nreverse out)))"#;
    crate::common::assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            "\"OK ((error \\\"Wrong type argument: integerp, nope\\\") (quit \\\"Wrong type argument: integerp, nope\\\") (gd-b-safe-condition \\\"Wrong type argument: integerp, nope\\\") escaped)\""
        ]],
    );
}
