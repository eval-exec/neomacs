//! Strict combo oracle probes, batch 347: cl-values / multiple-values deep.
//! cl-values, cl-multiple-value-bind, cl-multiple-value-list,
//! cl-multiple-value-setq, and cl-nth-value.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_values_multiple_value_bind_setq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-values 1 2 3)
      (cl-multiple-value-bind (a b c) (cl-values 10 20 30) (list a b c))
      (cl-multiple-value-list (cl-values 'x 'y))
      (progn
        (cl-multiple-value-setq (p q r) (cl-values 1 2 3))
        (list p q r))
      (cl-nth-value 1 (cl-values 'a 'b 'c)))
"##;
    let expect = expect_test::expect![[r#""OK ((1 2 3) (10 20 30) (x y) (1 2 3) b)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_values_from_function_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(cl-defun probe-vals (a b) (cl-values a b (* a b)))
(list (cl-multiple-value-list (probe-vals 3 4))
      (cl-multiple-value-bind (x y z) (probe-vals 5 6) (list x y z))
      (cl-nth-value 2 (probe-vals 7 8)))
"##;
    let expect = expect_test::expect![[r#""OK ((3 4 12) (5 6 30) 56)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_values_zero_single_extra() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-values)
      (cl-values 'single)
      (cl-multiple-value-list (cl-values))
      (cl-multiple-value-list (cl-values 'one))
      (cl-nth-value 0 (cl-values 'first 'second))
      (cl-nth-value 5 (cl-values 'a 'b)))
"##;
    let expect = expect_test::expect![[r#""OK (nil (single) nil (one) first nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
