//! Oracle parity tests for GNU `subr.el` `function-get` semantics.
//!
//! GNU walks symbol function definitions until it finds a property on a symbol,
//! reaches a non-symbol function object, or hits an unbound function.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_function_get_reads_direct_symbol_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (fset 'neovm--fg-direct (lambda () 'ok))
      (put 'neovm--fg-direct 'neovm-prop 'direct-value)
      (list
       (function-get 'neovm--fg-direct 'neovm-prop)
       (function-get 'neovm--fg-direct 'missing-prop)
       (function-get (symbol-function 'neovm--fg-direct) 'neovm-prop)))
  (fmakunbound 'neovm--fg-direct)
  (setplist 'neovm--fg-direct nil))
"#;

    let expect = expect_test::expect![[r#""OK (direct-value nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_function_get_follows_alias_chain_to_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (fset 'neovm--fg-target (lambda () 'target))
      (defalias 'neovm--fg-alias-1 'neovm--fg-target)
      (defalias 'neovm--fg-alias-2 'neovm--fg-alias-1)
      (put 'neovm--fg-target 'neovm-prop 'target-value)
      (list
       (function-get 'neovm--fg-alias-2 'neovm-prop)
       (function-get 'neovm--fg-alias-1 'neovm-prop)
       (function-get 'neovm--fg-target 'neovm-prop)))
  (mapc (lambda (sym)
          (fmakunbound sym)
          (setplist sym nil))
        '(neovm--fg-target neovm--fg-alias-1 neovm--fg-alias-2)))
"#;

    let expect = expect_test::expect![[r#""OK (target-value target-value target-value)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_function_get_stops_at_first_symbol_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (fset 'neovm--fg-base (lambda () 'base))
      (defalias 'neovm--fg-wrapper 'neovm--fg-base)
      (put 'neovm--fg-base 'neovm-prop 'base-value)
      (put 'neovm--fg-wrapper 'neovm-prop 'wrapper-value)
      (list
       (function-get 'neovm--fg-wrapper 'neovm-prop)
       (function-get 'neovm--fg-base 'neovm-prop)))
  (mapc (lambda (sym)
          (fmakunbound sym)
          (setplist sym nil))
        '(neovm--fg-base neovm--fg-wrapper)))
"#;

    let expect = expect_test::expect![[r#""OK (wrapper-value base-value)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_function_get_unbound_and_non_symbol_inputs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (list
     (function-get 'neovm--fg-unbound 'neovm-prop)
     (function-get (lambda () 'x) 'neovm-prop)
     (function-get #'car 'side-effect-free))
  (fmakunbound 'neovm--fg-unbound)
  (setplist 'neovm--fg-unbound nil))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
