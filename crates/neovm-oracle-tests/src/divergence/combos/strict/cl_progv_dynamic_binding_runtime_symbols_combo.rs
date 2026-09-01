//! Strict combo oracle probes, batch 352: cl-progv dynamic binding of runtime-
//! determined symbol lists. cl-progv with computed symbol/value lists,
//! nesting, and interaction with dynamic scoping.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_progv_runtime_symbol_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(defvar probe-pgv-a 'global)
(defvar probe-pgv-b 'global)
(defvar probe-pgv-c 'global)
(list (cl-progv '(probe-pgv-a probe-pgv-b probe-pgv-c) '(1 2 3)
         (list probe-pgv-a probe-pgv-b probe-pgv-c))
      probe-pgv-a   ;; restored
      probe-pgv-b
      probe-pgv-c)
"##;
    let expect = expect_test::expect![[r#""OK ((1 2 3) global global global)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_progv_computed_symbols_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(defvar probe-pgv-x 'outer)
(let ((syms '(probe-pgv-x))
      (vals '((inner))))
  (list (cl-progv syms vals probe-pgv-x)
        (cl-progv syms vals
          (cl-progv syms '((deeper)) probe-pgv-x))
        probe-pgv-x))
"##;
    let expect = expect_test::expect![[r#""OK ((inner) (deeper) outer)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_progv_empty_mismatched_lengths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(defvar probe-pgv-y 'global)
(list (cl-progv nil nil 'no-binding)
      (cl-progv '(probe-pgv-y) '(42) probe-pgv-y)
      probe-pgv-y))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 6 19)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
