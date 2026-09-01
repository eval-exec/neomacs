//! Oracle parity tests for GNU `subr.el` `values--store-value`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_values_store_value_pushes_and_returns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:values--store-value pushes the exact VALUE object onto the
    // obsolete dynamically-bound `values' list, preserves the old tail, and
    // returns VALUE rather than the updated list.
    let form = r#"
(let ((values '(old-tail)))
  (with-suppressed-warnings ((obsolete values))
    (let* ((cons-value (list 'alpha 'beta))
           (vector-value (vector 'gamma 'delta))
           (string-value (copy-sequence "epsilon"))
           (ret1 (values--store-value cons-value))
           (state1 values)
           (ret2 (values--store-value vector-value))
           (state2 values)
           (ret3 (values--store-value string-value))
           (state3 values))
      (list
       (eq ret1 cons-value)
       (eq ret2 vector-value)
       (eq ret3 string-value)
       (eq (car state1) cons-value)
       (equal state1 (list cons-value 'old-tail))
       (eq (car state2) vector-value)
       (eq (cadr state2) cons-value)
       (equal (cddr state2) '(old-tail))
       (eq (car state3) string-value)
       (eq (cadr state3) vector-value)
       (eq (caddr state3) cons-value)
       (equal (cdddr state3) '(old-tail))
       values))))
"#;
    let expect =
        expect_test::expect![[r#""OK (t t t nil nil nil nil nil nil nil nil nil (old-tail))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
