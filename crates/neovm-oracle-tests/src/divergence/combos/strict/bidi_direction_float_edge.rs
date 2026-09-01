//! Strict combo oracle probes, batch 87: bidi text direction (RTL handling,
//! char-direction, paragraph-direction) and float formatting edge cases (max/
//! min double, subnormals, many-decimal precision, NaN/inf formatting).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_q1_bidi_text_direction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-direction)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-direction ?a)
      (char-direction ?A)
      (char-direction 1488)
      (char-direction 1575)
      (with-temp-buffer
        (insert "Hello world")
        (bidi-paragraph-direction))
      (with-temp-buffer
        (insert "שלום עולם")
        (bidi-paragraph-direction)))
"##,
        expect,
    );
}

#[test]
fn div_q1_float_formatting_extreme_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"0.10000000000000000555\" \"1.00000000000000000000\" \"1.00000000000000000000e+00\" \"0.333333333333333314829616256247\" \"0.1\" \"1.7976931348623157e+308\" \"5e-324\" \"0\" \"1\" \"0.10000000000000001\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%.20f" 0.1)
      (format "%.20f" 1.0)
      (format "%.20e" 1.0)
      (format "%.30f" (/ 1.0 3.0))
      (format "%.15g" 0.1)
      (format "%s" 1.7976931348623157e+308)
      (format "%s" 5e-324)
      (format "%.0f" 0.49999999999999994)
      (format "%.0f" 0.5000000000000001)
      (format "%.17g" 0.1))
"##,
        expect,
    );
}

#[test]
fn div_q1_bidi_mixed_ltr_rtl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function bidi-paragraph-direction)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcשלוםdef")
  (list (point-min)
        (point-max)
        (buffer-substring 1 10)
        (bidi-paragraph-direction)))
"##,
        expect,
    );
}

#[test]
fn div_q1_float_arithmetic_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (1.0e+INF -1.0e+INF -0.0e+NaN t nil 1.0e+INF nil t -1.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (/ 1.0 0.0)
      (/ -1.0 0.0)
      (/ 0.0 0.0)
      (isnan (/ 0.0 0.0))
      (isnan 1.0)
      (* 1.0e+308 10.0)
      (isnan (* 1.0e+308 10.0))
      (< 5e-324 1.0e-300)
      (copysign 1.0 -1.0))
"##,
        expect,
    );
}
