use super::ENVS;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_gd_b_safe_call_its_own_binding_is_outside_the_condition_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((old (symbol-function 'substitute-command-keys)) out)
  (unwind-protect
      (progn
        (fset 'substitute-command-keys (lambda (text) text))
        (dolist (operation '(let unlet))
          (let ((watcher (lambda (_symbol _value observed _where)
                           (when (eq observed operation)
                             (signal 'error '("safe binding"))))))
            (unwind-protect
                (progn
                  (add-variable-watcher 'inhibit-redisplay watcher)
                  (push (list operation
                              (condition-case err
                                  (error-message-string '(wrong-type-argument integerp nope))
                                (error err))) out))
              (remove-variable-watcher 'inhibit-redisplay watcher)))))
    (fset 'substitute-command-keys old))
  (nreverse out))"#;
    crate::common::assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            "\"OK ((let (error \\\"safe binding\\\")) (unlet (error \\\"safe binding\\\")))\""
        ]],
    );
}
