//! Strict combo oracle probes, batch 261: gc / debug / max-depth / random
//! variable existence sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable
//! bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_gc_threshold_percentage_messages_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'gc-cons-threshold)
      (boundp 'gc-cons-percentage)
      (boundp 'garbage-collection-messages)
      (boundp 'gc-elapsed)
      (boundp 'gcs-done)
      (boundp 'pure-bytes-used)
      (boundp 'purify-flag)
      (boundp 'max-specpdl-size)
      (boundp 'max-lisp-eval-depth)
      (boundp 'memory-full)
      (boundp 'memory-signal-data)
      (boundp 'cons-cells-consed))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_debug_on_error_quit_signal_hook_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'debug-on-error)
      (boundp 'debug-on-quit)
      (boundp 'debug-on-signal)
      (boundp 'stack-trace-on-error)
      (boundp 'debugger)
      (boundp 'signal-hook-function)
      (boundp 'debug-ignored-errors)
      (boundp 'debugger-args)
      (boundp 'debugger-bury-or-kill)
      (boundp 'backtrace-frame)
      (boundp 'command-debug-status)
      (boundp 'eval-expression-debug-on-error))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil t t t nil nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_random_seed_misc_runtime_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'random-seed)
      (boundp 'random)
      (boundp 'lambda-alist-key)
      (boundp 'print-gensym)
      (boundp 'print-circle)
      (boundp 'print-length)
      (boundp 'print-level)
      (boundp 'standard-input)
      (boundp 'standard-output)
      (boundp 'float-output-format)
      (boundp 'float-pi)
      (boundp 'most-positive-fixnum))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil nil t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
