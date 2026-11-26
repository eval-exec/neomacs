//! Oracle parity tests for the full bootstrapped runtime surface.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm_expect};

#[test]
fn oracle_prop_full_bootstrap_core_surface() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (not (null (get 'float 'cl--class)))
  (not (null (get 'integer 'cl--class)))
  (not (null (get 'float 'cl-deftype-satisfies)))
  (not (null (get 'integer 'cl-deftype-satisfies)))
    (coding-system-p 'iso-8859-15)
  (stringp system-configuration-features))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t t t t t t)", &oracle, &neovm);
}

/// GNU 31.1's `lisp/loadup.el` loads `emacs-lisp/pcase` only in a bootstrap
/// build whose `macroexpand-all` is still uncompiled; `rx` is autoloaded.
/// The dumped image therefore has neither feature, only the two loaddefs
/// autoload stubs (`rx--pcase-macroexpander`, `pcase--make-docstring`), and
/// no other interned `rx--`/`pcase--` names.
#[test]
fn oracle_bootstrap_does_not_intern_rx_and_pcase_internals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (featurep 'rx)
  (featurep 'pcase)
  (intern-soft "rx--translate")
  (intern-soft "pcase--expand")
  (intern-soft "rx--pcase-macroexpander")
  (intern-soft "pcase--make-docstring")
  (let ((n 0)) (mapatoms (lambda (s) (when (string-prefix-p "rx--" (symbol-name s)) (setq n (1+ n))))) n)
  (let ((n 0)) (mapatoms (lambda (s) (when (string-prefix-p "pcase--" (symbol-name s)) (setq n (1+ n))))) n))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil rx--pcase-macroexpander pcase--make-docstring 1 1)""#
    ]];
    let (oracle, neovm) = eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "(nil nil nil nil rx--pcase-macroexpander pcase--make-docstring 1 1)",
        &oracle,
        &neovm,
    );
}
