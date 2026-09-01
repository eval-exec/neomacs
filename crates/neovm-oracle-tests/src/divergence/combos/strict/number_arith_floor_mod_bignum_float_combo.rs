//! Strict combo oracle probes, batch 152: number arithmetic edge cases.
//! floor/ceiling/round/truncate (incl divisor), mod vs % sign rules with
//! negatives, bignum arithmetic (expt 2 64/2 100, factorial), fixnum boundary
//! (1+ most-positive-fixnum transitions to bignum), float NaN/inf and isnan,
//! frexp/ldexp/copysign/fma/ilogb, and rational arithmetic via /.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_floor_ceiling_round_truncate_mod_sign_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (floor 7 3)
      (floor -7 3)
      (floor 7 -3)
      (ceiling 7 3)
      (ceiling -7 3)
      (round 7 3)
      (round 6 4)
      (round -7 3)
      (truncate 7 3)
      (truncate -7 3)
      (mod -7 3)
      (mod 7 -3)
      (% -7 3)
      (% 7 -3)
      (mod 7 3)
      (% 7 3))
"##;
    let expect = expect_test::expect![[r#""OK (2 -3 -3 3 -2 2 2 -2 2 -2 2 -2 -1 1 1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_bignum_factorial_fixnum_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (expt 2 64)
      (expt 2 100)
      (1+ most-positive-fixnum)
      (1- most-negative-fixnum)
      (integerp (expt 2 64))
      (fixnump (expt 2 64))
      (fixnump most-positive-fixnum)
      (* (expt 2 64) (expt 2 64))
      (let ((acc 1))
        (dotimes (i 25) (setq acc (* acc (1+ i))))
        acc)
      (/ (expt 10 40) (expt 10 30))
      (% (expt 10 40) 7)
      (gcd (expt 2 20) (expt 2 30))
      (logand (expt 2 64) (1- (expt 2 64))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function gcd)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_float_nan_inf_frexp_ldexp_copysign_fma() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((inf (/ 1.0 0))
       (nan (/ 0.0 0.0)))
  (list (isnan nan)
        (isnan 1.5)
        (isnan inf)
        (numberp inf)
        (eq inf (/ -1.0 0))
        (= nan nan)
        (frexp 8.0)
        (frexp 0.75)
        (ldexp 0.5 4)
        (copysign 3.0 -2.0)
        (copysign -3.0 2.0)
        (fma 2.0 3.0 4.0)
        (ilogb 8.0)
        (isnan (sqrt -1.0))
        (/ 0.0 0)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function fma)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
