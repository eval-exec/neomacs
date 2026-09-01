//! Strict combo oracle probes, batch 359: condition-case + unwind-protect +
//! catch/throw deep nesting combo. Nested condition-case handler chains,
//! unwind cleanup after non-local exit, and catch/throw across condition-case.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_nested_condition_case_handler_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (condition-case outer
      (condition-case inner
          (signal 'arith-error '("inner"))
        (wrong-type-argument (push 'inner-wta log)))
    (arith-error (push 'outer-arith log)))
  (nreverse log))
"##;
    let expect = expect_test::expect![[r#""OK (outer-arith)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_unwind_cleanup_after_throw_across_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (catch 'escape
    (unwind-protect
        (condition-case err
            (signal 'error '("boom"))
          (error
           (push 'caught-error log)
           (throw 'escape 'escaped)))
      (push 'cleanup log)))
  (nreverse log))
"##;
    let expect = expect_test::expect![[r#""OK (caught-error cleanup)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
