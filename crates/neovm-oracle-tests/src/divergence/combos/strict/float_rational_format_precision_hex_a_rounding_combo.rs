//! Strict combo oracle probes, batch 170: float + rational formatting edge
//! cases. hex-float %a, high-precision %g round-trip, rational printing (1/3,
//! 22/7), infinity/zero print, least-positive-normalized-float, fixnum
//! boundary print, and banker's/round-half-even %.2f edges.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_float_hex_high_precision_rational_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format "%.17g" 0.1)
      (format "%a" 0.1)
      (format "%a" 1.0)
      (format "%a" 2.0)
      (format "%.30g" (/ 1.0 3.0))
      (number-to-string 0.1)
      (prin1-to-string (/ 1 3))
      (prin1-to-string (/ 22 7))
      (format "%s" (/ 1 7))
      (prin1-to-string (/ 10 4))
      (format "%g" 12345678901234567890.0)
      (format "%e" 0.0)
      (format "%.0f" 3.7)
      (format "%.2f" 2.675)
      (format "%.2f" 0.125)
      (format "%.2f" 2.5)
      (format "%.2f" 3.5))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Invalid format operation %a\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_float_extremes_infinity_zero_denormal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((inf (/ 1.0 0.0)))
  (list (prin1-to-string inf)
        (prin1-to-string (- inf))
        (prin1-to-string (/ 0.0 0.0))
        (prin1-to-string 0.0)
        (prin1-to-string -0.0)
        (eq 0.0 -0.0)
        (= 0.0 -0.0)
        (prin1-to-string 1e300)
        (prin1-to-string 1e-300)
        (prin1-to-string most-positive-fixnum)
        (prin1-to-string (1+ most-positive-fixnum))
        (prin1-to-string least-positive-normalized-float)
        (format "%.16e" least-positive-normalized-float)
        (floatp least-positive-normalized-float)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable least-positive-normalized-float)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_rational_exact_arithmetic_and_convert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (+ (/ 1 3) (/ 1 6))
      (/ (/ 1 3) (/ 1 6))
      (* (/ 2 3) (/ 3 4))
      (denominator (/ 6 9))
      (numerator (/ 6 9))
      (float (/ 1 4))
      (number-to-string (/ 1 100))
      (truncate (/ 10 3))
      (floor (/ -10 3))
      (ceiling (/ 10 3))
      (round (/ 7 2))
      (* 1/3 3)
      (= (* 1/10 10) 1)
      (= (* 0.1 10) 1))
"##;
    let expect = expect_test::expect![[r#""ERR (arith-error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
