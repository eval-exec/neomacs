//! Strict combo oracle probes, batch 307: hook running deep. run-hooks,
//! run-hook-with-args / -until-success / -until-failure / -wrapped, and
//! add-hook/remove-hook with append depth + buffer-local hooks.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_run_hook_with_args_until_success_failure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((probe-hook-rha nil))
  (add-hook 'probe-hook-rha (lambda (x) (and (> x 5) 'big)))
  (add-hook 'probe-hook-rha (lambda (x) (and (< x 3) 'small)))
  (add-hook 'probe-hook-rha (lambda (x) 'default))
  (list (run-hook-with-args-until-success 'probe-hook-rha 10)
        (run-hook-with-args-until-success 'probe-hook-rha 2)
        (run-hook-with-args-until-success 'probe-hook-rha 4)
        (run-hook-with-args-until-failure 'probe-hook-rha 4)
        (run-hook-with-args 'probe-hook-rha 7)))
"##;
    let expect = expect_test::expect![[r#""OK (default default default nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_run_hook_wrapped_add_hook_append_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((probe-hook-wrapped nil)
      (order nil))
  (add-hook 'probe-hook-wrapped (lambda () (push 'a order)))
  (add-hook 'probe-hook-wrapped (lambda () (push 'b order)) t)
  (add-hook 'probe-hook-wrapped (lambda () (push 'c order)))
  (run-hook-wrapped 'probe-hook-wrapped
                    (lambda (fn args)
                      (push (cons 'wrap (apply fn args)) order)
                      nil))
  (nreverse order))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure ((order)) (fn args) (setq order (cons (cons 'wrap (apply fn args)) order)) nil) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_add_remove_hook_buffer_local_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((probe-hook-ar nil)
      (log nil))
  (add-hook 'probe-hook-ar (lambda () (push 'fired log)))
  (run-hooks 'probe-hook-ar)
  (let ((c1 (copy-sequence log)))
    (remove-hook 'probe-hook-ar (lambda () (push 'fired log)))
    (run-hooks 'probe-hook-ar)
    (list c1
          log
          probe-hook-ar
          (default-value 'probe-hook-ar))))
"##;
    let expect = expect_test::expect![[r#""OK ((fired) (fired) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
