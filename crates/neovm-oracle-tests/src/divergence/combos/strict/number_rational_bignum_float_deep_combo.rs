//! Strict combo oracle probes, batch 294: number/rational deep. rational
//! arithmetic, bignum bit ops, float precision edges, and number predicates.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_rational_arithmetic_simplify_gcd() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (+ 1/2 1/3)
      (- 5/6 1/6)
      (* 2/3 3/4)
      (/ 1/2 1/4)
      (+ 1/3 2)
      (denominator 6/9)
      (numerator 6/9)
      (gcd 12 18)
      (gcd 0 5)
      (gcd 48 36 24)
      (float 1/4)
      (numberp 1/3)
      (wholenump 5))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable 1/2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_bignum_bitwise_shift_ash_logand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (ash 1 64)
      (ash (expt 2 70) -10)
      (logand (expt 2 64) (1- (expt 2 65)))
      (logior (expt 2 64) 1)
      (logxor (expt 2 64) (expt 2 64))
      (lognot 0)
      (logcount (expt 2 64))
      (logcount (1- (expt 2 64)))
      (% (expt 2 70) 7)
      (lsh 1 32)
      (integer-length (expt 2 64)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function integer-length)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_float_precision_predicate_nan_inf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((inf (/ 1.0 0.0))
      (nan (/ 0.0 0.0)))
  (list (isnan nan)
        (isnan 1.5)
        (floatp inf)
        (numberp inf)
        (isnan inf)
        (< most-positive-fixnum (expt 2 64))
        (bignump (expt 2 64))
        (fixnump 5)
        (= 0.0 -0.0)
        (eq 0.0 -0.0)
        (< 1e-300 1e-200)
        (<= 3.0 3)))
"##;
    let expect = expect_test::expect![[r#""OK (t nil t t nil t t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
