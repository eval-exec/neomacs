//! Oracle parity tests for GNU `run-hook-wrapped` primitive semantics.
//!
//! GNU implements `run-hook-wrapped` in `src/eval.c` through
//! `run_hook_with_args`.  It runs hook entries through a wrapper function,
//! stops on the first non-nil wrapper result, supports function-valued hook
//! variables, and treats `t` in a local hook as a splice of the global hook.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_run_hook_wrapped_function_value_and_stop_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil)
      (hook (lambda (x y)
              (push (list 'hook x y) log)
              nil)))
  (list
   (run-hook-wrapped
    'hook
    (lambda (fun x y)
      (push (list 'wrap fun x y) log)
      (funcall fun x y)
      'wrapped-result)
    1 2)
   (nreverse log)
   (let ((log nil)
         (hook
          (list
           (lambda (x) (push (list 'first x) log) nil)
           (lambda (x) (push (list 'second x) log) nil))))
     (list
      (run-hook-wrapped
       'hook
       (lambda (fun x)
         (funcall fun x)
         (and (eq fun (cadr hook)) 'stop-here))
       9)
      (nreverse log)))))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil (nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_run_hook_wrapped_local_t_splices_global_and_ignores_global_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--rhw-hook nil)
  (let ((log nil))
    (unwind-protect
        (progn
          (setq neovm--rhw-hook
                (list
                 (lambda (x) (push (list 'global-a x) log) nil)
                 t
                 (lambda (x) (push (list 'global-b x) log) nil)))
          (with-temp-buffer
            (setq-local neovm--rhw-hook
                        (list
                         (lambda (x) (push (list 'local x) log) nil)
                         t
                         (lambda (x) (push (list 'after-global x) log) nil)))
            (list
             (run-hook-wrapped
              'neovm--rhw-hook
              (lambda (fun x)
                (funcall fun x))
              7)
             (nreverse log))))
      (makunbound 'neovm--rhw-hook))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil ((local 7) (global-a 7) (global-b 7) (after-global 7)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
